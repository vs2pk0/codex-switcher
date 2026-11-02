use serde_json::Value;

/// Third-party APIs can confuse response IDs and tool call IDs with item IDs.
/// Messages, function calls and custom tool calls accept optional item IDs with
/// msg_, fc_ and ctc_ prefixes respectively. Their call_id links must stay intact.
/// Codex forwards IDs with a nonempty prefix and suffix separated by `_`. Keep
/// non-prefixed local IDs stable; remove only remote-looking IDs of the wrong type.
pub(super) fn repair_record_item_ids(record: &mut Value) -> usize {
    match record.get("type").and_then(Value::as_str) {
        Some("response_item") => record.get_mut("payload").map(repair_item_id).unwrap_or(0),
        Some("compacted") => record
            .pointer_mut("/payload/replacement_history")
            .and_then(Value::as_array_mut)
            .map(|items| items.iter_mut().map(repair_item_id).sum())
            .unwrap_or(0),
        _ => 0,
    }
}

fn repair_item_id(item: &mut Value) -> usize {
    let Some(item) = item.as_object_mut() else {
        return 0;
    };
    // Only these replay items have optional IDs with known type prefixes.
    // Leave required IDs, tool outputs and unknown future item types alone.
    let expected_prefix = match item.get("type").and_then(Value::as_str) {
        Some("message") => "msg",
        Some("function_call") => "fc",
        Some("custom_tool_call") => "ctc",
        _ => return 0,
    };
    if item
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| id.split_once('_'))
        .is_some_and(|(prefix, suffix)| {
            prefix != expected_prefix && !prefix.is_empty() && !suffix.is_empty()
        })
    {
        item.remove("id");
        return 1;
    }
    0
}

pub(super) fn rewrite_rollout_item_ids(
    content: &str,
    protected_prefix_bytes: u64,
) -> Result<Option<String>, String> {
    let mut output = String::with_capacity(content.len());
    let mut changed = false;
    let mut offset = 0u64;
    for segment in content.split_inclusive('\n') {
        let protected = offset < protected_prefix_bytes;
        offset += segment.len() as u64;
        if protected || !segment.contains("\"id\"") {
            output.push_str(segment);
            continue;
        }
        let Ok(mut record) = serde_json::from_str::<Value>(segment) else {
            output.push_str(segment);
            continue;
        };
        if repair_record_item_ids(&mut record) == 0 {
            output.push_str(segment);
            continue;
        }
        let ending = if segment.ends_with("\r\n") {
            "\r\n"
        } else if segment.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        output.push_str(
            &serde_json::to_string(&record)
                .map_err(|error| format!("序列化会话条目 ID 修复结果失败：{error}"))?,
        );
        output.push_str(ending);
        changed = true;
    }
    Ok(changed.then_some(output))
}

#[cfg(test)]
mod tests {
    use super::super::{SessionRepairRecord, SessionStore};
    use super::{repair_record_item_ids, rewrite_rollout_item_ids};
    use serde_json::{json, Value};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn repairs_message_ids_without_changing_content_or_tool_links() {
        let message = json!({
            "type": "message", "id": "resp_1792831216516_msg", "role": "assistant",
            "status": "completed",
            "content": [{"type": "output_text", "text": "正文中的 resp_keep_msg 保留"},
                {"type": "input_image", "image_url": "data:image/png;base64,KEEP"}]
        });
        let mut expected_message = message.clone();
        expected_message.as_object_mut().unwrap().remove("id");
        let valid_message =
            json!({"type": "message", "id": "msg_valid", "role": "user", "content": []});
        let no_id_message = json!({"type": "message", "role": "user", "content": []});
        let null_id_message = json!({"type": "message", "id": null, "role": "user", "content": []});
        let call = json!({"type": "function_call", "id": "fc_keep", "call_id": "resp_tool_call", "name": "shell", "arguments": "{}"});
        let output =
            json!({"type": "function_call_output", "call_id": "resp_tool_call", "output": "keep"});
        let custom_call = json!({"type": "custom_tool_call", "id": "ctc_keep", "call_id": "call_custom", "name": "apply_patch", "input": "keep"});
        let custom_output =
            json!({"type": "custom_tool_call_output", "call_id": "call_custom", "output": "keep"});
        let reasoning =
            json!({"type": "reasoning", "id": "rs_keep", "encrypted_content": "ciphertext"});
        let compaction =
            json!({"type": "compaction", "id": "cmp_keep", "encrypted_content": "ciphertext"});
        let unchanged = vec![
            valid_message,
            no_id_message,
            null_id_message,
            call,
            output,
            custom_call,
            custom_output,
            reasoning,
            compaction,
            json!({"type":"message","id":"0123456789abcdef0123456789abcdef","role":"user","content":[]}),
            json!({"type":"message","id":"user-images","role":"user","content":[]}),
            json!({"type":"message","id":"visible-summary","role":"assistant","content":[]}),
            json!({"type":"message","id":"_local","role":"user","content":[]}),
            json!({"type":"message","id":"local_","role":"user","content":[]}),
        ];
        let mut history = vec![message.clone()];
        history.extend(unchanged.clone());
        let mut record = json!({"type": "compacted", "payload": {"message": "保留摘要", "replacement_history": history}});
        let mut expected = record.clone();
        expected["payload"]["replacement_history"][0] = expected_message.clone();
        assert_eq!(repair_record_item_ids(&mut record), 1);
        assert_eq!(record, expected);
        assert_eq!(repair_record_item_ids(&mut record), 0);

        let mut record = json!({"type": "response_item", "ordinal": 1986, "payload": message});
        let expected =
            json!({"type": "response_item", "ordinal": 1986, "payload": expected_message});
        assert_eq!(repair_record_item_ids(&mut record), 1);
        assert_eq!(record, expected);
        for item in unchanged {
            let mut record = json!({"type": "response_item", "payload": item});
            let original = record.clone();
            assert_eq!(repair_record_item_ids(&mut record), 0);
            assert_eq!(record, original);
        }
    }

