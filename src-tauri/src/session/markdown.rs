use super::*;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownExportResult {
    pub file_name: String,
    pub path: String,
    pub attachment_count: usize,
    pub missing_attachment_count: usize,
}

impl SessionStore {
    pub fn export_markdown(
        &self,
        session_id: &str,
        title: &str,
        directory: &Path,
    ) -> Result<MarkdownExportResult, String> {
        let session_id = normalize_required_session_id(session_id)?;
        let source = self.find_session_read_path(&session_id)?;
        fs::create_dir_all(directory).map_err(|e| format!("创建下载目录失败: {e}"))?;
        // Snapshot once: offsets for embedded images must refer to the same content as the messages.
        let snapshot = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        fs::copy(&source, snapshot.path()).map_err(|e| format!("读取会话失败: {e}"))?;
        export_snapshot(snapshot.path(), title, directory)
    }
}

fn export_snapshot(
    source: &Path,
    title: &str,
    directory: &Path,
) -> Result<MarkdownExportResult, String> {
    // A unique directory keeps repeated/concurrent exports from overwriting previous documents.
    // It is automatically cleaned up if writing the document fails.
    let staging = tempfile::Builder::new()
        .prefix(&format!("codex-session-{}-", safe_name(title)))
        .tempdir_in(directory)
        .map_err(|e| format!("创建导出目录失败: {e}"))?;
    let file_name = format!("{}.md", safe_name(title));
    let path = staging.path().join(&file_name);
    let file = fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut output = BufWriter::new(file);
    let mut assets = Assets {
        directory: staging.path().join("assets"),
        source,
        saved: HashMap::new(),
        recovered_paths: HashMap::new(),
        missing: HashSet::new(),
    };
    writeln!(output, "# {}\n", label(title)).map_err(|e| e.to_string())?;
    let mut cursor = 0;
    let mut sequence = 0;
    loop {
        let (turns, next) = read_session_turn_page(source, cursor, 50)?;
        for turn in turns {
            sequence += 1;
            writeln!(output, "## 对话轮次 {sequence}\n").map_err(|e| e.to_string())?;
            for message in turn.public.messages {
                let role = if message.role == "user" {
                    "用户"
                } else {
                    "助手"
                };
                writeln!(output, "### {role} · {}\n", label(&message.timestamp))
                    .map_err(|e| e.to_string())?;
                // Resolve attachments first so recovered inline images also fix body links.
                let attachments = message
                    .attachments
                    .iter()
                    .map(|attachment| assets.attachment(attachment))
                    .collect::<Result<Vec<_>, _>>()?;
                let text = assets.rewrite_links(&message.text)?;
                writeln!(output, "{text}\n").map_err(|e| e.to_string())?;
                for (attachment, target) in message.attachments.iter().zip(attachments) {
                    let name = label(&attachment.name);
                    if let Some(target) = target {
                        let image = if attachment.kind == "image" { "!" } else { "" };
                        writeln!(output, "{image}[{name}](<{target}>)\n")
                            .map_err(|e| e.to_string())?;
                    } else {
                        writeln!(
                            output,
                            "> 附件未能导出：{name}{}\n",
                            attachment
                                .source_path
                                .as_ref()
                                .map(|path| format!("（{}）", label(path)))
                                .unwrap_or_default()
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
            }
        }
        match next {
            Some(next) if next > cursor => cursor = next,
            Some(_) => return Err("会话读取游标未前进，请重试".to_string()),
            None => break,
        }
    }
    if !assets.missing.is_empty() {
        writeln!(
            output,
            "---\n\n> 有 {} 个附件无法读取，原始引用已保留，请检查源文件。",
            assets.missing.len()
        )
        .map_err(|e| e.to_string())?;
    }
    output
        .flush()
        .map_err(|e| format!("保存 Markdown 失败: {e}"))?;
    output.get_ref().sync_all().map_err(|e| e.to_string())?;
    let result = MarkdownExportResult {
        file_name,
        path: path.to_string_lossy().into_owned(),
        attachment_count: assets.saved.len(),
        missing_attachment_count: assets.missing.len(),
    };
    let _ = staging.keep();
    Ok(result)
}

fn safe_name(value: &str) -> String {
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

fn label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(['\r', '\n'], " ")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

struct Assets<'a> {
    directory: PathBuf,
    source: &'a Path,
    saved: HashMap<String, String>,
    recovered_paths: HashMap<String, String>,
    missing: HashSet<String>,
}

impl Assets<'_> {
    fn attachment(
        &mut self,
        attachment: &CodexSessionAttachment,
    ) -> Result<Option<String>, String> {
        if let Some(path) = &attachment.source_path {
            if let Some(target) = self.local_file(path)? {
                return Ok(Some(target));
            }
        }
        if attachment.inline {
            let key = format!("inline:{}", attachment.id);
            if let Ok(asset) = read_session_asset(self.source, &attachment.id) {
                let target = self.data_url(&asset.data_url)?;
                if let Some(path) = &attachment.source_path {
                    self.missing.remove(path);
                    if let Some(target) = &target {
                        self.recovered_paths.insert(path.clone(), target.clone());
                    }
                }
                return Ok(target);
            }
            if attachment.source_path.is_none() {
                self.missing.insert(key);
            }
        } else if attachment.source_path.is_none() {
            self.missing.insert(attachment.id.clone());
        }
        Ok(None)
    }

    fn local_file(&mut self, source: &str) -> Result<Option<String>, String> {
        if let Some(target) = self
            .recovered_paths
            .get(source)
            .or_else(|| self.saved.get(source))
        {
            return Ok(Some(target.clone()));
        }
        if self.missing.contains(source) {
            return Ok(None);
        }
        let path = Path::new(source);
        if !path.is_file() {
            self.missing.insert(source.to_string());
            return Ok(None);
        }
        fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        // Prefix prevents collisions and reserved Windows device names; keep a useful extension.
        let name: String = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .map(|ch| {
                if ch.is_control() || "/\\:*?\"<>|".contains(ch) {
                    '_'
                } else {
                    ch
                }
            })
            .collect();
        let name = format!("{}-{}", self.saved.len() + 1, truncate_filename(&name));
        match fs::copy(path, self.directory.join(&name)) {
            Ok(_) => (),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                self.missing.insert(source.to_string());
                return Ok(None);
            }
            Err(e) => return Err(format!("保存附件失败: {e}")),
        }
        let target = format!("assets/{}", urlencoding::encode(&name));
        self.saved.insert(source.to_string(), target.clone());
        Ok(Some(target))
    }

    fn data_url(&mut self, source: &str) -> Result<Option<String>, String> {
        let key = format!("data:{}", hex_sha256(source.as_bytes()));
        if let Some(target) = self.saved.get(&key) {
            return Ok(Some(target.clone()));
        }
        let Some((header, encoded)) = source.split_once(',') else {
            self.missing.insert(key);
            return Ok(None);
        };
        if !header.ends_with(";base64") {
            self.missing.insert(key);
            return Ok(None);
        }
        let Ok(bytes) = general_purpose::STANDARD.decode(encoded) else {
            self.missing.insert(key);
            return Ok(None);
        };
        let extension = match header
            .strip_prefix("data:")
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
        {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/svg+xml" => "svg",
            "image/bmp" => "bmp",
            "audio/mpeg" => "mp3",
            "audio/wav" => "wav",
            "application/pdf" => "pdf",
            _ => "bin",
        };
        fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        let name = format!("{}-attachment.{extension}", self.saved.len() + 1);
        fs::write(self.directory.join(&name), bytes).map_err(|e| format!("保存附件失败: {e}"))?;
        let target = format!("assets/{name}");
        self.saved.insert(key, target.clone());
        Ok(Some(target))
    }

    fn link(&mut self, href: &str) -> Result<Option<String>, String> {
        if href.starts_with("data:") {
            return self.data_url(href);
        }
        let path = if href.starts_with("file://") {
            url::Url::parse(href)
                .ok()
                .and_then(|url| url.to_file_path().ok())
                .map(|path| path.to_string_lossy().into_owned())
        } else {
            let decoded = urlencoding::decode(href)
                .unwrap_or_else(|_| href.into())
                .into_owned();
            Path::new(&decoded).is_absolute().then_some(decoded)
        };
        match path {
            Some(path) => self.local_file(&path),
            None => Ok(None),
        }
    }

    fn rewrite_links(&mut self, text: &str) -> Result<String, String> {
        // Keep code examples intact. Support normal/angle-bracket destinations and reference links.
        static LINKS: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
            r#"(?m)!?\[(?:\\.|[^\]\\])*\]\(\s*(?:<(?P<angle>[^>\n]+)>|(?P<plain>(?:[^\s()]+|\([^()]*\))+))(?:\s+\"[^\"\n]*\")?\s*\)|^ {0,3}\[[^\]\n]+\]:\s*(?:<(?P<refangle>[^>\n]+)>|(?P<reference>\S+))"#
        ).expect("valid Markdown link pattern")
        });
        let code_ranges = code_ranges(text);
        let mut code_index = 0;
        let mut output = String::new();
        let mut cursor = 0;
        for captures in LINKS.captures_iter(text) {
            let start = captures.get(0).expect("full match").start();
            while code_index < code_ranges.len() && code_ranges[code_index].end <= start {
                code_index += 1;
            }
            if code_ranges
                .get(code_index)
                .is_some_and(|range| range.contains(&start))
            {
                continue;
            }
            let Some(destination) = ["angle", "plain", "refangle", "reference"]
                .iter()
                .find_map(|name| captures.name(name))
            else {
                continue;
            };
            if let Some(target) = self.link(destination.as_str())? {
                output.push_str(&text[cursor..destination.start()]);
                output.push_str(&target);
                cursor = destination.end();
            }
        }
        output.push_str(&text[cursor..]);
        Ok(output)
    }
}

