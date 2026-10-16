use super::{
    emit_progress, install_error_path, parse_version, safe_file_name, CLEANUP_MARKER_FILE,
};
use crate::system_command::new as command;
use semver::Version;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tauri::AppHandle;

const RESTART_STAGE_ARG: &str = "--codex-switcher-update-stage";
const INSTALL_SCRIPT: &str = include_str!("macos-install.sh");

fn run(cmd: &mut Command, action: &str) -> Result<(), String> {
    let output = cmd.output().map_err(|error| format!("{action}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn bundle_from_executable(executable: &Path) -> Result<PathBuf, String> {
    let macos = executable.parent().ok_or("无法确定应用安装位置")?;
    let contents = macos.parent().ok_or("无法确定应用安装位置")?;
    let bundle = contents.parent().ok_or("无法确定应用安装位置")?;
    if macos.file_name().is_none_or(|name| name != "MacOS")
        || contents.file_name().is_none_or(|name| name != "Contents")
        || bundle.extension().is_none_or(|ext| ext != "app")
    {
        return Err("应用内更新需要运行已安装的 .app 应用".to_string());
    }
    if bundle.starts_with("/Volumes")
        || bundle
            .components()
            .any(|part| part.as_os_str() == "AppTranslocation")
    {
        return Err("请先将 Codex Switcher 移到“应用程序”文件夹，再使用应用内更新".to_string());
    }
    Ok(bundle.to_path_buf())
}

fn installed_bundle() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|error| format!("读取当前应用位置失败: {error}"))?;
    bundle_from_executable(&executable)
}