    #[test]
    fn preserves_unrelated_records_and_fields() {
        for mut record in [
            json!({"type": "event_msg", "payload": {"type": "message", "id": "resp_event"}}),
            json!({"type": "session_meta", "payload": {"id": "resp_session"}}),
            json!({"type": "response_item", "payload": {"type": "unknown", "id": "resp_unknown"}}),
            json!({"type": "compacted", "payload": {"replacement_history": null}}),
        ] {
            let original = record.clone();
            assert_eq!(repair_record_item_ids(&mut record), 0);
            assert_eq!(record, original);
        }
    }

    #[test]
    fn repairs_foreign_remote_prefixes_by_item_type() {
        for (kind, valid_prefix) in [
            ("message", "msg"),
            ("function_call", "fc"),
            ("custom_tool_call", "ctc"),
        ] {
            for prefix in ["resp", "chatcmpl", "call", "msg", "fc", "ctc", "rs", "cmp"] {
                let id = format!("{prefix}_foreign");
                let mut record = json!({"type":"response_item","payload":{"type":kind,"id":id}});
                let expected_changes = usize::from(prefix != valid_prefix);
                assert_eq!(
                    repair_record_item_ids(&mut record),
                    expected_changes,
                    "{kind}: {id}"
                );
                assert_eq!(
                    record["payload"].get("id").is_none(),
                    prefix != valid_prefix
                );
                assert_eq!(repair_record_item_ids(&mut record), 0);
            }
        }
    }

    #[test]
    fn repairs_tool_item_ids_without_changing_call_ids_or_outputs() {
        let call_id = "call_bf2d04695dce4706847f9eab";
        let items = vec![
            json!({"type":"function_call","id":call_id,"call_id":call_id,"name":"shell","arguments":"{\"id\":\"call_keep\"}","status":"completed"}),
            json!({"type":"function_call_output","call_id":call_id,"output":"保留执行结果"}),
            json!({"type":"custom_tool_call","id":"resp_custom","call_id":"call_patch","name":"apply_patch","input":"保留补丁内容","status":"completed"}),
            json!({"type":"custom_tool_call_output","call_id":"call_patch","output":"保留补丁结果"}),
        ];
        let mut expected = items.clone();
        expected[0].as_object_mut().unwrap().remove("id");
        expected[2].as_object_mut().unwrap().remove("id");

        let mut record = json!({"type":"compacted","payload":{"message":"保留摘要","replacement_history":items}});
        assert_eq!(repair_record_item_ids(&mut record), 2);
        assert_eq!(record["payload"]["replacement_history"], json!(expected));
        assert_eq!(repair_record_item_ids(&mut record), 0);
        for (item, expected) in items.into_iter().zip(expected) {
            let changed = usize::from(item != expected);
            let mut record = json!({"type":"response_item","ordinal":1987,"payload":item});
            assert_eq!(repair_record_item_ids(&mut record), changed);
            assert_eq!(
                record,
                json!({"type":"response_item","ordinal":1987,"payload":expected})
            );
        }

        for (kind, valid_id) in [
            ("function_call", "fc_valid"),
            ("custom_tool_call", "ctc_valid"),
        ] {
            for id in [
                Some(valid_id),
                Some("0123456789abcdef0123456789abcdef"),
                Some("local-call"),
                None,
            ] {
                let mut record = json!({"type":"response_item","payload":{"type":kind,"id":id,"call_id":call_id}});
                let original = record.clone();
                assert_eq!(repair_record_item_ids(&mut record), 0);
                assert_eq!(record, original);
            }
        }
    }

