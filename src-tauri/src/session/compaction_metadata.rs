use serde_json::Value;

/// Codex requires one metadata entry per replacement item. Older repairs removed
/// items without removing their metadata, causing the entire compaction to be
/// skipped. Missing metadata is supported; mismatched entries cannot be safely
/// associated with items again, so discard only that optional metadata array.
pub(super) fn repair_compaction_metadata(record: &mut Value) -> usize {
    if record.get("type").and_then(Value::as_str) != Some("compacted") {
        return 0;
    }
    let Some(payload) = record.get_mut("payload").and_then(Value::as_object_mut) else {
        return 0;
    };
    let Some(metadata) = payload.get("replacement_history_metadata") else {
        return 0;
    };
    if metadata.is_null() {
        return 0;
    }
    let history_len = payload
        .get("replacement_history")
        .and_then(Value::as_array)
        .map(Vec::len);
    if metadata
        .as_array()
        .map(Vec::len)
        .is_some_and(|len| Some(len) == history_len)
    {
        return 0;
    }
    payload.remove("replacement_history_metadata");
    1
}

/// Filter replacement items and their positional metadata in one operation.
pub(super) fn retain_compacted_history(
    record: &mut Value,
    mut keep: impl FnMut(&mut Value) -> bool,
) -> usize {
    repair_compaction_metadata(record);
    let Some(payload) = record.get_mut("payload").and_then(Value::as_object_mut) else {
        return 0;
    };
    let Some(history) = payload
        .get_mut("replacement_history")
        .and_then(Value::as_array_mut)
    else {
        return 0;
    };
    let mut kept = Vec::with_capacity(history.len());
    let before = history.len();
    history.retain_mut(|item| {
        let retain = keep(item);
        kept.push(retain);
        retain
    });
    let removed = before - history.len();
    if let Some(metadata) = payload
        .get_mut("replacement_history_metadata")
        .and_then(Value::as_array_mut)
    {
        let mut index = 0;
        metadata.retain(|_| {
            let retain = kept[index];
            index += 1;
            retain
        });
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionRepairRecord, SessionStore};
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn filters_metadata_at_the_same_positions_and_preserves_other_state() {
        let mut record = json!({"type":"compacted","payload":{
            "replacement_history":[{"type":"message","id":"msg_a"},{"type":"compaction"},{"type":"message","id":"msg_b"}],
            "replacement_history_metadata":[{"client_authored":true},{"client_authored":false},{"retained_source":{"id":"keep"}}],
            "retained_context":{"user_messages":["keep"]},"window_id":"keep"
        }});
        let state = record["payload"]["retained_context"].clone();
        assert_eq!(
            retain_compacted_history(&mut record, |item| item["type"] != "compaction"),
            1
        );
        assert_eq!(
            record["payload"]["replacement_history_metadata"],
            json!([{"client_authored":true},{"retained_source":{"id":"keep"}}])
        );
        assert_eq!(record["payload"]["retained_context"], state);
        assert_eq!(repair_compaction_metadata(&mut record), 0);
    }

    #[test]
    fn repairs_previously_damaged_metadata_without_guessing_item_associations() {
        let mut record = json!({"type":"compacted","payload":{
            "replacement_history":[{"type":"message","id":"msg_keep"}],
            "replacement_history_metadata":[{"client_authored":false},{"client_authored":true}],
            "message":"keep summary"
        }});
        assert_eq!(repair_compaction_metadata(&mut record), 1);
        assert!(record["payload"]
            .get("replacement_history_metadata")
            .is_none());
        assert_eq!(
            record["payload"]["replacement_history"][0]["id"],
            "msg_keep"
        );
        assert_eq!(repair_compaction_metadata(&mut record), 0);
    }

    #[test]
    fn both_repair_entry_points_fix_old_damage_and_migration_keeps_metadata_aligned() {
        for mode in ["switch", "history"] {
            let home = tempdir().unwrap();
            fs::create_dir(home.path().join("sessions")).unwrap();
            let path = home.path().join("sessions/metadata.jsonl");
            let original = [
                json!({"type":"session_meta","ordinal":0,"payload":{"id":"metadata","model_provider":"openai","history_mode":"paginated"}}),
                json!({"type":"compacted","ordinal":1,"payload":{
                    "replacement_history":[{"type":"message","role":"user","content":[{"type":"input_text","text":"保留旧摘要"}]}],
                    "replacement_history_metadata":[{"client_authored":false},{"client_authored":true}],"message":"keep"
                }}),
                json!({"type":"compacted","ordinal":2,"payload":{
                    "replacement_history":[{"type":"message","role":"user","content":[{"type":"input_text","text":"保留新摘要"}]},{"type":"compaction","id":"cmp_old","encrypted_content":"cipher"}],
                    "replacement_history_metadata":[{"client_authored":true},{"client_authored":false}],"message":"keep"
                }}),
            ].iter().map(|r| format!("{r}\n")).collect::<String>();
            fs::write(&path, &original).unwrap();
            let store = SessionStore::new(home.path().to_path_buf());
            let backup_dirs = if mode == "switch" {
                let summary = store.repair_model_compatibility("openai").unwrap();
                assert_eq!(summary.repaired_compaction_metadata_count, 1);
                assert_eq!(summary.removed_encrypted_compaction_item_count, 1);
                summary.backup_dirs
            } else {
                store
                    .repair_rollout_visibility_guarded(
                        "openai",
                        &[SessionRepairRecord {
                            id: "metadata".into(),
                            title: "Metadata".into(),
                            path: path.clone(),
                            updated_at: 0,
                        }],
                        true,
                        &Default::default(),
                    )
                    .unwrap()
                    .backup_dirs
            };
            let repaired = fs::read_to_string(&path).unwrap();
            let records = repaired
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .collect::<Vec<_>>();
            assert!(records[1]["payload"]
                .get("replacement_history_metadata")
                .is_none());
            let last = &records[2]["payload"];
            assert_eq!(
                last["replacement_history"].as_array().unwrap().len(),
                last["replacement_history_metadata"]
                    .as_array()
                    .unwrap()
                    .len()
            );
            assert!(repaired.contains("保留旧摘要") && repaired.contains("保留新摘要"));
            assert!(backup_dirs.iter().any(|directory| fs::read_dir(directory)
                .unwrap()
                .flatten()
                .any(|entry| fs::read_to_string(entry.path()).ok().as_deref() == Some(&original))));
        }
    }
}