fn plist_value(bundle: &Path, key: &str) -> Result<String, String> {
    let output = command("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print :{key}")])
        .arg(bundle.join("Contents/Info.plist"))
        .output()
        .map_err(|error| format!("读取更新应用信息失败: {error}"))?;
    if !output.status.success() {
        return Err(format!("更新应用缺少 {key}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn validate_bundle(bundle: &Path, identifier: &str, version: &Version) -> Result<(), String> {
    if !fs::symlink_metadata(bundle).is_ok_and(|metadata| metadata.is_dir()) {
        return Err("更新安装包中没有有效的应用".to_string());
    }
    if plist_value(bundle, "CFBundleIdentifier")? != identifier {
        return Err("更新应用标识与当前应用不一致".to_string());
    }
    if parse_version(&plist_value(bundle, "CFBundleShortVersionString")?)? != *version {
        return Err("更新应用版本与下载版本不一致，请重新下载".to_string());
    }
    let executable_name = safe_file_name(&plist_value(bundle, "CFBundleExecutable")?)?;
    let executable = bundle.join("Contents/MacOS").join(executable_name);
    let canonical_executable = executable
        .canonicalize()
        .map_err(|error| format!("更新应用可执行文件无效: {error}"))?;
    if !canonical_executable.starts_with(bundle.canonicalize().map_err(|e| e.to_string())?) {
        return Err("更新应用可执行文件包含非法路径".to_string());
    }
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => return Err("当前 macOS 架构暂不支持应用内更新".to_string()),
    };
    run(
        command("/usr/bin/lipo")
            .args(["-verify_arch", arch])
            .arg(&executable),
        "更新应用架构不兼容",
    )?;
    run(
        command("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(bundle),
        "更新应用签名校验失败",
    )
}

struct MountedImage(tempfile::TempDir);

impl Drop for MountedImage {
    fn drop(&mut self) {
        let _ = command("/usr/bin/hdiutil")
            .arg("detach")
            .arg(self.0.path())
            .output();
    }
}

fn stage_bundle_from_dmg(
    installer: &Path,
    destination: &Path,
    identifier: &str,
    version: &Version,
) -> Result<(), String> {
    let mount = tempfile::Builder::new()
        .prefix("codex-switcher-mount-")
        .tempdir()
        .map_err(|error| format!("创建安装包挂载目录失败: {error}"))?;
    run(
        command("/usr/bin/hdiutil")
            .args(["attach", "-readonly", "-nobrowse", "-quiet", "-mountpoint"])
            .arg(mount.path())
            .arg(installer),
        "挂载更新安装包失败",
    )?;
    let mounted = MountedImage(mount);
    let source = mounted.0.path().join("Codex Switcher.app");
    validate_bundle(&source, identifier, version)?;
    run(
        command("/usr/bin/ditto").arg(&source).arg(destination),
        "准备新版应用失败",
    )?;
    validate_bundle(destination, identifier, version)
}

pub(super) fn prepare_install_and_restart(
    app: &AppHandle,
    installer: &Path,
    updates: &Path,
) -> Result<(), String> {
    if installer.extension().is_none_or(|ext| ext != "dmg") {
        return Err("macOS 应用内更新需要 DMG 安装包".to_string());
    }
    let version = installer
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .ok_or("无法确定更新版本")?;
    let version = parse_version(version)?;
    if version <= app.package_info().version {
        return Err("更新版本必须高于当前版本，请重新检查更新".to_string());
    }
    let target = installed_bundle()?;
    if plist_value(&target, "CFBundleIdentifier")? != app.config().identifier {
        return Err("当前应用标识不匹配，无法更新".to_string());
    }
    let parent = target.parent().ok_or("无法确定应用安装目录")?;
    // Staging beside the app proves the directory is writable and keeps renames on one volume.
    let stage = tempfile::Builder::new()
        .prefix(".codex-switcher-update-")
        .tempdir_in(parent)
        .map_err(|error| {
            format!("应用安装目录不可写，请将应用移到可写的“应用程序”文件夹后重试: {error}")
        })?;
    emit_progress(
        app,
        "installing",
        &version.to_string(),
        "",
        0,
        None,
        Some("正在校验并准备更新，完成后将自动重启"),
    );
    stage_bundle_from_dmg(
        installer,
        &stage.path().join("new.app"),
        &app.config().identifier,
        &version,
    )?;
    fs::write(stage.path().join("expected-version"), version.to_string())
        .map_err(|e| e.to_string())?;
    fs::write(
        stage.path().join("expected-target"),
        target.as_os_str().as_encoded_bytes(),
    )
    .map_err(|e| e.to_string())?;
    let script = stage.path().join("install.sh");
    fs::write(&script, INSTALL_SCRIPT).map_err(|error| format!("准备更新助手失败: {error}"))?;
    let error_file = install_error_path()?;
    let mut helper = command("/bin/sh")
        .arg(&script)
        .arg(std::process::id().to_string())
        .arg(&target)
        .arg(stage.path())
        .arg(error_file)
        .arg(updates.join(CLEANUP_MARKER_FILE))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("启动更新助手失败: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !stage.path().join("ready").is_file() {
        if helper.try_wait().map_err(|e| e.to_string())?.is_some() || Instant::now() >= deadline {
            let _ = helper.kill();
            let _ = helper.wait();
            return Err("更新助手启动失败，当前应用未退出，请重试".to_string());
        }
        thread::sleep(Duration::from_millis(50));
    }
    // The helper owns these files from now on, including the rollback copy.
    let _ = stage.keep();
    Ok(())
}

pub(super) fn confirm_update_restart(app: &AppHandle) -> Result<(), String> {
    let args = std::env::args_os().collect::<Vec<_>>();
    let Some(index) = args.iter().position(|arg| arg == RESTART_STAGE_ARG) else {
        return Ok(());
    };
    let stage = args
        .get(index + 1)
        .map(PathBuf::from)
        .ok_or("更新启动参数无效")?;
    let target = installed_bundle()?;
    confirm_restart_at(&stage, &target, &app.package_info().version)
}

fn confirm_restart_at(stage: &Path, target: &Path, version: &Version) -> Result<(), String> {
    let stage = stage.canonicalize().map_err(|e| e.to_string())?;
    if stage.parent() != target.parent()
        || !stage
            .file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.starts_with(".codex-switcher-update-"))
        || fs::read(stage.join("expected-target")).map_err(|e| e.to_string())?
            != target.as_os_str().as_encoded_bytes()
        || fs::read_to_string(stage.join("expected-version")).map_err(|e| e.to_string())?
            != version.to_string()
    {
        return Err("更新启动确认信息不匹配".to_string());
    }
    fs::write(stage.join("launch-confirmed"), b"ok").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_an_installed_bundle_outside_disk_images_and_translocation() {
        assert_eq!(
            bundle_from_executable(Path::new(
                "/Applications/Codex Switcher.app/Contents/MacOS/codex-switcher"
            ))
            .unwrap(),
            Path::new("/Applications/Codex Switcher.app")
        );
        for path in [
            "/Volumes/Update/Codex Switcher.app/Contents/MacOS/codex-switcher",
            "/private/var/folders/test/AppTranslocation/id/d/Codex Switcher.app/Contents/MacOS/codex-switcher",
            "/tmp/release/codex-switcher",
        ] {
            assert!(bundle_from_executable(Path::new(path)).is_err(), "{path}");
        }
    }

    #[test]
    fn validates_version_identity_architecture_and_signature_before_replacing() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("Candidate.app");
        fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        fs::copy(
            std::env::current_exe().unwrap(),
            bundle.join("Contents/MacOS/codex-switcher"),
        )
        .unwrap();
        fs::write(
            bundle.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.codex.switcher</string>
<key>CFBundleShortVersionString</key><string>1.2.3</string>
<key>CFBundleVersion</key><string>1.2.3</string>
<key>CFBundleExecutable</key><string>codex-switcher</string>
</dict></plist>"#,
        )
        .unwrap();
        run(
            command("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(&bundle),
            "sign fixture",
        )
        .unwrap();
        validate_bundle(&bundle, "com.codex.switcher", &Version::new(1, 2, 3)).unwrap();
        assert!(validate_bundle(&bundle, "wrong.id", &Version::new(1, 2, 3))
            .unwrap_err()
            .contains("标识"));
        assert!(
            validate_bundle(&bundle, "com.codex.switcher", &Version::new(1, 2, 4))
                .unwrap_err()
                .contains("版本")
        );
        fs::write(bundle.join("Contents/unsealed-resource"), b"tampered").unwrap();
        assert!(
            validate_bundle(&bundle, "com.codex.switcher", &Version::new(1, 2, 3))
                .unwrap_err()
                .contains("签名")
        );
        let link = dir.path().join("Symlink.app");
        std::os::unix::fs::symlink(&bundle, &link).unwrap();
        assert!(validate_bundle(&link, "com.codex.switcher", &Version::new(1, 2, 3)).is_err());
    }

    #[test]
    fn restart_acknowledgement_requires_matching_app_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let target = root.join("Codex Switcher.app");
        let stage = root.join(".codex-switcher-update-test");
        fs::create_dir(&stage).unwrap();
        fs::write(
            stage.join("expected-target"),
            target.as_os_str().as_encoded_bytes(),
        )
        .unwrap();
        fs::write(stage.join("expected-version"), "1.2.3").unwrap();
        assert!(confirm_restart_at(&stage, &target, &Version::new(1, 2, 2)).is_err());
        assert!(
            confirm_restart_at(&stage, &root.join("Other.app"), &Version::new(1, 2, 3)).is_err()
        );
        assert!(!stage.join("launch-confirmed").exists());
        confirm_restart_at(&stage, &target, &Version::new(1, 2, 3)).unwrap();
        assert!(stage.join("launch-confirmed").exists());
    }

    struct InstallFixture {
        _root: tempfile::TempDir,
        target: PathBuf,
        work: PathBuf,
        error: PathBuf,
        marker: PathBuf,
        launches: PathBuf,
    }

    impl InstallFixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            // Spaces, shell substitutions and quotes must remain literal argv data.
            let target = root.path().join("App 'quoted' $(touch injected).app");
            let work = root.path().join(".codex-switcher-update-test");
            fs::create_dir(&target).unwrap();
            fs::write(target.join("version"), b"old").unwrap();
            fs::create_dir_all(work.join("new.app")).unwrap();
            fs::write(work.join("new.app/version"), b"new").unwrap();
            let error = root.path().join("install-error");
            let marker = root.path().join("cleanup-marker");
            let launches = root.path().join("launches");
            Self {
                _root: root,
                target,
                work,
                error,
                marker,
                launches,
            }
        }

        fn start(&self, pid: u32) -> std::process::Child {
            let launcher = r#"
launch() {
    /usr/bin/printf '%s\n' "$*" >> "$TEST_LAUNCH_LOG"
    if [ "$#" -gt 2 ]; then
        if [ -f "$work/fail-launch" ]; then return 1; fi
        if [ ! -f "$work/no-ack" ]; then : > "$work/launch-confirmed"; fi
    fi
    return 0
}
"#;
            let script = format!(
                "{launcher}\n{}",
                INSTALL_SCRIPT
                    .replace("/usr/bin/open", "launch")
                    .replace("/usr/bin/pgrep -x codex-switcher", "/usr/bin/false")
            );
            // Bound failure tests without changing the production timeout.
            let script = script
                .replace("-lt 150", "-lt 2")
                .replace("-ge 600", "-ge 5");
            let path = self.work.join("test-install.sh");
            fs::write(&path, script).unwrap();
            command("/bin/sh")
                .arg(path)
                .arg(pid.to_string())
                .arg(&self.target)
                .arg(&self.work)
                .arg(&self.error)
                .arg(&self.marker)
                .env("TEST_LAUNCH_LOG", &self.launches)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap()
        }

        fn assert_old(&self) {
            assert_eq!(fs::read(self.target.join("version")).unwrap(), b"old");
            assert!(!self.marker.exists());
            assert!(self.error.exists());
        }
    }

    #[test]
    fn installer_waits_for_exit_then_replaces_relaunches_and_cleans_up() {
        let fixture = InstallFixture::new();
        let mut old_process = command("/bin/sleep").arg("10").spawn().unwrap();
        let mut helper = fixture.start(old_process.id());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !fixture.work.join("ready").exists() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(fs::read(fixture.target.join("version")).unwrap(), b"old");
        old_process.kill().unwrap();
        old_process.wait().unwrap();
        assert!(helper.wait().unwrap().success());
        assert_eq!(fs::read(fixture.target.join("version")).unwrap(), b"new");
        assert!(fixture.marker.exists());
        assert!(!fixture.work.exists());
        assert!(!fixture.error.exists());
        assert!(!fixture._root.path().join("injected").exists());
        assert!(fs::read_to_string(&fixture.launches)
            .unwrap()
            .contains("--codex-switcher-update-stage"));
    }

    #[test]
    fn installer_rolls_back_replacement_or_launch_failures() {
        for failure in ["missing-staged", "fail-launch", "no-ack"] {
            let fixture = InstallFixture::new();
            if failure == "missing-staged" {
                fs::remove_dir_all(fixture.work.join("new.app")).unwrap();
            } else {
                fs::write(fixture.work.join(failure), b"yes").unwrap();
            }
            assert!(
                !fixture.start(99_999_999).wait().unwrap().success(),
                "{failure}"
            );
            fixture.assert_old();
            assert!(fixture.work.join("install.log").exists());
            assert!(fs::read_to_string(&fixture.launches)
                .unwrap()
                .contains("App 'quoted' $(touch injected).app"));
        }
    }

    #[test]
    fn installer_cancels_without_touching_app_if_old_process_cannot_exit() {
        let fixture = InstallFixture::new();
        assert!(!fixture.start(std::process::id()).wait().unwrap().success());
        fixture.assert_old();
        assert!(fixture.work.join("new.app").exists());
        assert!(!fixture.work.join("previous.app").exists());
    }

    #[test]
    #[ignore = "requires local macOS Launch Services and Xcode command line tools"]
    fn real_dmg_install_restarts_an_isolated_background_app() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let source_root = root.join("image");
        let source_app = source_root.join("Codex Switcher.app");
        fs::create_dir_all(source_app.join("Contents/MacOS")).unwrap();
        let fixture_c = root.join("fixture.c");
        fs::write(
            &fixture_c,
            r#"
#include <stdio.h>
#include <string.h>
int main(int argc, char **argv) {
    for (int i = 1; i + 1 < argc; i++) {
        if (strcmp(argv[i], "--codex-switcher-update-stage") == 0) {
            char path[4096];
            snprintf(path, sizeof(path), "%s/launch-confirmed", argv[i + 1]);
            FILE *file = fopen(path, "w");
            if (!file) return 2;
            fputs("ok", file);
            fclose(file);
            return 0;
        }
    }
    return 0;
}
"#,
        )
        .unwrap();
        let executable = source_app.join("Contents/MacOS/update-fixture");
        run(
            command("/usr/bin/clang")
                .arg(&fixture_c)
                .arg("-o")
                .arg(&executable),
            "compile isolated fixture",
        )
        .unwrap();
        let identifier = format!("com.codex.switcher.update-test-{}", rand::random::<u64>());
        fs::write(
            source_app.join("Contents/Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{identifier}</string>
<key>CFBundleShortVersionString</key><string>1.2.3</string>
<key>CFBundleVersion</key><string>1.2.3</string>
<key>CFBundleExecutable</key><string>update-fixture</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSBackgroundOnly</key><true/>
</dict></plist>"#
            ),
        )
        .unwrap();
        run(
            command("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(&source_app),
            "sign isolated fixture",
        )
        .unwrap();
        let installer = root.join("fixture.dmg");
        run(
            command("/usr/bin/hdiutil")
                .args(["create", "-quiet", "-format", "UDZO", "-srcfolder"])
                .arg(&source_root)
                .arg(&installer),
            "create isolated dmg",
        )
        .unwrap();
        let target = root.join("Update Test.app");
        run(
            command("/usr/bin/ditto").arg(&source_app).arg(&target),
            "copy old fixture",
        )
        .unwrap();
        fs::write(target.join("old-version-marker"), b"old").unwrap();
        let work = root.join(".codex-switcher-update-fixture");
        fs::create_dir(&work).unwrap();
        stage_bundle_from_dmg(
            &installer,
            &work.join("new.app"),
            &identifier,
            &Version::new(1, 2, 3),
        )
        .unwrap();
        let script = work.join("install.sh");
        fs::write(&script, INSTALL_SCRIPT).unwrap();
        let error = root.join("error");
        let marker = root.join("cleanup");
        let status = command("/bin/sh")
            .arg(script)
            .arg("99999999")
            .arg(&target)
            .arg(&work)
            .arg(&error)
            .arg(&marker)
            .status()
            .unwrap();
        let failure = fs::read_to_string(&error).unwrap_or_default();
        assert!(status.success(), "{failure}");
        assert!(!target.join("old-version-marker").exists());
        assert!(!work.exists());
        assert!(marker.exists());
        validate_bundle(&target, &identifier, &Version::new(1, 2, 3)).unwrap();
    }
}
