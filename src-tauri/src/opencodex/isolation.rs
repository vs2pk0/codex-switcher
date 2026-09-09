use std::fs;
use std::path::{Path, PathBuf};

pub fn validate_directory(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                return Err(format!(
                    "实例目录不能经过符号链接或普通文件：{}",
                    ancestor.display()
                ))
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

pub fn read_port(path: &Path) -> Option<u16> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    u16::try_from(value.get("port")?.as_u64()?)
        .ok()
        .filter(|p| *p >= 1024)
}

pub fn validate_cleanup(home: &Path, manager: &Path, codex: &Path) -> Result<(), String> {
    let real_home = dirs::home_dir().ok_or("无法定位用户目录")?;
    let resolved_codex = codex.canonicalize().unwrap_or_else(|_| codex.to_path_buf());
    for path in [home, manager] {
        validate_directory(path)?;
        if path.parent().is_none()
            || codex.starts_with(path)
            || resolved_codex.starts_with(path)
            || path == real_home
        {
            return Err("拒绝清理包含 Codex 或用户主目录的路径".into());
        }
    }
    if home.file_name().and_then(|v| v.to_str()) != Some(".opencodex")
        || manager.file_name().and_then(|v| v.to_str()) != Some("opencodex-manager")
    {
        return Err("OpenCodex 清理目录不符合实例结构".into());
    }
    Ok(())
}

/// The caller must stop/unregister the service and restore Codex routing first.
pub fn remove_instance_data(home: &Path, manager: &Path, codex: &Path) -> Result<(), String> {
    validate_cleanup(home, manager, codex)?;
    // Preserve the launcher for retry if removing the data fails.
    if home.exists() {
        fs::remove_dir_all(home).map_err(|e| format!("清理实例 OpenCodex 数据失败：{e}"))?;
    }
    if manager.exists() {
        fs::remove_dir_all(manager)
            .map_err(|e| format!("数据已清理，但 Engine 目录删除失败：{e}"))?;
    }
    Ok(())
}

/// Copy an installed tree without retaining links into the source instance.
/// npm's file shims are materialized; directory links and escaping links fail closed.
pub fn copy_engine_tree(source: &Path, target: &Path) -> Result<(), String> {
    validate_directory(source)?;
    validate_directory(target)?;
    if target.exists() {
        return Err("Engine 复制目标已存在".into());
    }
    let root = source.canonicalize().map_err(|e| e.to_string())?;
    fn walk(source: &Path, target: &Path, root: &Path) -> Result<(), String> {
        let meta = fs::symlink_metadata(source).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            fs::create_dir(target).map_err(|e| e.to_string())?;
            for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                walk(&entry.path(), &target.join(entry.file_name()), root)?;
            }
        } else {
            let resolved = source.canonicalize().map_err(|e| e.to_string())?;
            if !resolved.starts_with(root) || !resolved.is_file() {
                return Err("Engine 包包含外部链接或特殊文件，不能复制".into());
            }
            fs::copy(resolved, target).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    walk(source, target, &root)
}

/// Rebind the copied package, never the source package, to its new instance.
pub fn rebind_engine(package: &Path, target_id: &str, real_home: &Path) -> Result<(), String> {
    let marker = package.join(".switcher-instance");
    if marker.exists() {
        let source_id = fs::read_to_string(&marker).map_err(|e| e.to_string())?;
        if source_id.is_empty()
            || !source_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err("源 Engine 实例标识无效".into());
        }
        fn restore(path: &Path, id: &str) -> Result<(), String> {
            for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.is_dir() {
                    restore(&path, id)?;
                } else if path.extension().is_some_and(|ext| ext == "ts") {
                    let original = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                    let updated = original
                        .replace(&format!("com.opencodex.proxy.{id}"), "com.opencodex.proxy")
                        .replace(&format!("opencodex-proxy-{id}"), "opencodex-proxy");
                    if original != updated {
                        fs::write(path, updated).map_err(|e| e.to_string())?;
                    }
                }
            }
            Ok(())
        }
        restore(&package.join("src"), &source_id)?;
        fs::remove_file(marker).map_err(|e| e.to_string())?;
    }
    scope_engine(package, target_id, real_home)
}

