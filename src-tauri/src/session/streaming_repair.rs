//! Stage rollout repairs a record at a time. Unchanged files never get copied;
//! changed files are replaced only after the caller has saved a backup.
use super::*;

pub(super) struct StagedRewrite {
    pub(super) path: PathBuf,
}

impl StagedRewrite {
    fn create(source: &Path) -> Result<(Self, BufWriter<fs::File>), String> {
        let path = source.with_file_name(format!(
            ".rollout-repair-{:016x}.tmp",
            rand::random::<u64>()
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .map_err(|e| format!("创建会话修复临时文件失败：{e}"))?;
        Ok((Self { path }, BufWriter::new(file)))
    }

    pub(super) fn commit(self, destination: &Path) -> Result<(), String> {
        replace_file_atomic(destination, &self.path)
    }
}

impl Drop for StagedRewrite {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn copy_prefix(source: &Path, bytes: u64, writer: &mut impl Write) -> Result<(), String> {
    let input = fs::File::open(source).map_err(|e| e.to_string())?;
    let copied = std::io::copy(&mut input.take(bytes), writer).map_err(|e| e.to_string())?;
    if copied != bytes {
        return Err("会话在修复期间发生变化，未写入修复结果".to_string());
    }
    Ok(())
}

fn finish(writer: &mut BufWriter<fs::File>) -> Result<(), String> {
    writer.flush().map_err(|e| e.to_string())?;
    writer.get_ref().sync_all().map_err(|e| e.to_string())
}

pub(super) fn stage_lines(
    source: &Path,
    mut rewrite: impl FnMut(&[u8], u64) -> Result<Option<Vec<u8>>, String>,
) -> Result<Option<StagedRewrite>, String> {
    let input = fs::File::open(source).map_err(|e| format!("读取会话失败：{e}"))?;
    let mut reader = BufReader::new(input);
    let mut staged = None;
    let mut writer = None;
    let mut line = Vec::new();
    let mut offset = 0;
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        let next = rewrite(&line, offset)?.filter(|next| next != &line);
        if next.is_some() && writer.is_none() {
            let (temp, mut output) = StagedRewrite::create(source)?;
            copy_prefix(source, offset, &mut output)?;
            staged = Some(temp);
            writer = Some(output);
        }
        if let Some(writer) = writer.as_mut() {
            writer
                .write_all(next.as_deref().unwrap_or(&line))
                .map_err(|e| e.to_string())?;
        }
        offset += read as u64;
    }
    if let Some(writer) = writer.as_mut() {
        finish(writer)?;
    }
    Ok(staged)
}

#[derive(Deserialize)]
struct RecordHeader {
    #[serde(rename = "type", default)]
    kind: Value,
    #[serde(default)]
    ordinal: RecordOrdinal,
}

#[derive(Default)]
struct RecordOrdinal {
    present: bool,
    value: Option<u64>,
}

impl<'de> Deserialize<'de> for RecordOrdinal {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Ok(Self {
            present: true,
            value: value.as_u64(),
        })
    }
}

fn record_header(line: &[u8]) -> Option<RecordHeader> {
    // serde can deserialize a struct from a JSON sequence. Rollout records must
    // be objects; accepting a sequence could later panic when assigning ordinal.
    if line.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
        return None;
    }
    serde_json::from_slice(line).ok()
}

fn serialize_line(value: &Value, line: &[u8]) -> Result<Vec<u8>, String> {
    let mut next = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if line.ends_with(b"\r\n") {
        next.extend_from_slice(b"\r\n");
    } else if line.ends_with(b"\n") {
        next.push(b'\n');
    }
    Ok(next)
}

