use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionRecord {
    pub id: String,
    pub title: String,
    pub project_name: String,
    pub path: String,
    pub updated_at: i64,
    pub message_count: usize,
    pub char_count: usize,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionVisibilityRepairItem {
    pub instance_id: String,
    pub instance_name: String,
    pub target_provider: String,
    pub changed_rollout_file_count: usize,
    pub updated_sqlite_row_count: usize,
    pub updated_sqlite_timestamp_row_count: usize,
    pub added_session_index_entry_count: usize,
    pub updated_session_index_entry_count: usize,
    pub backup_dir: Option<String>,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexTrashedSessionRecord {
    pub id: String,
    pub title: String,
    pub original_path: String,
    pub trash_path: String,
    pub deleted_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionTokenStats {
    pub session_id: String,
    pub approximate_tokens: usize,
    pub char_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionTrashSummary {
    pub moved: usize,
    pub restored: usize,
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionVisibilityRepairSummary {
    pub scanned: usize,
    pub repaired: usize,
    pub instance_count: usize,
    pub mutated_instance_count: usize,
    pub changed_rollout_file_count: usize,
    pub updated_sqlite_row_count: usize,
    pub updated_sqlite_timestamp_row_count: usize,
    pub added_session_index_entry_count: usize,
    pub updated_session_index_entry_count: usize,
    pub backup_dirs: Vec<String>,
    pub items: Vec<CodexSessionVisibilityRepairItem>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionVisibilityRepairInstanceOption {
    pub id: String,
    pub name: String,
    pub user_data_dir: String,
    pub current_provider: String,
    pub is_default: bool,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionVisibilityRepairInstanceList {
    pub default_instance_id: String,
    pub instances: Vec<CodexSessionVisibilityRepairInstanceOption>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexSessionVisibilityRepairProviderSource {
    Config,
    Sqlite,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionVisibilityRepairProviderOption {
    pub id: String,
    pub sources: Vec<CodexSessionVisibilityRepairProviderSource>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionVisibilityRepairProviderList {
    pub default_provider: String,
    pub providers: Vec<CodexSessionVisibilityRepairProviderOption>,
}

#[derive(Debug, Clone)]
struct SessionRepairRecord {
    id: String,
    title: String,
    path: PathBuf,
    updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    codex_home: PathBuf,
}

impl Default for SessionStore {
    fn default() -> Self {
        let codex_home = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
            .unwrap_or_else(|| PathBuf::from(".codex"));
        Self::new(codex_home)
    }
}

impl SessionStore {
    pub fn new(codex_home: PathBuf) -> Self {
        Self { codex_home }
    }

    #[allow(dead_code)]
    pub fn repair_visibility(&self) -> Result<CodexSessionVisibilityRepairSummary, String> {
        self.repair_visibility_with_options(None, None, None, None, None)
    }

    pub fn list_sessions(
        &self,
        title_query: Option<String>,
        content_query: Option<String>,
    ) -> Result<Vec<CodexSessionRecord>, String> {
        let title_query = normalize_query(title_query);
        let content_query = normalize_query(content_query);
        let mut sessions = Vec::new();
        for path in collect_jsonl_files(&self.sessions_dir())? {
            let content = fs::read_to_string(&path)
                .map_err(|error| format!("读取会话失败 {}: {}", path.display(), error))?;
            let record = build_session_record(&path, &content)?;
            if let Some(query) = title_query.as_deref() {
                if !record.title.to_lowercase().contains(query) {
                    continue;
                }
            }
            if let Some(query) = content_query.as_deref() {
                if !content.to_lowercase().contains(query) {
                    continue;
                }
            }
            sessions.push(record);
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
        Ok(sessions)
    }

    pub fn token_stats(
        &self,
        session_ids: &[String],
    ) -> Result<Vec<CodexSessionTokenStats>, String> {
        let sessions = self.list_sessions(None, None)?;
        Ok(session_ids
            .iter()
            .filter_map(|id| sessions.iter().find(|session| &session.id == id))
            .map(|session| CodexSessionTokenStats {
                session_id: session.id.clone(),
                approximate_tokens: session.char_count.div_ceil(4),
                char_count: session.char_count,
            })
            .collect())
    }

    pub fn move_to_trash(
        &self,
        session_ids: &[String],
    ) -> Result<CodexSessionTrashSummary, String> {
        self.migrate_legacy_trash();
        let sessions = self.list_sessions(None, None)?;
        fs::create_dir_all(self.trash_dir())
            .map_err(|error| format!("创建回收站失败: {}", error))?;
        let mut moved = 0;
        let mut failed = Vec::new();
        for session_id in session_ids {
            let Some(session) = sessions.iter().find(|item| &item.id == session_id) else {
                failed.push(format!("会话不存在: {}", session_id));
                continue;
            };
            let source = PathBuf::from(&session.path);
            let trash_path = self.trash_dir().join(format!("{}.jsonl", session.id));
            let metadata_path = self.trash_dir().join(format!("{}.json", session.id));
            match fs::rename(&source, &trash_path) {
                Ok(()) => {
                    let metadata = serde_json::json!({
                        "id": session.id,
                        "title": session.title,
                        "originalPath": session.path,
                        "trashPath": trash_path.to_string_lossy(),
                        "deletedAt": now_timestamp()
                    });
                    let _ = fs::write(
                        metadata_path,
                        serde_json::to_string_pretty(&metadata).unwrap_or_default(),
                    );
                    moved += 1;
                }
                Err(error) => failed.push(format!("移动失败 {}: {}", session.id, error)),
            }
        }
        Ok(CodexSessionTrashSummary {
            moved,
            restored: 0,
            failed,
        })
    }

    pub fn list_trashed(&self) -> Result<Vec<CodexTrashedSessionRecord>, String> {
        self.migrate_legacy_trash();
        let mut records = Vec::new();
        let mut seen = HashSet::new();
        for dir in self.trash_dirs() {
            for path in collect_json_files(&dir)? {
                let content = fs::read_to_string(&path)
                    .map_err(|error| format!("读取回收站失败 {}: {}", path.display(), error))?;
                let value: Value = serde_json::from_str(&content)
                    .map_err(|error| format!("解析回收站失败 {}: {}", path.display(), error))?;
                let id = read_string(&value, "id").unwrap_or_else(|| file_stem(&path));
                if !seen.insert(id.clone()) {
                    continue;
                }
                let stored_trash_path = read_string(&value, "trashPath").unwrap_or_default();
                let trash_path =
                    if stored_trash_path.is_empty() || !Path::new(&stored_trash_path).exists() {
                        path.with_extension("jsonl").to_string_lossy().to_string()
                    } else {
                        stored_trash_path
                    };
                records.push(CodexTrashedSessionRecord {
                    id,
                    title: read_string(&value, "title").unwrap_or_else(|| "未命名会话".to_string()),
                    original_path: read_string(&value, "originalPath").unwrap_or_default(),
                    trash_path,
                    deleted_at: value
                        .get("deletedAt")
                        .and_then(Value::as_i64)
                        .unwrap_or_default(),
                });
            }
        }
        records.sort_by_key(|record| std::cmp::Reverse(record.deleted_at));
        Ok(records)
    }

    pub fn restore_from_trash(
        &self,
        session_ids: &[String],
    ) -> Result<CodexSessionTrashSummary, String> {
        let trashed = self.list_trashed()?;
        let mut restored = 0;
        let mut failed = Vec::new();
        for session_id in session_ids {
            let Some(record) = trashed.iter().find(|item| &item.id == session_id) else {
                failed.push(format!("回收站中不存在: {}", session_id));
                continue;
            };
            let trash_path = PathBuf::from(&record.trash_path);
            let original_path = PathBuf::from(&record.original_path);
            if let Some(parent) = original_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("创建恢复目录失败: {}", error))?;
            }
            match fs::rename(&trash_path, &original_path) {
                Ok(()) => {
                    let _ = fs::remove_file(trash_path.with_extension("json"));
                    restored += 1;
                }
                Err(error) => failed.push(format!("恢复失败 {}: {}", session_id, error)),
            }
        }
        Ok(CodexSessionTrashSummary {
            moved: 0,
            restored,
            failed,
        })
    }

    pub fn repair_visibility_with_options(
        &self,
        mode: Option<&str>,
        target_provider: Option<String>,
        target_instance_id: Option<String>,
        repair_instance_ids: Option<Vec<String>>,
        session_ids: Option<Vec<String>>,
    ) -> Result<CodexSessionVisibilityRepairSummary, String> {
        if !should_repair_default_instance(target_instance_id, repair_instance_ids) {
            return Ok(CodexSessionVisibilityRepairSummary {
                scanned: 0,
                repaired: 0,
                instance_count: 0,
                mutated_instance_count: 0,
                changed_rollout_file_count: 0,
                updated_sqlite_row_count: 0,
                updated_sqlite_timestamp_row_count: 0,
                added_session_index_entry_count: 0,
                updated_session_index_entry_count: 0,
                backup_dirs: Vec::new(),
                items: Vec::new(),
                message: "没有匹配到需要修复的 Codex 实例".to_string(),
            });
        }
        let all_sessions = self.list_sessions(None, None)?;
        let selected_ids = normalized_id_set(session_ids);
        let sessions = if let Some(ids) = selected_ids.as_ref() {
            all_sessions
                .iter()
                .filter(|session| ids.contains(&session.id))
                .cloned()
                .collect::<Vec<_>>()
        } else {
            all_sessions.clone()
        };
        let target_provider = target_provider
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or(self.read_target_provider()?);
        let repair_records = sessions
            .iter()
            .map(|session| SessionRepairRecord {
                id: session.id.clone(),
                title: session.title.clone(),
                path: PathBuf::from(&session.path),
                updated_at: session.updated_at,
            })
            .collect::<Vec<_>>();
        let deep = mode
            .map(|value| value.eq_ignore_ascii_case("deep"))
            .unwrap_or(false);
        let (changed_rollout_files, rollout_backup_dirs) =
            self.repair_rollout_visibility(&target_provider, &repair_records, deep)?;
        let (updated_rows, backup_dirs) =
            self.repair_sqlite_visibility(&target_provider, Some(&repair_records))?;
        let added_index_entries = if deep {
            self.repair_session_index(&repair_records)?
        } else {
            0
        };
        let repaired = changed_rollout_files + updated_rows + added_index_entries;
        let mut all_backup_dirs = rollout_backup_dirs;
        all_backup_dirs.extend(backup_dirs);
        all_backup_dirs.sort();
        all_backup_dirs.dedup();
        let repair_marker_path = visibility_repair_marker_path();
        if let Some(parent) = repair_marker_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("创建修复标记目录失败: {}", error))?;
        }
        fs::write(
            repair_marker_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "scanned": sessions.len(),
                "repaired": repaired,
                "targetProvider": target_provider,
                "mode": if deep { "deep" } else { "quick" },
                "updatedAt": now_timestamp()
            }))
            .unwrap_or_default(),
        )
        .map_err(|error| format!("写入修复标记失败: {}", error))?;
        Ok(CodexSessionVisibilityRepairSummary {
            scanned: sessions.len(),
            repaired,
            instance_count: 1,
            mutated_instance_count: usize::from(repaired > 0),
            changed_rollout_file_count: changed_rollout_files,
            updated_sqlite_row_count: updated_rows,
            updated_sqlite_timestamp_row_count: 0,
            added_session_index_entry_count: added_index_entries,
            updated_session_index_entry_count: 0,
            backup_dirs: all_backup_dirs.clone(),
            items: vec![CodexSessionVisibilityRepairItem {
                instance_id: "__default__".to_string(),
                instance_name: "默认实例".to_string(),
                target_provider: target_provider.clone(),
                changed_rollout_file_count: changed_rollout_files,
                updated_sqlite_row_count: updated_rows,
                updated_sqlite_timestamp_row_count: 0,
                added_session_index_entry_count: added_index_entries,
                updated_session_index_entry_count: 0,
                backup_dir: all_backup_dirs.first().cloned(),
                running: false,
            }],
            message: format!(
                "已为 1 个实例修复会话可见性：校正 {} 个会话文件，更新 {} 条 SQLite 可见性记录，校正 0 条 SQLite 时间记录",
                changed_rollout_files, updated_rows
            ),
        })
    }

    pub fn list_visibility_repair_instances(
        &self,
    ) -> Result<CodexSessionVisibilityRepairInstanceList, String> {
        Ok(CodexSessionVisibilityRepairInstanceList {
            default_instance_id: "__default__".to_string(),
            instances: vec![CodexSessionVisibilityRepairInstanceOption {
                id: "__default__".to_string(),
                name: "默认实例".to_string(),
                user_data_dir: self.codex_home.to_string_lossy().to_string(),
                current_provider: self.read_target_provider()?,
                is_default: true,
                running: false,
            }],
        })
    }

    pub fn list_visibility_repair_providers(
        &self,
    ) -> Result<CodexSessionVisibilityRepairProviderList, String> {
        let default_provider = self.read_target_provider()?;
        let mut providers: Vec<(String, Vec<CodexSessionVisibilityRepairProviderSource>)> = vec![(
            default_provider.clone(),
            vec![CodexSessionVisibilityRepairProviderSource::Config],
        )];
        if sqlite3_available() {
            for db_path in self.sqlite_candidate_paths() {
                for provider in sqlite_provider_ids(&db_path)? {
                    if let Some((_, sources)) = providers.iter_mut().find(|(id, _)| id == &provider)
                    {
                        if !sources.contains(&CodexSessionVisibilityRepairProviderSource::Sqlite) {
                            sources.push(CodexSessionVisibilityRepairProviderSource::Sqlite);
                        }
                    } else {
                        providers.push((
                            provider,
                            vec![CodexSessionVisibilityRepairProviderSource::Sqlite],
                        ));
                    }
                }
            }
        }
        providers.sort_by(|left, right| {
            (right.0 == default_provider)
                .cmp(&(left.0 == default_provider))
                .then_with(|| left.0.cmp(&right.0))
        });
        Ok(CodexSessionVisibilityRepairProviderList {
            default_provider: default_provider.clone(),
            providers: providers
                .into_iter()
                .map(|(id, sources)| CodexSessionVisibilityRepairProviderOption {
                    is_default: id == default_provider,
                    id,
                    sources,
                })
                .collect(),
        })
    }

    fn sessions_dir(&self) -> PathBuf {
        self.codex_home.join("sessions")
    }

    fn trash_dir(&self) -> PathBuf {
        session_trash_dir(&self.codex_home)
    }

    fn legacy_trash_dir(&self) -> PathBuf {
        self.codex_home
            .join(".codex-switcher")
            .join("session-trash")
    }

    fn trash_dirs(&self) -> Vec<PathBuf> {
        let primary = self.trash_dir();
        let legacy = self.legacy_trash_dir();
        if legacy == primary {
            vec![primary]
        } else {
            vec![primary, legacy]
        }
    }

    fn migrate_legacy_trash(&self) {
        migrate_directory_files(&self.legacy_trash_dir(), &self.trash_dir());
    }

    fn read_target_provider(&self) -> Result<String, String> {
        let config_path = self.codex_home.join("config.toml");
        if !config_path.exists() {
            return Ok("openai".to_string());
        }
        let content = fs::read_to_string(&config_path)
            .map_err(|error| format!("读取 Codex config.toml 失败: {}", error))?;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || !trimmed.starts_with("model_provider") {
                continue;
            }
            let Some((_, value)) = trimmed.split_once('=') else {
                continue;
            };
            let provider = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim()
                .to_string();
            if !provider.is_empty() {
                return Ok(provider);
            }
        }
        Ok("openai".to_string())
    }

    fn repair_sqlite_visibility(
        &self,
        target_provider: &str,
        sessions: Option<&[SessionRepairRecord]>,
    ) -> Result<(usize, Vec<String>), String> {
        if !sqlite3_available() {
            return Ok((0, Vec::new()));
        }
        let mut repaired = 0usize;
        let mut backup_dirs = Vec::new();
        for db_path in self.sqlite_candidate_paths() {
            let backup_dir = backup_sqlite_file(&db_path)?;
            let changed = repair_sqlite_db(&db_path, target_provider, sessions)?;
            if changed > 0 {
                backup_dirs.push(backup_dir);
                repaired += changed;
            }
        }
        Ok((repaired, backup_dirs))
    }

    fn repair_rollout_visibility(
        &self,
        target_provider: &str,
        sessions: &[SessionRepairRecord],
        rewrite_all_meta: bool,
    ) -> Result<(usize, Vec<String>), String> {
        if sessions.is_empty() {
            return Ok((0, Vec::new()));
        }
        let backup_dir = visibility_backup_dir();
        let mut changed = 0usize;
        let mut backup_created = false;
        for session in sessions {
            let content = match fs::read_to_string(&session.path) {
                Ok(content) => content,
                Err(_) => continue,
            };
            let Some(next) =
                rewrite_session_meta_provider(&content, target_provider, rewrite_all_meta)?
            else {
                continue;
            };
            if !backup_created {
                fs::create_dir_all(&backup_dir)
                    .map_err(|error| format!("创建会话文件备份目录失败: {}", error))?;
                backup_created = true;
            }
            let backup_path = backup_dir.join(
                session
                    .path
                    .file_name()
                    .and_then(|item| item.to_str())
                    .unwrap_or("session.jsonl"),
            );
            fs::copy(&session.path, &backup_path).map_err(|error| {
                format!("备份会话文件失败 ({}): {}", session.path.display(), error)
            })?;
            fs::write(&session.path, next).map_err(|error| {
                format!("写入会话文件失败 ({}): {}", session.path.display(), error)
            })?;
            let _ = Command::new("touch")
                .arg("-r")
                .arg(&backup_path)
                .arg(&session.path)
                .output();
            changed += 1;
        }
        let backup_dirs = if backup_created {
            vec![backup_dir.to_string_lossy().to_string()]
        } else {
            Vec::new()
        };
        Ok((changed, backup_dirs))
    }

    fn sqlite_candidate_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let sqlite_state_db = self.codex_home.join("sqlite").join("state_5.sqlite");
        if sqlite_state_db.exists() {
            paths.push(sqlite_state_db);
        }
        let state_db = self.codex_home.join("state_5.sqlite");
        if state_db.exists() {
            paths.push(state_db);
        }
        let sqlite_dir = self.codex_home.join("sqlite");
        if let Ok(entries) = fs::read_dir(sqlite_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|item| item.to_str()) else {
                    continue;
                };
                if path.is_file() && (name.ends_with(".db") || name.ends_with(".sqlite")) {
                    if paths.iter().any(|item| item == &path) {
                        continue;
                    }
                    paths.push(path);
                }
            }
        }
        paths
    }

    fn repair_session_index(&self, sessions: &[SessionRepairRecord]) -> Result<usize, String> {
        if sessions.is_empty() {
            return Ok(0);
        }
        let path = self.codex_home.join("session_index.jsonl");
        let existing = fs::read_to_string(&path).unwrap_or_default();
        let existing_ids = existing
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|value| read_string(&value, "id"))
            .collect::<HashSet<_>>();
        let mut additions = Vec::new();
        for session in sessions {
            if existing_ids.contains(&session.id) {
                continue;
            }
            let relative_path = session
                .path
                .strip_prefix(&self.codex_home)
                .unwrap_or(&session.path)
                .to_string_lossy()
                .to_string();
            additions.push(serde_json::json!({
                "id": session.id,
                "title": session.title,
                "rollout_path": relative_path,
                "updated_at": session.updated_at,
                "updated_at_ms": session.updated_at * 1000
            }));
        }
        if additions.is_empty() {
            return Ok(0);
        }
        let backup_dir = visibility_backup_dir();
        fs::create_dir_all(&backup_dir)
            .map_err(|error| format!("创建会话索引备份失败: {}", error))?;
        if path.exists() {
            let _ = fs::copy(&path, backup_dir.join("session_index.jsonl"));
        }
        let mut next = existing;
        if !next.is_empty() && !next.ends_with('\n') {
            next.push('\n');
        }
        for item in &additions {
            next.push_str(
                &serde_json::to_string(item)
                    .map_err(|error| format!("序列化 session_index 失败: {}", error))?,
            );
            next.push('\n');
        }
        fs::write(&path, next)
            .map_err(|error| format!("写入 session_index.jsonl 失败: {}", error))?;
        Ok(additions.len())
    }
}