// Identify fenced/indented blocks and inline code before rewriting destinations.
fn code_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut fence: Option<(u8, usize, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let marker = trimmed.as_bytes().first().copied().unwrap_or_default();
        let width = trimmed.bytes().take_while(|byte| *byte == marker).count();
        if let Some((opening, count, start)) = fence {
            if indent <= 3
                && marker == opening
                && width >= count
                && trimmed[width..].trim().is_empty()
            {
                ranges.push(start..offset + line.len());
                fence = None;
            }
        } else if indent <= 3 && matches!(marker, b'`' | b'~') && width >= 3 {
            fence = Some((marker, width, offset));
        } else if indent >= 4 || line.starts_with('\t') {
            ranges.push(offset..offset + line.len());
        }
        offset += line.len();
    }
    if let Some((_, _, start)) = fence {
        ranges.push(start..text.len());
    }
    let blocks = ranges.clone();
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut block = 0;
    while index < bytes.len() {
        while block < blocks.len() && blocks[block].end <= index {
            block += 1;
        }
        if let Some(range) = blocks.get(block).filter(|range| range.contains(&index)) {
            index = range.end;
            continue;
        }
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let width = bytes[index..]
            .iter()
            .take_while(|byte| **byte == b'`')
            .count();
        let mut end = index + width;
        let limit = blocks
            .get(block)
            .map(|range| range.start)
            .unwrap_or(bytes.len());
        let mut closing = None;
        while end < limit {
            if bytes[end] != b'`' {
                end += 1;
                continue;
            }
            let run = bytes[end..limit]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            if run == width {
                closing = Some(end + run);
                break;
            }
            end += run;
        }
        if let Some(end) = closing {
            ranges.push(index..end);
            index = end;
        } else {
            index += width;
        }
    }
    ranges.sort_by_key(|range| range.start);
    ranges
}