/// Same ordinal/provider/item repair as visibility repair, without retaining any
/// previous record. A changed fork prefix rejects the entire staged rewrite.
pub(super) fn prepare_visibility(
    source: &Path,
    provider: Option<(&str, bool)>,
    protected_bytes: u64,
    repair_items: bool,
    guard_ordinals: bool,
) -> Result<(Option<StagedRewrite>, bool), String> {
    let mut first = true;
    let mut paginated = false;
    let mut expected = 0u64;
    let mut saw_meta = false;
    let mut handled_provider = false;
    let mut protected_change = false;
    let staged = stage_lines(source, |line, offset| {
        if line.iter().all(u8::is_ascii_whitespace) {
            return Ok(None);
        }
        let header = record_header(line);
        if first {
            first = false;
            paginated = header
                .as_ref()
                .is_some_and(|h| h.kind == "session_meta" && h.ordinal.value.is_some());
            if paginated {
                let value: Value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
                expected =
                    rollout_history_base(&value).map_or(0, |base| base.end_ordinal_exclusive);
            }
        }
        let Some(header) = header else {
            if paginated {
                return Err(format!("分页会话第 {expected} 条记录不是有效 JSON 对象"));
            }
            return Ok(None);
        };
        let protected = offset < protected_bytes;
        let normalize = paginated && !(protected && guard_ordinals);
        let duplicate_meta = normalize && saw_meta && header.kind == "session_meta";
        if paginated {
            saw_meta |= header.kind == "session_meta";
        }
        if duplicate_meta {
            protected_change |= protected;
            return Ok(Some(Vec::new()));
        }
        let ordinal = normalize.then_some(expected);
        if paginated {
            expected = if protected && guard_ordinals {
                header
                    .ordinal
                    .value
                    .map_or(expected, |value| value.saturating_add(1))
            } else {
                expected.saturating_add(1)
            };
        }
        let update_provider = provider
            .is_some_and(|(_, all)| header.kind == "session_meta" && (!handled_provider || all));
        handled_provider |= update_provider;
        let update_items = repair_items
            && !protected
            && (header.kind == "compacted"
                || (header.kind == "response_item" && line.windows(4).any(|w| w == b"\"id\"")));
        if !update_items
            && !update_provider
            && ordinal.is_none_or(|n| Some(n) == header.ordinal.value)
        {
            return Ok(None);
        }
        let mut value: Value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
        let mut changed = false;
        if update_items {
            changed |=
                repair_record_item_ids(&mut value) + repair_compaction_metadata(&mut value) > 0;
        }
        if update_provider {
            let (target, _) = provider.unwrap();
            if let Some(payload) = value.get_mut("payload").and_then(Value::as_object_mut) {
                if payload.get("model_provider").and_then(Value::as_str) != Some(target) {
                    payload.insert("model_provider".into(), Value::String(target.into()));
                    changed = true;
                }
            }
        }
        if let Some(ordinal) = ordinal {
            if Some(ordinal) != header.ordinal.value {
                value["ordinal"] = ordinal.into();
                changed = true;
            }
        }
        if !changed {
            return Ok(None);
        }
        protected_change |= protected;
        Ok(Some(serialize_line(&value, line)?))
    })?;
    if protected_change {
        Ok((None, true))
    } else {
        Ok((staged, false))
    }
}

pub(super) fn prepare_local_images(
    source: &Path,
    home: &Path,
    session_id: &str,
    protected_bytes: u64,
) -> Result<(Option<StagedRewrite>, GeneratedImageRepairResult, bool), String> {
    let mut result = GeneratedImageRepairResult::default();
    let mut pending = Vec::new();
    let mut protected_change = false;
    let staged = stage_lines(source, |line, offset| {
        let Ok(header) = serde_json::from_slice::<SessionRecordKind>(line) else {
            return Ok(None);
        };
        let Some(payload) = header.payload else {
            return Ok(None);
        };
        let relevant =
            (header.kind == "response_item" && payload.kind == "message" && payload.role == "user")
                || (header.kind == "event_msg"
                    && !pending.is_empty()
                    && matches!(payload.kind.as_str(), "user_message" | "item_completed"));
        if !relevant {
            return Ok(None);
        }
        let mut value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
        if !rewrite_local_image_attachment_record(
            &mut value,
            home,
            session_id,
            &mut pending,
            &mut result,
        )? {
            return Ok(None);
        }
        protected_change |= offset < protected_bytes;
        Ok(Some(serialize_line(&value, line)?))
    })?;
    Ok((
        if protected_change { None } else { staged },
        result,
        protected_change,
    ))
}