fn repair_sqlite_db(
    db_path: &Path,
    target_provider: &str,
    sessions: Option<&[SessionRepairRecord]>,
) -> Result<usize, String> {
    if !db_path.exists() {
        return Ok(0);
    }
    let columns = sqlite_thread_columns(db_path)?;
    if !columns.contains("id") {
        return Ok(0);
    }
    let before = sqlite_count_repairable_rows(db_path, target_provider, sessions)?;
    if before == 0 {
        return sqlite_insert_missing_session_rows(db_path, target_provider, sessions);
    }
    let escaped_provider = sql_quote(target_provider);
    let set_clause = sqlite_repair_set_clause(&columns, &escaped_provider);
    let where_clause = sqlite_repair_where_clause(&columns, &escaped_provider, sessions);
    if set_clause.is_empty() || where_clause.is_empty() {
        return Ok(0);
    }
    let sql = format!("UPDATE threads SET {set_clause} WHERE {where_clause};");
    run_sqlite(db_path, &sql)?;
    let inserted = sqlite_insert_missing_session_rows(db_path, target_provider, sessions)?;
    Ok(before + inserted)
}

fn sqlite_count_repairable_rows(
    db_path: &Path,
    target_provider: &str,
    sessions: Option<&[SessionRepairRecord]>,
) -> Result<usize, String> {
    let columns = sqlite_thread_columns(db_path)?;
    if !columns.contains("id") {
        return Ok(0);
    }
    let escaped_provider = sql_quote(target_provider);
    let where_clause = sqlite_repair_where_clause(&columns, &escaped_provider, sessions);
    if where_clause.is_empty() {
        return Ok(0);
    }
    let sql = format!("SELECT COUNT(*) FROM threads WHERE {where_clause};");
    let output = run_sqlite(db_path, &sql)?;
    Ok(output.trim().parse::<usize>().unwrap_or(0))
}