/// Each managed Engine has its own source copy. Scope upstream's hard-coded
/// service identifiers before executing it, including service discovery.
pub fn scope_engine(package: &Path, id: &str, real_home: &Path) -> Result<(), String> {
    if id == "default" {
        return Ok(());
    }
    let marker = package.join(".switcher-instance");
    if let Ok(saved) = fs::read_to_string(&marker) {
        return if saved == id {
            Ok(())
        } else {
            Err("Engine 目录属于其他实例".into())
        };
    }
    let src = package.join("src");
    let service = fs::read_to_string(src.join("service.ts")).map_err(|e| e.to_string())?;
    if !service.contains("com.opencodex.proxy") || !service.contains("opencodex-proxy") {
        return Err("当前 Engine 服务结构不支持实例隔离，请选择兼容版本".into());
    }
    let real_home =
        serde_json::to_string(&real_home.to_string_lossy()).map_err(|e| e.to_string())?;
    fn walk(path: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                walk(&entry.path(), paths)?;
            } else if kind.is_file() && entry.path().extension().is_some_and(|e| e == "ts") {
                paths.push(entry.path());
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    walk(&src, &mut paths)?;
    for path in paths {
        let original = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut updated = original
            .replace(&format!("com.opencodex.proxy.{id}"), "com.opencodex.proxy")
            .replace(&format!("opencodex-proxy-{id}"), "opencodex-proxy")
            .replace("com.opencodex.proxy", &format!("com.opencodex.proxy.{id}"))
            .replace("opencodex-proxy", &format!("opencodex-proxy-{id}"));
        // launchd/systemd need definitions in the actual user's autostart folder.
        // All OpenCodex data still resolves through the isolated process home.
        if path == src.join("service.ts") {
            updated = updated
                .replace(
                    "join(homedir(), \"Library\", \"LaunchAgents\"",
                    &format!("join({real_home}, \"Library\", \"LaunchAgents\""),
                )
                .replace(
                    "join(homedir(), \".config\", \"systemd\", \"user\")",
                    &format!("join({real_home}, \".config\", \"systemd\", \"user\")"),
                );
            // Persist the scoped fallback home across OS service restarts.
            let anchor = "const envLines = [";
            let scoped = "const envLines = [\n    ...[\"HOME\", \"USERPROFILE\", \"XDG_CONFIG_HOME\", \"XDG_DATA_HOME\", \"XDG_CACHE_HOME\"].filter(name => process.env[name]).map(name => process.platform === \"darwin\" ? `    <key>${name}</key><string>${plistString(process.env[name]!)}</string>` : systemdEnvironmentAssignment(name, process.env[name])),";
            if !updated.contains("filter(name => process.env[name]).map(name") {
                updated = updated.replace(anchor, scoped);
            }
        }
        if path == src.join("service-manager-probe.ts") {
            updated = updated.replace(
                "const home = deps.home ?? homedir();",
                &format!("const home = deps.home ?? {real_home};"),
            );
        }
        if updated != original {
            fs::write(&path, updated).map_err(|e| e.to_string())?;
        }
    }
    fs::write(marker, id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_removes_only_opencodex_and_rejects_codex_overlap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join(".opencodex");
        let manager = root.join("opencodex-manager");
        let codex = root.join("codex-home");
        for path in [&home, &manager, &codex, &root.join("other-instance")] {
            fs::create_dir_all(path).unwrap();
            fs::write(path.join("keep.json"), "fixture").unwrap();
        }
        assert!(remove_instance_data(&home, &manager, &home.join("codex-home")).is_err());
        assert!(home.exists());
        remove_instance_data(&home, &manager, &codex).unwrap();
        assert!(!home.exists());
        assert!(!manager.exists());
        assert!(codex.join("keep.json").exists());
        assert!(root.join("other-instance/keep.json").exists());
    }
    #[test]
    fn copied_engine_is_rebound_without_changing_source() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let source = root.join("source");
        fs::create_dir_all(source.join("src")).unwrap();
        fs::write(
            source.join("src/service.ts"),
            "com.opencodex.proxy opencodex-proxy",
        )
        .unwrap();
        scope_engine(&source, "first", &root).unwrap();
        let original = fs::read(source.join("src/service.ts")).unwrap();
        let copied = root.join("copied");
        copy_engine_tree(&source, &copied).unwrap();
        rebind_engine(&copied, "second", &root).unwrap();
        let updated = fs::read_to_string(copied.join("src/service.ts")).unwrap();
        assert!(updated.contains("com.opencodex.proxy.second"));
        assert!(!updated.contains("first"));
        assert_eq!(original, fs::read(source.join("src/service.ts")).unwrap());
        rebind_engine(&copied, "default", &root).unwrap();
        assert_eq!(
            fs::read_to_string(copied.join("src/service.ts")).unwrap(),
            "com.opencodex.proxy opencodex-proxy"
        );
    }

    #[test]
    #[cfg(unix)]
    fn copying_materializes_internal_file_links_and_rejects_external_links() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("entry"), "entry").unwrap();
        std::os::unix::fs::symlink("entry", source.join("shim")).unwrap();
        let target = root.join("target");
        copy_engine_tree(&source, &target).unwrap();
        assert!(!fs::symlink_metadata(target.join("shim"))
            .unwrap()
            .file_type()
            .is_symlink());
        fs::write(root.join("outside"), "do not copy").unwrap();
        std::os::unix::fs::symlink("../outside", source.join("escape")).unwrap();
        assert!(copy_engine_tree(&source, &root.join("unsafe-copy")).is_err());
    }
    #[test]
    #[ignore = "requires OPENCODEX_TEST_ENGINE_DIR and OPENCODEX_TEST_BUN; uses temporary data only"]
    #[cfg(target_os = "macos")]
    fn real_engine_instances_are_isolated() {
        let engine = std::env::var("OPENCODEX_TEST_ENGINE_DIR").expect("test engine directory");
        let bun = std::env::var("OPENCODEX_TEST_BUN").expect("test Bun runtime");
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        for id in ["smoke-a", "smoke-b"] {
            let dest = root.join(id);
            if id == "smoke-b" {
                copy_engine_tree(&root.join("smoke-a"), &dest).unwrap();
                rebind_engine(&dest.join("node_modules/@bitkyc08/opencodex"), id, &root).unwrap();
            } else {
                assert!(std::process::Command::new("cp")
                    .args(["-R", &engine])
                    .arg(&dest)
                    .status()
                    .unwrap()
                    .success());
            }
            let package = dest.join("node_modules/@bitkyc08/opencodex");
            scope_engine(&package, id, &root).unwrap();
            let once = fs::read(package.join("src/service.ts")).unwrap();
            scope_engine(&package, id, &root).unwrap();
            assert_eq!(once, fs::read(package.join("src/service.ts")).unwrap());
        }
        let output = std::process::Command::new(bun)
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../opencodex-engine/manager-runtime-smoke.ts"),
            )
            .arg(&root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
    #[test]
    fn rejects_symlink_instance_roots() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(temp.path(), temp.path().join("alias")).unwrap();
            assert!(validate_directory(&root.join("alias/child")).is_err());
        }
        assert!(validate_directory(&root.join("new/child")).is_ok());
    }
}
