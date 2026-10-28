use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportDownloadResult {
    file_name: String,
    path: String,
}

const SESSION_EXPORT_FOLDER: &str = "Codex Account Switcher";

fn downloads_directory() -> Result<PathBuf, String> {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .ok_or_else(|| "无法定位系统下载目录".to_string())
}

pub(crate) fn session_export_directory() -> Result<PathBuf, String> {
    Ok(downloads_directory()?.join(SESSION_EXPORT_FOLDER))
}

pub(crate) fn session_file_stem(value: &str) -> String {
    let name: String = value
        .chars()
        .take(60)
        .scan(0, |bytes, ch| {
            *bytes += ch.len_utf8();
            (*bytes <= 160).then_some(ch)
        })
        .map(|ch| {
            if ch.is_control() || "/\\:*?\"<>|".contains(ch) {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let name = name.trim().trim_matches('.').trim();
    if name.is_empty() {
        "session".into()
    } else {
        format!("session-{name}")
    }
}

#[tauri::command]
pub(crate) async fn download_codex_export(
    content: String,
    filename: String,
) -> Result<ExportDownloadResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let directory = downloads_directory()?;
        save_to_directory(&directory, &filename, &content)
    })
    .await
    .map_err(|error| format!("保存导出文件任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn save_codex_session_pdf(
    request: tauri::ipc::Request<'_>,
) -> Result<ExportDownloadResult, String> {
    let title = request
        .headers()
        .get("x-session-title")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "缺少 PDF 会话标题".to_string())?;
    let title = urlencoding::decode(title)
        .map_err(|_| "PDF 会话标题编码无效".to_string())?
        .into_owned();
    // 直接接收二进制，避免大 PDF 转成 JSON 数字数组时占用大量内存。
    let tauri::ipc::InvokeBody::Raw(content) = request.body() else {
        return Err("PDF 导出内容必须为二进制".to_string());
    };
    let content = content.clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_pdf_to_directory(&session_export_directory()?, &title, &content)
    })
    .await
    .map_err(|error| format!("保存 PDF 任务失败：{error}"))?
}

fn save_pdf_to_directory(
    directory: &Path,
    title: &str,
    content: &[u8],
) -> Result<ExportDownloadResult, String> {
    if !content.starts_with(b"%PDF-") {
        return Err("导出内容不是有效的 PDF".to_string());
    }
    save_bytes_to_directory(
        directory,
        &format!("{}.pdf", session_file_stem(title)),
        content,
    )
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
    save_bytes_to_directory(directory, filename, content.as_bytes())
}

fn save_bytes_to_directory(
    directory: &Path,
    filename: &str,
    content: &[u8],
) -> Result<ExportDownloadResult, String> {
    std::fs::create_dir_all(directory).map_err(|error| format!("创建下载目录失败：{error}"))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".codex-export-")
        .tempfile_in(directory)
        .map_err(|error| format!("创建导出文件失败：{error}"))?;
    temporary
        .write_all(content)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("保存导出文件失败：{error}"))?;

    // 原子保存且不覆盖已有导出；并发下载也不会相互覆盖。
    let (stem, extension) = filename.rsplit_once('.').unwrap();
    // Windows 的设备名即使带扩展名也不能用作普通文件名。
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
    let stem = &filename[..filename.len() - extension.len() - 1];
    for index in 0..1000 {
        let file_name = if index == 0 {
            filename.to_string()
        } else {
            format!("{stem} ({index}).{extension}")
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
    fn session_documents_share_app_folder_and_keep_markdown_assets_together() {
        let root = tempfile::tempdir().unwrap();
        let sessions = root.path().join("codex/sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let records = [
            serde_json::json!({"type":"session_meta","payload":{"id":"export-test"}}),
            serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1"}}),
            serde_json::json!({"type":"response_item","timestamp":"2026-10-27T12:00:00Z","payload":{
                "type":"message","role":"user","content":[
                    {"type":"input_text","text":"导出正文"},
                    {"type":"input_image","image_url":"data:image/png;base64,aGVsbG8="}
                ]
            }}),
        ];
        let source = records
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(sessions.join("session.jsonl"), format!("{source}\n")).unwrap();
        let directory = root.path().join("Downloads").join(SESSION_EXPORT_FOLDER);
        assert!(!directory.exists());
        let store = crate::session::SessionStore::new(root.path().join("codex"));
        let markdown = store
            .export_markdown("export-test", "测试会话", &directory)
            .unwrap();
        let markdown_path = Path::new(&markdown.path);
        assert_eq!(markdown_path.parent().unwrap().parent().unwrap(), directory);
        assert!(std::fs::read_to_string(markdown_path)
            .unwrap()
            .contains("导出正文"));
        assert_eq!(markdown.attachment_count, 1);
        let assets = markdown_path.parent().unwrap().join("assets");
        let attachment = std::fs::read_dir(assets)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(std::fs::read(attachment).unwrap(), b"hello");

        let pdf = b"%PDF-1.4\n\0\xff\n%%EOF";
        let result = save_pdf_to_directory(&directory, "测试会话", pdf).unwrap();
        assert_eq!(Path::new(&result.path).parent().unwrap(), directory);
        assert_eq!(std::fs::read(result.path).unwrap(), pdf);
        assert!(markdown_path.is_file());
    }

    #[test]
    fn concurrent_pdf_exports_create_folder_and_never_overwrite() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("Downloads").join(SESSION_EXPORT_FOLDER);
        let writers: Vec<_> = (0..8)
            .map(|index| {
                let directory = directory.clone();
                std::thread::spawn(move || {
                    let content = format!("%PDF-1.4\nexport-{index}\n%%EOF");
                    let result =
                        save_pdf_to_directory(&directory, "重复会话", content.as_bytes()).unwrap();
                    assert!(result.file_name.ends_with(".pdf"));
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
        assert_eq!(std::fs::read_dir(directory).unwrap().count(), 8);
    }

    #[test]
    fn pdf_titles_are_portable_and_cannot_escape_export_folder() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(SESSION_EXPORT_FOLDER);
        for title in ["../CON:会话\\test\0", "COM1", "..", "", &"🐈".repeat(100)] {
            let result = save_pdf_to_directory(&directory, title, b"%PDF-1.4\n%%EOF").unwrap();
            let path = Path::new(&result.path);
            assert_eq!(path.parent().unwrap(), directory);
            assert!(result.file_name.len() < 255);
            assert!(!result
                .file_name
                .chars()
                .any(|ch| ch.is_control() || "/\\:*?\"<>|".contains(ch)));
            assert!(path.is_file());
        }
    }

    #[test]
    fn invalid_pdf_or_unwritable_destination_never_reports_success() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(SESSION_EXPORT_FOLDER);
        assert!(save_pdf_to_directory(&directory, "test", b"not a PDF").is_err());
        assert!(!directory.exists());
        std::fs::write(&directory, "existing").unwrap();
        assert!(save_pdf_to_directory(&directory, "test", b"%PDF-1.4\n%%EOF").is_err());
        assert_eq!(std::fs::read_to_string(directory).unwrap(), "existing");
    }

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