fn sqlite_thread_columns(db_path: &Path) -> Result<HashSet<String>, String> {
    let table_exists = run_sqlite(
        db_path,
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='threads';",
    )?;
    if table_exists.trim() != "1" {
        return Ok(HashSet::new());
    }
    let output = run_sqlite(db_path, "SELECT name FROM pragma_table_info('threads');")?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn sqlite_repair_set_clause(columns: &HashSet<String>, escaped_provider: &str) -> String {
    let mut assignments = Vec::new();
    if columns.contains("model_provider") {
        assignments.push(format!("model_provider = {escaped_provider}"));
    }
    if columns.contains("has_user_event") && columns.contains("first_user_message") {
        assignments.push(
            "has_user_event = CASE WHEN COALESCE(first_user_message, '') <> '' THEN 1 ELSE has_user_event END".to_string(),
        );
    }
    if columns.contains("thread_source") && columns.contains("first_user_message") {
        assignments.push(
            "thread_source = CASE WHEN COALESCE(thread_source, '') = '' AND COALESCE(first_user_message, '') <> '' THEN 'user' ELSE thread_source END".to_string(),
        );
    }
    assignments.join(", ")
}

fn sqlite_repair_where_clause(
    columns: &HashSet<String>,
    escaped_provider: &str,
    sessions: Option<&[SessionRepairRecord]>,
) -> String {
    let mut predicates = Vec::new();
    if columns.contains("model_provider") {
        predicates.push(format!(
            "COALESCE(model_provider, '') <> {escaped_provider}"
        ));
    }
    if columns.contains("has_user_event") && columns.contains("first_user_message") {
        predicates.push(
            "(COALESCE(first_user_message, '') <> '' AND COALESCE(has_user_event, 0) <> 1)"
                .to_string(),
        );
    }
    if columns.contains("thread_source") && columns.contains("first_user_message") {
        predicates.push(
            "(COALESCE(first_user_message, '') <> '' AND COALESCE(thread_source, '') = '')"
                .to_string(),
        );
    }
    let base = predicates.join(" OR ");
    let Some(sessions) = sessions else {
        return base;
    };
    let ids = sessions
        .iter()
        .map(|session| sql_quote(&session.id))
        .collect::<Vec<_>>();
    if ids.is_empty() || base.is_empty() {
        return base;
    }
    format!("({base}) AND id IN ({})", ids.join(", "))
}

fn sqlite_insert_missing_session_rows(
    db_path: &Path,
    target_provider: &str,
    sessions: Option<&[SessionRepairRecord]>,
) -> Result<usize, String> {
    let Some(sessions) = sessions else {
        return Ok(0);
    };
    if sessions.is_empty() {
        return Ok(0);
    }
    let columns = sqlite_thread_columns(db_path)?;
    if !columns.contains("id") {
        return Ok(0);
    }
    let mut inserted = 0usize;
    for session in sessions {
        let exists_sql = format!(
            "SELECT COUNT(*) FROM threads WHERE id = {};",
            sql_quote(&session.id)
        );
        let exists = run_sqlite(db_path, &exists_sql)?
            .trim()
            .parse::<usize>()
            .unwrap_or(0);
        if exists > 0 {
            continue;
        }
        let metadata = sqlite_metadata_for_session(session);
        let mut names = Vec::new();
        let mut values = Vec::new();
        push_sql_value(&mut names, &mut values, &columns, "id", &session.id);
        push_sql_value(&mut names, &mut values, &columns, "title", &session.title);
        push_sql_value(
            &mut names,
            &mut values,
            &columns,
            "model_provider",
            target_provider,
        );
        push_sql_value(
            &mut names,
            &mut values,
            &columns,
            "first_user_message",
            &session.title,
        );
        push_sql_value(&mut names, &mut values, &columns, "preview", &session.title);
        push_sql_value(
            &mut names,
            &mut values,
            &columns,
            "source",
            &metadata.source,
        );
        push_sql_value(&mut names, &mut values, &columns, "cwd", &metadata.cwd);
        push_sql_value(
            &mut names,
            &mut values,
            &columns,
            "cli_version",
            &metadata.cli_version,
        );
        push_sql_value(
            &mut names,
            &mut values,
            &columns,
            "sandbox_policy",
            &metadata.sandbox_policy,
        );
        push_sql_value(
            &mut names,
            &mut values,
            &columns,
            "approval_mode",
            &metadata.approval_mode,
        );
        push_sql_value(&mut names, &mut values, &columns, "history_mode", "legacy");
        push_sql_value(&mut names, &mut values, &columns, "thread_source", "user");
        if columns.contains("has_user_event") {
            names.push("has_user_event".to_string());
            values.push("1".to_string());
        }
        push_sql_i64(
            &mut names,
            &mut values,
            &columns,
            "created_at",
            metadata.created_at,
        );
        push_sql_i64(
            &mut names,
            &mut values,
            &columns,
            "created_at_ms",
            metadata.created_at.saturating_mul(1000),
        );
        if columns.contains("updated_at") {
            names.push("updated_at".to_string());
            values.push(session.updated_at.to_string());
        }
        if columns.contains("updated_at_ms") {
            names.push("updated_at_ms".to_string());
            values.push(session.updated_at.saturating_mul(1000).to_string());
        }
        if columns.contains("recency_at") {
            names.push("recency_at".to_string());
            values.push(session.updated_at.to_string());
        }
        if columns.contains("recency_at_ms") {
            names.push("recency_at_ms".to_string());
            values.push(session.updated_at.saturating_mul(1000).to_string());
        }
        if columns.contains("rollout_path") {
            names.push("rollout_path".to_string());
            values.push(sql_quote(&session.path.to_string_lossy()));
        }
        if names.is_empty() {
            continue;
        }
        let sql = format!(
            "INSERT INTO threads ({}) VALUES ({});",
            names.join(", "),
            values.join(", ")
        );
        run_sqlite(db_path, &sql)?;
        inserted += 1;
    }
    Ok(inserted)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionSqliteMetadata {
    created_at: i64,
    source: String,
    cwd: String,
    cli_version: String,
    sandbox_policy: String,
    approval_mode: String,
}

fn sqlite_metadata_for_session(session: &SessionRepairRecord) -> SessionSqliteMetadata {
    let mut metadata = SessionSqliteMetadata {
        created_at: session.updated_at,
        source: "cli".to_string(),
        cwd: String::new(),
        cli_version: String::new(),
        sandbox_policy: "read-only".to_string(),
        approval_mode: "on-request".to_string(),
    };
    let Ok(content) = fs::read_to_string(&session.path) else {
        return metadata;
    };
    for line in content.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let item_type = value.get("type").and_then(Value::as_str);
        let Some(payload) = value.get("payload") else {
            continue;
        };
        if item_type == Some("session_meta") {
            if let Some(timestamp) = payload
                .get("timestamp")
                .and_then(Value::as_str)
                .or_else(|| value.get("timestamp").and_then(Value::as_str))
                .and_then(parse_rfc3339_timestamp)
            {
                metadata.created_at = timestamp;
            }
            if let Some(source) = payload.get("source").and_then(metadata_value_to_string) {
                metadata.source = source;
            }
            if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                metadata.cwd = cwd.to_string();
            }
            if let Some(cli_version) = payload.get("cli_version").and_then(Value::as_str) {
                metadata.cli_version = cli_version.to_string();
            }
        } else if item_type == Some("turn_context") {
            if metadata.cwd.is_empty() {
                if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                    metadata.cwd = cwd.to_string();
                }
            }
            if let Some(policy) = payload
                .get("permission_profile")
                .filter(|value| !value.is_null())
                .and_then(metadata_value_to_string)
                .or_else(|| {
                    payload
                        .get("sandbox_policy")
                        .and_then(metadata_value_to_string)
                })
            {
                metadata.sandbox_policy = policy;
            }
            if let Some(mode) = payload
                .get("approval_policy")
                .and_then(metadata_value_to_string)
            {
                metadata.approval_mode = mode;
            }
        }
    }
    metadata
}