    #[test]
    fn both_repairs_fix_tool_id_only_changes_and_preserve_backups() {
        for mode in ["switch", "history"] {
            let home = tempdir().unwrap();
            let dir = home.path().join("sessions");
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join("tool-ids.jsonl");
            let values = vec![
                json!({"type":"session_meta","ordinal":0,"payload":{"id":"tools","model_provider":"openai","history_mode":"paginated"}}),
                json!({"type":"response_item","ordinal":1,"payload":{"type":"function_call","id":"call_bf2d04695dce4706847f9eab","call_id":"call_bf2d04695dce4706847f9eab","name":"shell","arguments":"{}"}}),
                json!({"type":"response_item","ordinal":2,"payload":{"type":"function_call_output","call_id":"call_bf2d04695dce4706847f9eab","output":"保留工具输出"}}),
                json!({"type":"compacted","ordinal":3,"payload":{"message":"保留摘要","replacement_history":[
                    {"type":"function_call","id":"call_summary","call_id":"call_summary","name":"shell","arguments":"{}"},
                    {"type":"function_call_output","call_id":"call_summary","output":"保留压缩历史工具输出"},
                    {"type":"custom_tool_call","id":"call_patch","call_id":"call_patch","name":"apply_patch","input":"保留补丁"},
                    {"type":"custom_tool_call_output","call_id":"call_patch","output":"保留补丁结果"}
                ]}}),
            ];
            let original = values
                .iter()
                .map(|value| format!("{value}\n"))
                .collect::<String>();
            fs::write(&path, &original).unwrap();
            let store = SessionStore::new(home.path().to_path_buf());
            let records = [SessionRepairRecord {
                id: "tools".into(),
                title: "Tools".into(),
                path: path.clone(),
                updated_at: 0,
            }];
            let backup_dirs = if mode == "switch" {
                let summary = store.repair_model_compatibility("openai").unwrap();
                assert_eq!(summary.repaired_rollout_file_count, 1);
                assert_eq!(summary.repaired_item_id_count, 3);
                summary.backup_dirs
            } else {
                let outcome = store
                    .repair_rollout_visibility_guarded(
                        "openai",
                        &records,
                        true,
                        &Default::default(),
                    )
                    .unwrap();
                assert_eq!(outcome.changed, 1);
                assert_eq!(outcome.changed_session_ids.len(), 1);
                assert!(outcome.changed_session_ids.contains("tools"));
                outcome.backup_dirs
            };
            let actual: Vec<Value> = fs::read_to_string(&path)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            let mut expected = values;
            expected[1]["payload"].as_object_mut().unwrap().remove("id");
            for index in [0, 2] {
                expected[3]["payload"]["replacement_history"][index]
                    .as_object_mut()
                    .unwrap()
                    .remove("id");
            }
            assert_eq!(
                actual, expected,
                "{mode}: only incompatible item IDs should change"
            );
            assert!(backup_dirs
                .iter()
                .any(|dir| fs::read_dir(dir)
                    .unwrap()
                    .flatten()
                    .any(|entry| fs::read_to_string(entry.path())
                        .is_ok_and(|backup| backup == original))));
            if mode == "switch" {
                assert_eq!(
                    store
                        .repair_model_compatibility("openai")
                        .unwrap()
                        .repaired_rollout_file_count,
                    0
                );
            } else {
                assert_eq!(
                    store
                        .repair_rollout_visibility_guarded(
                            "openai",
                            &records,
                            true,
                            &Default::default()
                        )
                        .unwrap()
                        .changed,
                    0
                );
            }
        }
    }

