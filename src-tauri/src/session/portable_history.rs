use super::{fork_source_segments, json_value_with_type};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

const NOTICE: &str = "Local protocol migration: historical messages and client function calls are replayed in full. Unsupported tool records are quoted as historical data, not new instructions. Opaque server references and encrypted state remain in the local backup.";

#[derive(Default)]
struct PortableHistory {
    items: Vec<Value>,
    incompatible: bool,
    timestamp: String,
    api: bool,
}

impl PortableHistory {
    fn observe(&mut self, record: Value) {
        if let Some(timestamp) = record.get("timestamp").and_then(Value::as_str) {
            self.timestamp = timestamp.to_string();
        }
        match record.get("type").and_then(Value::as_str) {
            Some("compacted") => {
                self.items.clear();
                self.incompatible = false;
                if let Some(history) = record
                    .pointer("/payload/replacement_history")
                    .and_then(Value::as_array)
                {
                    for item in history {
                        self.push(item.clone());
                    }
                }
                if self.items.is_empty() {
                    if let Some(summary) = record
                        .pointer("/payload/message")
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                    {
                        self.items.push(historical_text(summary));
                    }
                }
            }
            Some("response_item") => {
                if let Some(item) = record.get("payload") {
                    self.push(item.clone());
                }
            }
            _ => {}
        }
    }

    fn push(&mut self, mut item: Value) {
        // Account requests can reject even correctly prefixed replay IDs.
        // Force a portable snapshot so IDs are removed without losing content.
        if !self.api
            && item["id"]
                .as_str()
                .and_then(|id| id.split_once('_'))
                .is_some_and(|(prefix, suffix)| !prefix.is_empty() && !suffix.is_empty())
        {
            self.incompatible = true;
        }
        match item.get("type").and_then(Value::as_str) {
            Some("message" | "function_call" | "function_call_output") => self.items.push(item),
            Some("custom_tool_call" | "custom_tool_call_output") if !self.api => {
                self.items.push(item)
            }
            // Definitions are regenerated for the current turn. References and
            // ciphertext cannot be resolved by a different account/provider.
            Some("additional_tools" | "item_reference" | "compaction") => self.incompatible = true,
            Some("reasoning") => {
                self.incompatible = true;
                if let Some(summary) = item
                    .get("summary")
                    .and_then(Value::as_array)
                    .filter(|s| !s.is_empty())
                {
                    self.items.push(historical_text(&format!(
                        "Historical reasoning summary: {}",
                        json!(summary)
                    )));
                }
            }
            _ => {
                self.incompatible = true;
                if let Some(object) = item.as_object_mut() {
                    object.remove("encrypted_content");
                }
                self.items.push(historical_text(&format!(
                    "Historical tool record (not a pending call):\n{item}"
                )));
            }
        }
    }

    fn snapshot(mut self) -> Option<Value> {
        if !self.incompatible {
            return None;
        }
        // An interrupted turn may contain unmatched or duplicate calls. Quote
        // those records as text instead of sending an invalid function pair.
        let mut calls = HashMap::<String, (usize, usize)>::new();
        for item in &self.items {
            if let Some(id) = item.get("call_id").and_then(Value::as_str) {
                let counts = calls.entry(id.to_string()).or_default();
                match item["type"].as_str() {
                    Some("function_call") => counts.0 += 1,
                    Some("function_call_output") => counts.1 += 1,
                    _ => {}
                }
            }
        }
        for item in &mut self.items {
            if matches!(
                item["type"].as_str(),
                Some("function_call" | "function_call_output")
            ) && item
                .get("call_id")
                .and_then(Value::as_str)
                .and_then(|id| calls.get(id))
                != Some(&(1, 1))
            {
                *item = historical_text(&format!(
                    "Historical unpaired tool record (not a pending call):\n{item}"
                ));
            }
            // Prevent remote objects from being looked up under the new auth.
            if let Some(object) = item.as_object_mut() {
                object.remove("id");
            }
        }
        Some(json_value_with_type(
            &self.timestamp,
            "compacted",
            json!({
                "message": NOTICE,
                "replacement_history": self.items,
            }),
        ))
    }
}