fn metadata_value_to_string(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Null => return None,
        other => other.to_string(),
    };
    (!text.trim().is_empty()).then_some(text)
}

fn parse_rfc3339_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.timestamp())
}

fn push_sql_value(
    names: &mut Vec<String>,
    values: &mut Vec<String>,
    columns: &HashSet<String>,
    column: &str,
    value: &str,
) {
    if columns.contains(column) {
        names.push(column.to_string());
        values.push(sql_quote(value));
    }
}

fn push_sql_i64(
    names: &mut Vec<String>,
    values: &mut Vec<String>,
    columns: &HashSet<String>,
    column: &str,
    value: i64,
) {
    if columns.contains(column) {
        names.push(column.to_string());
        values.push(value.to_string());
    }
}

fn sqlite_provider_ids(db_path: &Path) -> Result<Vec<String>, String> {
    let columns = sqlite_thread_columns(db_path)?;
    if !columns.contains("model_provider") {
        return Ok(Vec::new());
    }
    let output = run_sqlite(
        db_path,
        "SELECT DISTINCT model_provider FROM threads WHERE COALESCE(model_provider, '') <> '';",
    )?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect())
}

fn backup_sqlite_file(db_path: &Path) -> Result<String, String> {
    let backup_dir = visibility_backup_dir();
    fs::create_dir_all(&backup_dir)
        .map_err(|error| format!("创建 SQLite 备份目录失败: {}", error))?;
    let filename = db_path
        .file_name()
        .and_then(|item| item.to_str())
        .unwrap_or("state.sqlite");
    fs::copy(db_path, backup_dir.join(filename))
        .map_err(|error| format!("备份 SQLite 失败 ({}): {}", db_path.display(), error))?;
    Ok(backup_dir.to_string_lossy().to_string())
}