    #[test]
    fn preserves_line_endings_malformed_lines_and_protected_prefix() {
        let protected = "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"resp_parent_msg\",\"content\":[]}}\r\n";
        let unchanged = " {\"type\":\"response_item\", \"payload\":{\"type\":\"message\",\"id\":\"msg_valid\",\"content\":[]}}\n";
        let invalid = "not valid JSON \"id\"\r\n\n";
        let tail = "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"resp_tail_msg\",\"content\":[]}}";
        for ending in ["\r\n", "\n", ""] {
            let content = format!("{protected}{unchanged}{invalid}{tail}{ending}");
            let repaired = rewrite_rollout_item_ids(&content, protected.len() as u64)
                .unwrap()
                .unwrap();
            assert!(repaired.starts_with(&format!("{protected}{unchanged}{invalid}")));
            let last = &repaired[protected.len() + unchanged.len() + invalid.len()..];
            let record: Value = serde_json::from_str(last).unwrap();
            assert!(record["payload"].get("id").is_none());
            assert!(last.strip_suffix(ending).unwrap().ends_with('}'));
            assert!(last.ends_with(ending));
            assert_eq!(
                rewrite_rollout_item_ids(&repaired, protected.len() as u64).unwrap(),
                None
            );
        }
    }

    #[test]
    fn both_repairs_handle_id_only_changes_back_up_and_are_idempotent() {
        for mode in ["switch", "history"] {
            let home = tempdir().unwrap();
            let store = SessionStore::new(home.path().to_path_buf());
            let mut records = Vec::new();
            let mut originals = Vec::new();
            let mut expected_records = Vec::new();
            for directory in ["sessions", "archived_sessions"] {
                let dir = home.path().join(directory);
                fs::create_dir_all(&dir).unwrap();
                let path = dir.join(format!("{directory}.jsonl"));
                let values = vec![
                    json!({"type":"session_meta","ordinal":0,"payload":{"id":directory,"model_provider":"openai","history_mode":"paginated"}}),
                    json!({"type":"turn_context","ordinal":1,"payload":{"model":"gpt-5.5"}}),
                    json!({"type":"response_item","ordinal":2,"payload":{"type":"message","id":"resp_1792831216516_msg","role":"assistant","content":[{"type":"output_text","text":"保留回答"}]}}),
                    json!({"type":"compacted","ordinal":3,"payload":{"message":"保留摘要","replacement_history":[
                        {"type":"message","id":"resp_summary_msg","role":"user","content":[{"type":"input_text","text":"保留问题"}]},
                        {"type":"message","id":"msg_valid","role":"assistant","content":[]},
                        {"type":"function_call","id":"fc_keep","call_id":"call_keep","name":"shell","arguments":"{}"},
                        {"type":"function_call_output","call_id":"call_keep","output":"保留工具结果"}
                    ]}}),
                ];
                let original = values.iter().map(|v| format!("{v}\n")).collect::<String>();
                fs::write(&path, &original).unwrap();
                originals.push(original);
                let mut expected = values;
                expected[2]["payload"].as_object_mut().unwrap().remove("id");
                expected[3]["payload"]["replacement_history"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("id");
                expected_records.push(expected);
                records.push(SessionRepairRecord {
                    id: directory.to_string(),
                    title: directory.to_string(),
                    path,
                    updated_at: 0,
                });
            }
            let history =
                rusqlite::Connection::open(home.path().join("thread_history_1.sqlite")).unwrap();
            history.execute_batch("
                CREATE TABLE thread_items (thread_id TEXT, item_id TEXT);
                CREATE TABLE thread_turns (thread_id TEXT, turn_id TEXT);
                CREATE TABLE thread_history_projection_state (thread_id TEXT PRIMARY KEY, next_rollout_byte_offset INTEGER, next_rollout_ordinal INTEGER);
                INSERT INTO thread_items VALUES ('sessions','resp_1792831216516_msg'), ('archived_sessions','resp_summary_msg');
                INSERT INTO thread_history_projection_state VALUES ('sessions',1,4), ('archived_sessions',1,4);
            ").unwrap();

            let backup_dirs = if mode == "switch" {
                let summary = store.repair_model_compatibility("openai").unwrap();
                assert_eq!(summary.repaired_rollout_file_count, 2);
                assert_eq!(summary.repaired_item_id_count, 4);
                assert_eq!(summary.rewritten_rollout_model_field_count, 0);
                assert_eq!(summary.synchronized_rollout_provider_count, 0);
                assert_eq!(summary.removed_encrypted_reasoning_item_count, 0);
                assert_eq!(summary.removed_encrypted_compaction_item_count, 0);
                let stale_count: usize = history
                    .query_row("SELECT COUNT(*) FROM thread_items", [], |row| row.get(0))
                    .unwrap();
                assert_eq!(stale_count, 0, "ID-only changes must reset cached history");
                summary.backup_dirs
            } else {
                let outcome = store
                    .repair_rollout_visibility_guarded(
                        "openai",
                        &records,
                        true,
                        &Default::default(),
                    )
                    .unwrap();
                assert_eq!(outcome.changed, 2);
                assert!(records
                    .iter()
                    .all(|record| outcome.changed_session_ids.contains(&record.id)));
                outcome.backup_dirs
            };
            for ((record, original), expected) in
                records.iter().zip(&originals).zip(&expected_records)
            {
                let content = fs::read_to_string(&record.path).unwrap();
                let actual: Vec<Value> = content
                    .lines()
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
                assert_eq!(&actual, expected, "{mode}: {}", record.id);
                assert!(
                    backup_dirs
                        .iter()
                        .any(|dir| fs::read_dir(dir).unwrap().flatten().any(|entry| {
                            entry.path().extension().is_some_and(|ext| ext == "jsonl")
                                && fs::read_to_string(entry.path())
                                    .is_ok_and(|backup| backup == *original)
                        })),
                    "{mode}: missing original backup for {}",
                    record.id
                );
            }
            if mode == "switch" {
                let summary = store.repair_model_compatibility("openai").unwrap();
                assert_eq!(summary.repaired_rollout_file_count, 0);
                assert_eq!(summary.repaired_item_id_count, 0);
                assert!(summary.backup_dirs.is_empty());
            } else {
                let outcome = store
                    .repair_rollout_visibility_guarded(
                        "openai",
                        &records,
                        true,
                        &Default::default(),
                    )
                    .unwrap();
                assert_eq!(outcome.changed, 0);
                assert!(outcome.backup_dirs.is_empty());
            }
        }
    }

    #[test]
    fn both_repairs_keep_fork_prefix_bytes_and_repair_the_tail() {
        for mode in ["switch", "history"] {
            for (kind, bad_id) in [
                ("message", "resp_tail_msg"),
                ("function_call", "call_tail"),
                ("custom_tool_call", "call_patch"),
            ] {
                let home = tempdir().unwrap();
                let sessions = home.path().join("sessions");
                fs::create_dir_all(&sessions).unwrap();
                let parent = sessions.join("parent.jsonl");
                let prefix = concat!(
                "{\"type\":\"session_meta\",\"ordinal\":0,\"payload\":{\"id\":\"parent\",\"model_provider\":\"openai\"}}\n",
                "{\"type\":\"response_item\",\"ordinal\":1,\"payload\":{\"type\":\"message\",\"id\":\"resp_parent_msg\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"保留分支历史\"}]}}\n"
            );
                let tail = json!({"type":"response_item","ordinal":2,"payload":{"type":kind,"id":bad_id,"role":"assistant","call_id":"call_keep","name":"shell","arguments":"{}","input":"保留工具输入","content":[{"type":"output_text","text":"保留后续回复"}]}});
                fs::write(&parent, format!("{prefix}{tail}\n")).unwrap();
                fs::write(sessions.join("child.jsonl"), format!("{}\n", json!({
                "type":"session_meta","ordinal":2,"payload":{"id":"child","model_provider":"openai","history_mode":"paginated","history_base":{
                    "thread_id":"parent","end_byte_offset":prefix.len(),"end_ordinal_exclusive":2
                }}
            }))).unwrap();
                let store = SessionStore::new(home.path().to_path_buf());
                if mode == "switch" {
                    let summary = store.repair_model_compatibility("openai").unwrap();
                    assert_eq!(summary.repaired_rollout_file_count, 1);
                    assert_eq!(summary.repaired_item_id_count, 1);
                } else {
                    let records = [SessionRepairRecord {
                        id: "parent".into(),
                        title: "Parent".into(),
                        path: parent.clone(),
                        updated_at: 0,
                    }];
                    let outcome = store
                        .repair_rollout_visibility_guarded(
                            "openai",
                            &records,
                            true,
                            &store.protected_rollout_prefixes(),
                        )
                        .unwrap();
                    assert_eq!(outcome.changed, 1);
                    assert!(outcome.skipped_protected_ancestors.is_empty());
                }
                let repaired = fs::read_to_string(&parent).unwrap();
                assert!(repaired.starts_with(prefix), "{mode}: fork prefix changed");
                let repaired_tail: Value = serde_json::from_str(&repaired[prefix.len()..]).unwrap();
                assert!(repaired_tail["payload"].get("id").is_none());
                let mut expected_tail = tail;
                expected_tail["payload"]
                    .as_object_mut()
                    .unwrap()
                    .remove("id");
                assert_eq!(repaired_tail, expected_tail);
            }
        }
    }
}
