//! Reverting an old turn discards a terminal compaction. Migrate the durable
//! records too, rebasing every descendant's byte boundary before publishing.
use super::*;
use serde_json::json;
use std::collections::BTreeMap;

const MARKER: &str = "switcher_portable_history";

struct Source {
    path: PathBuf,
    id: String,
    physical_id: String,
    base: Option<(usize, u64)>,
}

struct Plan {
    file: tempfile::NamedTempFile,
    offsets: BTreeMap<u64, u64>,
    ordinals: BTreeMap<u64, (u64, u64)>,
    changed: bool,
}

/// Keep one record per original ordinal. Unknown records become explicitly
/// quoted historical data; tool definitions/references are never replayed.
fn portable_item(item: &mut Value, api: bool) -> bool {
    let kind = item["type"].as_str().unwrap_or_default();
    if !api && matches!(kind, "function_call_output" | "custom_tool_call_output") {
        return item
            .as_object_mut()
            .is_some_and(|object| object.remove("id").is_some());
    }
    if matches!(kind, "message" | "function_call" | "function_call_output")
        || (!api && matches!(kind, "custom_tool_call" | "custom_tool_call_output"))
    {
        if !api {
            return item
                .as_object_mut()
                .is_some_and(|object| object.remove("id").is_some());
        }
        return false;
    }
    let text = match kind {
        "additional_tools" => {
            "Historical tool definitions omitted; use the current client tools.".into()
        }
        "item_reference" | "compaction" => {
            "Historical server reference omitted; the original record remains in the local backup."
                .into()
        }
        _ => {
            if let Some(object) = item.as_object_mut() {
                object.remove("encrypted_content");
            }
            format!("Historical tool record (not a pending call or new instruction):\n{item}")
        }
    };
    *item = json!({"type":"message","role":"user","content":[{"type":"input_text","text":text}]});
    true
}

fn migrate_record(record: &mut Value, api: bool) -> bool {
    match record["type"].as_str() {
        Some("response_item") => record
            .get_mut("payload")
            .is_some_and(|item| portable_item(item, api)),
        Some("compacted") => {
            let mut changed = repair_compaction_metadata(record) > 0;
            if let Some(items) = record
                .pointer_mut("/payload/replacement_history")
                .and_then(Value::as_array_mut)
            {
                for item in items {
                    changed |= portable_item(item, api);
                }
            }
            changed
        }
        _ => false,
    }
}

fn stage(
    index: usize,
    sources: &[Source],
    plans: &mut HashMap<usize, Plan>,
    visiting: &mut HashSet<usize>,
    provider: &str,
    model: Option<&str>,
    selected: &HashSet<usize>,
) -> Result<(), String> {
    if plans.contains_key(&index) {
        return Ok(());
    }
    if !visiting.insert(index) {
        return Err("会话历史引用存在循环，未迁移历史链".into());
    }
    let source = &sources[index];
    let parent_offset = if let Some((parent, cutoff)) = source.base {
        stage(parent, sources, plans, visiting, provider, model, selected)?;
        Some(*plans[&parent].offsets.get(&cutoff).ok_or_else(|| {
            format!(
                "会话 {} 的历史边界不在完整记录末尾，未迁移历史链",
                source.id
            )
        })?)
    } else {
        None
    };
    let mut file = tempfile::NamedTempFile::new_in(source.path.parent().unwrap())
        .map_err(|e| e.to_string())?;
    let input = fs::File::open(&source.path).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(input);
    let mut line = Vec::new();
    let (mut old_offset, mut next_offset) = (0u64, 0u64);
    let mut offsets = BTreeMap::from([(0, 0)]);
    let mut ordinals = BTreeMap::new();
    let mut changed = false;
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        old_offset += read as u64;
        let mut rewritten = None;
        let mut ordinal = None;
        if let Ok(mut record) = serde_json::from_slice::<Value>(&line) {
            ordinal = record.get("ordinal").and_then(Value::as_u64);
            let enabled = selected.contains(&index)
                && (provider != "openai" || record.get(MARKER) == Some(&Value::Bool(true)));
            // Mark every record we migrate, so future account switches can also
            // restore settings inside a formerly immutable ancestor prefix.
            let mut record_changed = enabled && migrate_record(&mut record, provider != "openai");
            if enabled {
                let stats = rewrite_rollout_model_compatibility_value(&mut record, provider, model);
                record_changed |= stats.changed();
                // Encrypted reasoning has already become a plain historical message.
            }
            if record["type"] == "session_meta" {
                if let Some(offset) = parent_offset {
                    let pointer = if record.pointer("/payload/history_base").is_some() {
                        "/payload/history_base/end_byte_offset"
                    } else {
                        "/history_base/end_byte_offset"
                    };
                    if record.pointer(pointer).and_then(Value::as_u64) != Some(offset) {
                        *record.pointer_mut(pointer).ok_or("历史段缺少字节边界")? = json!(offset);
                        record_changed = true;
                    }
                }
            }
            if record_changed {
                record[MARKER] = json!(true);
                let mut output = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
                output.push(b'\n');
                rewritten = Some(output);
                changed = true;
            }
        }
        let output = rewritten.as_deref().unwrap_or(&line);
        file.write_all(output).map_err(|e| e.to_string())?;
        if let Some(ordinal) = ordinal {
            ordinals.insert(ordinal, (next_offset, next_offset + output.len() as u64));
        }
        next_offset += output.len() as u64;
        offsets.insert(old_offset, next_offset);
    }
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    visiting.remove(&index);
    plans.insert(
        index,
        Plan {
            file,
            offsets,
            ordinals,
            changed,
        },
    );
    Ok(())
}