fn visibility_backup_dir() -> PathBuf {
    switcher_root_dir()
        .join("visibility-backups")
        .join(chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string())
}

fn visibility_repair_marker_path() -> PathBuf {
    switcher_root_dir().join("session-visibility-repair.json")
}

fn session_trash_dir(codex_home: &Path) -> PathBuf {
    switcher_root_dir()
        .join("session-trash")
        .join(short_hash(&codex_home.to_string_lossy()))
}

fn switcher_root_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("CODEX_SWITCHER_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return path;
    }
    default_switcher_root_dir()
}

#[cfg(test)]
fn default_switcher_root_dir() -> PathBuf {
    test_switcher_root_dir()
}

#[cfg(not(test))]
fn default_switcher_root_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join(".codex_switcher")
}

#[cfg(test)]
fn test_switcher_root_dir() -> PathBuf {
    let thread_id = format!("{:?}", std::thread::current().id())
        .chars()
        .filter(|value| value.is_ascii_alphanumeric())
        .collect::<String>();
    std::env::temp_dir()
        .join("codex-switcher-tests")
        .join(format!("{}-{}", std::process::id(), thread_id))
}

fn migrate_directory_files(from: &Path, to: &Path) {
    if !from.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(from) else {
        return;
    };
    if fs::create_dir_all(to).is_err() {
        return;
    }
    for entry in entries.flatten() {
        let source = entry.path();
        if !source.is_file() {
            continue;
        }
        let Some(file_name) = source.file_name() else {
            continue;
        };
        let target = to.join(file_name);
        if target.exists() {
            continue;
        }
        if fs::rename(&source, &target).is_err() && fs::copy(&source, &target).is_ok() {
            let _ = fs::remove_file(&source);
        }
    }
}