/// Append a protocol snapshot through a temporary file. Only the first/last
/// record's pagination metadata is kept while the original bytes are copied.
pub(super) fn prepare_compaction(
    source: &Path,
    mut compacted: Value,
) -> Result<StagedRewrite, String> {
    let input = fs::File::open(source).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(input);
    let (staged, mut writer) = StagedRewrite::create(source)?;
    let mut line = Vec::new();
    let mut first = true;
    let mut paginated = false;
    let mut last_ordinal = None;
    let mut ends_in_newline = false;
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?
            == 0
        {
            break;
        }
        writer.write_all(&line).map_err(|e| e.to_string())?;
        ends_in_newline = line.ends_with(b"\n");
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let header = record_header(&line);
        if first {
            paginated = header.as_ref().is_some_and(|h| h.ordinal.present);
            first = false;
        }
        last_ordinal = header.and_then(|h| h.ordinal.value);
    }
    if paginated {
        compacted["ordinal"] = last_ordinal
            .ok_or("会话分页序号无效，未迁移上下文")?
            .checked_add(1)
            .ok_or("会话分页序号溢出")?
            .into();
    }
    if !ends_in_newline {
        writer.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    serde_json::to_writer(&mut writer, &compacted).map_err(|e| e.to_string())?;
    writer.write_all(b"\n").map_err(|e| e.to_string())?;
    finish(&mut writer)?;
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn visibility_stream_matches_previous_repairs_and_is_idempotent() {
        let home = tempdir().unwrap();
        let path = home.path().join("history.jsonl");
        let content = [
            json!({"type":"session_meta","ordinal":0,"payload":{"id":"child","model_provider":"old","history_base":{"thread_id":"parent","end_byte_offset":100,"end_ordinal_exclusive":7}}}),
            json!({"type":"response_item","ordinal":88,"payload":{"type":"message","id":"resp_bad","role":"user","content":[{"type":"input_text","text":"keep"}]}}),
            json!({"type":"session_meta","ordinal":0,"payload":{"id":"duplicate"}}),
            json!({"type":"response_item","ordinal":"broken","payload":{"type":"function_call_output","call_id":"keep_call","output":"unchanged"}}),
            json!({"type":"compacted","payload":{"replacement_history":[],"replacement_history_metadata":[{}]}}),
        ].iter().map(Value::to_string).collect::<Vec<_>>().join("\r\n");
        let ids = rewrite_rollout_item_ids(&content, 0).unwrap().unwrap();
        let provider = rewrite_session_meta_provider(&ids, "openai", true)
            .unwrap()
            .unwrap();
        let expected = normalize_paginated_rollout_ordinals(&provider)
            .unwrap()
            .unwrap();
        fs::write(&path, &content).unwrap();
        let (staged, protected) =
            prepare_visibility(&path, Some(("openai", true)), 0, true, false).unwrap();
        assert!(!protected);
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
        staged.unwrap().commit(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert!(
            prepare_visibility(&path, Some(("openai", true)), 0, true, false)
                .unwrap()
                .0
                .is_none()
        );
        assert_eq!(fs::read_dir(home.path()).unwrap().count(), 1);
    }

    #[test]
    fn malformed_pagination_leaves_original_and_removes_staging_file() {
        let home = tempdir().unwrap();
        let path = home.path().join("history.jsonl");
        let original = "{\"type\":\"session_meta\",\"ordinal\":0,\"payload\":{\"model_provider\":\"old\"}}\ninvalid JSON\n";
        fs::write(&path, original).unwrap();
        assert!(prepare_visibility(&path, Some(("new", true)), 0, true, false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert_eq!(fs::read_dir(home.path()).unwrap().count(), 1);
        let original = "{\"type\":\"session_meta\",\"ordinal\":0,\"payload\":{\"model_provider\":\"old\"}}\n[\"event_msg\",7]\n";
        fs::write(&path, original).unwrap();
        assert!(prepare_visibility(&path, Some(("new", true)), 0, true, false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert_eq!(fs::read_dir(home.path()).unwrap().count(), 1);
    }

    #[test]
    fn protected_prefix_is_rejected_or_preserved_with_guarded_ordinals() {
        let home = tempdir().unwrap();
        let path = home.path().join("history.jsonl");
        let prefix =
            "{\"type\":\"session_meta\",\"ordinal\":5,\"payload\":{\"model_provider\":\"old\"}}\n";
        let original = format!("{prefix}{{\"type\":\"event_msg\",\"ordinal\":99,\"payload\":{{\"type\":\"task_complete\"}}}}\n");
        fs::write(&path, &original).unwrap();
        let (staged, skipped) =
            prepare_visibility(&path, Some(("new", true)), prefix.len() as u64, true, false)
                .unwrap();
        assert!(skipped && staged.is_none());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let (staged, skipped) =
            prepare_visibility(&path, None, prefix.len() as u64, false, true).unwrap();
        assert!(!skipped);
        staged.unwrap().commit(&path).unwrap();
        let repaired = fs::read_to_string(&path).unwrap();
        assert!(repaired.starts_with(prefix));
        let tail: Value = serde_json::from_str(repaired.lines().last().unwrap()).unwrap();
        assert_eq!(tail["ordinal"], 6);
    }

    #[test]
    fn compaction_append_preserves_bytes_and_continues_ordinal() {
        let home = tempdir().unwrap();
        let path = home.path().join("history.jsonl");
        let original = "{\"type\":\"session_meta\",\"ordinal\":9,\"payload\":{}}\r\n{\"type\":\"event_msg\",\"ordinal\":10}";
        fs::write(&path, original).unwrap();
        prepare_compaction(
            &path,
            json!({"type":"compacted","payload":{"replacement_history":[]}}),
        )
        .unwrap()
        .commit(&path)
        .unwrap();
        let repaired = fs::read_to_string(&path).unwrap();
        assert!(repaired.starts_with(original));
        let compacted: Value = serde_json::from_str(repaired.lines().last().unwrap()).unwrap();
        assert_eq!(compacted["ordinal"], 11);
        fs::write(&path, "{\"type\":\"session_meta\",\"ordinal\":null}\n").unwrap();
        assert!(prepare_compaction(&path, json!({"type":"compacted","payload":{}})).is_err());
    }

    #[test]
    #[ignore = "manual large-file benchmark; uses only a temporary copy"]
    fn benchmark_large_rollout_repair() {
        let source = PathBuf::from(
            std::env::var_os("CODEX_REPAIR_BENCHMARK_FILE")
                .expect("set CODEX_REPAIR_BENCHMARK_FILE"),
        );
        let home = tempdir().unwrap();
        let directory = home.path().join("sessions");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(source.file_name().unwrap());
        fs::copy(&source, &path).unwrap();
        let original = fs::metadata(&source).unwrap();
        fs::write(
            home.path().join("config.toml"),
            "model_provider = \"openai\"\n",
        )
        .unwrap();
        let connection = Connection::open(home.path().join("state_5.sqlite")).unwrap();
        connection.execute_batch("CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT, archived INTEGER DEFAULT 0)").unwrap();
        drop(connection);
        let store = SessionStore::new(home.path().into());
        let started = std::time::Instant::now();
        for pass in 0..2 {
            let start = std::time::Instant::now();
            let outcome = crate::one_click_repair_session_store(&store, &mut |step| {
                eprintln!(
                    "repair pass {} stage {}: {:?}",
                    pass + 1,
                    step,
                    start.elapsed()
                );
            })
            .unwrap();
            assert!(outcome.warnings.is_empty(), "repair returned warnings");
            assert_eq!(outcome.session_count, 1);
            if pass > 0 {
                assert_eq!(outcome.model_compatibility.repaired_rollout_file_count, 0);
                assert_eq!(outcome.visibility.changed_rollout_file_count, 0);
            }
            eprintln!("repair pass {}: {:?}", pass + 1, start.elapsed());
        }
        assert_eq!(fs::metadata(&source).unwrap().len(), original.len());
        assert_eq!(
            fs::metadata(&source).unwrap().modified().unwrap(),
            original.modified().unwrap()
        );
        eprintln!(
            "source_bytes={} total={:?}",
            original.len(),
            started.elapsed()
        );
    }

    #[test]
    fn large_tool_history_rewrites_only_tail_and_images_are_noop() {
        let home = tempdir().unwrap();
        let path = home.path().join("large.jsonl");
        let mut file = BufWriter::new(fs::File::create(&path).unwrap());
        let line = format!(
            "{}\n",
            json!({"type":"response_item","payload":{"type":"function_call_output","output":"x".repeat(64 * 1024)}})
        );
        for _ in 0..512 {
            file.write_all(line.as_bytes()).unwrap();
        }
        file.write_all(b"{\"type\":\"session_meta\",\"payload\":{\"model_provider\":\"old\"}}\n")
            .unwrap();
        file.flush().unwrap();
        drop(file);
        let (staged, images, protected) =
            prepare_local_images(&path, home.path(), "test", 0).unwrap();
        assert!(staged.is_none() && !protected);
        assert_eq!(images.recreated, 0);
        let staged = prepare_visibility(&path, Some(("new", true)), 0, true, false)
            .unwrap()
            .0
            .unwrap();
        staged.commit(&path).unwrap();
        let mut reader = BufReader::new(fs::File::open(&path).unwrap());
        let mut buffer = String::new();
        for _ in 0..512 {
            buffer.clear();
            reader.read_line(&mut buffer).unwrap();
            assert_eq!(buffer, line);
        }
        buffer.clear();
        reader.read_line(&mut buffer).unwrap();
        let tail: Value = serde_json::from_str(&buffer).unwrap();
        assert_eq!(tail["payload"]["model_provider"], "new");
    }

    #[test]
    fn metadata_scan_keeps_latest_turn_settings_after_large_tool_output() {
        let home = tempdir().unwrap();
        let path = home.path().join("history.jsonl");
        let records = [
            json!({"type":"session_meta","ordinal":0,"payload":{"id":"test","cwd":"/project","source":"cli"}}),
            json!({"type":"turn_context","payload":{"approval_policy":"never","sandbox_policy":"read-only"}}),
            json!({"type":"response_item","payload":{"type":"function_call_output","output":"payload".repeat(1024 * 1024)}}),
            json!({"type":"turn_context","payload":{"approval_policy":"on-request","sandbox_policy":"workspace-write"}}),
        ];
        let mut writer = BufWriter::new(fs::File::create(&path).unwrap());
        for record in records {
            serde_json::to_writer(&mut writer, &record).unwrap();
            writer.write_all(b"\n").unwrap();
        }
        writer.flush().unwrap();
        let metadata = sqlite_metadata_for_session(&SessionRepairRecord {
            id: "test".into(),
            path,
            title: String::new(),
            updated_at: 123,
        });
        assert_eq!(metadata.cwd, "/project");
        assert_eq!(metadata.approval_mode, "on-request");
        assert_eq!(metadata.sandbox_policy, "workspace-write");
        assert_eq!(metadata.history_mode, "paginated");
    }
}