fn copy_lineage_file_atomic(source: &Path, destination: &Path) -> Result<(), String> {
    let input = fs::File::open(source).map_err(|error| error.to_string())?;
    // Keep staged plans and backups intact. Publication and rollback both stream
    // into a second temporary file beside the destination before replacing it.
    crate::account::write_reader_atomic(destination, input)
}

fn publish_plans(
    sources: &[Source],
    plans: &HashMap<usize, Plan>,
    backups: &[(usize, PathBuf)],
) -> Result<(), String> {
    for (published, (index, _)) in backups.iter().enumerate() {
        let result = copy_lineage_file_atomic(plans[index].file.path(), &sources[*index].path);
        if let Err(error) = result {
            let mut failures = Vec::new();
            for (old, backup) in &backups[..published] {
                if let Err(error) = copy_lineage_file_atomic(backup, &sources[*old].path) {
                    failures.push(error);
                }
            }
            return Err(format!(
                "迁移历史链失败：{error}；回滚失败项：{}",
                failures.join("；")
            ));
        }
    }
    Ok(())
}

impl SessionStore {
    pub(super) fn migrate_portable_lineage(
        &self,
        provider: &str,
        model: Option<&str>,
        selected_ids: Option<&HashSet<String>>,
    ) -> Result<(Vec<SessionRepairRecord>, Vec<String>), String> {
        let mut paths = collect_jsonl_files(&self.sessions_dir())?;
        paths.extend(collect_jsonl_files(
            &self.codex_home.join("archived_sessions"),
        )?);
        let mut sources = Vec::new();
        let mut by_physical = HashMap::new();
        for path in paths {
            let Ok(meta) = read_session_meta_value(&path) else {
                continue;
            };
            let Some(id) = meta.pointer("/payload/id").and_then(Value::as_str) else {
                continue;
            };
            let physical = physical_rollout_id(&path, id);
            // Match the native lineage resolver when duplicate physical files exist.
            let len = fs::metadata(&path).map_err(|e| e.to_string())?.len();
            let index = sources.len();
            sources.push(Source {
                path,
                id: id.into(),
                physical_id: physical.clone(),
                base: None,
            });
            let entry = by_physical.entry(physical).or_insert((index, len));
            if len > entry.1 {
                *entry = (index, len);
            }
        }
        for index in 0..sources.len() {
            if let Some(base) = read_rollout_history_base(&sources[index].path) {
                let Some(&(parent, _)) = by_physical.get(&base.thread_id) else {
                    return Err(format!("缺少历史段 {}，未迁移历史链", base.thread_id));
                };
                sources[index].base = Some((parent, base.end_byte_offset));
            }
        }
        let mut plans = HashMap::new();
        let mut selected = HashSet::new();
        for (index, source) in sources.iter().enumerate() {
            if selected_ids.is_none_or(|ids| ids.contains(&source.id)) {
                let mut current = Some(index);
                while let Some(index) = current {
                    if !selected.insert(index) {
                        break;
                    }
                    current = sources[index].base.map(|(parent, _)| parent);
                }
            }
        }
        for index in 0..sources.len() {
            stage(
                index,
                &sources,
                &mut plans,
                &mut HashSet::new(),
                provider,
                model,
                &selected,
            )?;
        }
        // Stage and back up the entire graph first. On publication failure roll
        // back every written file, keeping all original cross-file offsets valid.
        let mut backups = Vec::new();
        for (index, source) in sources.iter().enumerate() {
            if plans[&index].changed {
                backups.push((
                    index,
                    self.backup_session_file(&source.path, "portable-lineage")?,
                ));
            }
        }
        let mut directories = Vec::new();
        let mut databases = Vec::new();
        if !backups.is_empty() {
            for path in self.thread_history_db_paths() {
                directories.push(backup_sqlite_file(&path)?);
                let connection = Connection::open(path).map_err(|e| e.to_string())?;
                connection
                    .busy_timeout(std::time::Duration::from_secs(5))
                    .map_err(|e| e.to_string())?;
                connection
                    .execute_batch("BEGIN IMMEDIATE")
                    .map_err(|e| e.to_string())?;
                for (index, _) in &backups {
                    rebase_projection(&connection, &sources[*index].physical_id, &plans[index])?;
                }
                databases.push(connection);
            }
        }
        publish_plans(&sources, &plans, &backups)?;
        for connection in databases {
            connection
                .execute_batch("COMMIT")
                .map_err(|e| format!("历史链已迁移，但分页位置提交失败，请使用备份恢复：{e}"))?;
        }
        let records = backups
            .iter()
            .map(|(index, backup)| {
                let source = &sources[*index];
                let _ = system_command::new("touch")
                    .arg("-r")
                    .arg(backup)
                    .arg(&source.path)
                    .output();
                SessionRepairRecord {
                    id: source.id.clone(),
                    path: source.path.clone(),
                    title: String::new(),
                    updated_at: 0,
                }
            })
            .collect();
        directories.extend(
            backups
                .iter()
                .filter_map(|(_, path)| path.parent())
                .map(|p| p.to_string_lossy().into_owned()),
        );
        Ok((records, directories))
    }
}