fn normalized_id_set(values: Option<Vec<String>>) -> Option<HashSet<String>> {
    let set = values?
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();
    if set.is_empty() {
        None
    } else {
        Some(set)
    }
}

fn should_repair_default_instance(
    target_instance_id: Option<String>,
    repair_instance_ids: Option<Vec<String>>,
) -> bool {
    let is_default = |value: &str| {
        let value = value.trim();
        value.is_empty() || value == "__default__" || value == "default"
    };
    if let Some(ids) = repair_instance_ids {
        let ids = ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        if !ids.is_empty() && !ids.iter().any(|value| is_default(value)) {
            return false;
        }
    }
    target_instance_id
        .as_deref()
        .map(is_default)
        .unwrap_or(true)
}

fn rewrite_session_meta_provider(
    content: &str,
    target_provider: &str,
    rewrite_all_meta: bool,
) -> Result<Option<String>, String> {
    let mut output = String::with_capacity(content.len());
    let mut changed = false;
    let mut handled_meta = false;

    for segment in content.split_inclusive('\n') {
        let (body, ending) = if let Some(body) = segment.strip_suffix("\r\n") {
            (body, "\r\n")
        } else if let Some(body) = segment.strip_suffix('\n') {
            (body, "\n")
        } else {
            (segment, "")
        };

        if body.trim().is_empty() || (handled_meta && !rewrite_all_meta) {
            output.push_str(segment);
            continue;
        }

        let mut value: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => {
                output.push_str(segment);
                continue;
            }
        };

        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            output.push_str(segment);
            continue;
        }

        handled_meta = true;
        let Some(payload) = value.get_mut("payload").and_then(Value::as_object_mut) else {
            output.push_str(segment);
            continue;
        };
        if payload
            .get("model_provider")
            .and_then(Value::as_str)
            .map(|value| value == target_provider)
            .unwrap_or(false)
        {
            output.push_str(segment);
            continue;
        }
        payload.insert(
            "model_provider".to_string(),
            Value::String(target_provider.to_string()),
        );
        let line = serde_json::to_string(&value)
            .map_err(|error| format!("序列化会话元数据失败: {}", error))?;
        output.push_str(&line);
        output.push_str(ending);
        changed = true;
    }

    if changed {
        Ok(Some(output))
    } else {
        Ok(None)
    }
}

fn run_sqlite(db_path: &Path, sql: &str) -> Result<String, String> {
    let output = Command::new("sqlite3")
        .arg(db_path)
        .arg(sql)
        .output()
        .map_err(|error| format!("调用 sqlite3 失败: {}", error))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no such table") || stderr.contains("no such column") {
            Ok(String::new())
        } else {
            Err(format!(
                "修复 SQLite 失败 ({}): {}",
                db_path.display(),
                stderr
            ))
        }
    }
}

fn sqlite3_available() -> bool {
    Command::new("sqlite3").arg("-version").output().is_ok()
}

fn sql_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn build_session_record(path: &Path, content: &str) -> Result<CodexSessionRecord, String> {
    let title = extract_title(content).unwrap_or_else(|| file_stem(path));
    let project_name = extract_project_name(content).unwrap_or_else(|| "未归属项目".to_string());
    let id = extract_session_id(content).unwrap_or_else(|| session_id_for_path(path));
    let metadata = fs::metadata(path).ok();
    let updated_at = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let size_bytes = metadata.map(|metadata| metadata.len()).unwrap_or_default();
    Ok(CodexSessionRecord {
        id,
        title,
        project_name,
        path: path.to_string_lossy().to_string(),
        updated_at,
        message_count: content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count(),
        char_count: content.chars().count(),
        size_bytes,
    })
}

fn extract_session_id(content: &str) -> Option<String> {
    for line in content.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            continue;
        }
        let payload = value.get("payload").and_then(Value::as_object)?;
        for key in ["id", "session_id", "thread_id"] {
            if let Some(id) = payload.get(key).and_then(Value::as_str) {
                let id = id.trim();
                if !id.is_empty() {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            continue;
        }
        if !is_user_message_value(&value) {
            continue;
        }
        if let Some(title) = normalize_session_title(find_text(&value)) {
            return Some(title);
        }
    }

    for line in content.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            continue;
        }
        if let Some(title) = normalize_session_title(find_text(&value)) {
            return Some(title);
        }
    }
    None
}

fn normalize_session_title(text: Option<String>) -> Option<String> {
    let raw = text?;
    let source = extract_user_request_text(&raw).unwrap_or(raw);
    let visible_lines = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take_while(|line| !line.starts_with("<image "))
        .collect::<Vec<_>>();
    let compact = visible_lines
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if compact.is_empty() || is_internal_session_title(&compact) {
        return None;
    }
    Some(compact.chars().take(60).collect())
}

fn extract_user_request_text(text: &str) -> Option<String> {
    for marker in [
        "## My request for Codex:",
        "My request for Codex:",
        "## 我的请求：",
        "## 我的请求:",
    ] {
        if let Some((_, after_marker)) = text.split_once(marker) {
            let cleaned = after_marker.trim();
            if !cleaned.is_empty() {
                return Some(cleaned.to_string());
            }
        }
    }
    None
}