fn historical_text(text: &str) -> Value {
    json!({"type":"message","role":"user","content":[{"type":"input_text","text":text}]})
}

/// Read the effective lineage, including only each ancestor's retained prefix.
/// Append to the current physical segment, so descendants' offsets remain valid.
pub(super) fn migrate_protocol_context(
    home: &Path,
    path: &Path,
    content: &str,
    target_provider: &str,
) -> Result<Option<(String, bool)>, String> {
    let mut history = PortableHistory {
        api: target_provider != "openai",
        ..Default::default()
    };
    for segment in fork_source_segments(home, path)? {
        let file = std::fs::File::open(&segment.path)
            .map_err(|e| format!("读取会话协议上下文失败：{e}"))?;
        let mut reader = BufReader::new(file);
        let mut consumed = 0u64;
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .map_err(|e| format!("读取会话协议上下文失败：{e}"))?;
            if read == 0 {
                break;
            }
            consumed = consumed.saturating_add(read as u64);
            if segment.end_byte_offset.is_some_and(|end| consumed > end) {
                break;
            }
            if line.trim().is_empty() {
                continue;
            }
            let value = serde_json::from_str(&line)
                .map_err(|e| format!("会话协议上下文无效，未迁移：{e}"))?;
            history.observe(value);
        }
    }
    let Some(snapshot) = history.snapshot() else {
        return Ok(None);
    };
    let mut recovery = super::LocalRecoveryContext::default();
    recovery.observe(&snapshot);
    let reduced = recovery.build_compaction();
    let was_reduced = reduced.is_some();
    super::recovery_context::append_compaction(content, reduced.unwrap_or(snapshot))
        .map(|content| Some((content, was_reduced)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionStore;
    use std::fs;
    use tempfile::tempdir;

    fn lines(records: &[Value]) -> String {
        records.iter().map(|value| format!("{value}\n")).collect()
    }

    #[test]
    fn account_snapshot_removes_valid_replay_ids_without_changing_tool_links() {
        let mut history = PortableHistory::default();
        history.observe(json!({"type":"compacted","payload":{"replacement_history":[
            {"type":"message","id":"msg_valid","role":"user","content":[{"type":"input_text","text":"keep"}]},
            {"type":"function_call","id":"fc_valid","call_id":"call_keep","name":"shell","arguments":"{}"},
            {"type":"function_call_output","id":"fco_valid","call_id":"call_keep","output":"result"}
        ]}}));
        let snapshot = history.snapshot().unwrap();
        let items = snapshot["payload"]["replacement_history"]
            .as_array()
            .unwrap();
        assert!(items.iter().all(|item| item.get("id").is_none()));
        assert_eq!(items[0]["content"][0]["text"], "keep");
        assert_eq!(items[1]["call_id"], "call_keep");
        assert_eq!(items[2]["call_id"], "call_keep");
        assert_eq!(items[2]["output"], "result");
    }

    #[test]
    fn migration_preserves_full_messages_images_and_function_pairs() {
        let mut history = PortableHistory {
            api: true,
            ..Default::default()
        };
        let text = "完整正文".repeat(5000);
        let message = json!({"type":"message","role":"user","content":[{"type":"input_text","text":text},{"type":"input_image","image_url":"data:image/png;base64,KEEP"}]});
        for item in [
            message.clone(),
            json!({"type":"additional_tools","tools":[]}),
            json!({"type":"function_call","id":"fc_old","call_id":"call_keep","name":"shell","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"call_keep","output":"FULL_OUTPUT"}),
            json!({"type":"custom_tool_call","call_id":"custom_keep","name":"apply_patch","input":"FULL_PATCH"}),
            json!({"type":"custom_tool_call_output","call_id":"custom_keep","output":"PATCH_RESULT"}),
            json!({"type":"item_reference","id":"msg_remote"}),
        ] {
            history.push(item);
        }
        let snapshot = history.snapshot().unwrap();
        let items = snapshot["payload"]["replacement_history"]
            .as_array()
            .unwrap();
        assert_eq!(items[0], message);
        assert_eq!(items[1]["call_id"], "call_keep");
        assert_eq!(items[2]["output"], "FULL_OUTPUT");
        assert!(items[1].get("id").is_none());
        assert!(
            snapshot.to_string().contains("FULL_PATCH")
                && snapshot.to_string().contains("PATCH_RESULT")
        );
        assert!(!snapshot.to_string().contains("msg_remote"));
        let mut replay = PortableHistory::default();
        replay.observe(snapshot);
        assert!(replay.snapshot().is_none());
    }

    #[test]
    fn final_compaction_supersedes_incompatible_old_items_and_unpaired_calls_are_quoted() {
        let mut history = PortableHistory::default();
        history.push(json!({"type":"additional_tools"}));
        history.observe(json!({"type":"compacted","payload":{"replacement_history":[historical_text("current context")]}}));
        assert!(history.snapshot().is_none());
        let mut history = PortableHistory::default();
        history.push(json!({"type":"web_search_call","id":"ws_old","result":"keep results"}));
        history.push(
            json!({"type":"function_call_output","call_id":"missing","output":"keep output"}),
        );
        let snapshot = history.snapshot().unwrap();
        assert!(snapshot["payload"]["replacement_history"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["type"] == "message"));
        assert!(
            snapshot.to_string().contains("keep results")
                && snapshot.to_string().contains("keep output")
        );
    }

    #[test]
    fn split_thread_migration_repairs_the_head_and_preserves_ancestor_cutoffs() {
        let home = tempdir().unwrap();
        let directory = home.path().join("sessions");
        fs::create_dir(&directory).unwrap();
        let id = "00000000-0000-0000-0000-000000000001";
        let physical = "00000000-0000-0000-0000-000000000002";
        let parent = directory.join(format!("rollout-{id}.jsonl"));
        let head = directory.join(format!("rollout-{id}_{physical}.jsonl"));
        let prefix = lines(&[
            json!({"type":"session_meta","ordinal":0,"payload":{"id":id,"history_mode":"paginated","model_provider":"old_api"}}),
            json!({"type":"response_item","ordinal":1,"payload":{"type":"additional_tools","tools":[]}}),
            json!({"type":"response_item","ordinal":2,"payload":historical_text("继承的完整任务")}),
        ]);
        // This previous repair is physically present, but outside the child's
        // retained prefix. It must never suppress repair of the actual head.
        let parent_content = prefix.clone()
            + &lines(&[
                json!({"type":"compacted","ordinal":3,"payload":{"message":"old repair","replacement_history":[historical_text("不可继承的旧修复")]}}),
            ]);
        fs::write(&parent, &parent_content).unwrap();
        fs::write(&head, lines(&[
            json!({"type":"session_meta","ordinal":3,"payload":{"id":id,"history_mode":"paginated","model_provider":"old_api","history_base":{"thread_id":id,"end_byte_offset":prefix.len(),"end_ordinal_exclusive":3}}}),
            json!({"type":"response_item","ordinal":4,"payload":historical_text("最新问题")}),
            json!({"type":"event_msg","ordinal":5,"payload":{"type":"thread_settings_applied","thread_settings":{"model":"old_api/Kimi-K3.1-Pro","model_provider_id":"old_api","reasoning_effort":"high","collaboration_mode":{"settings":{"model":"old_api/Kimi-K3.1-Pro","reasoning_effort":"high"}}}}}),
        ])).unwrap();
        let original_head = fs::read_to_string(&head).unwrap();
        fs::write(
            home.path().join("config.toml"),
            "model_provider = 'openai' # account\nmodel = 'account-model'\n",
        )
        .unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        assert_eq!(store.read_target_provider().unwrap(), "openai");
        let summary = store.repair_model_compatibility("openai").unwrap();
        assert_eq!(summary.rebuilt_protocol_context_count, 1);
        assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);
        assert!(fs::read(&parent).unwrap().starts_with(prefix.as_bytes()));
        let records: Vec<Value> = fs::read_to_string(&head)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records.last().unwrap()["ordinal"], 6);
        let snapshot = records.last().unwrap().to_string();
        assert!(snapshot.contains("继承的完整任务") && snapshot.contains("最新问题"));
        assert!(!snapshot.contains("不可继承的旧修复") && !snapshot.contains("additional_tools"));
        let settings = &records[2]["payload"]["thread_settings"];
        assert_eq!(settings["model"], "account-model");
        assert_eq!(settings["model_provider_id"], "openai");
        assert!(settings["reasoning_effort"].is_null());
        let repaired = fs::read_to_string(&head).unwrap();
        let repeated = store.repair_model_compatibility("openai").unwrap();
        assert_eq!(repeated.rebuilt_protocol_context_count, 0);
        assert_eq!(repaired, fs::read_to_string(&head).unwrap());
        assert!(summary.backup_dirs.iter().any(|dir| {
            fs::read_dir(dir).unwrap().flatten().any(|entry| {
                fs::read_to_string(entry.path()).ok().as_deref() == Some(&original_head)
            })
        }));
    }

    #[test]
    fn destination_catalog_replaces_api_only_models_in_all_resume_settings() {
        let home = tempdir().unwrap();
        fs::create_dir(home.path().join("sessions")).unwrap();
        let path = home.path().join("sessions/model.jsonl");
        fs::write(
            home.path().join("config.toml"),
            "model_provider = 'openai'\nmodel = 'Kimi-K3.1-Pro'\n",
        )
        .unwrap();
        fs::write(
            home.path().join("models_cache.json"),
            json!({"models":[{"slug":"account-model","priority":1,"visibility":"list"}]})
                .to_string(),
        )
        .unwrap();
        fs::write(&path, lines(&[
            json!({"type":"session_meta","payload":{"id":"model","model_provider":"api"}}),
            json!({"type":"compacted","payload":{"message":"summary","replacement_history":[historical_text("完整摘要")],"resume_metadata":{"previous_turn_settings":{"model":"api/Kimi-K3.1-Pro","model_provider_id":"api","reasoning_effort":"xhigh","collaboration_mode":{"settings":{"model":"api/Kimi-K3.1-Pro","reasoning_effort":"xhigh"}}}}}}),
            json!({"type":"turn_context","payload":{"model":"Kimi-K3.1-Pro","effort":"xhigh"}}),
        ])).unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        store.repair_model_compatibility("openai").unwrap();
        let records: Vec<Value> = fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let restored = &records[1]["payload"]["resume_metadata"]["previous_turn_settings"];
        assert_eq!(restored["model"], "account-model");
        assert_eq!(restored["model_provider_id"], "openai");
        assert_eq!(
            restored["collaboration_mode"]["settings"]["model"],
            "account-model"
        );
        assert!(restored["reasoning_effort"].is_null());
        assert_eq!(records[2]["payload"]["model"], "account-model");
        assert!(records[2]["payload"]["effort"].is_null());
    }

    #[test]
    fn oversized_archived_protocol_context_is_bounded_and_original_is_backed_up() {
        let home = tempdir().unwrap();
        fs::create_dir(home.path().join("archived_sessions")).unwrap();
        let path = home.path().join("archived_sessions/archived.jsonl");
        fs::write(
            home.path().join("config.toml"),
            "model_provider = 'api'\nmodel = 'api-model'\n",
        )
        .unwrap();
        let original = lines(&[
            json!({"type":"session_meta","payload":{"id":"archived","model_provider":"openai"}}),
            json!({"type":"response_item","payload":{"type":"additional_tools","tools":[]}}),
            json!({"type":"response_item","payload":historical_text(&"large history ".repeat(100000))}),
            json!({"type":"response_item","payload":historical_text("最新的问题")}),
        ]);
        fs::write(&path, &original).unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        let summary = store.repair_model_compatibility("api").unwrap();
        assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);
        assert_eq!(summary.rebuilt_protocol_context_count, 1);
        assert_eq!(summary.reduced_protocol_context_count, 1);
        let repaired = fs::read_to_string(&path).unwrap();
        let tail = repaired.lines().last().unwrap();
        assert!(tail.len() <= super::super::LOCAL_RECOVERY_CONTEXT_MAX_BYTES);
        assert!(tail.contains("最新的问题"));
        assert!(!tail.contains("additional_tools"));
        assert!(repaired.contains(&"large history ".repeat(100000)));
        assert!(summary.backup_dirs.iter().any(|dir| fs::read_dir(dir)
            .unwrap()
            .flatten()
            .any(|entry| fs::read_to_string(entry.path()).ok().as_deref() == Some(&original))));
        assert_eq!(
            store
                .repair_model_compatibility("api")
                .unwrap()
                .rebuilt_protocol_context_count,
            0
        );
    }

    #[test]
    fn single_session_context_recovery_updates_tool_mode_and_resume_model() {
        let home = tempdir().unwrap();
        fs::create_dir(home.path().join("sessions")).unwrap();
        let path = home.path().join("sessions/target.jsonl");
        fs::write(
            home.path().join("config.toml"),
            "model_provider = 'api'\nmodel = 'api-model'\n[features]\nincremental_tools = true\n",
        )
        .unwrap();
        fs::write(&path, lines(&[
            json!({"type":"session_meta","payload":{"id":"target","model_provider":"openai","cwd":home.path()}}),
            json!({"type":"turn_context","payload":{"model":"account-model","effort":"high"}}),
            json!({"type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"model":"account-model","model_provider_id":"openai","reasoning_effort":"high"}}}),
            json!({"type":"response_item","payload":{"type":"additional_tools","tools":[]}}),
            json!({"type":"response_item","payload":historical_text("保留任务")}),
        ])).unwrap();
        let summary = SessionStore::new(home.path().to_path_buf())
            .repair_visibility_with_options(
                Some("context"),
                None,
                None,
                None,
                Some(vec!["target".into()]),
            )
            .unwrap();
        assert_eq!(summary.recovered_context_count, 1);
        let config = fs::read_to_string(home.path().join("config.toml"))
            .unwrap()
            .parse::<toml_edit::Document>()
            .unwrap();
        assert_eq!(
            config["features"]["incremental_tools"].as_bool(),
            Some(false)
        );
        let records: Vec<Value> = fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let settings = &records[2]["payload"]["thread_settings"];
        assert_eq!(settings["model"], "api/api-model");
        assert_eq!(settings["model_provider_id"], "api");
        assert!(settings["reasoning_effort"].is_null());
        assert!(!records
            .last()
            .unwrap()
            .to_string()
            .contains("additional_tools"));
    }

    #[test]
    #[ignore = "requires an isolated local fixture; never run against the live Codex home"]
    fn repair_isolated_real_session_fixture() {
        let fixture =
            std::path::PathBuf::from(std::env::var("CODEX_SWITCHER_REPAIR_FIXTURE_HOME").unwrap())
                .canonicalize()
                .unwrap();
        assert!(
            fixture.starts_with(std::env::temp_dir().canonicalize().unwrap())
                || fixture.starts_with(std::path::Path::new("/tmp").canonicalize().unwrap())
        );
        assert!(fixture
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("codex-switcher-session-check-"));
        let store = SessionStore::new(fixture);
        if std::env::var_os("CODEX_SWITCHER_REPAIR_FIXTURE_CONTEXT").is_some() {
            let sessions = store.list_sessions(None, None).unwrap();
            assert_eq!(
                sessions.len(),
                1,
                "the isolated fixture must contain exactly one logical thread"
            );
            let summary = store
                .repair_visibility_with_options(
                    Some("context"),
                    None,
                    None,
                    None,
                    Some(vec![sessions[0].id.clone()]),
                )
                .unwrap();
            assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);
            println!(
                "isolated real fixture: {} single-session contexts rebuilt",
                summary.recovered_context_count
            );
            return;
        }
        let summary = store
            .repair_model_compatibility(&store.read_target_provider().unwrap())
            .unwrap();
        assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);
        println!(
            "isolated real fixture: {} protocol contexts migrated, {} rollout files repaired",
            summary.rebuilt_protocol_context_count, summary.repaired_rollout_file_count
        );
    }
}
