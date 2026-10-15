use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportDownloadResult {
    file_name: String,
    path: String,
}

#[tauri::command]
pub(crate) async fn download_codex_export(
    content: String,
    filename: String,
) -> Result<ExportDownloadResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let directory = dirs::download_dir()
            .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
            .ok_or_else(|| "无法定位系统下载目录".to_string())?;
        save_to_directory(&directory, &filename, &content)
    })
    .await
    .map_err(|error| format!("保存导出文件任务失败：{error}"))?
}

fn save_to_directory(
    directory: &Path,
    filename: &str,
    content: &str,
) -> Result<ExportDownloadResult, String> {
    if filename.is_empty()
        || filename
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
        || !filename.to_lowercase().ends_with(".json")
    {
        return Err("导出文件名无效".to_string());
    }
    serde_json::from_str::<serde_json::Value>(content)
        .map_err(|_| "导出内容不是有效的 JSON".to_string())?;
    std::fs::create_dir_all(directory).map_err(|error| format!("创建下载目录失败：{error}"))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".codex-export-")
        .tempfile_in(&directory)
        .map_err(|error| format!("创建导出文件失败：{error}"))?;
    temporary
        .write_all(content.as_bytes())
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("保存导出文件失败：{error}"))?;

    // 原子保存且不覆盖已有导出；并发下载也不会相互覆盖。
    let stem = &filename[..filename.len() - 5];
    // Windows 的设备名即使带 .json 扩展名也不能用作普通文件名。
    let base = stem
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let numbered_device = base
        .strip_prefix("COM")
        .or_else(|| base.strip_prefix("LPT"))
        .is_some_and(|number| number.len() == 1 && matches!(number.as_bytes()[0], b'1'..=b'9'));
    let filename = if numbered_device || matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        format!("codex-{filename}")
    } else {
        filename.to_string()
    };
    let stem = &filename[..filename.len() - 5];
    for index in 0..1000 {
        let file_name = if index == 0 {
            filename.to_string()
        } else {
            format!("{stem} ({index}).json")
        };
        let path: PathBuf = directory.join(&file_name);
        match temporary.persist_noclobber(&path) {
            Ok(_) => {
                return Ok(ExportDownloadResult {
                    file_name,
                    path: path.to_string_lossy().into_owned(),
                })
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                temporary = error.file;
            }
            Err(error) => return Err(format!("保存导出文件失败：{}", error.error)),
        }
    }
    Err("同名导出文件过多，请整理下载目录后重试".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_complete_json_and_returns_actual_unicode_path() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("下载 文件");
        let content = "{\"账号\":\"fixture@example.com\"}";
        let result = save_to_directory(&directory, "账号.json", content).unwrap();
        assert_eq!(result.file_name, "账号.json");
        assert!(Path::new(&result.path).is_absolute());
        assert_eq!(std::fs::read_to_string(&result.path).unwrap(), content);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&result.path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn concurrent_exports_never_overwrite_previous_downloads() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("export.json"), "original").unwrap();
        let writers: Vec<_> = (0..8)
            .map(|index| {
                let directory = directory.path().to_path_buf();
                std::thread::spawn(move || {
                    let content = format!("{{\"index\":{index}}}");
                    let result = save_to_directory(&directory, "export.json", &content).unwrap();
                    assert_eq!(std::fs::read_to_string(&result.path).unwrap(), content);
                    result.path
                })
            })
            .collect();
        let paths: std::collections::HashSet<_> = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect();
        assert_eq!(paths.len(), 8);
        assert_eq!(
            std::fs::read_to_string(directory.path().join("export.json")).unwrap(),
            "original"
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 9);
    }

    #[test]
    fn rejects_paths_and_invalid_json_without_creating_files() {
        let directory = tempfile::tempdir().unwrap();
        for filename in [
            "../export.json",
            "sub/export.json",
            "..\\export.json",
            "C:\\export.json",
            "a:b.json",
            "export.txt",
        ] {
            assert!(save_to_directory(directory.path(), filename, "{}").is_err());
        }
        assert!(save_to_directory(directory.path(), "export.json", "not json").is_err());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn device_names_are_portable_between_windows_and_macos() {
        let directory = tempfile::tempdir().unwrap();
        for name in ["CON.json", "nul.json", "COM1.json", "LPT9.json"] {
            let result = save_to_directory(directory.path(), name, "{}").unwrap();
            assert_eq!(result.file_name, format!("codex-{name}"));
            assert_eq!(std::fs::read_to_string(result.path).unwrap(), "{}");
        }
    }

    #[test]
    fn failed_write_returns_error_without_a_success_result() {
        let directory = tempfile::tempdir().unwrap();
        let blocked = directory.path().join("not-a-directory");
        std::fs::write(&blocked, "existing").unwrap();
        assert!(save_to_directory(&blocked, "export.json", "{}").is_err());
        assert_eq!(std::fs::read_to_string(blocked).unwrap(), "existing");
    }
}