fn is_user_message_value(value: &Value) -> bool {
    value
        .get("payload")
        .and_then(|payload| payload.get("role"))
        .and_then(Value::as_str)
        == Some("user")
        || value.get("role").and_then(Value::as_str) == Some("user")
}

fn is_internal_session_title(text: &str) -> bool {
    let normalized = text.trim().to_lowercase();
    normalized.starts_with("<permissions instructions>")
        || normalized.starts_with("# files mentioned by the user")
        || normalized.starts_with("files mentioned by the user:")
        || (normalized.contains("codex-clipboard-") && normalized.contains("/var/folders/"))
        || normalized.starts_with("<environment_context>")
        || normalized.starts_with("<app-context>")
        || normalized.starts_with("<collaboration_mode>")
        || normalized.starts_with("<personality_spec>")
        || normalized.starts_with("<skills_instructions>")
        || normalized.starts_with("<plugins_instructions>")
        || normalized.starts_with("<extremely_important>")
        || normalized.starts_with("<system>")
        || normalized.starts_with("<developer>")
        || normalized.starts_with("<command-name>")
        || normalized.starts_with("<local-command-stdout>")
        || normalized.starts_with("<turn_aborted>")
        || normalized.starts_with("# agents.md instructions")
        || normalized.starts_with("knowledge cutoff:")
        || normalized.starts_with("current date:")
        || normalized.contains("filesystem sandboxing defines")
        || normalized.contains("you are codex")
        || normalized.contains("you have superpowers")
        || normalized.contains("primary agent in a team of agents")
}