fn truncate_filename(name: &str) -> String {
    let name = name.trim_end_matches(['.', ' ']);
    if name.len() <= 160 {
        return name.to_string();
    }
    let extension = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    format!(
        "{}.{}",
        name.chars().take(35).collect::<String>(),
        extension.chars().take(10).collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write_session(path: &Path, messages: Vec<Value>) {
        let mut file = fs::File::create(path).unwrap();
        writeln!(
            file,
            "{}",
            json!({"type":"event_msg", "payload":{"type":"task_started","turn_id":"fixture-turn"}})
        )
        .unwrap();
        for message in messages {
            writeln!(file, "{message}").unwrap();
        }
    }

    fn message(id: usize, role: &str, content: Vec<Value>) -> Value {
        json!({"type":"response_item", "timestamp":"2026-10-22T12:00:00Z",
            "payload":{"type":"message", "role":role,"id":format!("msg-{id}"), "content":content}})
    }

    #[test]
    fn exports_all_pages_and_preserves_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("session.jsonl");
        let mut records = Vec::new();
        for id in 0..105 {
            records.push(json!({"type":"event_msg", "payload":{"type":"task_started","turn_id":format!("turn-{id}")}}));
            records.push(message(id, "user", vec![json!({"type":"input_text","text":format!("**message-{id}**\n\n```rust\nlet x = 1;\n```")})]));
            records.push(json!({"type":"event_msg", "payload":{"type":"task_complete","turn_id":format!("turn-{id}")}}));
        }
        write_session(&source, records);
        let first = export_snapshot(&source, "../CON:会话", dir.path()).unwrap();
        let second = export_snapshot(&source, "../CON:会话", dir.path()).unwrap();
        assert_ne!(first.path, second.path);
        let text = fs::read_to_string(&first.path).unwrap();
        assert_eq!(text.matches("## 对话轮次 ").count(), 105);
        assert!(text.contains("**message-104**\n\n```rust\nlet x = 1;\n```"));
        assert!(Path::new(&first.path).starts_with(dir.path()));
    }

    #[test]
    fn bundles_binary_attachments_and_embedded_images_with_relative_links() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("session.jsonl");
        let attachment = dir.path().join("报告 [final] (1).pdf");
        fs::write(&attachment, b"%PDF-test\0binary").unwrap();
        let body = format!(
            "# Files mentioned by the user:\n\n## 报告: {}\n\n## My request:\n测试导出",
            attachment.display()
        );
        write_session(
            &source,
            vec![message(
                1,
                "user",
                vec![
                    json!({"type":"input_text","text":body}),
                    json!({"type":"input_image","image_url":"data:image/png;base64,aGVsbG8="}),
                    json!({"type":"input_image","image_url":"data:image/png;base64,aGVsbG8="}),
                ],
            )],
        );
        let result = export_snapshot(&source, "附件", dir.path()).unwrap();
        assert_eq!(result.attachment_count, 2);
        assert_eq!(result.missing_attachment_count, 0);
        let text = fs::read_to_string(&result.path).unwrap();
        assert!(text.contains("![图片 1](<assets/"));
        assert!(!text.contains("data:image"));
        let root = Path::new(&result.path).parent().unwrap();
        let bytes = fs::read_dir(root.join("assets"))
            .unwrap()
            .map(|e| fs::read(e.unwrap().path()).unwrap())
            .collect::<Vec<_>>();
        assert!(bytes.contains(&b"hello".to_vec()));
        assert!(bytes.contains(&b"%PDF-test\0binary".to_vec()));
    }

    #[test]
    fn rewrites_markdown_links_but_keeps_code_and_web_urls() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("photo (1).png");
        fs::write(&file, b"png").unwrap();
        let file = file.to_string_lossy();
        let mut assets = Assets {
            directory: dir.path().join("assets"),
            source: Path::new("unused"),
            saved: HashMap::new(),
            recovered_paths: HashMap::new(),
            missing: HashSet::new(),
        };
        let text = format!("![photo](<{file}>)\n[file]({})\n[ref]: <{file}>\n`![example](<{file}>)`\n```md\n[example](<{file}>)\n```\n[web](https://example.com/file.png)", urlencoding::encode(&file));
        let output = assets.rewrite_links(&text).unwrap();
        assert_eq!(output.matches("assets/").count(), 3);
        assert_eq!(assets.saved.len(), 1);
        assert!(output.contains(&format!("`![example](<{file}>)`")));
        assert!(output.contains(&format!("```md\n[example](<{file}>)\n```")));
        assert!(output.contains("[web](https://example.com/file.png)"));
    }

    #[test]
    fn missing_files_are_reported_without_losing_text() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("session.jsonl");
        let absent = dir.path().join("missing.pdf");
        write_session(
            &source,
            vec![message(
                1,
                "assistant",
                vec![
                    json!({"type":"output_text","text":format!("正文\n[missing](<{}>)", absent.display())}),
                ],
            )],
        );
        let result = export_snapshot(&source, "缺失附件", dir.path()).unwrap();
        let text = fs::read_to_string(&result.path).unwrap();
        assert_eq!(result.missing_attachment_count, 1);
        assert!(text.contains("正文"));
        assert!(text.contains("有 1 个附件无法读取"));
        assert!(text.contains(absent.to_str().unwrap()));
    }

    #[test]
    fn preserves_code_fences_containing_backticks_and_unmatched_inline_ticks() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.png");
        fs::write(&file, b"png").unwrap();
        let link = format!("![test](<{}>)", file.display());
        let text = format!("````md\n`example` {link}\n```\n````\n~~~md\n{link}\n~~~\n    {link}\n`unmatched\n{link}");
        let mut assets = Assets {
            directory: dir.path().join("assets"),
            source: Path::new("unused"),
            saved: HashMap::new(),
            recovered_paths: HashMap::new(),
            missing: HashSet::new(),
        };
        let output = assets.rewrite_links(&text).unwrap();
        assert_eq!(output.matches("assets/").count(), 1);
        assert_eq!(output.matches(&link).count(), 3);
    }

    #[test]
    fn uses_inline_image_when_original_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("session.jsonl");
        write_session(
            &source,
            vec![message(
                1,
                "user",
                vec![json!({"type":"input_image","image_url":"data:image/png;base64,aGVsbG8="})],
            )],
        );
        let (turns, _) = read_session_turn_page(&source, 0, 50).unwrap();
        let mut attachment = turns[0].public.messages[0].attachments[0].clone();
        attachment.source_path = Some(
            dir.path()
                .join("missing.png")
                .to_string_lossy()
                .into_owned(),
        );
        let mut assets = Assets {
            directory: dir.path().join("assets"),
            source: &source,
            saved: HashMap::new(),
            recovered_paths: HashMap::new(),
            missing: HashSet::new(),
        };
        assert!(assets.attachment(&attachment).unwrap().is_some());
        assert!(assets.missing.is_empty());
        assert_eq!(assets.saved.len(), 1);
    }

    #[test]
    fn long_unicode_titles_fit_filesystem_component_limits() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("session.jsonl");
        write_session(
            &source,
            vec![message(
                1,
                "user",
                vec![json!({"type":"input_text", "text":"hello"})],
            )],
        );
        let result = export_snapshot(&source, &"🐈".repeat(80), dir.path()).unwrap();
        let path = Path::new(&result.path);
        assert!(path.is_file());
        assert!(path.file_name().unwrap().len() <= 255);
        assert!(path.parent().unwrap().file_name().unwrap().len() <= 255);
    }

    #[test]
    fn recovered_inline_images_rewrite_body_links_and_deduplicate() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("session.jsonl");
        let missing = dir
            .path()
            .join("deleted image.png")
            .to_string_lossy()
            .into_owned();
        let text = format!("![图片](<{missing}>)");
        write_session(
            &source,
            vec![
                json!({"type":"event_msg", "payload":{"type":"user_message", "message":text, "local_images":[missing]}}),
                message(
                    1,
                    "user",
                    vec![
                        json!({"type":"input_text","text":text}),
                        json!({"type":"input_image","image_url":"data:image/png;base64,aGVsbG8="}),
                    ],
                ),
            ],
        );
        let result = export_snapshot(&source, "恢复图片", dir.path()).unwrap();
        let output = fs::read_to_string(&result.path).unwrap();
        assert!(!output.contains(&missing), "{output}");
        assert_eq!(output.matches("assets/1-attachment.png").count(), 2);
        assert_eq!(result.attachment_count, 1);
        assert_eq!(result.missing_attachment_count, 0);
    }

    #[test]
    fn cleans_up_failed_exports() {
        let dir = tempfile::tempdir().unwrap();
        assert!(export_snapshot(&dir.path().join("absent.jsonl"), "failed", dir.path()).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