/// Preserve the inherited display rows. Only their physical byte positions
/// change; deleting ancestor projections would hide old messages and images.
fn rebase_projection(connection: &Connection, physical: &str, plan: &Plan) -> Result<(), String> {
    let offsets = &plan.offsets;
    let mapped = |old: Option<i64>| {
        old.and_then(|n| u64::try_from(n).ok())
            .and_then(|n| offsets.get(&n))
            .and_then(|n| i64::try_from(*n).ok())
            .or(old)
    };
    let ordinal_offset = |ordinal: Option<i64>, end: bool| {
        ordinal
            .and_then(|n| u64::try_from(n).ok())
            .and_then(|n| plan.ordinals.get(&n))
            .and_then(|(start, finish)| i64::try_from(if end { *finish } else { *start }).ok())
    };
    let columns = sqlite_table_columns_with_connection(connection, "thread_turns")?;
    if [
        "thread_id",
        "turn_id",
        "rollout_byte_offset",
        "rollout_end_byte_offset",
    ]
    .iter()
    .all(|c| columns.contains(*c))
    {
        let start_ordinal = if columns.contains("rollout_ordinal") {
            "rollout_ordinal"
        } else {
            "NULL"
        };
        let end_ordinal = if columns.contains("rollout_end_ordinal") {
            "rollout_end_ordinal"
        } else {
            "NULL"
        };
        let mut query = connection.prepare(&format!("SELECT turn_id, rollout_byte_offset, rollout_end_byte_offset, {start_ordinal}, {end_ordinal} FROM thread_turns WHERE thread_id=?1")).map_err(|e| e.to_string())?;
        let rows = query
            .query_map(params![physical], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (turn, start, end, start_ordinal, end_ordinal) in rows {
            connection.execute("UPDATE thread_turns SET rollout_byte_offset=?1, rollout_end_byte_offset=?2 WHERE thread_id=?3 AND turn_id=?4", params![ordinal_offset(start_ordinal, false).or_else(|| mapped(start)), ordinal_offset(end_ordinal, true).or_else(|| mapped(end)), physical, turn]).map_err(|e| e.to_string())?;
        }
    }
    let columns =
        sqlite_table_columns_with_connection(connection, "thread_history_projection_state")?;
    if ["thread_id", "next_rollout_byte_offset"]
        .iter()
        .all(|c| columns.contains(*c))
    {
        let ordinal_column = if columns.contains("next_rollout_ordinal") {
            "next_rollout_ordinal"
        } else {
            "NULL"
        };
        let old = connection.query_row(&format!("SELECT next_rollout_byte_offset, {ordinal_column} FROM thread_history_projection_state WHERE thread_id=?1"), params![physical], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)))
            .optional().map_err(|e| e.to_string())?;
        if let Some((old, ordinal)) = old {
            let next = ordinal_offset(ordinal, false)
                .or_else(|| {
                    ordinal
                        .and_then(|n| n.checked_sub(1))
                        .and_then(|n| ordinal_offset(Some(n), true))
                })
                .or_else(|| mapped(Some(old)))
                .unwrap_or(old);
            connection.execute("UPDATE thread_history_projection_state SET next_rollout_byte_offset=?1 WHERE thread_id=?2", params![next, physical]).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_copy_streams_large_staging_file_and_keeps_it_available() {
        let home = tempfile::tempdir().unwrap();
        let mut staged = tempfile::NamedTempFile::new_in(home.path()).unwrap();
        let block: Vec<u8> = (0..64 * 1024).map(|index| (index % 251) as u8).collect();
        for _ in 0..128 {
            staged.write_all(&block).unwrap();
        }
        staged.as_file().sync_all().unwrap();
        let destination = home.path().join("published.jsonl");
        fs::write(&destination, "previous contents").unwrap();
        copy_lineage_file_atomic(staged.path(), &destination).unwrap();
        let expected_length = (block.len() * 128) as u64;
        assert_eq!(fs::metadata(staged.path()).unwrap().len(), expected_length);
        assert_eq!(fs::metadata(&destination).unwrap().len(), expected_length);
        let mut input = fs::File::open(&destination).unwrap();
        let mut actual = vec![0; block.len()];
        for _ in 0..128 {
            input.read_exact(&mut actual).unwrap();
            assert_eq!(actual, block);
        }
        assert_eq!(input.read(&mut actual).unwrap(), 0);
        // Publishing must not consume the plan: another destination can still
        // use it after a later file in the graph fails and rolls back.
        let retry = home.path().join("retry.jsonl");
        copy_lineage_file_atomic(staged.path(), &retry).unwrap();
        assert_eq!(fs::metadata(retry).unwrap().len(), expected_length);
    }

    #[test]
    fn failed_graph_publication_restores_prior_files_and_retains_plans_and_backups() {
        let home = tempfile::tempdir().unwrap();
        let first = home.path().join("parent.jsonl");
        fs::write(&first, "original parent").unwrap();
        let blocked = home.path().join("child.jsonl");
        fs::create_dir(&blocked).unwrap();
        fs::write(blocked.join("keep"), "keep directory").unwrap();
        let sources: Vec<Source> = [first.clone(), blocked.clone()]
            .into_iter()
            .enumerate()
            .map(|(index, path)| Source {
                path,
                id: index.to_string(),
                physical_id: index.to_string(),
                base: None,
            })
            .collect();
        let mut plans = HashMap::new();
        let mut backups = Vec::new();
        for (index, original) in ["original parent", "original child"]
            .into_iter()
            .enumerate()
        {
            let mut file = tempfile::NamedTempFile::new_in(home.path()).unwrap();
            file.write_all(format!("migrated {index}").as_bytes())
                .unwrap();
            file.as_file().sync_all().unwrap();
            plans.insert(
                index,
                Plan {
                    file,
                    offsets: BTreeMap::new(),
                    ordinals: BTreeMap::new(),
                    changed: true,
                },
            );
            let backup = home.path().join(format!("backup-{index}.jsonl"));
            fs::write(&backup, original).unwrap();
            backups.push((index, backup));
        }
        let files_before = fs::read_dir(home.path()).unwrap().count();
        let error = publish_plans(&sources, &plans, &backups).unwrap_err();
        assert!(error.contains("迁移历史链失败"));
        assert!(error.ends_with("回滚失败项："), "{error}");
        assert_eq!(fs::read_to_string(first).unwrap(), "original parent");
        assert_eq!(
            fs::read_to_string(blocked.join("keep")).unwrap(),
            "keep directory"
        );
        for (index, backup) in &backups {
            assert_eq!(
                fs::read_to_string(plans[index].file.path()).unwrap(),
                format!("migrated {index}")
            );
            assert_eq!(
                fs::read_to_string(backup).unwrap(),
                if *index == 0 {
                    "original parent"
                } else {
                    "original child"
                }
            );
        }
        assert_eq!(fs::read_dir(home.path()).unwrap().count(), files_before);
    }

    fn write(path: &Path, records: &[Value]) -> Vec<u64> {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut text = String::new();
        let mut ends = vec![0];
        for record in records {
            text.push_str(&format!("{record}\n"));
            ends.push(text.len() as u64);
        }
        fs::write(path, text).unwrap();
        ends
    }

    fn records(path: &Path) -> Vec<Value> {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn ancestor(path: &Path) -> Vec<u64> {
        write(
            path,
            &[
                json!({"type":"session_meta","ordinal":0,"payload":{"id":"parent","model_provider":"openai"}}),
                json!({"type":"turn_context","ordinal":1,"payload":{"model":"gpt-old","effort":"high"}}),
                json!({"type":"response_item","ordinal":2,"payload":{"type":"additional_tools","tools":[]}}),
                json!({"type":"response_item","ordinal":3,"payload":{"type":"message","role":"user","content":[{"type":"input_image","image_url":"data:image/png;base64,KEEP"}]}}),
                json!({"type":"response_item","ordinal":4,"payload":{"type":"custom_tool_call","call_id":"custom","name":"apply_patch","input":"original patch"}}),
            ],
        )
    }

    fn child(path: &Path, id: &str, cutoff: u64, ordinal: u64) {
        write(
            path,
            &[
                json!({"type":"session_meta","ordinal":ordinal,"payload":{"id":id,"history_base":{"thread_id":"parent","end_byte_offset":cutoff,"end_ordinal_exclusive":ordinal}}}),
                json!({"type":"response_item","ordinal":ordinal+1,"payload":{"type":"custom_tool_call_output","call_id":"custom","output":"done"}}),
            ],
        );
    }

    #[test]
    fn rebases_different_fork_cutoffs_and_preserves_projection_images_and_ordinals() {
        let home = tempfile::tempdir().unwrap();
        let parent = home.path().join("archived_sessions/parent.jsonl");
        let first = home.path().join("sessions/first.jsonl");
        let second = home.path().join("sessions/second.jsonl");
        let original_ends = ancestor(&parent);
        let original = records(&parent);
        child(&first, "first", original_ends[3], 3);
        child(&second, "second", original_ends[5], 5);
        let connection = Connection::open(home.path().join("thread_history_1.sqlite")).unwrap();
        connection.execute_batch("CREATE TABLE thread_turns(thread_id TEXT,turn_id TEXT,rollout_byte_offset INTEGER,rollout_end_byte_offset INTEGER,rollout_ordinal INTEGER,rollout_end_ordinal INTEGER,PRIMARY KEY(thread_id,turn_id)) WITHOUT ROWID;
            CREATE TABLE thread_history_projection_state(thread_id TEXT PRIMARY KEY,next_rollout_byte_offset INTEGER,next_rollout_ordinal INTEGER);
            CREATE TABLE thread_items(thread_id TEXT,item_json TEXT);").unwrap();
        // Reproduce byte positions left stale by an older repair. Ordinals are
        // authoritative even when the stored cursor now exceeds the file size.
        connection
            .execute(
                "INSERT INTO thread_turns VALUES('parent','turn',?1,?2,2,4)",
                params![original_ends[2] + 9000, original_ends[5] + 9000],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO thread_history_projection_state VALUES('parent',?1,5)",
                params![original_ends[5] + 9000],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO thread_items VALUES('parent',?1)",
                params![original[3].to_string()],
            )
            .unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        let (changed, backups) = store
            .migrate_portable_lineage("api", Some("DeepSeek"), None)
            .unwrap();
        assert_eq!(changed.len(), 3);
        assert!(!backups.is_empty());
        let migrated = records(&parent);
        assert_eq!(migrated.len(), original.len());
        for (before, after) in original.iter().zip(&migrated) {
            assert_eq!(before["ordinal"], after["ordinal"]);
        }
        assert_eq!(migrated[3]["payload"], original[3]["payload"]);
        assert_eq!(migrated[2]["payload"]["type"], "message");
        assert!(migrated[4]["payload"]
            .to_string()
            .contains("original patch"));
        let text = fs::read_to_string(&parent).unwrap();
        let mut ends = vec![0u64];
        for line in text.split_inclusive('\n') {
            ends.push(ends.last().unwrap() + line.len() as u64);
        }
        assert_eq!(
            read_rollout_history_base(&first).unwrap().end_byte_offset,
            ends[3]
        );
        assert_eq!(
            read_rollout_history_base(&second).unwrap().end_byte_offset,
            ends[5]
        );
        let positions = connection
            .query_row(
                "SELECT rollout_byte_offset,rollout_end_byte_offset FROM thread_turns",
                [],
                |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)),
            )
            .unwrap();
        assert_eq!(positions, (ends[2], ends[5]));
        let position = connection.query_row("SELECT next_rollout_byte_offset,next_rollout_ordinal FROM thread_history_projection_state", [], |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?))).unwrap();
        assert_eq!(position, (ends[5], 5));
        assert_eq!(
            connection
                .query_row("SELECT item_json FROM thread_items", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            original[3].to_string()
        );
        assert!(store
            .migrate_portable_lineage("api", Some("DeepSeek"), None)
            .unwrap()
            .0
            .is_empty());
        store
            .migrate_portable_lineage("openai", Some("gpt-new"), None)
            .unwrap();
        assert_eq!(records(&parent)[1]["payload"]["model"], "gpt-new");
        assert_eq!(records(&parent)[0]["payload"]["model_provider"], "openai");
        store
            .migrate_portable_lineage("api", Some("DeepSeek"), None)
            .unwrap();
        assert_eq!(records(&parent)[1]["payload"]["model"], "api/DeepSeek");
    }

    #[test]
    fn single_session_migrates_ancestors_and_rebases_siblings_without_changing_sibling_tools() {
        let home = tempfile::tempdir().unwrap();
        let parent = home.path().join("sessions/parent.jsonl");
        let selected = home.path().join("sessions/selected.jsonl");
        let sibling = home.path().join("sessions/sibling.jsonl");
        let ends = ancestor(&parent);
        child(&selected, "selected", ends[5], 5);
        child(&sibling, "sibling", ends[3], 3);
        let sibling_tools = records(&sibling)[1].clone();
        let unrelated = home.path().join("sessions/unrelated.jsonl");
        child(&unrelated, "unrelated", ends[5], 5);
        let store = SessionStore::new(home.path().to_path_buf());
        store
            .migrate_portable_lineage(
                "api",
                Some("DeepSeek"),
                Some(&HashSet::from(["selected".into()])),
            )
            .unwrap();
        assert_eq!(records(&selected)[1]["payload"]["type"], "message");
        assert_eq!(records(&sibling)[1], sibling_tools);
        assert_eq!(
            records(&unrelated)[1]["payload"]["type"],
            "custom_tool_call_output"
        );
        assert_ne!(
            read_rollout_history_base(&sibling).unwrap().end_byte_offset,
            ends[3]
        );
    }

    #[test]
    fn rejects_invalid_boundary_before_publishing_any_file() {
        let home = tempfile::tempdir().unwrap();
        let parent = home.path().join("sessions/parent.jsonl");
        let bad = home.path().join("sessions/bad.jsonl");
        let ends = ancestor(&parent);
        child(&bad, "bad", ends[3] - 1, 3);
        let original = fs::read(&parent).unwrap();
        let original_bad = fs::read(&bad).unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        assert!(store
            .migrate_portable_lineage("api", None, None)
            .unwrap_err()
            .contains("完整记录末尾"));
        assert_eq!(fs::read(parent).unwrap(), original);
        assert_eq!(fs::read(bad).unwrap(), original_bad);
    }

    #[test]
    fn compacted_records_keep_positional_metadata_and_remove_opaque_input() {
        let mut record = json!({"type":"compacted","payload":{
            "replacement_history":[{"type":"reasoning","encrypted_content":"secret","summary":[{"text":"summary"}]},{"type":"custom_tool_call","input":"patch"}],
            "replacement_history_metadata":[{"client_authored":false},{"client_authored":true}]
        }});
        let metadata = record["payload"]["replacement_history_metadata"].clone();
        assert!(migrate_record(&mut record, true));
        assert_eq!(record["payload"]["replacement_history_metadata"], metadata);
        assert!(!record.to_string().contains("secret"));
        assert!(record["payload"]["replacement_history"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["type"] == "message"));
    }
}