fn extract_project_name(content: &str) -> Option<String> {
    for line in content.lines() {
        let value: Value = serde_json::from_str(line).ok()?;
        let cwd = find_string_key(&value, "cwd")
            .or_else(|| find_string_key(&value, "workspace"))
            .or_else(|| find_string_key(&value, "projectPath"))
            .or_else(|| find_string_key(&value, "workingDirectory"))?;
        let name = Path::new(&cwd)
            .file_name()
            .and_then(|item| item.to_str())
            .map(str::to_string)
            .unwrap_or(cwd);
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn find_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => items.iter().find_map(find_text),
        Value::Object(map) => {
            for key in ["text", "content", "message", "payload"] {
                if let Some(found) = map.get(key).and_then(find_text) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn find_string_key(value: &Value, target_key: &str) -> Option<String> {
    match value {
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => None,
        Value::Array(items) => items
            .iter()
            .find_map(|item| find_string_key(item, target_key)),
        Value::Object(map) => {
            if let Some(found) = map.get(target_key).and_then(Value::as_str) {
                return Some(found.to_string());
            }
            map.values()
                .find_map(|item| find_string_key(item, target_key))
        }
    }
}

fn collect_jsonl_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    collect_files(root, "jsonl")
}

fn collect_json_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    collect_files(root, "json")
}

fn collect_files(root: &Path, extension: &str) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in
        fs::read_dir(root).map_err(|error| format!("读取目录失败 {}: {}", root.display(), error))?
    {
        let path = entry
            .map_err(|error| format!("读取目录项失败: {}", error))?
            .path();
        if path.is_dir() {
            files.extend(collect_files(&path, extension)?);
        } else if path.extension().and_then(|item| item.to_str()) == Some(extension) {
            files.push(path);
        }
    }
    Ok(files)
}

fn session_id_for_path(path: &Path) -> String {
    let stem = file_stem(path);
    if !stem.is_empty() {
        stem
    } else {
        short_hash(&path.to_string_lossy())
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|item| item.to_str())
        .unwrap_or("session")
        .to_string()
}

fn read_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn normalize_query(value: Option<String>) -> Option<String> {
    let trimmed = value?.trim().to_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn short_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .take(12)
        .collect()
}

fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::{extract_title, SessionStore};
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn lists_moves_and_restores_sessions() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions").join("2026").join("06");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-1.jsonl");
        fs::write(
            &session_path,
            r#"{"message":{"content":[{"text":"hello from codex"}]}}"#,
        )
        .expect("write session");
        let store = SessionStore::new(codex.path().to_path_buf());

        let sessions = store
            .list_sessions(Some("hello".to_string()), None)
            .expect("list sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "session-1");

        let stats = store
            .token_stats(&[sessions[0].id.clone()])
            .expect("token stats");
        assert!(stats[0].approximate_tokens > 0);

        let moved = store
            .move_to_trash(&[sessions[0].id.clone()])
            .expect("move to trash");
        assert_eq!(moved.moved, 1);
        assert!(!session_path.exists());

        let trashed = store.list_trashed().expect("list trash");
        assert_eq!(trashed.len(), 1);

        let restored = store
            .restore_from_trash(&[sessions[0].id.clone()])
            .expect("restore");
        assert_eq!(restored.restored, 1);
        assert!(session_path.exists());
    }

    #[test]
    fn lists_and_restores_legacy_trashed_sessions() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions").join("2026").join("06");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("legacy-session.jsonl");
        let legacy_trash = codex.path().join(".codex-switcher").join("session-trash");
        fs::create_dir_all(&legacy_trash).expect("legacy trash dir");
        let legacy_trash_file = legacy_trash.join("legacy-session.jsonl");
        fs::write(&legacy_trash_file, "{}").expect("write legacy trash");
        fs::write(
            legacy_trash.join("legacy-session.json"),
            serde_json::json!({
                "id": "legacy-session",
                "title": "Legacy",
                "originalPath": session_path.to_string_lossy(),
                "trashPath": legacy_trash_file.to_string_lossy(),
                "deletedAt": 1
            })
            .to_string(),
        )
        .expect("write legacy metadata");
        let store = SessionStore::new(codex.path().to_path_buf());

        let trashed = store.list_trashed().expect("list legacy trash");
        assert_eq!(trashed.len(), 1);
        assert_eq!(trashed[0].id, "legacy-session");
        assert!(trashed[0].trash_path.ends_with("legacy-session.jsonl"));

        let restored = store
            .restore_from_trash(&["legacy-session".to_string()])
            .expect("restore legacy trash");
        assert_eq!(restored.restored, 1);
        assert!(session_path.exists());
    }

    #[test]
    fn extracts_project_name_from_session_metadata_cwd() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        fs::write(
            sessions_dir.join("session-1.jsonl"),
            r#"{"type":"session_meta","payload":{"cwd":"/Users/dalong/Documents/codeDesign/operator-end-cloud"}}"#,
        )
        .expect("write session");
        let store = SessionStore::new(codex.path().to_path_buf());

        let sessions = store.list_sessions(None, None).expect("list sessions");

        assert_eq!(sessions[0].project_name, "operator-end-cloud");
    }

    #[test]
    fn extracts_title_from_user_message_before_internal_prompt_text() {
        let content = concat!(
            r#"{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"<EXTREMELY_IMPORTANT> You have superpowers."}]}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"提取账号切换与添加功能"}]}}"#
        );

        assert_eq!(
            extract_title(content).as_deref(),
            Some("提取账号切换与添加功能")
        );
    }

    #[test]
    fn skips_agents_instruction_user_message_for_title() {
        let content = concat!(
            r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions\n\n<INSTRUCTIONS>...</INSTRUCTIONS>"}]}}"##,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"更新安卓和PC国旗"}]}}"#
        );

        assert_eq!(extract_title(content).as_deref(), Some("更新安卓和PC国旗"));
    }

    #[test]
    fn extracts_title_from_request_after_file_mentions() {
        let content = r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# Files mentioned by the user:\n\n## codex-clipboard-demo.png: /var/folders/demo/codex-clipboard-demo.png\n\n## My request for Codex:\n会话显示的还是奇怪，仔细分析修改下\n<image name=[Image #1] path=\"/tmp/demo.png\">"}]}}"##;

        assert_eq!(
            extract_title(content).as_deref(),
            Some("会话显示的还是奇怪，仔细分析修改下")
        );
    }

    #[test]
    fn skips_file_mentions_without_request_marker_for_title() {
        let content = concat!(
            r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# Files mentioned by the user:\n\n## codex-clipboard-demo.png: /var/folders/demo/codex-clipboard-demo.png"}]}}"##,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"OAuth 授权端口改成 16666"}]}}"#
        );

        assert_eq!(
            extract_title(content).as_deref(),
            Some("OAuth 授权端口改成 16666")
        );
    }

    #[test]
    fn repair_visibility_updates_codex_thread_provider_rows() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        fs::write(sessions_dir.join("session-1.jsonl"), "{}").expect("write session");
        fs::write(
            codex.path().join("config.toml"),
            "model_provider = \"relay\"\n",
        )
        .expect("write config");
        let db_path = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &db_path,
            r#"
                CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    model_provider TEXT,
                    first_user_message TEXT,
                    has_user_event INTEGER,
                    thread_source TEXT
                );
                INSERT INTO threads (id, model_provider, first_user_message, has_user_event, thread_source)
                VALUES ('session-1', 'openai', 'hello', 0, '');
                "#,
        );
        let store = SessionStore::new(codex.path().to_path_buf());

        let summary = store.repair_visibility().expect("repair");

        assert_eq!(summary.scanned, 1);
        assert_eq!(summary.repaired, 1);
        let row = run_sqlite_test_output(
            &db_path,
            "SELECT model_provider || '|' || has_user_event || '|' || thread_source FROM threads WHERE id = 'session-1';",
        );
        assert_eq!(row.trim(), "relay|1|user");
    }

    #[test]
    fn repair_visibility_inserts_rows_for_current_required_thread_schema() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-required.jsonl");
        fs::write(
            &session_path,
            concat!(
                r#"{"timestamp":"2026-07-01T08:30:00Z","type":"session_meta","payload":{"id":"session-required","timestamp":"2026-07-01T08:30:00Z","cwd":"/tmp/demo","cli_version":"0.130.0","source":"vscode","model_provider":"openai"}}"#,
                "\n",
                r#"{"type":"turn_context","payload":{"cwd":"/tmp/demo","approval_policy":"never","sandbox_policy":"danger-full-access"}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"role":"user","content":[{"text":"hello"}]}}"#,
                "\n"
            ),
        )
        .expect("write session");
        fs::write(
            codex.path().join("config.toml"),
            "model_provider = \"relay\"\n",
        )
        .expect("write config");
        let db_path = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &db_path,
            r#"
                CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    rollout_path TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    source TEXT NOT NULL,
                    model_provider TEXT NOT NULL,
                    cwd TEXT NOT NULL,
                    title TEXT NOT NULL,
                    sandbox_policy TEXT NOT NULL,
                    approval_mode TEXT NOT NULL,
                    has_user_event INTEGER NOT NULL DEFAULT 0
                );
                "#,
        );
        let store = SessionStore::new(codex.path().to_path_buf());

        let summary = store.repair_visibility().expect("repair");

        assert_eq!(summary.updated_sqlite_row_count, 1);
        let row = run_sqlite_test_output(
            &db_path,
            "SELECT (created_at > 0) || '|' || (updated_at > 0) || '|' || source || '|' || model_provider || '|' || cwd || '|' || sandbox_policy || '|' || approval_mode || '|' || has_user_event FROM threads WHERE id = 'session-required';",
        );
        assert_eq!(
            row.trim(),
            "1|1|vscode|relay|/tmp/demo|danger-full-access|never|1"
        );
        let rollout_path = run_sqlite_test_output(
            &db_path,
            "SELECT rollout_path FROM threads WHERE id = 'session-required';",
        );
        assert_eq!(rollout_path.trim(), session_path.to_string_lossy());
    }

    #[test]
    fn repair_visibility_rewrites_rollout_session_meta_provider() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-1.jsonl");
        fs::write(
            &session_path,
            concat!(
                r#"{"type":"session_meta","payload":{"id":"session-1","model_provider":"openai","cwd":"/tmp/demo"}}"#,
                "\n",
                r#"{"message":{"content":[{"text":"hello"}]}}"#,
                "\n"
            ),
        )
        .expect("write session");
        fs::write(
            codex.path().join("config.toml"),
            "model_provider = \"api-key-provider\"\n",
        )
        .expect("write config");
        let store = SessionStore::new(codex.path().to_path_buf());

        let summary = store.repair_visibility().expect("repair");

        assert_eq!(summary.changed_rollout_file_count, 1);
        let content = fs::read_to_string(session_path).expect("read session");
        assert!(content.contains(r#""model_provider":"api-key-provider""#));
        assert!(content.contains(r#""text":"hello""#));
    }

    fn run_sqlite_test(db_path: &std::path::Path, sql: &str) {
        let output = Command::new("sqlite3")
            .arg(db_path)
            .arg(sql)
            .output()
            .expect("run sqlite3");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn run_sqlite_test_output(db_path: &std::path::Path, sql: &str) -> String {
        let output = Command::new("sqlite3")
            .arg(db_path)
            .arg(sql)
            .output()
            .expect("run sqlite3");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }
}
