use crate::account::{replace_file_atomic, write_bytes_atomic};
use base64::{engine::general_purpose, Engine as _};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionRecord {
    pub id: String,
    pub title: String,
    pub project_name: String,
    pub project_path: String,
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
pub struct CodexSessionMutationResult {
    pub session_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub backup_path: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionContentPage {
    pub session_id: String,
    pub cursor: u64,
    pub next_cursor: Option<u64>,
    pub turns: Vec<CodexSessionTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionTurn {
    pub id: String,
    pub timestamp: String,
    pub messages: Vec<CodexSessionMessage>,
    pub technical_item_count: usize,
    pub can_delete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionMessage {
    pub id: String,
    pub role: String,
    pub phase: String,
    pub timestamp: String,
    pub text: String,
    pub attachments: Vec<CodexSessionAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionAttachment {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub source_path: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    pub available: bool,
    pub inline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionAsset {
    pub data_url: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionTurnMutationResult {
    pub session_id: String,
    pub deleted_turn_id: String,
    pub backup_id: String,
    pub backup_path: String,
    pub removed_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionMessageMutationResult {
    pub session_id: String,
    pub deleted_message_ids: Vec<String>,
    pub backup_id: String,
    pub backup_path: String,
    pub removed_bytes: u64,
    pub warnings: Vec<String>,
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
    pub updated_catalog_row_count: usize,
    pub verified_visible_session_count: usize,
    pub skipped_non_sidebar_session_count: usize,
    pub remaining_invisible_session_count: usize,
    pub created_local_project_count: usize,
    pub assigned_local_project_session_count: usize,
    pub verified_local_project_count: usize,
    pub skipped_local_project_session_count: usize,
    pub recreated_generated_image_count: usize,
    pub verified_generated_image_count: usize,
    pub invalid_generated_image_count: usize,
    pub reset_history_projection_count: usize,
    pub desktop_reload_required: bool,
    pub desktop_reload_performed: bool,
    pub backup_dirs: Vec<String>,
    pub items: Vec<CodexSessionVisibilityRepairItem>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionModelCompatibilityRepairSummary {
    pub target_provider: String,
    pub repaired_rollout_file_count: usize,
    pub rewritten_rollout_model_field_count: usize,
    pub synchronized_rollout_provider_count: usize,
    pub removed_encrypted_reasoning_item_count: usize,
    pub removed_encrypted_compaction_item_count: usize,
    pub repaired_thread_count: usize,
    pub synchronized_catalog_row_count: usize,
    pub repaired_database_count: usize,
    pub backup_dirs: Vec<String>,
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

#[derive(Debug, Default)]
struct DesktopCatalogRepairResult {
    updated: usize,
    verified: usize,
    skipped: usize,
    missing_ids: Vec<String>,
    backup_dir: Option<String>,
}

#[derive(Debug, Default)]
struct DesktopProjectRepairResult {
    created: usize,
    updated_assignments: usize,
    verified_projects: usize,
    verified_assignments: usize,
    skipped: usize,
    backup_dir: Option<String>,
}

#[derive(Debug, Default)]
struct GeneratedImageRepairResult {
    recreated: usize,
    verified: usize,
    invalid: usize,
}

impl GeneratedImageRepairResult {
    fn add(&mut self, other: Self) {
        self.recreated += other.recreated;
        self.verified += other.verified;
        self.invalid += other.invalid;
    }
}

#[derive(Debug, Default)]
struct LocalImageRepairResult {
    images: GeneratedImageRepairResult,
    changed_rollout_files: usize,
    changed_session_ids: HashSet<String>,
    backup_dirs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PaginatedRolloutProjectionTarget {
    next_byte_offset: i64,
    next_ordinal: i64,
    turn_count: usize,
}

#[derive(Debug, Clone)]
struct ParsedSessionTurn {
    public: CodexSessionTurn,
    start_offset: u64,
    end_offset: u64,
    complete: bool,
    response_item_ids: HashSet<String>,
    response_item_fingerprints: HashMap<String, usize>,
}

#[derive(Debug)]
struct SessionMessageDeletionPlan {
    message_ids: Vec<String>,
    line_offsets: HashSet<u64>,
    response_item_ids: HashSet<String>,
    response_item_fingerprints: HashMap<String, usize>,
}

#[derive(Debug)]
struct SessionTurnAccumulator {
    id: String,
    timestamp: String,
    start_offset: u64,
    end_offset: u64,
    complete: bool,
    response_messages: Vec<CodexSessionMessage>,
    fallback_user_messages: Vec<CodexSessionMessage>,
    fallback_assistant_messages: Vec<CodexSessionMessage>,
    local_image_paths: Vec<String>,
    technical_item_count: usize,
    response_item_ids: HashSet<String>,
    response_item_fingerprints: HashMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionFileFingerprint {
    len: u64,
    modified: Option<std::time::SystemTime>,
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

    pub fn synchronize_model_provider(&self, target_provider: &str) -> Result<usize, String> {
        let target_provider = target_provider.trim();
        if target_provider.is_empty() {
            return Err("Codex model_provider 不能为空".to_string());
        }
        let mut updated = 0usize;
        for db_path in self.sqlite_candidate_paths() {
            let connection = Connection::open(&db_path).map_err(|error| {
                format!(
                    "打开 Codex 会话数据库失败 ({}): {}",
                    db_path.display(),
                    error
                )
            })?;
            connection
                .busy_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| format!("设置 Codex 会话数据库等待时间失败: {}", error))?;
            let threads_columns = sqlite_table_columns_with_connection(&connection, "threads")?;
            let catalog_columns =
                sqlite_table_columns_with_connection(&connection, "local_thread_catalog")?;
            let thread_count = if threads_columns.contains("model_provider") {
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM threads WHERE COALESCE(model_provider, '') <> ?1",
                        params![target_provider],
                        |row| row.get::<_, usize>(0),
                    )
                    .map_err(|error| format!("检查 Codex 会话 provider 失败: {}", error))?
            } else {
                0
            };
            let catalog_count = if catalog_columns.contains("model_provider") {
                let local_only = if catalog_columns.contains("host_id") {
                    " AND host_id = 'local'"
                } else {
                    ""
                };
                connection
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM local_thread_catalog WHERE COALESCE(model_provider, '') <> ?1{local_only}"
                        ),
                        params![target_provider],
                        |row| row.get::<_, usize>(0),
                    )
                    .map_err(|error| format!("检查 Codex 侧栏 provider 失败: {}", error))?
            } else {
                0
            };
            if thread_count == 0 && catalog_count == 0 {
                continue;
            }
            drop(connection);
            backup_sqlite_file(&db_path)?;

            let mut connection = Connection::open(&db_path).map_err(|error| {
                format!(
                    "重新打开 Codex 会话数据库失败 ({}): {}",
                    db_path.display(),
                    error
                )
            })?;
            connection
                .busy_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| format!("设置 Codex 会话数据库等待时间失败: {}", error))?;
            let transaction = connection
                .transaction()
                .map_err(|error| format!("开启 Codex provider 同步事务失败: {}", error))?;
            if thread_count > 0 {
                updated += transaction
                    .execute(
                        "UPDATE threads SET model_provider = ?1 WHERE COALESCE(model_provider, '') <> ?1",
                        params![target_provider],
                    )
                    .map_err(|error| format!("同步 Codex 会话 provider 失败: {}", error))?;
            }
            if catalog_count > 0 {
                let local_only = if catalog_columns.contains("host_id") {
                    " AND host_id = 'local'"
                } else {
                    ""
                };
                updated += transaction
                    .execute(
                        &format!(
                            "UPDATE local_thread_catalog SET model_provider = ?1 WHERE COALESCE(model_provider, '') <> ?1{local_only}"
                        ),
                        params![target_provider],
                    )
                    .map_err(|error| format!("同步 Codex 侧栏 provider 失败: {}", error))?;
                if sqlite_table_exists_with_connection(
                    &transaction,
                    "local_thread_catalog_metadata",
                )? {
                    transaction
                        .execute(
                            "UPDATE local_thread_catalog_metadata SET catalog_revision = catalog_revision + 1 WHERE id = 1",
                            [],
                        )
                        .map_err(|error| format!("更新 Codex 侧栏目录版本失败: {}", error))?;
                }
            }
            transaction
                .commit()
                .map_err(|error| format!("提交 Codex provider 同步失败: {}", error))?;
        }
        Ok(updated)
    }

    pub fn repair_model_compatibility(
        &self,
        target_provider: &str,
    ) -> Result<CodexSessionModelCompatibilityRepairSummary, String> {
        let target_provider = target_provider.trim();
        if target_provider.is_empty() {
            return Err("Codex model_provider 不能为空".to_string());
        }

        let mut summary = CodexSessionModelCompatibilityRepairSummary {
            target_provider: target_provider.to_string(),
            repaired_rollout_file_count: 0,
            rewritten_rollout_model_field_count: 0,
            synchronized_rollout_provider_count: 0,
            removed_encrypted_reasoning_item_count: 0,
            removed_encrypted_compaction_item_count: 0,
            repaired_thread_count: 0,
            synchronized_catalog_row_count: 0,
            repaired_database_count: 0,
            backup_dirs: Vec::new(),
        };

        for path in collect_jsonl_files(&self.sessions_dir())? {
            let Some((tmp_path, rewrite_stats)) =
                prepare_rollout_model_compatibility_rewrite(&path, target_provider)?
            else {
                continue;
            };
            let backup_path = match self.backup_session_file(&path, "model-compatibility") {
                Ok(path) => path,
                Err(error) => {
                    let _ = fs::remove_file(&tmp_path);
                    return Err(error);
                }
            };
            if let Err(error) = replace_file_atomic(&path, &tmp_path) {
                return Err(format!(
                    "写入会话模型修复结果失败 ({}): {}",
                    path.display(),
                    error
                ));
            }
            let _ = Command::new("touch")
                .arg("-r")
                .arg(&backup_path)
                .arg(&path)
                .output();
            if let Some(backup_dir) = backup_path.parent() {
                let backup_dir = backup_dir.to_string_lossy().to_string();
                if !summary.backup_dirs.contains(&backup_dir) {
                    summary.backup_dirs.push(backup_dir);
                }
            }
            summary.repaired_rollout_file_count += 1;
            summary.rewritten_rollout_model_field_count += rewrite_stats.rewritten_model_fields;
            summary.synchronized_rollout_provider_count += rewrite_stats.synchronized_providers;
            summary.removed_encrypted_reasoning_item_count +=
                rewrite_stats.removed_encrypted_reasoning_items;
            summary.removed_encrypted_compaction_item_count +=
                rewrite_stats.removed_encrypted_compaction_items;
        }

        for db_path in self.sqlite_candidate_paths() {
            let connection = Connection::open(&db_path).map_err(|error| {
                format!(
                    "打开 Codex 会话数据库失败 ({}): {}",
                    db_path.display(),
                    error
                )
            })?;
            connection
                .busy_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| format!("设置 Codex 会话数据库等待时间失败: {}", error))?;

            let thread_columns = sqlite_table_columns_with_connection(&connection, "threads")?;
            let catalog_columns =
                sqlite_table_columns_with_connection(&connection, "local_thread_catalog")?;
            let has_thread_provider = thread_columns.contains("model_provider");
            let has_thread_model = thread_columns.contains("model");
            let has_thread_effort = thread_columns.contains("reasoning_effort");

            let mut thread_predicates = Vec::new();
            if has_thread_provider {
                thread_predicates.push("COALESCE(model_provider, '') <> ?1");
            }
            if has_thread_model {
                thread_predicates.push("COALESCE(model, '') <> ''");
            }
            if has_thread_effort {
                thread_predicates.push("COALESCE(reasoning_effort, '') <> ''");
            }
            let thread_count = if thread_predicates.is_empty() {
                0
            } else {
                let sql = format!(
                    "SELECT COUNT(*) FROM threads WHERE {}",
                    thread_predicates.join(" OR ")
                );
                if has_thread_provider {
                    connection
                        .query_row(&sql, params![target_provider], |row| row.get::<_, usize>(0))
                        .map_err(|error| format!("检查 Codex 会话模型兼容性失败: {}", error))?
                } else {
                    connection
                        .query_row(&sql, [], |row| row.get::<_, usize>(0))
                        .map_err(|error| format!("检查 Codex 会话模型兼容性失败: {}", error))?
                }
            };

            let catalog_count = if catalog_columns.contains("model_provider") {
                let local_only = if catalog_columns.contains("host_id") {
                    " AND host_id = 'local'"
                } else {
                    ""
                };
                connection
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM local_thread_catalog WHERE COALESCE(model_provider, '') <> ?1{local_only}"
                        ),
                        params![target_provider],
                        |row| row.get::<_, usize>(0),
                    )
                    .map_err(|error| format!("检查 Codex 侧栏 provider 失败: {}", error))?
            } else {
                0
            };

            if thread_count == 0 && catalog_count == 0 {
                continue;
            }
            drop(connection);

            let backup_dir = backup_sqlite_file(&db_path)?;
            if !summary.backup_dirs.contains(&backup_dir) {
                summary.backup_dirs.push(backup_dir);
            }

            let mut connection = Connection::open(&db_path).map_err(|error| {
                format!(
                    "重新打开 Codex 会话数据库失败 ({}): {}",
                    db_path.display(),
                    error
                )
            })?;
            connection
                .busy_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| format!("设置 Codex 会话数据库等待时间失败: {}", error))?;
            let transaction = connection
                .transaction()
                .map_err(|error| format!("开启 Codex 会话模型修复事务失败: {}", error))?;

            if thread_count > 0 {
                let mut assignments = Vec::new();
                if has_thread_provider {
                    assignments.push("model_provider = ?1");
                }
                if has_thread_model {
                    assignments.push("model = NULL");
                }
                if has_thread_effort {
                    assignments.push("reasoning_effort = NULL");
                }
                let sql = format!(
                    "UPDATE threads SET {} WHERE {}",
                    assignments.join(", "),
                    thread_predicates.join(" OR ")
                );
                let updated = if has_thread_provider {
                    transaction.execute(&sql, params![target_provider])
                } else {
                    transaction.execute(&sql, [])
                }
                .map_err(|error| format!("修复 Codex 会话模型兼容性失败: {}", error))?;
                summary.repaired_thread_count += updated;
            }

            if catalog_count > 0 {
                let local_only = if catalog_columns.contains("host_id") {
                    " AND host_id = 'local'"
                } else {
                    ""
                };
                summary.synchronized_catalog_row_count += transaction
                    .execute(
                        &format!(
                            "UPDATE local_thread_catalog SET model_provider = ?1 WHERE COALESCE(model_provider, '') <> ?1{local_only}"
                        ),
                        params![target_provider],
                    )
                    .map_err(|error| format!("同步 Codex 侧栏 provider 失败: {}", error))?;
                if sqlite_table_exists_with_connection(
                    &transaction,
                    "local_thread_catalog_metadata",
                )? {
                    transaction
                        .execute(
                            "UPDATE local_thread_catalog_metadata SET catalog_revision = catalog_revision + 1 WHERE id = 1",
                            [],
                        )
                        .map_err(|error| format!("更新 Codex 侧栏目录版本失败: {}", error))?;
                }
            }

            transaction
                .commit()
                .map_err(|error| format!("提交 Codex 会话模型修复失败: {}", error))?;
            summary.repaired_database_count += 1;
        }

        Ok(summary)
    }

    pub fn list_sessions(
        &self,
        title_query: Option<String>,
        content_query: Option<String>,
    ) -> Result<Vec<CodexSessionRecord>, String> {
        let title_query = normalize_query(title_query);
        let content_query = normalize_query(content_query);
        let custom_titles = self.read_session_titles();
        let mut sessions = Vec::new();
        for path in collect_jsonl_files(&self.sessions_dir())? {
            let content = fs::read_to_string(&path)
                .map_err(|error| format!("读取会话失败 {}: {}", path.display(), error))?;
            let mut record = build_session_record(&path, &content)?;
            if let Some(title) = custom_titles.get(&record.id) {
                record.title = title.clone();
            }
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

    pub fn list_session_content(
        &self,
        session_id: &str,
        cursor: Option<u64>,
        limit: Option<usize>,
        direction: Option<&str>,
    ) -> Result<CodexSessionContentPage, String> {
        let session_id = normalize_required_session_id(session_id)?;
        let path = self.find_session_path(&session_id)?;
        let limit = limit.unwrap_or(20).clamp(1, 50);
        let direction = direction.unwrap_or("asc");
        let (cursor, turns, next_cursor) = match direction {
            "asc" => {
                let cursor = cursor.unwrap_or(0);
                let (turns, next_cursor) = read_session_turn_page(&path, cursor, limit)?;
                (cursor, turns, next_cursor)
            }
            "desc" => {
                let cursor = cursor.unwrap_or_else(|| {
                    fs::metadata(&path)
                        .map(|metadata| metadata.len())
                        .unwrap_or_default()
                });
                let (turns, next_cursor) = read_session_turn_page_desc(&path, cursor, limit)?;
                (cursor, turns, next_cursor)
            }
            _ => return Err("无效的会话内容排序方向".to_string()),
        };
        Ok(CodexSessionContentPage {
            session_id,
            cursor,
            next_cursor,
            turns: turns.into_iter().map(|turn| turn.public).collect(),
        })
    }

    pub fn get_session_asset(
        &self,
        session_id: &str,
        asset_id: &str,
    ) -> Result<CodexSessionAsset, String> {
        let session_id = normalize_required_session_id(session_id)?;
        let path = self.find_session_path(&session_id)?;
        read_session_asset(&path, asset_id)
    }

    pub fn delete_session_turn(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<CodexSessionTurnMutationResult, String> {
        let session_id = normalize_required_session_id(session_id)?;
        let turn_id = turn_id.trim();
        if turn_id.is_empty() {
            return Err("请选择要删除的对话轮次".to_string());
        }
        let path = self.find_session_path(&session_id)?;
        let before = session_file_fingerprint(&path)?;
        let (target_turn, has_open_turn) = find_session_turn(&path, turn_id)?;
        if has_open_turn {
            return Err("该会话仍在生成或写入内容，请等待当前对话结束后再删除".to_string());
        }
        if !target_turn.complete {
            return Err("未完成的对话轮次不能删除".to_string());
        }

        let tmp_path = rewrite_session_without_turn(&path, &target_turn)?;
        if session_file_fingerprint(&path)? != before {
            let _ = fs::remove_file(&tmp_path);
            return Err("会话在删除过程中发生了更新，本次操作已取消，请刷新后重试".to_string());
        }
        let backup_path = self.backup_session_file(&path, "turn-delete")?;
        if session_file_fingerprint(&path)? != before {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::remove_file(&backup_path);
            return Err("会话在备份过程中发生了更新，本次操作已取消，请刷新后重试".to_string());
        }
        replace_file_atomic(&path, &tmp_path)
            .map_err(|error| format!("写入删除后的会话失败: {}", error))?;
        let after_len = fs::metadata(&path)
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        Ok(CodexSessionTurnMutationResult {
            session_id,
            deleted_turn_id: turn_id.to_string(),
            backup_id: backup_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            backup_path: backup_path.to_string_lossy().to_string(),
            removed_bytes: before.len.saturating_sub(after_len),
            warnings: Vec::new(),
        })
    }

    pub fn delete_session_messages(
        &self,
        session_id: &str,
        turn_id: &str,
        message_ids: &[String],
    ) -> Result<CodexSessionMessageMutationResult, String> {
        let session_id = normalize_required_session_id(session_id)?;
        let turn_id = turn_id.trim();
        if turn_id.is_empty() {
            return Err("请选择消息所在的对话轮次".to_string());
        }
        let message_ids = message_ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<HashSet<_>>();
        if message_ids.is_empty() {
            return Err("请选择要删除的消息".to_string());
        }

        let path = self.find_session_path(&session_id)?;
        let before = session_file_fingerprint(&path)?;
        let (target_turn, has_open_turn) = find_session_turn(&path, turn_id)?;
        if has_open_turn {
            return Err("该会话仍在生成或写入内容，请等待当前对话结束后再删除".to_string());
        }
        if !target_turn.complete {
            return Err("未完成的对话轮次不能删除消息".to_string());
        }
        let plan = build_message_deletion_plan(&path, &target_turn, &message_ids)?;
        let tmp_path = rewrite_session_without_messages(&path, &plan)?;
        if session_file_fingerprint(&path)? != before {
            let _ = fs::remove_file(&tmp_path);
            return Err("会话在删除过程中发生了更新，本次操作已取消，请刷新后重试".to_string());
        }
        let backup_path = self.backup_session_file(&path, "message-delete")?;
        if session_file_fingerprint(&path)? != before {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::remove_file(&backup_path);
            return Err("会话在备份过程中发生了更新，本次操作已取消，请刷新后重试".to_string());
        }
        replace_file_atomic(&path, &tmp_path)
            .map_err(|error| format!("写入删除后的会话失败: {}", error))?;
        let after_len = fs::metadata(&path)
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        Ok(CodexSessionMessageMutationResult {
            session_id,
            deleted_message_ids: plan.message_ids,
            backup_id: backup_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            backup_path: backup_path.to_string_lossy().to_string(),
            removed_bytes: before.len.saturating_sub(after_len),
            warnings: Vec::new(),
        })
    }

    pub fn restore_session_turn_backup(
        &self,
        session_id: &str,
        backup_id: &str,
    ) -> Result<CodexSessionMutationResult, String> {
        let session_id = normalize_required_session_id(session_id)?;
        let path = self.find_session_path(&session_id)?;
        let backup_path = self.validated_turn_backup_path(&path, backup_id)?;
        if session_has_open_turn(&path)? {
            return Err("该会话仍在生成或写入内容，请等待当前对话结束后再恢复".to_string());
        }
        let before = session_file_fingerprint(&path)?;
        let rollback_path = self.backup_session_file(&path, "before-restore")?;
        if let Err(error) = restore_session_file_if_unchanged(&path, &backup_path, before) {
            let _ = fs::remove_file(&rollback_path);
            return Err(error);
        }
        let title = self
            .read_session_titles()
            .get(session_id.as_str())
            .cloned()
            .unwrap_or_else(|| file_stem(&path));
        Ok(CodexSessionMutationResult {
            session_id,
            title,
            project_path: None,
            backup_path: Some(rollback_path.to_string_lossy().to_string()),
            warnings: Vec::new(),
        })
    }

    pub fn copy_session_to_store(
        &self,
        target_store: &SessionStore,
        source_session_id: &str,
        copy_suffix: &str,
        target_project_path: &str,
    ) -> Result<CodexSessionMutationResult, String> {
        let source_session_id = source_session_id.trim();
        if source_session_id.is_empty() {
            return Err("源会话不能为空".to_string());
        }
        let target_project_path = normalize_copy_target_directory(target_project_path)?;
        let sessions = self.list_sessions(None, None)?;
        let source = sessions
            .iter()
            .find(|session| session.id == source_session_id)
            .ok_or_else(|| format!("源会话不存在: {}", source_session_id))?;
        if self.codex_home == target_store.codex_home
            && same_existing_directory(&source.project_path, &target_project_path)
        {
            return Err("目标工作目录不能与源会话目录相同，请选择其他目录".to_string());
        }
        let source_path = Path::new(&source.path);
        if !source_path.is_file() {
            return Err(format!("源会话文件不存在: {}", source_path.display()));
        }
        if session_has_open_turn(source_path)? {
            return Err("源会话仍在生成或写入内容，请等待当前对话结束后再复制".to_string());
        }
        let source_history_mode = rollout_history_mode(source_path);
        let target_session_paths_before_fork = collect_jsonl_files(&target_store.sessions_dir())?
            .into_iter()
            .collect::<HashSet<_>>();

        let target_provider = target_store.read_target_provider()?;
        let staged = target_store.stage_fork_source(
            &self.codex_home,
            source,
            source_path,
            &target_project_path,
            &target_provider,
        )?;
        let fork_result = run_codex_thread_fork(
            &target_store.codex_home,
            &staged.session,
            &target_project_path,
            &target_provider,
        );
        let fork = match fork_result {
            Ok(fork) => fork,
            Err(fork_error) => match target_store.cleanup_staged_fork_source(&staged) {
                Ok(()) => return Err(fork_error),
                Err(cleanup_error) => {
                    return Err(format!(
                        "{fork_error}；同时清理临时复制数据失败: {cleanup_error}"
                    ));
                }
            },
        };
        let (validation, should_cleanup_created_fork) =
            if fork.session_id == source.id || fork.session_id == staged.session.id {
                (
                    Err("Codex 返回了已有会话 ID，未能创建独立副本".to_string()),
                    false,
                )
            } else if !fork_history_modes_are_compatible(source_history_mode, &fork.history_mode) {
                (
                    Err(format!(
                        "Codex 创建的副本历史模式不兼容（源会话: {source_history_mode}，副本: {}）",
                        fork.history_mode
                    )),
                    true,
                )
            } else if !same_existing_directory(&fork.project_path, &target_project_path) {
                (
                    Err(format!(
                        "Codex 创建的副本未归属目标目录（实际目录: {}），本次操作未完成",
                        fork.project_path
                    )),
                    true,
                )
            } else {
                (Ok(()), false)
            };
        if let Err(validation_error) = validation {
            let cleanup_fork_error = should_cleanup_created_fork
                .then(|| {
                    target_store
                        .cleanup_created_fork(&fork.session_id, &target_session_paths_before_fork)
                        .err()
                })
                .flatten();
            let cleanup_staged_error = target_store.cleanup_staged_fork_source(&staged).err();
            return Err(format_copy_validation_failure(
                &validation_error,
                should_cleanup_created_fork,
                cleanup_fork_error.as_deref(),
                cleanup_staged_error.as_deref(),
            ));
        }

        let title = copied_session_title(&source.title, copy_suffix);
        let mut warnings = Vec::new();
        if let Err(error) = target_store.archive_staged_fork_source(&staged) {
            warnings.push(format!("隐藏副本历史底稿失败: {error}"));
        }
        if let Err(error) = target_store.write_session_title(&fork.session_id, &title) {
            warnings.push(error);
        }
        if let Err(error) = target_store.update_sqlite_session_title(&fork.session_id, &title) {
            warnings.push(error);
        }
        if let Err(error) =
            target_store.update_sqlite_session_cwd(&fork.session_id, &target_project_path)
        {
            warnings.push(error);
        }
        if let Err(error) = target_store.repair_visibility_with_options(
            Some("quick"),
            Some(target_provider),
            None,
            None,
            Some(vec![fork.session_id.clone()]),
        ) {
            warnings.push(format!("副本已创建，但同步 Codex 侧栏失败: {error}"));
        }

        Ok(CodexSessionMutationResult {
            session_id: fork.session_id,
            title,
            project_path: Some(target_project_path),
            backup_path: None,
            warnings,
        })
    }

    fn stage_fork_source(
        &self,
        source_codex_home: &Path,
        source: &CodexSessionRecord,
        source_path: &Path,
        target_project_path: &str,
        target_provider: &str,
    ) -> Result<StagedForkSource, String> {
        let source_before = session_file_fingerprint(source_path)?;
        let created_at = chrono::Utc::now();
        let staged_session_id = new_session_id();
        let staged_path =
            new_session_rollout_path(&self.sessions_dir(), &staged_session_id, created_at);
        create_staged_fork_rollout(
            source_codex_home,
            source_path,
            &staged_path,
            &staged_session_id,
            target_project_path,
            target_provider,
            created_at,
        )?;
        if session_file_fingerprint(source_path)? != source_before {
            let _ = fs::remove_file(&staged_path);
            return Err("源会话在复制准备期间发生了更新，本次操作已取消，请刷新后重试".to_string());
        }

        let staged_session = CodexSessionRecord {
            id: staged_session_id,
            title: source.title.clone(),
            project_name: project_name_for_path(target_project_path)
                .unwrap_or_else(|| target_project_path.to_string()),
            project_path: target_project_path.to_string(),
            path: staged_path.to_string_lossy().to_string(),
            updated_at: created_at.timestamp(),
            message_count: 0,
            char_count: 0,
            size_bytes: fs::metadata(&staged_path)
                .map(|metadata| metadata.len())
                .unwrap_or_default(),
        };
        let staged = StagedForkSource {
            session: staged_session,
            path: staged_path,
        };
        if let Err(error) = self.register_staged_fork_source(&staged, target_provider) {
            let cleanup_error = self.cleanup_staged_fork_source(&staged).err();
            return Err(match cleanup_error {
                Some(cleanup_error) => {
                    format!("准备临时会话索引失败: {error}；清理失败: {cleanup_error}")
                }
                None => format!("准备临时会话索引失败: {error}"),
            });
        }
        Ok(staged)
    }

    fn register_staged_fork_source(
        &self,
        staged: &StagedForkSource,
        target_provider: &str,
    ) -> Result<(), String> {
        let record = SessionRepairRecord {
            id: staged.session.id.clone(),
            title: staged.session.title.clone(),
            path: staged.path.clone(),
            updated_at: staged.session.updated_at,
        };
        let mut registered = false;
        for db_path in self.sqlite_candidate_paths() {
            if sqlite_insert_missing_session_rows(
                &db_path,
                target_provider,
                Some(std::slice::from_ref(&record)),
            )? > 0
            {
                registered = true;
            } else if sqlite_thread_columns(&db_path)?.contains("id") {
                let exists = run_sqlite(
                    &db_path,
                    &format!(
                        "SELECT COUNT(*) FROM threads WHERE id = {};",
                        sql_quote(&staged.session.id)
                    ),
                )?;
                registered |= exists.trim() == "1";
            }
        }
        if !registered {
            return Err("目标实例缺少可写的 Codex thread store，无法创建分页会话副本".to_string());
        }
        Ok(())
    }

    fn cleanup_staged_fork_source(&self, staged: &StagedForkSource) -> Result<(), String> {
        let mut failures = Vec::new();
        for db_path in self.staged_thread_cleanup_db_paths() {
            if let Err(error) = remove_staged_thread_rows(&db_path, &staged.session.id) {
                failures.push(error);
            }
        }
        if let Err(error) = fs::remove_file(&staged.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                failures.push(format!(
                    "删除临时会话文件失败 ({}): {error}",
                    staged.path.display()
                ));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("；"))
        }
    }

    fn cleanup_created_fork(
        &self,
        session_id: &str,
        session_paths_before_fork: &HashSet<PathBuf>,
    ) -> Result<(), String> {
        let path = self
            .find_session_path(session_id)
            .map_err(|error| format!("无法确认未通过校验的副本文件，已停止自动回滚: {error}"))?;
        if session_paths_before_fork.contains(&path) {
            return Err(format!(
                "副本 ID 指向复制前已存在的会话，已停止自动回滚: {}",
                path.display()
            ));
        }

        let mut failures = Vec::new();
        for db_path in self.staged_thread_cleanup_db_paths() {
            if let Err(error) = remove_staged_thread_rows(&db_path, session_id) {
                failures.push(error);
            }
        }
        if let Err(error) = fs::remove_file(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                failures.push(format!(
                    "删除未通过校验的副本失败 ({}): {error}",
                    path.display()
                ));
            }
        }
        if let Err(error) = self.remove_session_index_entries(&[session_id.to_string()]) {
            failures.push(error);
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("；"))
        }
    }

    fn archive_staged_fork_source(&self, staged: &StagedForkSource) -> Result<(), String> {
        let file_name = staged
            .path
            .file_name()
            .ok_or_else(|| "无法定位副本历史底稿文件名".to_string())?;
        let archived_path = self.codex_home.join("archived_sessions").join(file_name);
        fs::create_dir_all(
            archived_path
                .parent()
                .ok_or_else(|| "无法定位副本历史底稿目录".to_string())?,
        )
        .map_err(|error| format!("创建副本历史底稿目录失败: {error}"))?;
        fs::rename(&staged.path, &archived_path).map_err(|error| {
            format!(
                "归档副本历史底稿失败 ({} -> {}): {error}",
                staged.path.display(),
                archived_path.display()
            )
        })?;

        let mut failures = Vec::new();
        for db_path in self.sqlite_candidate_paths() {
            if let Err(error) =
                archive_staged_thread_row(&db_path, &staged.session.id, &archived_path)
            {
                failures.push(error);
            }
        }
        if let Err(error) =
            self.remove_session_index_entries(std::slice::from_ref(&staged.session.id))
        {
            failures.push(error);
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("；"))
        }
    }

    #[cfg(test)]
    pub fn copy_session_history(
        &self,
        source_session_id: &str,
        copy_suffix: &str,
    ) -> Result<CodexSessionMutationResult, String> {
        let source_session_id = source_session_id.trim();
        if source_session_id.is_empty() {
            return Err("源会话不能为空".to_string());
        }

        let sessions = self.list_sessions(None, None)?;
        let source = sessions
            .iter()
            .find(|session| session.id == source_session_id)
            .ok_or_else(|| format!("源会话不存在: {}", source_session_id))?;
        let source_path = Path::new(&source.path);
        if session_has_open_turn(source_path)? {
            return Err("源会话仍在生成或写入内容，请等待当前对话结束后再复制".to_string());
        }
        let source_before = session_file_fingerprint(source_path)?;
        let source_content = fs::read_to_string(source_path)
            .map_err(|error| format!("读取源会话失败: {}", error))?;
        if session_file_fingerprint(source_path)? != source_before {
            return Err("源会话在读取期间发生了更新，本次操作已取消，请刷新后重试".to_string());
        }

        let target_provider = self.read_target_provider()?;
        let created_at = chrono::Utc::now();
        let target_session_id = new_session_id();
        let target_meta = copied_session_meta(
            &source_content,
            &target_session_id,
            &target_provider,
            created_at,
        )?;
        let next_content = copy_history_onto_target(&source_content, &target_meta)?;
        if session_file_fingerprint(source_path)? != source_before {
            return Err("源会话在复制准备期间发生了更新，本次操作已取消，请刷新后重试".to_string());
        }
        let target_path =
            new_session_rollout_path(&self.sessions_dir(), &target_session_id, created_at);
        create_new_session_file(&target_path, next_content.as_bytes())?;

        let mut warnings = Vec::new();
        if let Err(error) = self.repair_visibility_with_options(
            Some("deep"),
            Some(target_provider),
            None,
            None,
            Some(vec![target_session_id.clone()]),
        ) {
            warnings.push(format!("新会话已创建，但同步 Codex 索引失败: {error}"));
        }
        let title = copied_session_title(&source.title, copy_suffix);
        if let Err(error) = self.write_session_title(&target_session_id, &title) {
            warnings.push(error);
        }
        if let Err(error) = self.update_sqlite_session_title(&target_session_id, &title) {
            warnings.push(error);
        }

        Ok(CodexSessionMutationResult {
            session_id: target_session_id,
            title,
            project_path: Some(source.project_path.clone()),
            backup_path: None,
            warnings,
        })
    }

    pub fn rename_session(
        &self,
        session_id: &str,
        title: &str,
    ) -> Result<CodexSessionMutationResult, String> {
        let session_id = session_id.trim();
        let title = normalize_custom_session_title(title)?;
        if session_id.is_empty() {
            return Err("会话不能为空".to_string());
        }
        if !self
            .list_sessions(None, None)?
            .iter()
            .any(|session| session.id == session_id)
        {
            return Err(format!("会话不存在: {}", session_id));
        }

        self.write_session_title(session_id, &title)?;
        let mut warnings = Vec::new();
        if let Err(error) = self.update_sqlite_session_title(session_id, &title) {
            warnings.push(error);
        }
        Ok(CodexSessionMutationResult {
            session_id: session_id.to_string(),
            title,
            project_path: None,
            backup_path: None,
            warnings,
        })
    }

    pub fn update_session_working_directory(
        &self,
        session_id: &str,
        project_path: &str,
    ) -> Result<CodexSessionMutationResult, String> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Err("会话不能为空".to_string());
        }
        let project_path = project_path.trim();
        if project_path.is_empty() {
            return Err("工作目录不能为空".to_string());
        }
        let directory = PathBuf::from(project_path);
        if !directory.is_absolute() {
            return Err("工作目录必须使用绝对路径".to_string());
        }
        if !directory.is_dir() {
            return Err(format!(
                "工作目录不存在或不是文件夹: {}",
                directory.display()
            ));
        }
        let normalized_path = directory.to_string_lossy().to_string();

        let session = self
            .list_sessions(None, None)?
            .into_iter()
            .find(|session| session.id == session_id)
            .ok_or_else(|| format!("会话不存在: {}", session_id))?;
        let session_path = Path::new(&session.path);
        let content =
            fs::read_to_string(session_path).map_err(|error| format!("读取会话失败: {}", error))?;
        let next_content = rewrite_session_meta_cwd(&content, &normalized_path)?;
        let backup_path = if let Some(next_content) = next_content {
            let backup_path = self.backup_session_file(session_path, "cwd")?;
            write_bytes_atomic(session_path, next_content.as_bytes())
                .map_err(|error| format!("写入会话工作目录失败: {}", error))?;
            Some(backup_path.to_string_lossy().to_string())
        } else {
            None
        };

        let mut warnings = Vec::new();
        if let Err(error) = self.update_sqlite_session_cwd(session_id, &normalized_path) {
            warnings.push(error);
        }
        Ok(CodexSessionMutationResult {
            session_id: session_id.to_string(),
            title: session.title,
            project_path: Some(normalized_path),
            backup_path,
            warnings,
        })
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
        let mut moved_sessions = Vec::new();
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
                    let metadata_content = serde_json::to_vec_pretty(&metadata)
                        .map_err(|error| format!("序列化回收站记录失败: {}", error))?;
                    if let Err(error) = write_bytes_atomic(&metadata_path, &metadata_content) {
                        let rollback = fs::rename(&trash_path, &source);
                        failed.push(match rollback {
                            Ok(()) => format!("写入回收站记录失败 {}: {}", session.id, error),
                            Err(rollback_error) => format!(
                                "写入回收站记录失败 {}: {}；同时回滚会话文件失败: {}",
                                session.id, error, rollback_error
                            ),
                        });
                        continue;
                    }
                    moved_sessions.push((session.clone(), trash_path, metadata_path));
                }
                Err(error) => failed.push(format!("移动失败 {}: {}", session.id, error)),
            }
        }
        if !moved_sessions.is_empty() {
            let moved_ids = moved_sessions
                .iter()
                .map(|(session, _, _)| session.id.clone())
                .collect::<Vec<_>>();
            if let Err(error) = self.hide_sessions_from_codex_indexes(&moved_ids) {
                let mut rolled_back_ids = Vec::new();
                let mut rollback_failures = Vec::new();
                for (session, trash_path, metadata_path) in &moved_sessions {
                    match fs::rename(trash_path, &session.path) {
                        Ok(()) => {
                            let _ = fs::remove_file(metadata_path);
                            rolled_back_ids.push(session.id.clone());
                        }
                        Err(rollback_error) => {
                            rollback_failures.push(format!("{}: {}", session.id, rollback_error))
                        }
                    }
                }
                if !rolled_back_ids.is_empty() {
                    if let Err(repair_error) =
                        self.restore_sessions_to_codex_indexes(&rolled_back_ids)
                    {
                        rollback_failures.push(format!("恢复 Codex 索引失败: {}", repair_error));
                    }
                }
                failed.push(if rollback_failures.is_empty() {
                    format!("同步 Codex 会话列表失败，已回滚文件移动: {}", error)
                } else {
                    format!(
                        "同步 Codex 会话列表失败: {}；部分回滚失败: {}",
                        error,
                        rollback_failures.join("；")
                    )
                });
                moved_sessions.retain(|(_, trash_path, _)| trash_path.exists());
            }
        }
        Ok(CodexSessionTrashSummary {
            moved: moved_sessions.len(),
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
                let stored_trash_path = read_string(&value, "trashPath").unwrap_or_default();
                let trash_path =
                    if stored_trash_path.is_empty() || !Path::new(&stored_trash_path).exists() {
                        path.with_extension("jsonl")
                    } else {
                        PathBuf::from(stored_trash_path)
                    };
                if !trash_path.is_file() {
                    continue;
                }
                if !seen.insert(id.clone()) {
                    continue;
                }
                records.push(CodexTrashedSessionRecord {
                    id,
                    title: read_string(&value, "title").unwrap_or_else(|| "未命名会话".to_string()),
                    original_path: read_string(&value, "originalPath").unwrap_or_default(),
                    trash_path: trash_path.to_string_lossy().to_string(),
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
        let mut restored_ids = Vec::new();
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
                    restored_ids.push(session_id.clone());
                }
                Err(error) => failed.push(format!("恢复失败 {}: {}", session_id, error)),
            }
        }
        if !restored_ids.is_empty() {
            if let Err(error) = self.restore_sessions_to_codex_indexes(&restored_ids) {
                failed.push(format!(
                    "会话文件已恢复，但同步 Codex 会话列表失败: {}",
                    error
                ));
            }
        }
        Ok(CodexSessionTrashSummary {
            moved: 0,
            restored: restored_ids.len(),
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
                updated_catalog_row_count: 0,
                verified_visible_session_count: 0,
                skipped_non_sidebar_session_count: 0,
                remaining_invisible_session_count: 0,
                created_local_project_count: 0,
                assigned_local_project_session_count: 0,
                verified_local_project_count: 0,
                skipped_local_project_session_count: 0,
                recreated_generated_image_count: 0,
                verified_generated_image_count: 0,
                invalid_generated_image_count: 0,
                reset_history_projection_count: 0,
                desktop_reload_required: false,
                desktop_reload_performed: false,
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
        let (mut changed_rollout_files, mut rollout_backup_dirs) =
            self.repair_rollout_visibility(&target_provider, &repair_records, deep)?;
        let (updated_rows, backup_dirs) =
            self.repair_sqlite_visibility(&target_provider, Some(&repair_records))?;
        let added_index_entries = if deep {
            self.repair_session_index(&repair_records)?
        } else {
            0
        };
        let catalog = self.repair_desktop_catalog(&target_provider, &repair_records)?;
        if !catalog.missing_ids.is_empty() {
            return Err(format!(
                "Codex 侧栏目录校验失败，仍有 {} 条主会话不可见：{}",
                catalog.missing_ids.len(),
                catalog.missing_ids.join(", ")
            ));
        }
        let projects = self.repair_desktop_projects(&repair_records)?;
        let mut generated_images = self.repair_generated_images(&repair_records)?;
        let local_images = if deep {
            self.repair_local_image_attachments(&repair_records)?
        } else {
            LocalImageRepairResult::default()
        };
        changed_rollout_files += local_images.changed_rollout_files;
        rollout_backup_dirs.extend(local_images.backup_dirs);
        generated_images.add(local_images.images);
        let (reset_history_projections, history_backup_dirs) = if deep {
            self.reset_stale_thread_history_projections(
                &repair_records,
                &local_images.changed_session_ids,
            )?
        } else {
            (0, Vec::new())
        };
        let repaired = changed_rollout_files
            + updated_rows
            + added_index_entries
            + catalog.updated
            + projects.created
            + projects.updated_assignments
            + generated_images.recreated
            + reset_history_projections;
        let mut all_backup_dirs = rollout_backup_dirs;
        all_backup_dirs.extend(backup_dirs);
        all_backup_dirs.extend(history_backup_dirs);
        if let Some(backup_dir) = catalog.backup_dir.clone() {
            all_backup_dirs.push(backup_dir);
        }
        if let Some(backup_dir) = projects.backup_dir.clone() {
            all_backup_dirs.push(backup_dir);
        }
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
                "updatedCatalogRows": catalog.updated,
                "verifiedVisible": catalog.verified,
                "skippedNonSidebar": catalog.skipped,
                "remainingInvisible": catalog.missing_ids.len(),
                "createdLocalProjects": projects.created,
                "assignedLocalProjectSessions": projects.verified_assignments,
                "verifiedLocalProjects": projects.verified_projects,
                "skippedLocalProjectSessions": projects.skipped,
                "recreatedGeneratedImages": generated_images.recreated,
                "verifiedGeneratedImages": generated_images.verified,
                "invalidGeneratedImages": generated_images.invalid,
                "resetHistoryProjections": reset_history_projections,
                "desktopReloadRequired": !sessions.is_empty(),
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
            updated_catalog_row_count: catalog.updated,
            verified_visible_session_count: catalog.verified,
            skipped_non_sidebar_session_count: catalog.skipped,
            remaining_invisible_session_count: catalog.missing_ids.len(),
            created_local_project_count: projects.created,
            assigned_local_project_session_count: projects.verified_assignments,
            verified_local_project_count: projects.verified_projects,
            skipped_local_project_session_count: projects.skipped,
            recreated_generated_image_count: generated_images.recreated,
            verified_generated_image_count: generated_images.verified,
            invalid_generated_image_count: generated_images.invalid,
            reset_history_projection_count: reset_history_projections,
            desktop_reload_required: !sessions.is_empty(),
            desktop_reload_performed: false,
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
                "已修复 Codex 会话、项目目录与图片：校正 {} 个会话文件，重置 {} 条分页历史投影，更新 {} 条线程记录，同步 {} 条侧栏目录，目录校验通过 {} 条；创建 {} 个本地项目，归组 {} 条会话，项目校验通过 {} 个，恢复 {} 张图片、校验 {} 张，跳过 {} 条非侧栏或无效目录会话；需要重载 ChatGPT/Codex 才会刷新当前侧栏",
                changed_rollout_files,
                reset_history_projections,
                updated_rows,
                catalog.updated,
                catalog.verified,
                projects.created,
                projects.verified_assignments,
                projects.verified_projects,
                generated_images.recreated,
                generated_images.verified,
                projects.skipped
            ),
        })
    }

    fn repair_desktop_catalog(
        &self,
        target_provider: &str,
        sessions: &[SessionRepairRecord],
    ) -> Result<DesktopCatalogRepairResult, String> {
        let db_path = self.codex_home.join("sqlite").join("codex-dev.db");
        if !db_path.exists() || sessions.is_empty() {
            return Ok(DesktopCatalogRepairResult::default());
        }
        let mut connection = Connection::open(&db_path).map_err(|error| {
            format!("打开 Codex 侧栏目录失败 ({}): {}", db_path.display(), error)
        })?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| format!("设置 Codex 侧栏目录等待时间失败: {}", error))?;
        let columns = sqlite_table_columns_with_connection(&connection, "local_thread_catalog")?;
        let required = [
            "host_id",
            "thread_id",
            "display_title",
            "source_created_at",
            "source_updated_at",
            "cwd",
            "source_kind",
            "model_provider",
            "observation_sequence",
            "missing_candidate",
            "source_recency_at",
        ];
        if required.iter().any(|column| !columns.contains(*column)) {
            return Ok(DesktopCatalogRepairResult::default());
        }

        let mut eligible = Vec::new();
        let mut skipped = 0usize;
        for session in sessions {
            let metadata = sqlite_metadata_for_session(session);
            let Some(source_kind) = sidebar_source_kind(&metadata.source) else {
                skipped += 1;
                continue;
            };
            eligible.push((session, metadata, source_kind));
        }
        if eligible.is_empty() {
            return Ok(DesktopCatalogRepairResult {
                skipped,
                ..Default::default()
            });
        }

        let backup_dir = backup_sqlite_file(&db_path)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开启 Codex 侧栏目录事务失败: {}", error))?;
        let observation_sequence = transaction
            .query_row(
                "SELECT COALESCE(MAX(observation_sequence), 0) + 1 FROM local_thread_catalog",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| format!("读取 Codex 侧栏目录序号失败: {}", error))?;

        if sqlite_table_exists_with_connection(&transaction, "local_thread_catalog_hosts")? {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO local_thread_catalog_hosts (host_id, host_kind) VALUES ('local', 'local')",
                    [],
                )
                .map_err(|error| format!("登记 Codex 本地目录失败: {}", error))?;
        }

        let has_source_detail = columns.contains("source_detail");
        let has_git_branch = columns.contains("git_branch");
        let has_thread_source = columns.contains("thread_source");
        let has_pending_title = columns.contains("pending_observed_title");
        let mut updated = 0usize;
        for (session, metadata, source_kind) in &eligible {
            let mut names = vec![
                "host_id",
                "thread_id",
                "display_title",
                "source_created_at",
                "source_updated_at",
                "cwd",
                "source_kind",
                "model_provider",
                "observation_sequence",
                "missing_candidate",
                "source_recency_at",
            ];
            let mut placeholders = vec![
                "?1", "?2", "?3", "?4", "?5", "?6", "?7", "?8", "?9", "0", "?5",
            ];
            if has_source_detail {
                names.push("source_detail");
                placeholders.push("NULL");
            }
            if has_git_branch {
                names.push("git_branch");
                placeholders.push("NULL");
            }
            if has_thread_source {
                names.push("thread_source");
                placeholders.push("'user'");
            }
            if has_pending_title {
                names.push("pending_observed_title");
                placeholders.push("0");
            }
            let updates = names
                .iter()
                .filter(|name| **name != "host_id" && **name != "thread_id")
                .map(|name| format!("{name} = excluded.{name}"))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "INSERT INTO local_thread_catalog ({}) VALUES ({}) \
                 ON CONFLICT(host_id, thread_id) DO UPDATE SET {}",
                names.join(", "),
                placeholders.join(", "),
                updates
            );
            updated += transaction
                .execute(
                    &sql,
                    params![
                        "local",
                        session.id,
                        session.title,
                        metadata.created_at,
                        session.updated_at,
                        metadata.cwd,
                        source_kind,
                        target_provider,
                        observation_sequence,
                    ],
                )
                .map_err(|error| format!("同步 Codex 侧栏目录失败 ({}): {}", session.id, error))?;
        }

        if sqlite_table_exists_with_connection(&transaction, "local_thread_catalog_metadata")? {
            transaction
                .execute(
                    "INSERT INTO local_thread_catalog_metadata (id, catalog_revision) VALUES (1, 1) \
                     ON CONFLICT(id) DO UPDATE SET catalog_revision = catalog_revision + 1",
                    [],
                )
                .map_err(|error| format!("更新 Codex 侧栏目录版本失败: {}", error))?;
        }
        if sqlite_table_exists_with_connection(&transaction, "local_thread_catalog_sync_state")? {
            let sync_columns = sqlite_table_columns_with_connection(
                &transaction,
                "local_thread_catalog_sync_state",
            )?;
            if sync_columns.contains("host_id") && sync_columns.contains("observation_sequence") {
                transaction
                    .execute(
                        "INSERT INTO local_thread_catalog_sync_state (host_id, observation_sequence) VALUES ('local', ?1) \
                         ON CONFLICT(host_id) DO UPDATE SET observation_sequence = MAX(observation_sequence, excluded.observation_sequence)",
                        params![observation_sequence],
                    )
                    .map_err(|error| format!("更新 Codex 侧栏同步状态失败: {}", error))?;
            }
        }

        let mut missing_ids = Vec::new();
        for (session, _, _) in &eligible {
            let visible = transaction
                .query_row(
                    "SELECT COUNT(*) FROM local_thread_catalog \
                     WHERE host_id = 'local' AND thread_id = ?1 AND missing_candidate = 0",
                    params![session.id],
                    |row| row.get::<_, usize>(0),
                )
                .map_err(|error| format!("校验 Codex 侧栏目录失败: {}", error))?;
            if visible != 1 {
                missing_ids.push(session.id.clone());
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("提交 Codex 侧栏目录修复失败: {}", error))?;
        Ok(DesktopCatalogRepairResult {
            updated,
            verified: eligible.len().saturating_sub(missing_ids.len()),
            skipped,
            missing_ids,
            backup_dir: Some(backup_dir),
        })
    }

    fn repair_desktop_projects(
        &self,
        sessions: &[SessionRepairRecord],
    ) -> Result<DesktopProjectRepairResult, String> {
        if sessions.is_empty() {
            return Ok(DesktopProjectRepairResult::default());
        }
        let mut candidates = Vec::new();
        let mut skipped = 0usize;
        for session in sessions {
            let metadata = sqlite_metadata_for_session(session);
            if sidebar_source_kind(&metadata.source).is_none() {
                skipped += 1;
                continue;
            }
            let cwd = metadata.cwd.trim();
            if cwd.is_empty() || cwd == "/" || cwd == "~" || !Path::new(cwd).is_dir() {
                skipped += 1;
                continue;
            }
            candidates.push((session.id.clone(), cwd.to_string()));
        }
        if candidates.is_empty() {
            return Ok(DesktopProjectRepairResult {
                skipped,
                ..Default::default()
            });
        }

        let state_path = self.codex_home.join(".codex-global-state.json");
        let mut state = if state_path.is_file() {
            let bytes = fs::read(&state_path).map_err(|error| {
                format!(
                    "读取 Codex 项目状态失败 ({}): {}",
                    state_path.display(),
                    error
                )
            })?;
            serde_json::from_slice::<Value>(&bytes).map_err(|error| {
                format!(
                    "解析 Codex 项目状态失败 ({}): {}",
                    state_path.display(),
                    error
                )
            })?
        } else {
            serde_json::json!({})
        };
        let state_object = state
            .as_object_mut()
            .ok_or_else(|| "Codex 项目状态不是 JSON 对象，已停止修复以避免覆盖".to_string())?;
        let local_projects = state_object
            .entry("local-projects".to_string())
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| "Codex local-projects 状态格式无效".to_string())?;
        let mut project_id_by_root = HashMap::new();
        for (project_id, project) in local_projects.iter() {
            let Some(root_paths) = project.get("rootPaths").and_then(Value::as_array) else {
                continue;
            };
            for root in root_paths.iter().filter_map(Value::as_str) {
                if !root.trim().is_empty() {
                    project_id_by_root
                        .entry(root.to_string())
                        .or_insert_with(|| project_id.clone());
                }
            }
        }

        let now_ms = now_timestamp().saturating_mul(1_000);
        let mut created = 0usize;
        for (_, cwd) in &candidates {
            if project_id_by_root.contains_key(cwd) {
                continue;
            }
            let mut project_id = desktop_project_id(cwd);
            let mut collision_index = 0usize;
            while local_projects.contains_key(&project_id) {
                collision_index += 1;
                project_id = desktop_project_id(&format!("{cwd}#{collision_index}"));
            }
            let name = project_name_for_path(cwd).unwrap_or_else(|| "Project".to_string());
            local_projects.insert(
                project_id.clone(),
                serde_json::json!({
                    "id": project_id,
                    "name": name,
                    "rootPaths": [cwd],
                    "createdAt": now_ms,
                    "updatedAt": now_ms
                }),
            );
            project_id_by_root.insert(cwd.clone(), project_id);
            created += 1;
        }

        let mut project_order = state_object
            .get("project-order")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut ordered_project_ids = project_order
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<HashSet<_>>();
        for (_, cwd) in &candidates {
            let Some(project_id) = project_id_by_root.get(cwd) else {
                continue;
            };
            if ordered_project_ids.insert(project_id.clone()) {
                project_order.push(Value::String(project_id.clone()));
            }
        }
        state_object.insert("project-order".to_string(), Value::Array(project_order));

        let assignments = state_object
            .entry("thread-project-assignments".to_string())
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| "Codex thread-project-assignments 状态格式无效".to_string())?;
        let mut updated_assignments = 0usize;
        for (session_id, cwd) in &candidates {
            let Some(project_id) = project_id_by_root.get(cwd) else {
                continue;
            };
            let assignment = serde_json::json!({
                "projectKind": "local",
                "projectId": project_id
            });
            if assignments.get(session_id) != Some(&assignment) {
                assignments.insert(session_id.clone(), assignment);
                updated_assignments += 1;
            }
        }
        let verified_projects = candidates
            .iter()
            .map(|(_, cwd)| cwd)
            .collect::<HashSet<_>>()
            .into_iter()
            .filter(|cwd| project_id_by_root.contains_key(*cwd))
            .count();
        let verified_assignments = candidates
            .iter()
            .filter(|(session_id, cwd)| {
                let Some(project_id) = project_id_by_root.get(cwd) else {
                    return false;
                };
                assignments
                    .get(session_id)
                    .and_then(|assignment| assignment.get("projectId"))
                    .and_then(Value::as_str)
                    == Some(project_id.as_str())
            })
            .count();
        let assigned_ids = candidates
            .iter()
            .map(|(session_id, _)| session_id.as_str())
            .collect::<HashSet<_>>();
        if let Some(projectless) = state_object
            .get_mut("projectless-thread-ids")
            .and_then(Value::as_array_mut)
        {
            projectless.retain(|value| {
                value
                    .as_str()
                    .is_none_or(|session_id| !assigned_ids.contains(session_id))
            });
        }
        let serialized = serde_json::to_vec(&state)
            .map_err(|error| format!("序列化 Codex 项目状态失败: {}", error))?;
        let backup_dir = if state_path.is_file() {
            let backup_dir = visibility_backup_dir();
            fs::create_dir_all(&backup_dir)
                .map_err(|error| format!("创建项目状态备份目录失败: {}", error))?;
            fs::copy(&state_path, backup_dir.join(".codex-global-state.json"))
                .map_err(|error| format!("备份 Codex 项目状态失败: {}", error))?;
            Some(backup_dir.to_string_lossy().to_string())
        } else {
            None
        };
        write_bytes_atomic(
            &state_path.with_file_name(".codex-global-state.json.bak"),
            &serialized,
        )
        .map_err(|error| format!("写入 Codex 项目状态备份失败: {}", error))?;
        write_bytes_atomic(&state_path, &serialized)
            .map_err(|error| format!("写入 Codex 项目状态失败: {}", error))?;

        Ok(DesktopProjectRepairResult {
            created,
            updated_assignments,
            verified_projects,
            verified_assignments,
            skipped,
            backup_dir,
        })
    }

    fn repair_generated_images(
        &self,
        sessions: &[SessionRepairRecord],
    ) -> Result<GeneratedImageRepairResult, String> {
        let mut result = GeneratedImageRepairResult::default();
        let mut handled = HashSet::new();
        for session in sessions {
            if !safe_generated_image_id(&session.id) {
                result.invalid += 1;
                continue;
            }
            let file = match fs::File::open(&session.path) {
                Ok(file) => file,
                Err(_) => continue,
            };
            for line in BufReader::new(file).lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(_) => continue,
                };
                let value: Value = match serde_json::from_str(&line) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let Some((image_id, encoded)) = generated_image_payload(&value) else {
                    continue;
                };
                if !handled.insert((session.id.clone(), image_id.to_string())) {
                    continue;
                }
                if !safe_generated_image_id(image_id) {
                    result.invalid += 1;
                    continue;
                }
                if let Some(extension) = encoded_generated_image_extension(encoded) {
                    let image_path = self
                        .codex_home
                        .join("generated_images")
                        .join(&session.id)
                        .join(format!("{image_id}.{extension}"));
                    if generated_image_extension_from_file(&image_path) == Some(extension) {
                        result.verified += 1;
                        continue;
                    }
                }
                let Some((bytes, extension)) = decode_generated_image(encoded) else {
                    result.invalid += 1;
                    continue;
                };
                let image_path = self
                    .codex_home
                    .join("generated_images")
                    .join(&session.id)
                    .join(format!("{image_id}.{extension}"));
                let already_valid =
                    generated_image_extension_from_file(&image_path) == Some(extension);
                if !already_valid {
                    if let Some(parent) = image_path.parent() {
                        fs::create_dir_all(parent).map_err(|error| {
                            format!("创建生成图片恢复目录失败 ({}): {}", parent.display(), error)
                        })?;
                    }
                    write_bytes_atomic(&image_path, &bytes).map_err(|error| {
                        format!("恢复生成图片失败 ({}): {}", image_path.display(), error)
                    })?;
                    result.recreated += 1;
                }
                if generated_image_extension_from_file(&image_path) == Some(extension) {
                    result.verified += 1;
                } else {
                    result.invalid += 1;
                }
            }
        }
        Ok(result)
    }

    fn repair_local_image_attachments(
        &self,
        sessions: &[SessionRepairRecord],
    ) -> Result<LocalImageRepairResult, String> {
        let mut result = LocalImageRepairResult::default();
        let backup_dir = visibility_backup_dir();
        let mut backup_created = false;
        for session in sessions {
            if !safe_generated_image_id(&session.id) {
                result.images.invalid += 1;
                continue;
            }
            let content = match fs::read_to_string(&session.path) {
                Ok(content) => content,
                Err(_) => continue,
            };
            let (rewritten, image_result) =
                rewrite_local_image_attachments(&content, &self.codex_home, &session.id)?;
            result.images.add(image_result);
            let Some(rewritten) = rewritten else {
                continue;
            };
            if !backup_created {
                fs::create_dir_all(&backup_dir)
                    .map_err(|error| format!("创建图片恢复备份目录失败: {error}"))?;
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
                format!("备份图片会话文件失败 ({}): {error}", session.path.display())
            })?;
            write_bytes_atomic(&session.path, rewritten.as_bytes()).map_err(|error| {
                format!(
                    "写入图片恢复会话文件失败 ({}): {error}",
                    session.path.display()
                )
            })?;
            let _ = Command::new("touch")
                .arg("-r")
                .arg(&backup_path)
                .arg(&session.path)
                .output();
            result.changed_rollout_files += 1;
            result.changed_session_ids.insert(session.id.clone());
        }
        if backup_created {
            result
                .backup_dirs
                .push(backup_dir.to_string_lossy().to_string());
        }
        Ok(result)
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

    fn read_session_titles(&self) -> std::collections::HashMap<String, String> {
        let path = self.codex_home.join("session_index.jsonl");
        let Ok(content) = fs::read_to_string(path) else {
            return std::collections::HashMap::new();
        };
        content
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|value| {
                let id = read_string(&value, "id")?;
                let title = ["thread_name", "title", "name"]
                    .iter()
                    .find_map(|key| read_string(&value, key))
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())?;
                Some((id, title))
            })
            .collect()
    }

    fn write_session_title(&self, session_id: &str, title: &str) -> Result<(), String> {
        let path = self.codex_home.join("session_index.jsonl");
        let content = fs::read_to_string(&path).unwrap_or_default();
        let now = chrono::Utc::now();
        let now_seconds = now.timestamp();
        let now_millis = now.timestamp_millis();
        let now_rfc3339 = now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
        let mut found = false;
        let mut lines = Vec::new();
        for line in content.lines() {
            let Ok(mut value) = serde_json::from_str::<Value>(line) else {
                lines.push(line.to_string());
                continue;
            };
            if read_string(&value, "id").as_deref() != Some(session_id) {
                lines.push(line.to_string());
                continue;
            }
            found = true;
            let Some(object) = value.as_object_mut() else {
                lines.push(line.to_string());
                continue;
            };
            object.insert("thread_name".to_string(), Value::String(title.to_string()));
            if object.contains_key("title") {
                object.insert("title".to_string(), Value::String(title.to_string()));
            }
            let updated_at = match object.get("updated_at") {
                Some(Value::Number(_)) => Value::from(now_seconds),
                _ => Value::String(now_rfc3339.clone()),
            };
            object.insert("updated_at".to_string(), updated_at);
            if object.contains_key("updated_at_ms") {
                object.insert("updated_at_ms".to_string(), Value::from(now_millis));
            }
            lines.push(
                serde_json::to_string(&value)
                    .map_err(|error| format!("序列化会话名称失败: {}", error))?,
            );
        }
        if !found {
            lines.push(
                serde_json::to_string(&serde_json::json!({
                    "id": session_id,
                    "thread_name": title,
                    "updated_at": now_rfc3339
                }))
                .map_err(|error| format!("序列化会话名称失败: {}", error))?,
            );
        }
        let mut next = lines.join("\n");
        next.push('\n');
        write_bytes_atomic(&path, next.as_bytes())
            .map_err(|error| format!("保存会话名称失败: {}", error))
    }

    fn update_sqlite_session_title(&self, session_id: &str, title: &str) -> Result<(), String> {
        for db_path in self.sqlite_candidate_paths() {
            let connection = Connection::open(&db_path).map_err(|error| {
                format!("打开会话 SQLite 失败 ({}): {}", db_path.display(), error)
            })?;
            let columns = sqlite_thread_columns_with_connection(&connection)?;
            let mut assignments = Vec::new();
            if columns.contains("title") {
                assignments.push("title = ?1");
            }
            if columns.contains("name") {
                assignments.push("name = ?1");
            }
            if assignments.is_empty() {
                continue;
            }
            connection
                .execute(
                    &format!(
                        "UPDATE threads SET {} WHERE id = ?2",
                        assignments.join(", ")
                    ),
                    params![title, session_id],
                )
                .map_err(|error| format!("同步会话名称失败 ({}): {}", db_path.display(), error))?;
        }
        Ok(())
    }

    fn update_sqlite_session_cwd(&self, session_id: &str, cwd: &str) -> Result<(), String> {
        for db_path in self.sqlite_candidate_paths() {
            let connection = Connection::open(&db_path).map_err(|error| {
                format!("打开会话 SQLite 失败 ({}): {}", db_path.display(), error)
            })?;
            let columns = sqlite_thread_columns_with_connection(&connection)?;
            if !columns.contains("cwd") {
                continue;
            }
            connection
                .execute(
                    "UPDATE threads SET cwd = ?1 WHERE id = ?2",
                    params![cwd, session_id],
                )
                .map_err(|error| {
                    format!("同步会话工作目录失败 ({}): {}", db_path.display(), error)
                })?;
        }
        Ok(())
    }

    pub(crate) fn hide_sessions_from_codex_indexes(
        &self,
        session_ids: &[String],
    ) -> Result<(), String> {
        self.set_sqlite_session_visibility(session_ids, false)?;
        self.remove_session_index_entries(session_ids)?;
        Ok(())
    }

    pub(crate) fn codex_indexes_contain_visible_sessions(
        &self,
        session_ids: &[String],
    ) -> Result<bool, String> {
        if session_ids.is_empty() {
            return Ok(false);
        }
        if session_ids.len() > 400 {
            for chunk in session_ids.chunks(400) {
                if self.codex_indexes_contain_visible_sessions(chunk)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let ids = session_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let index_path = self.codex_home.join("session_index.jsonl");
        if index_path.exists() {
            let content = fs::read_to_string(&index_path)
                .map_err(|error| format!("读取 session_index.jsonl 失败: {}", error))?;
            if content.lines().any(|line| {
                serde_json::from_str::<Value>(line)
                    .ok()
                    .and_then(|value| read_string(&value, "id"))
                    .is_some_and(|id| ids.contains(id.as_str()))
            }) {
                return Ok(true);
            }
        }

        let placeholders = std::iter::repeat_n("?", session_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        for db_path in self.sqlite_candidate_paths() {
            let connection = Connection::open(&db_path).map_err(|error| {
                format!("打开 Codex 会话索引失败 ({}): {}", db_path.display(), error)
            })?;
            connection
                .busy_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| format!("设置 Codex 会话索引等待时间失败: {}", error))?;
            let thread_columns = sqlite_thread_columns_with_connection(&connection)?;
            if thread_columns.contains("id") {
                let visible_predicate = if thread_columns.contains("archived") {
                    " AND COALESCE(archived, 0) <> 1"
                } else {
                    ""
                };
                let count = connection
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM threads WHERE id IN ({}){}",
                            placeholders, visible_predicate
                        ),
                        rusqlite::params_from_iter(session_ids.iter()),
                        |row| row.get::<_, usize>(0),
                    )
                    .map_err(|error| {
                        format!(
                            "检查 Codex 线程可见性失败 ({}): {}",
                            db_path.display(),
                            error
                        )
                    })?;
                if count > 0 {
                    return Ok(true);
                }
            }
            let catalog_columns =
                sqlite_table_columns_with_connection(&connection, "local_thread_catalog")?;
            if catalog_columns.contains("thread_id") {
                let local_only = if catalog_columns.contains("host_id") {
                    " AND host_id = 'local'"
                } else {
                    ""
                };
                let visible_only = if catalog_columns.contains("missing_candidate") {
                    " AND COALESCE(missing_candidate, 0) <> 1"
                } else {
                    ""
                };
                let count = connection
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM local_thread_catalog WHERE thread_id IN ({}){}{}",
                            placeholders, local_only, visible_only
                        ),
                        rusqlite::params_from_iter(session_ids.iter()),
                        |row| row.get::<_, usize>(0),
                    )
                    .map_err(|error| {
                        format!(
                            "检查 Codex 侧栏可见性失败 ({}): {}",
                            db_path.display(),
                            error
                        )
                    })?;
                if count > 0 {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn restore_sessions_to_codex_indexes(&self, session_ids: &[String]) -> Result<(), String> {
        self.set_sqlite_session_visibility(session_ids, true)?;
        let selected_ids = session_ids.iter().cloned().collect::<HashSet<_>>();
        let sessions = self
            .list_sessions(None, None)?
            .into_iter()
            .filter(|session| selected_ids.contains(&session.id))
            .collect::<Vec<_>>();
        if sessions.len() != selected_ids.len() {
            let found = sessions
                .iter()
                .map(|session| session.id.as_str())
                .collect::<HashSet<_>>();
            let missing = selected_ids
                .iter()
                .filter(|id| !found.contains(id.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            return Err(format!("恢复后的会话文件不可读: {}", missing.join(", ")));
        }
        let repair_records = sessions
            .iter()
            .map(|session| SessionRepairRecord {
                id: session.id.clone(),
                title: session.title.clone(),
                path: PathBuf::from(&session.path),
                updated_at: session.updated_at,
            })
            .collect::<Vec<_>>();
        let target_provider = self.read_target_provider()?;
        self.repair_sqlite_visibility(&target_provider, Some(&repair_records))?;
        self.repair_session_index(&repair_records)?;
        let catalog = self.repair_desktop_catalog(&target_provider, &repair_records)?;
        if !catalog.missing_ids.is_empty() {
            return Err(format!(
                "恢复 Codex 侧栏目录失败: {}",
                catalog.missing_ids.join(", ")
            ));
        }
        self.repair_desktop_projects(&repair_records)?;
        Ok(())
    }

    fn set_sqlite_session_visibility(
        &self,
        session_ids: &[String],
        visible: bool,
    ) -> Result<usize, String> {
        if session_ids.is_empty() {
            return Ok(0);
        }
        if session_ids.len() > 400 {
            let mut updated = 0usize;
            for chunk in session_ids.chunks(400) {
                updated += self.set_sqlite_session_visibility(chunk, visible)?;
            }
            return Ok(updated);
        }
        let placeholders = std::iter::repeat_n("?", session_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let mut updated = 0usize;
        for db_path in self.sqlite_candidate_paths() {
            let connection = Connection::open(&db_path).map_err(|error| {
                format!("打开 Codex 会话索引失败 ({}): {}", db_path.display(), error)
            })?;
            let thread_columns = sqlite_thread_columns_with_connection(&connection)?;
            let catalog_columns =
                sqlite_table_columns_with_connection(&connection, "local_thread_catalog")?;
            let has_threads = thread_columns.contains("id");
            let has_catalog = catalog_columns.contains("thread_id");
            if !has_threads && !has_catalog {
                continue;
            }
            drop(connection);
            backup_sqlite_file(&db_path)?;

            let mut connection = Connection::open(&db_path).map_err(|error| {
                format!(
                    "重新打开 Codex 会话索引失败 ({}): {}",
                    db_path.display(),
                    error
                )
            })?;
            connection
                .busy_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| format!("设置 Codex 会话索引等待时间失败: {}", error))?;
            let transaction = connection
                .transaction()
                .map_err(|error| format!("开启 Codex 会话索引事务失败: {}", error))?;

            if has_threads {
                let sql = if visible {
                    let mut assignments = Vec::new();
                    let mut predicates = Vec::new();
                    if thread_columns.contains("archived") {
                        assignments.push("archived = 0");
                        predicates.push("COALESCE(archived, 0) <> 0");
                    }
                    if thread_columns.contains("archived_at") {
                        assignments.push("archived_at = NULL");
                        predicates.push("archived_at IS NOT NULL");
                    }
                    (!assignments.is_empty()).then(|| {
                        format!(
                            "UPDATE threads SET {} WHERE id IN ({}) AND ({})",
                            assignments.join(", "),
                            placeholders,
                            predicates.join(" OR ")
                        )
                    })
                } else if thread_columns.contains("archived") {
                    let mut assignments = vec!["archived = 1".to_string()];
                    if thread_columns.contains("archived_at") {
                        assignments.push(format!("archived_at = {}", now_timestamp()));
                    }
                    Some(format!(
                        "UPDATE threads SET {} WHERE id IN ({}) AND COALESCE(archived, 0) <> 1",
                        assignments.join(", "),
                        placeholders
                    ))
                } else {
                    Some(format!(
                        "DELETE FROM threads WHERE id IN ({})",
                        placeholders
                    ))
                };
                if let Some(sql) = sql {
                    updated += transaction
                        .execute(&sql, rusqlite::params_from_iter(session_ids.iter()))
                        .map_err(|error| {
                            format!(
                                "更新 Codex 线程可见性失败 ({}): {}",
                                db_path.display(),
                                error
                            )
                        })?;
                }
            }

            let mut catalog_updated = 0usize;
            if has_catalog {
                let local_only = if catalog_columns.contains("host_id") {
                    " AND host_id = 'local'"
                } else {
                    ""
                };
                let sql = if catalog_columns.contains("missing_candidate") {
                    format!(
                        "UPDATE local_thread_catalog SET missing_candidate = {} WHERE thread_id IN ({}){} AND COALESCE(missing_candidate, 0) <> {}",
                        usize::from(!visible),
                        placeholders,
                        local_only,
                        usize::from(!visible)
                    )
                } else if visible {
                    String::new()
                } else {
                    format!(
                        "DELETE FROM local_thread_catalog WHERE thread_id IN ({}){}",
                        placeholders, local_only
                    )
                };
                if !sql.is_empty() {
                    catalog_updated = transaction
                        .execute(&sql, rusqlite::params_from_iter(session_ids.iter()))
                        .map_err(|error| {
                            format!(
                                "更新 Codex 侧栏可见性失败 ({}): {}",
                                db_path.display(),
                                error
                            )
                        })?;
                    updated += catalog_updated;
                }
            }
            if catalog_updated > 0
                && sqlite_table_exists_with_connection(
                    &transaction,
                    "local_thread_catalog_metadata",
                )?
            {
                transaction
                    .execute(
                        "INSERT INTO local_thread_catalog_metadata (id, catalog_revision) VALUES (1, 1) \
                         ON CONFLICT(id) DO UPDATE SET catalog_revision = catalog_revision + 1",
                        [],
                    )
                    .map_err(|error| format!("更新 Codex 侧栏目录版本失败: {}", error))?;
            }
            transaction
                .commit()
                .map_err(|error| format!("提交 Codex 会话索引更新失败: {}", error))?;
        }
        Ok(updated)
    }

    fn remove_session_index_entries(&self, session_ids: &[String]) -> Result<usize, String> {
        if session_ids.is_empty() {
            return Ok(0);
        }
        let path = self.codex_home.join("session_index.jsonl");
        if !path.exists() {
            return Ok(0);
        }
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("读取 session_index.jsonl 失败: {}", error))?;
        let ids = session_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let mut removed = 0usize;
        let mut lines = Vec::new();
        for line in content.lines() {
            let remove = serde_json::from_str::<Value>(line)
                .ok()
                .and_then(|value| read_string(&value, "id"))
                .is_some_and(|id| ids.contains(id.as_str()));
            if remove {
                removed += 1;
            } else {
                lines.push(line);
            }
        }
        if removed == 0 {
            return Ok(0);
        }
        let backup_dir = visibility_backup_dir();
        fs::create_dir_all(&backup_dir)
            .map_err(|error| format!("创建会话索引备份目录失败: {}", error))?;
        fs::copy(&path, backup_dir.join("session_index.jsonl"))
            .map_err(|error| format!("备份 session_index.jsonl 失败: {}", error))?;
        let mut next = lines.join("\n");
        if !next.is_empty() {
            next.push('\n');
        }
        write_bytes_atomic(&path, next.as_bytes())
            .map_err(|error| format!("更新 session_index.jsonl 失败: {}", error))?;
        Ok(removed)
    }

    fn find_session_path(&self, session_id: &str) -> Result<PathBuf, String> {
        for path in collect_jsonl_files(&self.sessions_dir())? {
            if session_id_for_path(&path) == session_id || session_file_has_id(&path, session_id)? {
                return Ok(path);
            }
        }
        Err(format!("会话不存在: {}", session_id))
    }

    fn validated_turn_backup_path(
        &self,
        session_path: &Path,
        backup_id: &str,
    ) -> Result<PathBuf, String> {
        let backup_id = backup_id.trim();
        if backup_id.is_empty()
            || Path::new(backup_id)
                .file_name()
                .and_then(|value| value.to_str())
                != Some(backup_id)
        {
            return Err("无效的会话删除备份".to_string());
        }
        let session_hash = short_hash(&session_path.to_string_lossy());
        let valid_suffix = ["turn-delete", "message-delete"]
            .iter()
            .any(|operation| backup_id.ends_with(&format!("-{operation}-{session_hash}.jsonl")));
        if !valid_suffix {
            return Err("会话删除备份与目标会话不匹配".to_string());
        }
        let path = session_edit_backup_dir().join(backup_id);
        if !path.is_file() {
            return Err("会话删除备份不存在".to_string());
        }
        Ok(path)
    }

    fn backup_session_file(&self, path: &Path, operation: &str) -> Result<PathBuf, String> {
        let backup_dir = session_edit_backup_dir();
        fs::create_dir_all(&backup_dir)
            .map_err(|error| format!("创建会话备份目录失败: {}", error))?;
        let file_name = format!(
            "{}-{}-{}.jsonl",
            chrono::Utc::now().format("%Y%m%d-%H%M%S-%6f"),
            operation,
            short_hash(&path.to_string_lossy())
        );
        let backup_path = backup_dir.join(file_name);
        if fs::hard_link(path, &backup_path).is_err() {
            fs::copy(path, &backup_path).map_err(|error| format!("备份目标会话失败: {}", error))?;
        }
        Ok(backup_path)
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

    pub(crate) fn read_target_provider(&self) -> Result<String, String> {
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

    fn reset_stale_thread_history_projections(
        &self,
        sessions: &[SessionRepairRecord],
        force_session_ids: &HashSet<String>,
    ) -> Result<(usize, Vec<String>), String> {
        if sessions.is_empty() {
            return Ok((0, Vec::new()));
        }
        let mut targets = Vec::new();
        for session in sessions {
            if let Some(target) = paginated_rollout_projection_target(&session.path)? {
                targets.push((session.id.clone(), target));
            }
        }
        if targets.is_empty() {
            return Ok((0, Vec::new()));
        }

        let mut reset_session_ids = HashSet::new();
        let mut backup_dirs = Vec::new();
        for db_path in self.thread_history_db_paths() {
            let connection = Connection::open(&db_path).map_err(|error| {
                format!(
                    "打开 Codex 分页历史数据库失败 ({}): {error}",
                    db_path.display()
                )
            })?;
            connection
                .busy_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| format!("设置分页历史数据库等待时间失败: {error}"))?;
            if !sqlite_table_exists_with_connection(&connection, "thread_history_projection_state")?
            {
                continue;
            }
            let has_turns = sqlite_table_exists_with_connection(&connection, "thread_turns")?;
            let has_items = sqlite_table_exists_with_connection(&connection, "thread_items")?;
            let mut stale_ids = Vec::new();
            for (session_id, target) in &targets {
                let state = connection
                    .query_row(
                        "SELECT next_rollout_byte_offset, next_rollout_ordinal FROM thread_history_projection_state WHERE thread_id = ?1",
                        params![session_id],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                    )
                    .optional()
                    .map_err(|error| format!("读取分页历史投影状态失败: {error}"))?;
                let turn_count = if has_turns {
                    connection
                        .query_row(
                            "SELECT COUNT(*) FROM thread_turns WHERE thread_id = ?1",
                            params![session_id],
                            |row| row.get::<_, usize>(0),
                        )
                        .map_err(|error| format!("统计分页历史对话轮次失败: {error}"))?
                } else {
                    0
                };
                let item_count = if has_items {
                    connection
                        .query_row(
                            "SELECT COUNT(*) FROM thread_items WHERE thread_id = ?1",
                            params![session_id],
                            |row| row.get::<_, usize>(0),
                        )
                        .map_err(|error| format!("统计分页历史项目失败: {error}"))?
                } else {
                    0
                };
                let state_is_stale = state.is_some_and(|(next_byte_offset, next_ordinal)| {
                    next_byte_offset != target.next_byte_offset
                        || next_ordinal != target.next_ordinal
                });
                let rows_without_state = state.is_none() && (turn_count > 0 || item_count > 0);
                let turn_projection_is_stale =
                    target.turn_count > 0 && turn_count != target.turn_count;
                if force_session_ids.contains(session_id)
                    || state_is_stale
                    || rows_without_state
                    || turn_projection_is_stale
                {
                    stale_ids.push(session_id.clone());
                }
            }
            drop(connection);
            if stale_ids.is_empty() {
                continue;
            }

            backup_dirs.push(backup_sqlite_file(&db_path)?);
            let mut connection = Connection::open(&db_path).map_err(|error| {
                format!(
                    "重新打开 Codex 分页历史数据库失败 ({}): {error}",
                    db_path.display()
                )
            })?;
            connection
                .busy_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| format!("设置分页历史数据库写入等待时间失败: {error}"))?;
            let transaction = connection
                .transaction()
                .map_err(|error| format!("开启分页历史投影重建事务失败: {error}"))?;
            for session_id in stale_ids {
                for table in [
                    "thread_items",
                    "thread_turns",
                    "thread_history_projection_state",
                ] {
                    if !sqlite_table_exists_with_connection(&transaction, table)? {
                        continue;
                    }
                    transaction
                        .execute(
                            &format!("DELETE FROM {table} WHERE thread_id = ?1"),
                            params![session_id],
                        )
                        .map_err(|error| {
                            format!(
                                "清理分页历史投影失败 ({} / {table}): {error}",
                                db_path.display()
                            )
                        })?;
                }
                reset_session_ids.insert(session_id);
            }
            transaction
                .commit()
                .map_err(|error| format!("提交分页历史投影重建失败: {error}"))?;
        }
        backup_dirs.sort();
        backup_dirs.dedup();
        Ok((reset_session_ids.len(), backup_dirs))
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
            let provider_rewrite =
                rewrite_session_meta_provider(&content, target_provider, rewrite_all_meta)?;
            let ordinal_rewrite = normalize_paginated_rollout_ordinals(
                provider_rewrite.as_deref().unwrap_or(&content),
            )?;
            let Some(next) = ordinal_rewrite.or(provider_rewrite) else {
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

    fn staged_thread_cleanup_db_paths(&self) -> Vec<PathBuf> {
        let mut paths = self.sqlite_candidate_paths();
        for path in self.thread_history_db_paths() {
            if !paths.iter().any(|item| item == &path) {
                paths.push(path);
            }
        }
        paths
    }

    fn thread_history_db_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.codex_home) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|item| item.to_str()) else {
                    continue;
                };
                if path.is_file()
                    && name.starts_with("thread_history_")
                    && (name.ends_with(".sqlite") || name.ends_with(".db"))
                {
                    paths.push(path);
                }
            }
        }
        paths.sort();
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
    let repaired_history_modes =
        sqlite_repair_paginated_history_modes(db_path, &columns, sessions)?;
    let before = sqlite_count_repairable_rows(db_path, target_provider, sessions)?;
    if before == 0 {
        return sqlite_insert_missing_session_rows(db_path, target_provider, sessions)
            .map(|inserted| repaired_history_modes + inserted);
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
    Ok(repaired_history_modes + before + inserted)
}

fn sqlite_repair_paginated_history_modes(
    db_path: &Path,
    columns: &HashSet<String>,
    sessions: Option<&[SessionRepairRecord]>,
) -> Result<usize, String> {
    if !columns.contains("history_mode") {
        return Ok(0);
    }
    let Some(sessions) = sessions else {
        return Ok(0);
    };
    let ids = sessions
        .iter()
        .filter(|session| rollout_history_mode(&session.path) == "paginated")
        .map(|session| sql_quote(&session.id))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(0);
    }
    let where_clause = format!(
        "id IN ({}) AND COALESCE(history_mode, '') <> 'paginated'",
        ids.join(", ")
    );
    let count = run_sqlite(
        db_path,
        &format!("SELECT COUNT(*) FROM threads WHERE {where_clause};"),
    )?
    .trim()
    .parse::<usize>()
    .unwrap_or(0);
    if count > 0 {
        run_sqlite(
            db_path,
            &format!("UPDATE threads SET history_mode = 'paginated' WHERE {where_clause};"),
        )?;
    }
    Ok(count)
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

fn sqlite_thread_columns_with_connection(
    connection: &Connection,
) -> Result<HashSet<String>, String> {
    sqlite_table_columns_with_connection(connection, "threads")
}

fn sqlite_table_columns_with_connection(
    connection: &Connection,
    table: &str,
) -> Result<HashSet<String>, String> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT name FROM pragma_table_info({})",
            sql_quote(table)
        ))
        .map_err(|error| format!("读取会话 SQLite 表结构失败: {}", error))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("查询会话 SQLite 表结构失败: {}", error))?;
    columns
        .collect::<rusqlite::Result<HashSet<_>>>()
        .map_err(|error| format!("解析会话 SQLite 表结构失败: {}", error))
}

fn sqlite_table_exists_with_connection(
    connection: &Connection,
    table: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |row| row.get::<_, usize>(0),
        )
        .map(|count| count == 1)
        .map_err(|error| format!("检查会话 SQLite 表失败: {}", error))
}

fn sidebar_source_kind(source: &str) -> Option<String> {
    let source = source.trim();
    if source.is_empty() || source.starts_with('{') || source.starts_with('[') {
        return None;
    }
    Some(source.trim_matches('"').to_string())
}

fn sqlite_repair_set_clause(columns: &HashSet<String>, escaped_provider: &str) -> String {
    let mut assignments = Vec::new();
    if columns.contains("model_provider") {
        assignments.push(format!("model_provider = {escaped_provider}"));
    }
    if columns.contains("first_user_message") && columns.contains("title") {
        assignments.push(
            "first_user_message = CASE WHEN COALESCE(first_user_message, '') = '' THEN COALESCE(title, '') ELSE first_user_message END".to_string(),
        );
    }
    if columns.contains("preview") && columns.contains("title") {
        assignments.push(
            "preview = CASE WHEN COALESCE(preview, '') = '' THEN COALESCE(title, '') ELSE preview END"
                .to_string(),
        );
    }
    if columns.contains("has_user_event") && columns.contains("first_user_message") {
        assignments.push(
            if columns.contains("title") {
                "has_user_event = CASE WHEN COALESCE(first_user_message, '') <> '' OR COALESCE(title, '') <> '' THEN 1 ELSE has_user_event END"
            } else {
                "has_user_event = CASE WHEN COALESCE(first_user_message, '') <> '' THEN 1 ELSE has_user_event END"
            }
            .to_string(),
        );
    }
    if columns.contains("thread_source") && columns.contains("first_user_message") {
        assignments.push(
            if columns.contains("title") {
                "thread_source = CASE WHEN COALESCE(thread_source, '') = '' AND (COALESCE(first_user_message, '') <> '' OR COALESCE(title, '') <> '') THEN 'user' ELSE thread_source END"
            } else {
                "thread_source = CASE WHEN COALESCE(thread_source, '') = '' AND COALESCE(first_user_message, '') <> '' THEN 'user' ELSE thread_source END"
            }
            .to_string(),
        );
    }
    if columns.contains("archived") {
        assignments.push("archived = 0".to_string());
    }
    if columns.contains("archived_at") {
        assignments.push("archived_at = NULL".to_string());
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
    if columns.contains("first_user_message") && columns.contains("title") {
        predicates.push(
            "(COALESCE(first_user_message, '') = '' AND COALESCE(title, '') <> '')".to_string(),
        );
    }
    if columns.contains("preview") && columns.contains("title") {
        predicates.push("(COALESCE(preview, '') = '' AND COALESCE(title, '') <> '')".to_string());
    }
    if columns.contains("has_user_event") && columns.contains("first_user_message") {
        let user_text = if columns.contains("title") {
            "(COALESCE(first_user_message, '') <> '' OR COALESCE(title, '') <> '')"
        } else {
            "COALESCE(first_user_message, '') <> ''"
        };
        predicates.push(format!(
            "({user_text} AND COALESCE(has_user_event, 0) <> 1)"
        ));
    }
    if columns.contains("thread_source") && columns.contains("first_user_message") {
        let user_text = if columns.contains("title") {
            "(COALESCE(first_user_message, '') <> '' OR COALESCE(title, '') <> '')"
        } else {
            "COALESCE(first_user_message, '') <> ''"
        };
        predicates.push(format!(
            "({user_text} AND COALESCE(thread_source, '') = '')"
        ));
    }
    if columns.contains("archived") {
        predicates.push("COALESCE(archived, 0) <> 0".to_string());
    }
    if columns.contains("archived_at") {
        predicates.push("archived_at IS NOT NULL".to_string());
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
        push_sql_value(
            &mut names,
            &mut values,
            &columns,
            "history_mode",
            &metadata.history_mode,
        );
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
    history_mode: String,
}

fn sqlite_metadata_for_session(session: &SessionRepairRecord) -> SessionSqliteMetadata {
    let mut metadata = SessionSqliteMetadata {
        created_at: session.updated_at,
        source: "cli".to_string(),
        cwd: String::new(),
        cli_version: String::new(),
        sandbox_policy: "read-only".to_string(),
        approval_mode: "on-request".to_string(),
        history_mode: rollout_history_mode(&session.path).to_string(),
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

fn rollout_history_mode(path: &Path) -> &'static str {
    let Ok(file) = fs::File::open(path) else {
        return "legacy";
    };
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            return "legacy";
        };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            return "legacy";
        };
        return if value.get("type").and_then(Value::as_str) == Some("session_meta")
            && value.get("ordinal").and_then(Value::as_u64).is_some()
        {
            "paginated"
        } else {
            "legacy"
        };
    }
    "legacy"
}

fn paginated_rollout_projection_target(
    path: &Path,
) -> Result<Option<PaginatedRolloutProjectionTarget>, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("读取分页会话文件失败 ({}): {error}", path.display()))?;
    let mut first_record = true;
    let mut last_ordinal = None;
    let mut turn_count = 0usize;
    for line in BufReader::new(file).lines() {
        let line =
            line.map_err(|error| format!("读取分页会话记录失败 ({}): {error}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(&line)
            .map_err(|error| format!("解析分页会话记录失败 ({}): {error}", path.display()))?;
        if first_record {
            first_record = false;
            if value.get("type").and_then(Value::as_str) != Some("session_meta")
                || value.get("ordinal").and_then(Value::as_u64).is_none()
            {
                return Ok(None);
            }
        }
        let ordinal = value
            .get("ordinal")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("分页会话记录缺少 ordinal ({})", path.display()))?;
        last_ordinal = Some(ordinal);
        if value.get("type").and_then(Value::as_str) == Some("event_msg")
            && value
                .get("payload")
                .and_then(|payload| payload.get("type"))
                .and_then(Value::as_str)
                == Some("task_started")
        {
            turn_count = turn_count.saturating_add(1);
        }
    }
    let Some(last_ordinal) = last_ordinal else {
        return Ok(None);
    };
    let next_byte_offset = i64::try_from(
        fs::metadata(path)
            .map_err(|error| format!("读取分页会话文件大小失败 ({}): {error}", path.display()))?
            .len(),
    )
    .map_err(|_| format!("分页会话文件过大，无法重建历史投影 ({})", path.display()))?;
    let next_ordinal = last_ordinal
        .checked_add(1)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or_else(|| format!("分页会话 ordinal 超出范围 ({})", path.display()))?;
    Ok(Some(PaginatedRolloutProjectionTarget {
        next_byte_offset,
        next_ordinal,
        turn_count,
    }))
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
        .join(chrono::Utc::now().format("%Y%m%d-%H%M%S-%6f").to_string())
}

fn visibility_repair_marker_path() -> PathBuf {
    switcher_root_dir().join("session-visibility-repair.json")
}

fn session_edit_backup_dir() -> PathBuf {
    switcher_root_dir().join("session-edit-backups")
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RolloutModelCompatibilityRewriteStats {
    rewritten_model_fields: usize,
    synchronized_providers: usize,
    removed_encrypted_reasoning_items: usize,
    removed_encrypted_compaction_items: usize,
}

impl RolloutModelCompatibilityRewriteStats {
    fn changed(self) -> bool {
        self.rewritten_model_fields > 0
            || self.synchronized_providers > 0
            || self.removed_encrypted_reasoning_items > 0
            || self.removed_encrypted_compaction_items > 0
    }

    fn add(&mut self, other: Self) {
        self.rewritten_model_fields += other.rewritten_model_fields;
        self.synchronized_providers += other.synchronized_providers;
        self.removed_encrypted_reasoning_items += other.removed_encrypted_reasoning_items;
        self.removed_encrypted_compaction_items += other.removed_encrypted_compaction_items;
    }
}

fn prepare_rollout_model_compatibility_rewrite(
    path: &Path,
    target_provider: &str,
) -> Result<Option<(PathBuf, RolloutModelCompatibilityRewriteStats)>, String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("session.jsonl");
    let tmp_path = parent.join(format!(
        ".{file_name}.model-compatibility-{:016x}.tmp",
        rand::random::<u64>()
    ));
    let input = fs::File::open(path)
        .map_err(|error| format!("读取会话模型状态失败 ({}): {}", path.display(), error))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let output = options.open(&tmp_path).map_err(|error| {
        format!(
            "创建会话模型修复临时文件失败 ({}): {}",
            path.display(),
            error
        )
    })?;
    let mut reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);
    let mut line = Vec::new();
    let mut rewrite_stats = RolloutModelCompatibilityRewriteStats::default();
    let result = (|| -> Result<(), String> {
        loop {
            line.clear();
            let read = reader
                .read_until(b'\n', &mut line)
                .map_err(|error| format!("读取会话模型状态失败 ({}): {}", path.display(), error))?;
            if read == 0 {
                break;
            }

            let may_contain_model = line
                .windows(b"\"model\"".len())
                .any(|window| window == b"\"model\"");
            let may_contain_provider = line
                .windows(b"\"model_provider\"".len())
                .any(|window| window == b"\"model_provider\"");
            let may_be_session_meta = line
                .windows(b"session_meta".len())
                .any(|window| window == b"session_meta");
            let may_contain_encrypted_content = line
                .windows(b"encrypted_content".len())
                .any(|window| window == b"encrypted_content");
            let may_contain_remote_reasoning_id = line
                .windows(b"\"rs_".len())
                .any(|window| window == b"\"rs_");
            let may_contain_remote_compaction_id = line
                .windows(b"\"cmp_".len())
                .any(|window| window == b"\"cmp_");
            if !may_contain_model
                && !may_contain_provider
                && !may_be_session_meta
                && !may_contain_encrypted_content
                && !may_contain_remote_reasoning_id
                && !may_contain_remote_compaction_id
            {
                writer.write_all(&line).map_err(|error| {
                    format!("写入会话模型修复结果失败 ({}): {}", path.display(), error)
                })?;
                continue;
            }

            let mut value = match serde_json::from_slice::<Value>(&line) {
                Ok(value) => value,
                Err(_) => {
                    writer.write_all(&line).map_err(|error| {
                        format!("写入会话模型修复结果失败 ({}): {}", path.display(), error)
                    })?;
                    continue;
                }
            };
            let line_stats = rewrite_rollout_model_compatibility_value(&mut value, target_provider);
            if !line_stats.changed() {
                writer.write_all(&line).map_err(|error| {
                    format!("写入会话模型修复结果失败 ({}): {}", path.display(), error)
                })?;
                continue;
            }
            rewrite_stats.add(line_stats);

            if line_stats.removed_encrypted_reasoning_items > 0 {
                continue;
            }

            let ending = if line.ends_with(b"\r\n") {
                b"\r\n".as_slice()
            } else if line.ends_with(b"\n") {
                b"\n".as_slice()
            } else {
                b"".as_slice()
            };
            serde_json::to_writer(&mut writer, &value).map_err(|error| {
                format!("序列化会话模型修复结果失败 ({}): {}", path.display(), error)
            })?;
            writer.write_all(ending).map_err(|error| {
                format!("写入会话模型修复结果失败 ({}): {}", path.display(), error)
            })?;
        }
        writer
            .flush()
            .map_err(|error| format!("刷新会话模型修复结果失败 ({}): {}", path.display(), error))?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|error| format!("同步会话模型修复结果失败 ({}): {}", path.display(), error))?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(writer);
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }
    drop(writer);
    if !rewrite_stats.changed() {
        let _ = fs::remove_file(&tmp_path);
        return Ok(None);
    }
    Ok(Some((tmp_path, rewrite_stats)))
}

fn rewrite_rollout_model_compatibility_value(
    value: &mut Value,
    target_provider: &str,
) -> RolloutModelCompatibilityRewriteStats {
    let record_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut stats = RolloutModelCompatibilityRewriteStats::default();

    if record_type == "response_item" {
        let should_remove = value
            .get("payload")
            .and_then(Value::as_object)
            .is_some_and(is_nonportable_encrypted_reasoning_item);
        if should_remove {
            stats.removed_encrypted_reasoning_items = 1;
            return stats;
        }
    }

    if record_type == "compacted" {
        if let Some(replacement_history) = value
            .pointer_mut("/payload/replacement_history")
            .and_then(Value::as_array_mut)
        {
            let previous_len = replacement_history.len();
            replacement_history.retain(|item| !is_nonportable_encrypted_compaction_item(item));
            stats.removed_encrypted_compaction_items =
                previous_len.saturating_sub(replacement_history.len());
        }
    }

    if record_type == "session_meta" {
        if let Some(payload) = value.get_mut("payload").and_then(Value::as_object_mut) {
            let needs_update = payload
                .get("model_provider")
                .and_then(Value::as_str)
                .map(|current| current != target_provider)
                .unwrap_or(true);
            if needs_update {
                payload.insert(
                    "model_provider".to_string(),
                    Value::String(target_provider.to_string()),
                );
                stats.synchronized_providers += 1;
            }
        }
    }

    let model_paths: &[&str] = match record_type.as_str() {
        "turn_context" => &[
            "/payload/model",
            "/payload/collaboration_mode/model",
            "/payload/collaboration_mode/settings/model",
        ],
        "event_msg" => &[
            "/payload/thread_settings/model",
            "/payload/thread_settings/collaboration_mode/model",
            "/payload/thread_settings/collaboration_mode/settings/model",
        ],
        "world_state" => &[
            "/payload/state/model",
            "/payload/state/collaboration_mode/model",
            "/payload/state/collaboration_mode/settings/model",
            "/payload/state/personality/model",
        ],
        _ => &[],
    };
    for pointer in model_paths {
        let Some(model) = value.pointer_mut(pointer) else {
            continue;
        };
        let Some(current) = model.as_str() else {
            continue;
        };
        let Some(normalized) = normalize_model_for_provider(current, target_provider) else {
            continue;
        };
        *model = Value::String(normalized);
        stats.rewritten_model_fields += 1;
    }
    stats
}

fn is_nonportable_encrypted_reasoning_item(payload: &serde_json::Map<String, Value>) -> bool {
    if payload.get("type").and_then(Value::as_str) != Some("reasoning") {
        return false;
    }
    payload.contains_key("encrypted_content")
        || payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("rs_"))
}

fn is_nonportable_encrypted_compaction_item(item: &Value) -> bool {
    let Some(item) = item.as_object() else {
        return false;
    };
    if item.get("type").and_then(Value::as_str) != Some("compaction") {
        return false;
    }
    item.contains_key("encrypted_content")
        || item
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("cmp_"))
}

fn normalize_model_for_provider(model: &str, target_provider: &str) -> Option<String> {
    let model = model.trim();
    if model.is_empty() {
        return None;
    }

    if target_provider == "openai" {
        let (current_provider, model_id) = model.split_once('/')?;
        if current_provider.is_empty() || model_id.is_empty() {
            return None;
        }
        return Some(model_id.to_string());
    }

    let target_prefix = format!("{target_provider}/");
    if model.starts_with(&target_prefix) {
        return None;
    }
    let model_id = model
        .split_once('/')
        .map(|(_, model_id)| model_id)
        .unwrap_or(model);
    if model_id.is_empty() {
        return None;
    }
    Some(format!("{target_provider}/{model_id}"))
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

fn rewrite_session_meta_cwd(content: &str, cwd: &str) -> Result<Option<String>, String> {
    let mut output = String::with_capacity(content.len() + cwd.len());
    let mut handled_meta = false;
    let mut changed = false;

    for segment in content.split_inclusive('\n') {
        let (body, ending) = if let Some(body) = segment.strip_suffix("\r\n") {
            (body, "\r\n")
        } else if let Some(body) = segment.strip_suffix('\n') {
            (body, "\n")
        } else {
            (segment, "")
        };
        if handled_meta || body.trim().is_empty() {
            output.push_str(segment);
            continue;
        }
        let Ok(mut value) = serde_json::from_str::<Value>(body) else {
            output.push_str(segment);
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            output.push_str(segment);
            continue;
        }
        let Some(payload) = value.get_mut("payload").and_then(Value::as_object_mut) else {
            output.push_str(segment);
            continue;
        };
        handled_meta = true;
        if payload.get("cwd").and_then(Value::as_str) == Some(cwd) {
            output.push_str(segment);
            continue;
        }
        payload.insert("cwd".to_string(), Value::String(cwd.to_string()));
        let line = serde_json::to_string(&value)
            .map_err(|error| format!("序列化会话工作目录失败: {}", error))?;
        output.push_str(&line);
        output.push_str(ending);
        changed = true;
    }

    if !handled_meta {
        return Err("会话缺少可修改的 session_meta".to_string());
    }
    Ok(changed.then_some(output))
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

impl SessionTurnAccumulator {
    fn new(id: String, timestamp: String, start_offset: u64) -> Self {
        Self {
            id,
            timestamp,
            start_offset,
            end_offset: start_offset,
            complete: false,
            response_messages: Vec::new(),
            fallback_user_messages: Vec::new(),
            fallback_assistant_messages: Vec::new(),
            local_image_paths: Vec::new(),
            technical_item_count: 0,
            response_item_ids: HashSet::new(),
            response_item_fingerprints: HashMap::new(),
        }
    }

    fn finish(mut self) -> Option<ParsedSessionTurn> {
        let has_user_response = self
            .response_messages
            .iter()
            .any(|message| message.role == "user");
        let has_assistant_response = self
            .response_messages
            .iter()
            .any(|message| message.role == "assistant");
        if !has_user_response {
            self.response_messages
                .append(&mut self.fallback_user_messages);
        }
        if !has_assistant_response {
            self.response_messages
                .append(&mut self.fallback_assistant_messages);
        }
        attach_local_image_paths(&mut self.response_messages, &self.local_image_paths);
        self.response_messages
            .sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
        if self.response_messages.is_empty() {
            return None;
        }
        if self.timestamp.is_empty() {
            self.timestamp = self
                .response_messages
                .first()
                .map(|message| message.timestamp.clone())
                .unwrap_or_default();
        }
        Some(ParsedSessionTurn {
            public: CodexSessionTurn {
                id: self.id,
                timestamp: self.timestamp,
                messages: self.response_messages,
                technical_item_count: self.technical_item_count,
                can_delete: self.complete,
            },
            start_offset: self.start_offset,
            end_offset: self.end_offset,
            complete: self.complete,
            response_item_ids: self.response_item_ids,
            response_item_fingerprints: self.response_item_fingerprints,
        })
    }
}

fn normalize_required_session_id(session_id: &str) -> Result<String, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("会话不能为空".to_string());
    }
    Ok(session_id.to_string())
}

fn session_file_fingerprint(path: &Path) -> Result<SessionFileFingerprint, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("读取会话文件状态失败 ({}): {}", path.display(), error))?;
    Ok(SessionFileFingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn session_file_has_id(path: &Path, session_id: &str) -> Result<bool, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("读取会话失败 {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    for _ in 0..20 {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| format!("读取会话失败 {}: {}", path.display(), error))?
            == 0
        {
            break;
        }
        if let Some(found) = extract_session_id(&line) {
            return Ok(found == session_id);
        }
    }
    Ok(false)
}

fn read_session_turn_page(
    path: &Path,
    cursor: u64,
    limit: usize,
) -> Result<(Vec<ParsedSessionTurn>, Option<u64>), String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("读取会话内容失败 ({}): {}", path.display(), error))?;
    let file_len = file
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    if cursor > file_len {
        return Err("会话内容游标已失效，请重新打开会话".to_string());
    }
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(cursor))
        .map_err(|error| format!("定位会话内容失败: {}", error))?;
    let mut turns = Vec::new();
    let mut current: Option<SessionTurnAccumulator> = None;
    let mut line = Vec::new();
    let mut position = cursor;

    loop {
        line.clear();
        let line_start = position;
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("读取会话内容失败: {}", error))?;
        if read == 0 {
            break;
        }
        position = position.saturating_add(read as u64);
        let text = String::from_utf8_lossy(&line);
        let needs_parse = text.contains("task_started")
            || text.contains("task_complete")
            || text.contains("turn_aborted")
            || text.contains("response_item")
            || text.contains("user_message")
            || text.contains("agent_message");
        let value = needs_parse
            .then(|| serde_json::from_slice::<Value>(&line).ok())
            .flatten();

        if let Some((turn_id, timestamp)) = value.as_ref().and_then(task_started_marker) {
            if let Some(previous) = current.take() {
                if let Some(turn) = previous.finish() {
                    turns.push(turn);
                }
            }
            if turns.len() >= limit {
                return Ok((turns, Some(line_start)));
            }
            let turn_id = if turn_id.is_empty() {
                format!("offset-{line_start:x}")
            } else {
                turn_id
            };
            current = Some(SessionTurnAccumulator::new(turn_id, timestamp, line_start));
            continue;
        }

        if current.is_none() {
            if let Some(value) = value.as_ref() {
                if is_visible_user_response(value) {
                    current = Some(SessionTurnAccumulator::new(
                        format!("offset-{line_start:x}"),
                        read_string(value, "timestamp").unwrap_or_default(),
                        line_start,
                    ));
                }
            }
        }

        let Some(accumulator) = current.as_mut() else {
            continue;
        };
        accumulator.end_offset = position;
        let mut handled = false;
        if let Some(value) = value.as_ref() {
            if let Some((item_id, fingerprint)) = response_item_identity(value) {
                if let Some(item_id) = item_id {
                    accumulator.response_item_ids.insert(item_id);
                }
                *accumulator
                    .response_item_fingerprints
                    .entry(fingerprint)
                    .or_insert(0) += 1;
            }
            if let Some(message) = parse_response_message(value, line_start) {
                handled = true;
                if should_display_session_message(&message) {
                    accumulator.response_messages.push(message);
                } else {
                    accumulator.technical_item_count += 1;
                }
            } else if let Some((message, local_images)) =
                parse_event_user_message(value, line_start)
            {
                handled = true;
                accumulator.local_image_paths.extend(local_images);
                if should_display_session_message(&message) {
                    accumulator.fallback_user_messages.push(message);
                } else {
                    accumulator.technical_item_count += 1;
                }
            } else if let Some(message) = parse_event_assistant_message(value, line_start) {
                handled = true;
                accumulator.fallback_assistant_messages.push(message);
            }
        }
        if !handled && is_technical_session_line(&text) {
            accumulator.technical_item_count += 1;
        }

        if value
            .as_ref()
            .is_some_and(|value| task_finished_marker(value, accumulator.id.as_str()))
        {
            accumulator.complete = true;
            if let Some(turn) = current.take().and_then(SessionTurnAccumulator::finish) {
                turns.push(turn);
            }
            if turns.len() >= limit {
                return Ok((turns, (position < file_len).then_some(position)));
            }
        }
    }

    if let Some(turn) = current.and_then(SessionTurnAccumulator::finish) {
        turns.push(turn);
    }
    Ok((turns, None))
}

fn read_session_turn_page_desc(
    path: &Path,
    cursor: u64,
    limit: usize,
) -> Result<(Vec<ParsedSessionTurn>, Option<u64>), String> {
    let file_len = fs::metadata(path)
        .map_err(|error| format!("读取会话内容失败 ({}): {}", path.display(), error))?
        .len();
    if cursor > file_len {
        return Err("会话内容游标已失效，请重新打开会话".to_string());
    }
    if cursor == 0 {
        return Ok((Vec::new(), None));
    }

    let mut marker_limit = limit.saturating_add(1).max(2);
    loop {
        let scan = find_task_started_offsets_reverse(path, cursor, marker_limit)?;
        if scan.offsets.is_empty() {
            return read_session_turn_page_desc_legacy(path, cursor, limit);
        }
        let earliest_offset = *scan.offsets.last().unwrap_or(&0);
        let (mut turns, _) = read_session_turn_page(path, earliest_offset, marker_limit)?;
        turns.retain(|turn| turn.start_offset < cursor);
        if turns.len() > limit {
            let selected = turns.split_off(turns.len() - limit);
            let next_cursor = selected.first().map(|turn| turn.start_offset);
            let mut selected = selected;
            selected.reverse();
            return Ok((selected, next_cursor));
        }
        if scan.reached_start {
            turns.reverse();
            return Ok((turns, None));
        }
        marker_limit = marker_limit.saturating_mul(2);
        if marker_limit == usize::MAX {
            return read_session_turn_page_desc_legacy(path, cursor, limit);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ReverseTurnStartScan {
    offsets: Vec<u64>,
    reached_start: bool,
}

fn find_task_started_offsets_reverse(
    path: &Path,
    cursor: u64,
    max_count: usize,
) -> Result<ReverseTurnStartScan, String> {
    const REVERSE_READ_CHUNK_SIZE: usize = 256 * 1024;

    let mut file = fs::File::open(path)
        .map_err(|error| format!("读取会话内容失败 ({}): {}", path.display(), error))?;
    let file_len = file
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    if cursor > file_len {
        return Err("会话内容游标已失效，请重新打开会话".to_string());
    }

    let mut position = cursor;
    let mut suffix = Vec::new();
    let mut offsets = Vec::with_capacity(max_count);
    while position > 0 && offsets.len() < max_count {
        let start = position.saturating_sub(REVERSE_READ_CHUNK_SIZE as u64);
        let chunk_len =
            usize::try_from(position - start).map_err(|_| "会话内容分块长度无效".to_string())?;
        let mut chunk = vec![0u8; chunk_len];
        file.seek(SeekFrom::Start(start))
            .map_err(|error| format!("定位会话内容失败: {}", error))?;
        file.read_exact(&mut chunk)
            .map_err(|error| format!("读取会话内容失败: {}", error))?;
        chunk.extend_from_slice(&suffix);

        let first_complete_index = if start == 0 {
            0
        } else {
            chunk
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|index| index + 1)
                .unwrap_or(chunk.len())
        };
        let mut line_end = chunk.len();
        if line_end > first_complete_index && chunk[line_end - 1] == b'\n' {
            line_end -= 1;
        }
        while line_end > first_complete_index && offsets.len() < max_count {
            let line_start = chunk[first_complete_index..line_end]
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map(|index| first_complete_index + index + 1)
                .unwrap_or(first_complete_index);
            let line = &chunk[line_start..line_end];
            if line_contains_task_started(line) {
                offsets.push(start.saturating_add(line_start as u64));
            }
            if line_start == first_complete_index {
                break;
            }
            line_end = line_start - 1;
        }

        suffix = chunk[..first_complete_index].to_vec();
        position = start;
    }

    let reached_start = position == 0 && offsets.len() < max_count;
    Ok(ReverseTurnStartScan {
        offsets,
        reached_start,
    })
}

fn line_contains_task_started(line: &[u8]) -> bool {
    line.windows(12).any(|window| window == b"task_started")
        && serde_json::from_slice::<Value>(line)
            .ok()
            .as_ref()
            .and_then(task_started_marker)
            .is_some()
}

fn read_session_turn_page_desc_legacy(
    path: &Path,
    cursor: u64,
    limit: usize,
) -> Result<(Vec<ParsedSessionTurn>, Option<u64>), String> {
    let mut recent = VecDeque::with_capacity(limit.saturating_add(1));
    let mut scan_cursor = 0u64;
    loop {
        let (page, next_cursor) = read_session_turn_page(path, scan_cursor, 50)?;
        let mut reached_cursor = false;
        for turn in page {
            if turn.start_offset >= cursor {
                reached_cursor = true;
                break;
            }
            recent.push_back(turn);
            if recent.len() > limit.saturating_add(1) {
                recent.pop_front();
            }
        }
        if reached_cursor {
            break;
        }
        let Some(next_cursor) = next_cursor else {
            break;
        };
        if next_cursor <= scan_cursor || next_cursor >= cursor {
            break;
        }
        scan_cursor = next_cursor;
    }
    let has_more = recent.len() > limit;
    while recent.len() > limit {
        recent.pop_front();
    }
    let next_cursor = has_more
        .then(|| recent.front().map(|turn| turn.start_offset))
        .flatten();
    let mut turns = recent.into_iter().collect::<Vec<_>>();
    turns.reverse();
    Ok((turns, next_cursor))
}

fn task_started_marker(value: &Value) -> Option<(String, String)> {
    if value.get("type").and_then(Value::as_str) != Some("event_msg")
        || value
            .get("payload")
            .and_then(|payload| payload.get("type"))
            .and_then(Value::as_str)
            != Some("task_started")
    {
        return None;
    }
    let payload = value.get("payload")?;
    Some((
        payload
            .get("turn_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_default(),
        value
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    ))
}

fn task_finished_marker(value: &Value, turn_id: &str) -> bool {
    if value.get("type").and_then(Value::as_str) != Some("event_msg") {
        return false;
    }
    let Some(payload) = value.get("payload") else {
        return false;
    };
    let event_type = payload.get("type").and_then(Value::as_str);
    if !matches!(event_type, Some("task_complete" | "turn_aborted")) {
        return false;
    }
    payload
        .get("turn_id")
        .and_then(Value::as_str)
        .map(|value| value == turn_id)
        .unwrap_or(true)
}

fn is_visible_user_response(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("response_item")
        && value
            .get("payload")
            .and_then(|payload| payload.get("type"))
            .and_then(Value::as_str)
            == Some("message")
        && value
            .get("payload")
            .and_then(|payload| payload.get("role"))
            .and_then(Value::as_str)
            == Some("user")
}

fn parse_response_message(value: &Value, line_offset: u64) -> Option<CodexSessionMessage> {
    if value.get("type").and_then(Value::as_str) != Some("response_item") {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let role = payload.get("role").and_then(Value::as_str)?;
    if !matches!(role, "user" | "assistant") {
        return None;
    }
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let message_id = payload
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let id = message_id
        .clone()
        .unwrap_or_else(|| format!("message-{line_offset:x}"));
    let phase = payload
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut text_parts = Vec::new();
    let mut attachments = Vec::new();
    for (index, item) in payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
        if matches!(item_type, "input_text" | "output_text" | "text") {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                if !text.trim().is_empty() {
                    text_parts.push(text.to_string());
                }
            }
            continue;
        }
        if matches!(item_type, "input_image" | "image") {
            let data_url = item
                .get("image_url")
                .or_else(|| item.get("imageUrl"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            attachments.push(CodexSessionAttachment {
                id: format!("{line_offset:x}:{index}"),
                kind: "image".to_string(),
                name: format!("图片 {}", attachments.len() + 1),
                source_path: None,
                mime_type: data_url_mime_type(data_url),
                size_bytes: data_url_size(data_url),
                available: !data_url.is_empty(),
                inline: true,
            });
        }
    }
    let text = text_parts.join("\n\n");
    attachments.extend(extract_mentioned_files(&text));
    Some(CodexSessionMessage {
        id,
        role: role.to_string(),
        phase,
        timestamp,
        text,
        attachments,
    })
}

fn parse_event_user_message(
    value: &Value,
    line_offset: u64,
) -> Option<(CodexSessionMessage, Vec<String>)> {
    if value.get("type").and_then(Value::as_str) != Some("event_msg") {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("user_message") {
        return None;
    }
    let text = payload
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let local_images = payload
        .get("local_images")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let attachments = extract_mentioned_files(&text);
    Some((
        CodexSessionMessage {
            id: payload
                .get("client_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("event-user-{line_offset:x}")),
            role: "user".to_string(),
            phase: String::new(),
            timestamp: value
                .get("timestamp")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            text,
            attachments,
        },
        local_images,
    ))
}

fn parse_event_assistant_message(value: &Value, line_offset: u64) -> Option<CodexSessionMessage> {
    if value.get("type").and_then(Value::as_str) != Some("event_msg") {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("agent_message") {
        return None;
    }
    Some(CodexSessionMessage {
        id: format!("event-assistant-{line_offset:x}"),
        role: "assistant".to_string(),
        phase: payload
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        timestamp: value
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        text: payload
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        attachments: Vec::new(),
    })
}

fn should_display_session_message(message: &CodexSessionMessage) -> bool {
    if message.role != "user" {
        return !message.text.trim().is_empty() || !message.attachments.is_empty();
    }
    let text = message.text.trim();
    if text.is_empty() {
        return !message.attachments.is_empty();
    }
    !is_internal_session_title(text) || text.contains("My request") || text.contains("我的请求")
}

fn is_technical_session_line(line: &str) -> bool {
    !line.contains("\"type\":\"session_meta\"")
        && !line.contains("\"type\": \"session_meta\"")
        && !line.contains("\"type\":\"turn_context\"")
        && !line.contains("\"type\": \"turn_context\"")
        && !line.contains("\"type\":\"world_state\"")
        && !line.contains("\"type\": \"world_state\"")
}

fn attach_local_image_paths(messages: &mut [CodexSessionMessage], paths: &[String]) {
    let mut path_index = 0usize;
    for message in messages.iter_mut().filter(|message| message.role == "user") {
        for attachment in message
            .attachments
            .iter_mut()
            .filter(|attachment| attachment.kind == "image" && attachment.inline)
        {
            let Some(path) = paths.get(path_index) else {
                break;
            };
            attachment.source_path = Some(path.clone());
            attachment.name = Path::new(path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("图片")
                .to_string();
            path_index += 1;
        }
        deduplicate_message_attachments(&mut message.attachments);
    }
    if path_index >= paths.len() {
        return;
    }
    let Some(message) = messages
        .iter_mut()
        .rev()
        .find(|message| message.role == "user")
    else {
        return;
    };
    for path in &paths[path_index..] {
        message.attachments.push(file_attachment(path, None));
    }
    deduplicate_message_attachments(&mut message.attachments);
}

fn deduplicate_message_attachments(attachments: &mut Vec<CodexSessionAttachment>) {
    let mut seen = HashSet::new();
    attachments.retain(|attachment| {
        let key = attachment
            .source_path
            .clone()
            .unwrap_or_else(|| attachment.id.clone());
        seen.insert(key)
    });
}

fn extract_mentioned_files(text: &str) -> Vec<CodexSessionAttachment> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let line = line.strip_prefix("## ")?;
            let (name, source_path) = line.rsplit_once(": ")?;
            let source_path = source_path.trim();
            if source_path.is_empty() || !Path::new(source_path).is_absolute() {
                return None;
            }
            Some(file_attachment(source_path, Some(name.trim())))
        })
        .collect()
}

fn file_attachment(source_path: &str, preferred_name: Option<&str>) -> CodexSessionAttachment {
    let path = Path::new(source_path);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kind = if matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg"
    ) {
        "image"
    } else if matches!(extension.as_str(), "mp3" | "wav" | "m4a" | "aac" | "ogg") {
        "audio"
    } else {
        "file"
    };
    CodexSessionAttachment {
        id: format!("file-{}", short_hash(source_path)),
        kind: kind.to_string(),
        name: preferred_name
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "附件".to_string()),
        source_path: Some(source_path.to_string()),
        mime_type: None,
        size_bytes: fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or_default(),
        available: path.is_file(),
        inline: false,
    }
}

fn data_url_mime_type(data_url: &str) -> Option<String> {
    data_url
        .strip_prefix("data:")
        .and_then(|value| value.split_once(';').map(|(mime, _)| mime))
        .filter(|mime| !mime.trim().is_empty())
        .map(str::to_string)
}

fn data_url_size(data_url: &str) -> u64 {
    let encoded_len = data_url
        .split_once(',')
        .map(|(_, encoded)| encoded.len())
        .unwrap_or_default();
    (encoded_len.saturating_mul(3) / 4) as u64
}

fn response_item_identity(value: &Value) -> Option<(Option<String>, String)> {
    if value.get("type").and_then(Value::as_str) != Some("response_item") {
        return None;
    }
    let payload = value.get("payload")?;
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    Some((id, response_item_payload_fingerprint(payload)?))
}

fn response_item_payload_fingerprint(payload: &Value) -> Option<String> {
    let mut normalized = payload.clone();
    normalized.as_object_mut()?.remove("id");
    let serialized = serde_json::to_vec(&normalized).ok()?;
    Some(hex_sha256(&serialized))
}

fn hex_sha256(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_session_asset(path: &Path, asset_id: &str) -> Result<CodexSessionAsset, String> {
    let (offset, content_index) = asset_id
        .trim()
        .split_once(':')
        .ok_or_else(|| "无效的会话附件标识".to_string())?;
    let offset = u64::from_str_radix(offset, 16).map_err(|_| "无效的会话附件位置".to_string())?;
    let content_index = content_index
        .parse::<usize>()
        .map_err(|_| "无效的会话附件序号".to_string())?;
    let file = fs::File::open(path).map_err(|error| format!("读取会话附件失败: {}", error))?;
    let file_len = file
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    if offset >= file_len {
        return Err("会话附件位置已失效，请刷新后重试".to_string());
    }
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|error| format!("定位会话附件失败: {}", error))?;
    let mut line = Vec::new();
    reader
        .read_until(b'\n', &mut line)
        .map_err(|error| format!("读取会话附件失败: {}", error))?;
    let value: Value =
        serde_json::from_slice(&line).map_err(|error| format!("解析会话附件失败: {}", error))?;
    let item = value
        .get("payload")
        .and_then(|payload| payload.get("content"))
        .and_then(Value::as_array)
        .and_then(|content| content.get(content_index))
        .ok_or_else(|| "会话附件不存在".to_string())?;
    let data_url = item
        .get("image_url")
        .or_else(|| item.get("imageUrl"))
        .or_else(|| item.get("audio_url"))
        .or_else(|| item.get("audioUrl"))
        .and_then(Value::as_str)
        .filter(|value| value.starts_with("data:"))
        .ok_or_else(|| "该附件不是可预览的内嵌资源".to_string())?
        .to_string();
    Ok(CodexSessionAsset {
        mime_type: data_url_mime_type(&data_url)
            .unwrap_or_else(|| "application/octet-stream".to_string()),
        size_bytes: data_url_size(&data_url),
        data_url,
    })
}

fn find_session_turn(path: &Path, turn_id: &str) -> Result<(ParsedSessionTurn, bool), String> {
    let mut cursor = 0u64;
    let mut found = None;
    loop {
        let (turns, next_cursor) = read_session_turn_page(path, cursor, 50)?;
        if let Some(turn) = turns.into_iter().find(|turn| turn.public.id == turn_id) {
            found = Some(turn);
        }
        let Some(next_cursor) = next_cursor else {
            break;
        };
        if next_cursor <= cursor {
            break;
        }
        cursor = next_cursor;
    }
    let turn = found.ok_or_else(|| "要删除的对话轮次不存在，请刷新后重试".to_string())?;
    Ok((turn, session_has_open_turn(path)?))
}

fn session_has_open_turn(path: &Path) -> Result<bool, String> {
    let file = fs::File::open(path).map_err(|error| format!("读取会话状态失败: {}", error))?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut open_turn_id: Option<String> = None;
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("读取会话状态失败: {}", error))?
            == 0
        {
            break;
        }
        if !line.windows(12).any(|window| window == b"task_started")
            && !line.windows(13).any(|window| window == b"task_complete")
            && !line.windows(12).any(|window| window == b"turn_aborted")
        {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        if let Some((turn_id, _)) = task_started_marker(&value) {
            open_turn_id = Some(turn_id);
        } else if open_turn_id
            .as_deref()
            .is_some_and(|turn_id| task_finished_marker(&value, turn_id))
        {
            open_turn_id = None;
        }
    }
    Ok(open_turn_id.is_some())
}

fn rewrite_session_without_turn(path: &Path, turn: &ParsedSessionTurn) -> Result<PathBuf, String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("session.jsonl");
    let tmp_path = parent.join(format!(
        ".{file_name}.turn-delete-{:016x}.tmp",
        rand::random::<u64>()
    ));
    let input = fs::File::open(path).map_err(|error| format!("读取会话失败: {}", error))?;
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .map_err(|error| format!("创建会话删除临时文件失败: {}", error))?;
    let mut reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);
    let mut line = Vec::new();
    let mut position = 0u64;
    let result = (|| -> Result<(), String> {
        loop {
            line.clear();
            let line_start = position;
            let read = reader
                .read_until(b'\n', &mut line)
                .map_err(|error| format!("读取待删除会话失败: {}", error))?;
            if read == 0 {
                break;
            }
            position = position.saturating_add(read as u64);
            if line_start >= turn.start_offset && line_start < turn.end_offset {
                continue;
            }
            let next_line = scrub_compacted_line(
                &line,
                &turn.response_item_ids,
                &turn.response_item_fingerprints,
            )?;
            writer
                .write_all(&next_line)
                .map_err(|error| format!("写入删除后的会话失败: {}", error))?;
        }
        writer
            .flush()
            .map_err(|error| format!("刷新会话删除临时文件失败: {}", error))?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|error| format!("同步会话删除临时文件失败: {}", error))?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(writer);
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }
    drop(writer);
    Ok(tmp_path)
}

fn build_message_deletion_plan(
    path: &Path,
    turn: &ParsedSessionTurn,
    requested_message_ids: &HashSet<String>,
) -> Result<SessionMessageDeletionPlan, String> {
    let public_messages = turn
        .public
        .messages
        .iter()
        .filter(|message| requested_message_ids.contains(&message.id))
        .map(|message| {
            (
                message.id.clone(),
                (message.role.clone(), message.text.clone()),
            )
        })
        .collect::<HashMap<_, _>>();
    if public_messages.len() != requested_message_ids.len() {
        return Err("部分消息已不存在，请刷新会话内容后重试".to_string());
    }

    let input = fs::File::open(path).map_err(|error| format!("读取会话失败: {}", error))?;
    let mut reader = BufReader::new(input);
    let mut line = Vec::new();
    let mut position = 0u64;
    let mut line_offsets = HashSet::new();
    let mut matched_message_ids = HashSet::new();
    let mut response_item_ids = HashSet::new();
    let mut response_item_fingerprints = HashMap::new();

    loop {
        line.clear();
        let line_start = position;
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("读取待删除消息失败: {}", error))?;
        if read == 0 {
            break;
        }
        position = position.saturating_add(read as u64);
        if line_start < turn.start_offset || line_start >= turn.end_offset {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };

        if let Some(message) = parse_response_message(&value, line_start) {
            if public_messages.contains_key(&message.id) {
                matched_message_ids.insert(message.id);
                line_offsets.insert(line_start);
                if let Some((item_id, fingerprint)) = response_item_identity(&value) {
                    if let Some(item_id) = item_id {
                        response_item_ids.insert(item_id);
                    }
                    *response_item_fingerprints.entry(fingerprint).or_insert(0) += 1;
                }
            }
            continue;
        }

        let event_message = parse_event_user_message(&value, line_start)
            .map(|(message, _)| message)
            .or_else(|| parse_event_assistant_message(&value, line_start));
        let Some(event_message) = event_message else {
            continue;
        };
        let direct_match = public_messages.contains_key(&event_message.id);
        let duplicate_match = public_messages.values().any(|(role, text)| {
            role == &event_message.role && text.trim() == event_message.text.trim()
        });
        if direct_match || duplicate_match {
            if direct_match {
                matched_message_ids.insert(event_message.id.clone());
            }
            line_offsets.insert(line_start);
        }
    }

    if matched_message_ids.len() != requested_message_ids.len() {
        return Err("消息对应的会话记录已变化，请刷新后重试".to_string());
    }
    let mut message_ids = requested_message_ids.iter().cloned().collect::<Vec<_>>();
    message_ids.sort();
    Ok(SessionMessageDeletionPlan {
        message_ids,
        line_offsets,
        response_item_ids,
        response_item_fingerprints,
    })
}

fn rewrite_session_without_messages(
    path: &Path,
    plan: &SessionMessageDeletionPlan,
) -> Result<PathBuf, String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("session.jsonl");
    let tmp_path = parent.join(format!(
        ".{file_name}.message-delete-{:016x}.tmp",
        rand::random::<u64>()
    ));
    let input = fs::File::open(path).map_err(|error| format!("读取会话失败: {}", error))?;
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .map_err(|error| format!("创建消息删除临时文件失败: {}", error))?;
    let mut reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);
    let mut line = Vec::new();
    let mut position = 0u64;
    let result = (|| -> Result<(), String> {
        loop {
            line.clear();
            let line_start = position;
            let read = reader
                .read_until(b'\n', &mut line)
                .map_err(|error| format!("读取待删除消息失败: {}", error))?;
            if read == 0 {
                break;
            }
            position = position.saturating_add(read as u64);
            if plan.line_offsets.contains(&line_start) {
                continue;
            }
            let next_line = scrub_compacted_line(
                &line,
                &plan.response_item_ids,
                &plan.response_item_fingerprints,
            )?;
            writer
                .write_all(&next_line)
                .map_err(|error| format!("写入删除后的会话失败: {}", error))?;
        }
        writer
            .flush()
            .map_err(|error| format!("刷新消息删除临时文件失败: {}", error))?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|error| format!("同步消息删除临时文件失败: {}", error))?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(writer);
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }
    drop(writer);
    Ok(tmp_path)
}

fn restore_session_file_if_unchanged(
    path: &Path,
    backup_path: &Path,
    expected: SessionFileFingerprint,
) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("session.jsonl");
    let tmp_path = parent.join(format!(
        ".{file_name}.turn-restore-{:016x}.tmp",
        rand::random::<u64>()
    ));
    let result = (|| -> Result<(), String> {
        let input = fs::File::open(backup_path)
            .map_err(|error| format!("读取会话删除备份失败: {}", error))?;
        let output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .map_err(|error| format!("创建会话恢复临时文件失败: {}", error))?;
        let mut reader = BufReader::new(input);
        let mut writer = BufWriter::new(output);
        std::io::copy(&mut reader, &mut writer)
            .map_err(|error| format!("复制会话删除备份失败: {}", error))?;
        writer
            .flush()
            .map_err(|error| format!("刷新会话恢复临时文件失败: {}", error))?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|error| format!("同步会话恢复临时文件失败: {}", error))?;
        drop(writer);

        if session_file_fingerprint(path)? != expected {
            return Err("会话在恢复过程中发生了更新，本次操作已取消，请刷新后重试".to_string());
        }
        replace_file_atomic(path, &tmp_path)
            .map_err(|error| format!("恢复会话删除备份失败: {}", error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    result
}

fn scrub_compacted_line(
    line: &[u8],
    response_item_ids: &HashSet<String>,
    response_item_fingerprints: &HashMap<String, usize>,
) -> Result<Vec<u8>, String> {
    if !line.windows(9).any(|window| window == b"compacted") {
        return Ok(line.to_vec());
    }
    let Ok(mut value) = serde_json::from_slice::<Value>(line) else {
        return Ok(line.to_vec());
    };
    if value.get("type").and_then(Value::as_str) != Some("compacted") {
        return Ok(line.to_vec());
    }
    let Some(history) = value
        .get_mut("payload")
        .and_then(|payload| payload.get_mut("replacement_history"))
        .and_then(Value::as_array_mut)
    else {
        return Ok(line.to_vec());
    };
    let mut removed_by_fingerprint = HashMap::<String, usize>::new();
    history.retain(|item| {
        let id_matches = item
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| response_item_ids.contains(id));
        if id_matches {
            return false;
        }
        let Some(fingerprint) = response_item_payload_fingerprint(item) else {
            return true;
        };
        let allowed = response_item_fingerprints
            .get(&fingerprint)
            .copied()
            .unwrap_or_default();
        let removed = removed_by_fingerprint.entry(fingerprint).or_insert(0);
        if *removed < allowed {
            *removed += 1;
            false
        } else {
            true
        }
    });
    let had_newline = line.ends_with(b"\n");
    let mut serialized =
        serde_json::to_vec(&value).map_err(|error| format!("更新会话压缩历史失败: {}", error))?;
    if had_newline {
        serialized.push(b'\n');
    }
    Ok(serialized)
}

fn build_session_record(path: &Path, content: &str) -> Result<CodexSessionRecord, String> {
    let title = extract_title(content).unwrap_or_else(|| file_stem(path));
    let project_path = extract_project_path(content).unwrap_or_default();
    let project_name =
        project_name_for_path(&project_path).unwrap_or_else(|| "未归属项目".to_string());
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
        project_path,
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

#[cfg(test)]
fn copied_session_meta(
    source: &str,
    target_session_id: &str,
    target_provider: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> Result<String, String> {
    let mut value = source
        .lines()
        .find_map(|line| {
            let value = serde_json::from_str::<Value>(line).ok()?;
            (value.get("type").and_then(Value::as_str) == Some("session_meta")).then_some(value)
        })
        .ok_or_else(|| "源会话缺少 session_meta，不能安全复制".to_string())?;
    let timestamp = created_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let object = value
        .as_object_mut()
        .ok_or_else(|| "源会话的 session_meta 格式无效".to_string())?;
    object.insert("timestamp".to_string(), Value::String(timestamp.clone()));
    object.insert("ordinal".to_string(), Value::from(0));
    let payload = object
        .get_mut("payload")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "源会话的 session_meta 缺少 payload".to_string())?;
    payload.insert(
        "id".to_string(),
        Value::String(target_session_id.to_string()),
    );
    payload.insert(
        "session_id".to_string(),
        Value::String(target_session_id.to_string()),
    );
    if payload.contains_key("thread_id") {
        payload.insert(
            "thread_id".to_string(),
            Value::String(target_session_id.to_string()),
        );
    }
    payload.insert("timestamp".to_string(), Value::String(timestamp));
    payload.insert(
        "model_provider".to_string(),
        Value::String(target_provider.to_string()),
    );
    serde_json::to_string(&value).map_err(|error| format!("生成新会话身份失败: {error}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexThreadForkResult {
    session_id: String,
    history_mode: String,
    project_path: String,
}

#[derive(Debug)]
struct StagedForkSource {
    session: CodexSessionRecord,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ForkSourceSegment {
    path: PathBuf,
    end_byte_offset: Option<u64>,
}

fn fork_history_modes_are_compatible(source_mode: &str, fork_mode: &str) -> bool {
    fork_mode == "paginated" || (source_mode == "legacy" && fork_mode == "legacy")
}

fn format_copy_validation_failure(
    validation_error: &str,
    rollback_attempted: bool,
    cleanup_fork_error: Option<&str>,
    cleanup_staged_error: Option<&str>,
) -> String {
    let mut message = validation_error.to_string();
    if rollback_attempted {
        match cleanup_fork_error {
            Some(error) => message.push_str(&format!("；回滚新建副本失败: {error}")),
            None => message.push_str("，本次新建副本已回滚"),
        }
    }
    if let Some(error) = cleanup_staged_error {
        message.push_str(&format!("；清理临时复制数据失败: {error}"));
    }
    message
}

fn create_staged_fork_rollout(
    source_codex_home: &Path,
    source_path: &Path,
    staged_path: &Path,
    staged_session_id: &str,
    target_project_path: &str,
    target_provider: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    let segments = fork_source_segments(source_codex_home, source_path)?;
    let parent = staged_path
        .parent()
        .ok_or_else(|| "无法定位临时会话目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建临时会话目录失败: {error}"))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let target = options
        .open(staged_path)
        .map_err(|error| format!("创建临时会话失败 ({}): {error}", staged_path.display()))?;
    let mut writer = BufWriter::new(target);
    let timestamp = created_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut meta = read_session_meta_value(source_path)?;
    rewrite_staged_session_meta(
        &mut meta,
        staged_session_id,
        target_project_path,
        target_provider,
        &timestamp,
    )?;
    let mut history_count = 0_u64;
    let write_result = (|| {
        serde_json::to_writer(&mut writer, &meta)
            .map_err(|error| format!("生成临时会话身份失败: {error}"))?;
        writer
            .write_all(b"\n")
            .map_err(|error| format!("写入临时会话失败: {error}"))?;
        for segment in &segments {
            let source = fs::File::open(&segment.path)
                .map_err(|error| format!("读取源会话失败 ({}): {error}", segment.path.display()))?;
            let mut reader = BufReader::new(source);
            let mut consumed = 0_u64;
            let mut line = String::new();
            loop {
                line.clear();
                let read = reader
                    .read_line(&mut line)
                    .map_err(|error| format!("读取源会话记录失败: {error}"))?;
                if read == 0 {
                    break;
                }
                let next_consumed = consumed.saturating_add(read as u64);
                if segment
                    .end_byte_offset
                    .is_some_and(|end| next_consumed > end)
                {
                    break;
                }
                consumed = next_consumed;
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(mut value) = serde_json::from_str::<Value>(&line) else {
                    // An incomplete or malformed record cannot be consumed by Codex. Omitting it
                    // and assigning fresh ordinals keeps subsequent valid history usable.
                    continue;
                };
                if value.get("type").and_then(Value::as_str) == Some("session_meta") {
                    continue;
                }
                history_count = history_count.saturating_add(1);
                value
                    .as_object_mut()
                    .ok_or_else(|| "源会话记录格式无效".to_string())?
                    .insert("ordinal".to_string(), Value::from(history_count));
                serde_json::to_writer(&mut writer, &value)
                    .map_err(|error| format!("生成临时会话记录失败: {error}"))?;
                writer
                    .write_all(b"\n")
                    .map_err(|error| format!("写入临时会话失败: {error}"))?;
            }
        }
        if history_count == 0 {
            return Err("源会话没有可复制的历史数据".to_string());
        }
        writer
            .flush()
            .map_err(|error| format!("刷新临时会话失败: {error}"))?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|error| format!("同步临时会话失败: {error}"))
    })();
    if let Err(error) = write_result {
        drop(writer);
        let _ = fs::remove_file(staged_path);
        return Err(error);
    }
    Ok(())
}

fn read_session_meta_value(path: &Path) -> Result<Value, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("读取会话身份失败 ({}): {error}", path.display()))?;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| format!("读取会话身份失败: {error}"))?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            return Ok(value);
        }
    }
    Err(format!(
        "源会话缺少 session_meta，不能安全复制: {}",
        path.display()
    ))
}

fn rewrite_staged_session_meta(
    value: &mut Value,
    staged_session_id: &str,
    target_project_path: &str,
    target_provider: &str,
    timestamp: &str,
) -> Result<(), String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| "源会话的 session_meta 格式无效".to_string())?;
    object.insert(
        "timestamp".to_string(),
        Value::String(timestamp.to_string()),
    );
    object.insert("ordinal".to_string(), Value::from(0));
    object.remove("history_base");
    object.remove("subagent_history_start_ordinal");
    let payload = object
        .get_mut("payload")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "源会话的 session_meta 缺少 payload".to_string())?;
    payload.insert(
        "id".to_string(),
        Value::String(staged_session_id.to_string()),
    );
    payload.insert(
        "session_id".to_string(),
        Value::String(staged_session_id.to_string()),
    );
    if payload.contains_key("thread_id") {
        payload.insert(
            "thread_id".to_string(),
            Value::String(staged_session_id.to_string()),
        );
    }
    payload.insert(
        "timestamp".to_string(),
        Value::String(timestamp.to_string()),
    );
    payload.insert(
        "cwd".to_string(),
        Value::String(target_project_path.to_string()),
    );
    payload.insert(
        "model_provider".to_string(),
        Value::String(target_provider.to_string()),
    );
    payload.remove("history_base");
    payload.remove("subagent_history_start_ordinal");
    Ok(())
}

fn fork_source_segments(
    source_codex_home: &Path,
    source_path: &Path,
) -> Result<Vec<ForkSourceSegment>, String> {
    fn collect(
        codex_home: &Path,
        path: PathBuf,
        end_byte_offset: Option<u64>,
        seen: &mut HashSet<String>,
        segments: &mut Vec<ForkSourceSegment>,
    ) -> Result<(), String> {
        let meta = read_session_meta_value(&path)?;
        let payload = meta
            .get("payload")
            .and_then(Value::as_object)
            .ok_or_else(|| "源会话的 session_meta 缺少 payload".to_string())?;
        let history_base = payload
            .get("history_base")
            .or_else(|| meta.get("history_base"));
        if let Some(base) = history_base.and_then(Value::as_object) {
            let parent_id = base
                .get("thread_id")
                .or_else(|| base.get("threadId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| "源会话 history_base 缺少 thread_id".to_string())?;
            let parent_end = base
                .get("end_byte_offset")
                .or_else(|| base.get("endByteOffset"))
                .and_then(Value::as_u64)
                .ok_or_else(|| "源会话 history_base 缺少 end_byte_offset".to_string())?;
            if !seen.insert(parent_id.to_string()) {
                return Err(format!("源会话历史引用存在循环: {parent_id}"));
            }
            let parent_path = find_rollout_path_in_store(codex_home, parent_id)?;
            collect(codex_home, parent_path, Some(parent_end), seen, segments)?;
        }
        segments.push(ForkSourceSegment {
            path,
            end_byte_offset,
        });
        Ok(())
    }

    let source_meta = read_session_meta_value(source_path)?;
    let source_id = extract_session_id(
        &serde_json::to_string(&source_meta)
            .map_err(|error| format!("解析源会话 ID 失败: {error}"))?,
    )
    .unwrap_or_else(|| source_path.to_string_lossy().to_string());
    let mut seen = HashSet::from([source_id]);
    let mut segments = Vec::new();
    collect(
        source_codex_home,
        source_path.to_path_buf(),
        None,
        &mut seen,
        &mut segments,
    )?;
    Ok(segments)
}

fn find_rollout_path_in_store(codex_home: &Path, rollout_id: &str) -> Result<PathBuf, String> {
    for root in [
        codex_home.join("sessions"),
        codex_home.join("archived_sessions"),
    ] {
        for path in collect_jsonl_files(&root)? {
            if path
                .file_stem()
                .and_then(|value| value.to_str())
                .is_some_and(|stem| stem.ends_with(rollout_id))
                || session_file_has_id(&path, rollout_id)?
            {
                return Ok(path);
            }
        }
    }
    Err(format!("源会话引用的历史底稿不存在: {rollout_id}"))
}

fn remove_staged_thread_rows(db_path: &Path, session_id: &str) -> Result<(), String> {
    if !db_path.exists() {
        return Ok(());
    }
    let mut connection = Connection::open(db_path)
        .map_err(|error| format!("打开临时会话索引失败 ({}): {error}", db_path.display()))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| format!("设置临时会话索引等待时间失败: {error}"))?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("开启临时会话清理事务失败: {error}"))?;
    for (table, column) in [
        ("thread_items", "thread_id"),
        ("thread_turns", "thread_id"),
        ("thread_history_projection_state", "thread_id"),
        ("local_thread_catalog", "thread_id"),
        ("threads", "id"),
    ] {
        if !sqlite_table_exists_with_connection(&transaction, table)? {
            continue;
        }
        let columns = sqlite_table_columns_with_connection(&transaction, table)?;
        if !columns.contains(column) {
            continue;
        }
        transaction
            .execute(
                &format!("DELETE FROM {table} WHERE {column} = ?1"),
                params![session_id],
            )
            .map_err(|error| {
                format!(
                    "清理临时会话索引失败 ({} / {table}): {error}",
                    db_path.display()
                )
            })?;
    }
    transaction
        .commit()
        .map_err(|error| format!("提交临时会话清理失败: {error}"))
}

fn archive_staged_thread_row(
    db_path: &Path,
    session_id: &str,
    archived_path: &Path,
) -> Result<(), String> {
    if !db_path.exists() {
        return Ok(());
    }
    let mut connection = Connection::open(db_path)
        .map_err(|error| format!("打开副本历史底稿索引失败 ({}): {error}", db_path.display()))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| format!("设置副本历史底稿索引等待时间失败: {error}"))?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("开启副本历史底稿归档事务失败: {error}"))?;
    let thread_columns = sqlite_table_columns_with_connection(&transaction, "threads")?;
    if thread_columns.contains("id") {
        let mut assignments = Vec::new();
        if thread_columns.contains("rollout_path") {
            assignments.push("rollout_path = ?2".to_string());
        }
        if thread_columns.contains("archived") {
            assignments.push("archived = 1".to_string());
        }
        if thread_columns.contains("archived_at") {
            assignments.push(format!("archived_at = {}", now_timestamp()));
        }
        if !assignments.is_empty() {
            transaction
                .execute(
                    &format!(
                        "UPDATE threads SET {} WHERE id = ?1",
                        assignments.join(", ")
                    ),
                    params![session_id, archived_path.to_string_lossy()],
                )
                .map_err(|error| {
                    format!("归档副本历史底稿线程失败 ({}): {error}", db_path.display())
                })?;
        }
    }
    let catalog_columns =
        sqlite_table_columns_with_connection(&transaction, "local_thread_catalog")?;
    if catalog_columns.contains("thread_id") {
        let local_only = if catalog_columns.contains("host_id") {
            " AND host_id = 'local'"
        } else {
            ""
        };
        transaction
            .execute(
                &format!("DELETE FROM local_thread_catalog WHERE thread_id = ?1{local_only}"),
                params![session_id],
            )
            .map_err(|error| {
                format!(
                    "隐藏副本历史底稿侧栏记录失败 ({}): {error}",
                    db_path.display()
                )
            })?;
    }
    transaction
        .commit()
        .map_err(|error| format!("提交副本历史底稿归档失败: {error}"))
}

fn normalize_copy_target_directory(project_path: &str) -> Result<String, String> {
    let project_path = project_path.trim();
    if project_path.is_empty() {
        return Err("目标工作目录不能为空".to_string());
    }
    let directory = PathBuf::from(project_path);
    if !directory.is_absolute() {
        return Err("目标工作目录必须使用绝对路径".to_string());
    }
    if !directory.is_dir() {
        return Err(format!(
            "目标工作目录不存在或不是文件夹: {}",
            directory.display()
        ));
    }
    Ok(directory.to_string_lossy().to_string())
}

fn same_existing_directory(left: &str, right: &str) -> bool {
    let left = Path::new(left);
    let right = Path::new(right);
    if !left.is_dir() || !right.is_dir() {
        return false;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn codex_cli_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        for path in [
            "/Applications/ChatGPT.app/Contents/Resources/codex",
            "/Applications/Codex.app/Contents/Resources/codex",
        ] {
            let candidate = PathBuf::from(path);
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            let local_app_data = PathBuf::from(local_app_data);
            for relative in [
                ["Programs", "Codex", "resources", "codex.exe"].as_slice(),
                ["Programs", "Codex", "app", "resources", "codex.exe"].as_slice(),
                ["Codex", "resources", "codex.exe"].as_slice(),
            ] {
                let mut candidate = local_app_data.clone();
                for segment in relative {
                    candidate.push(segment);
                }
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }

    PathBuf::from(if cfg!(target_os = "windows") {
        "codex.exe"
    } else {
        "codex"
    })
}

fn write_codex_app_server_request(writer: &mut impl Write, request: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, request)
        .map_err(|error| format!("生成 Codex 会话复制请求失败: {error}"))?;
    writer
        .write_all(b"\n")
        .and_then(|_| writer.flush())
        .map_err(|error| format!("发送 Codex 会话复制请求失败: {error}"))
}

fn read_codex_app_server_response(
    reader: &mut impl BufRead,
    request_id: i64,
) -> Result<Value, String> {
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| format!("读取 Codex 会话复制响应失败: {error}"))?;
        if read == 0 {
            return Err("Codex 会话复制服务提前退出".to_string());
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("id").and_then(Value::as_i64) != Some(request_id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Codex 会话复制失败");
            return Err(message.to_string());
        }
        return Ok(value);
    }
}

fn parse_codex_thread_fork_response(response: &Value) -> Result<CodexThreadForkResult, String> {
    let session_id = response
        .pointer("/result/thread/id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Codex 会话复制响应缺少新会话 ID".to_string())?;
    let history_mode = response
        .pointer("/result/thread/historyMode")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Codex 会话复制响应缺少 history_mode".to_string())?;
    let project_path = response
        .pointer("/result/thread/cwd")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Codex 会话复制响应缺少工作目录".to_string())?;
    Ok(CodexThreadForkResult {
        session_id: session_id.to_string(),
        history_mode: history_mode.to_string(),
        project_path: project_path.to_string(),
    })
}

fn run_codex_thread_fork(
    codex_home: &Path,
    source: &CodexSessionRecord,
    target_project_path: &str,
    target_provider: &str,
) -> Result<CodexThreadForkResult, String> {
    let cli_path = codex_cli_path();
    let mut child = Command::new(&cli_path)
        .args(["app-server", "--stdio"])
        .env("CODEX_HOME", codex_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "无法启动 Codex 会话复制服务 ({}): {error}",
                cli_path.display()
            )
        })?;

    let stderr = child.stderr.take();
    let stderr_reader = std::thread::spawn(move || {
        let mut output = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut output);
        }
        output
    });
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "无法连接 Codex 会话复制服务输入流".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法连接 Codex 会话复制服务输出流".to_string())?;
    let mut reader = BufReader::new(stdout);

    let operation = (|| {
        write_codex_app_server_request(
            &mut stdin,
            &serde_json::json!({
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": {
                        "name": "codex-account-switcher",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {
                        "experimentalApi": true,
                    }
                }
            }),
        )?;
        read_codex_app_server_response(&mut reader, 1)?;
        write_codex_app_server_request(
            &mut stdin,
            &serde_json::json!({
                "id": 2,
                "method": "thread/fork",
                "params": {
                    "threadId": source.id,
                    "path": source.path,
                    "cwd": target_project_path,
                    "modelProvider": target_provider,
                    "ephemeral": false,
                    "excludeTurns": true,
                    "deferGoalContinuation": true,
                }
            }),
        )?;
        let response = read_codex_app_server_response(&mut reader, 2)?;
        parse_codex_thread_fork_response(&response)
    })();

    drop(stdin);
    drop(reader);
    let status = child.wait();
    let stderr = stderr_reader.join().unwrap_or_default();
    let fork = operation?;
    let status = status.map_err(|error| format!("等待 Codex 会话复制服务退出失败: {error}"))?;
    if !status.success() {
        let detail = stderr.lines().last().unwrap_or("").trim();
        return Err(if detail.is_empty() {
            "Codex 会话复制服务异常退出".to_string()
        } else {
            format!("Codex 会话复制服务异常退出: {detail}")
        });
    }
    Ok(fork)
}

fn new_session_id() -> String {
    let mut bytes = rand::random::<[u8; 16]>();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn new_session_rollout_path(
    sessions_dir: &Path,
    session_id: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> PathBuf {
    let local_time = created_at.with_timezone(&chrono::Local);
    sessions_dir
        .join(local_time.format("%Y").to_string())
        .join(local_time.format("%m").to_string())
        .join(local_time.format("%d").to_string())
        .join(format!(
            "rollout-{}-{session_id}.jsonl",
            local_time.format("%Y-%m-%dT%H-%M-%S")
        ))
}

#[cfg(test)]
fn create_new_session_file(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "无法定位新会话目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建新会话目录失败: {error}"))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("创建新会话失败 ({}): {error}", path.display()))?;
    if let Err(error) = file.write_all(content).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!("写入新会话失败 ({}): {error}", path.display()));
    }
    Ok(())
}

fn copied_session_title(source_title: &str, copy_suffix: &str) -> String {
    let base = source_title.trim();
    let base = if base.is_empty() {
        "未命名会话"
    } else {
        base
    };
    let suffix = copy_suffix.trim();
    let suffix = if suffix.is_empty() { "副本" } else { suffix };
    let suffix = format!(" {}", suffix.chars().take(30).collect::<String>());
    let max_base_chars = 100usize.saturating_sub(suffix.chars().count());
    format!(
        "{}{suffix}",
        base.chars().take(max_base_chars).collect::<String>()
    )
}

#[cfg(test)]
fn copy_history_onto_target(source: &str, target: &str) -> Result<String, String> {
    let target_meta = target
        .lines()
        .find(|line| is_session_meta_line(line))
        .ok_or_else(|| "目标会话缺少 session_meta，不能安全复制".to_string())?;
    let source_history = source
        .lines()
        .filter(|line| !line.trim().is_empty() && !is_session_meta_line(line))
        .collect::<Vec<_>>();
    if source_history.is_empty() {
        return Err("源会话没有可复制的历史数据".to_string());
    }
    let mut output = String::with_capacity(source.len() + target_meta.len());
    output.push_str(target_meta);
    output.push('\n');
    let mut response_item_ids = HashMap::new();
    let mut next_ordinal = rollout_line_ordinal(target_meta).map(|value| value.saturating_add(1));
    let mut copied_history_count = 0usize;
    for line in source_history {
        let Some(rewritten) = rewrite_copied_history_line(line, &mut response_item_ids)? else {
            continue;
        };
        output.push_str(&align_rollout_line_ordinal(&rewritten, next_ordinal)?);
        output.push('\n');
        if let Some(ordinal) = next_ordinal.as_mut() {
            *ordinal = ordinal.saturating_add(1);
        }
        copied_history_count = copied_history_count.saturating_add(1);
    }
    if copied_history_count == 0 {
        return Err("源会话只有不可跨会话使用的加密历史，无法安全复制".to_string());
    }
    let target_session_id = extract_session_id(target_meta)
        .ok_or_else(|| "目标会话缺少有效 ID，不能重建可见历史".to_string())?;
    append_portable_history_projection(&mut output, source, &target_session_id, &mut next_ordinal)?;
    Ok(output)
}

#[cfg(test)]
fn append_portable_history_projection(
    output: &mut String,
    source: &str,
    target_session_id: &str,
    next_ordinal: &mut Option<u64>,
) -> Result<(), String> {
    let messages = portable_history_messages(source);
    let turns = portable_history_turns(messages);
    if turns.is_empty() {
        return Err("源会话没有可显示的用户与助手消息".to_string());
    }

    let context_messages = turns
        .iter()
        .flat_map(|turn| turn.iter())
        .collect::<Vec<_>>();
    let desired_context_start = context_messages.len().saturating_sub(40);
    let context_start = context_messages[..=desired_context_start]
        .iter()
        .rposition(|message| message.role == "user")
        .or_else(|| {
            context_messages
                .iter()
                .position(|message| message.role == "user")
        })
        .unwrap_or(context_messages.len());
    let replacement_history = context_messages[context_start..]
        .iter()
        .map(|message| portable_response_message_payload(message))
        .collect::<Vec<_>>();
    let compacted_timestamp = context_messages
        .last()
        .map(|message| message.timestamp.clone())
        .filter(|timestamp| !timestamp.is_empty())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    append_rollout_value(
        output,
        json_value_with_type(
            &compacted_timestamp,
            "compacted",
            serde_json::json!({
                "message": "",
                "replacement_history": replacement_history,
            }),
        ),
        next_ordinal,
    )?;

    for turn in turns {
        let Some(user_message) = turn.first() else {
            continue;
        };
        let user_timestamp = user_message.timestamp.clone();
        let turn_id = new_local_response_item_id();
        let started_at_ms = portable_timestamp_millis(&user_timestamp);
        append_rollout_value(
            output,
            json_value_with_type(
                &user_timestamp,
                "event_msg",
                serde_json::json!({
                    "type": "task_started",
                    "turn_id": turn_id,
                    "started_at": started_at_ms / 1_000,
                    "model_context_window": 258_400,
                    "collaboration_mode_kind": "default",
                }),
            ),
            next_ordinal,
        )?;

        let mut last_agent_message = String::new();
        let mut completed_at_ms = started_at_ms;
        for message in turn {
            completed_at_ms = portable_timestamp_millis(&message.timestamp).max(completed_at_ms);
            let item = if message.role == "user" {
                serde_json::json!({
                    "type": "UserMessage",
                    "id": new_local_response_item_id(),
                    "client_id": new_local_response_item_id(),
                    "content": [{
                        "type": "text",
                        "text": portable_message_text(&message),
                        "text_elements": [],
                    }],
                })
            } else {
                last_agent_message = portable_message_text(&message);
                serde_json::json!({
                    "type": "AgentMessage",
                    "id": new_local_response_item_id(),
                    "content": [{
                        "type": "Text",
                        "text": last_agent_message,
                    }],
                    "phase": if message.phase.is_empty() { "final_answer" } else { message.phase.as_str() },
                })
            };
            append_rollout_value(
                output,
                json_value_with_type(
                    &message.timestamp,
                    "event_msg",
                    serde_json::json!({
                        "type": "item_completed",
                        "thread_id": target_session_id,
                        "turn_id": turn_id,
                        "item": item,
                        "started_at_ms": portable_timestamp_millis(&message.timestamp),
                        "completed_at_ms": portable_timestamp_millis(&message.timestamp),
                    }),
                ),
                next_ordinal,
            )?;
        }

        append_rollout_value(
            output,
            json_value_with_type(
                &user_timestamp,
                "event_msg",
                serde_json::json!({
                    "type": "task_complete",
                    "turn_id": turn_id,
                    "last_agent_message": last_agent_message,
                    "started_at": started_at_ms / 1_000,
                    "completed_at": completed_at_ms / 1_000,
                    "duration_ms": completed_at_ms.saturating_sub(started_at_ms),
                }),
            ),
            next_ordinal,
        )?;
    }
    Ok(())
}

#[cfg(test)]
fn portable_history_messages(source: &str) -> Vec<CodexSessionMessage> {
    let mut messages = Vec::new();
    let mut line_offset = 0u64;
    for segment in source.split_inclusive('\n') {
        let body = segment.trim_end_matches(['\r', '\n']);
        if let Ok(value) = serde_json::from_str::<Value>(body) {
            if let Some(mut message) = parse_response_message(&value, line_offset) {
                if message.text.trim().is_empty() {
                    message.text = value
                        .get("payload")
                        .and_then(|payload| payload.get("content"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|item| item.get("text").and_then(Value::as_str))
                        .filter(|text| !text.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join("\n\n");
                }
                if should_display_session_message(&message) {
                    messages.push(message);
                }
            }
        }
        line_offset = line_offset.saturating_add(segment.len() as u64);
    }
    messages
}

#[cfg(test)]
fn portable_history_turns(messages: Vec<CodexSessionMessage>) -> Vec<Vec<CodexSessionMessage>> {
    let mut turns = Vec::<Vec<CodexSessionMessage>>::new();
    for message in messages {
        if message.role == "user" {
            turns.push(vec![message]);
        } else if let Some(turn) = turns.last_mut() {
            turn.push(message);
        }
    }
    turns
}

#[cfg(test)]
fn portable_response_message_payload(message: &CodexSessionMessage) -> Value {
    let content_type = if message.role == "user" {
        "input_text"
    } else {
        "output_text"
    };
    serde_json::json!({
        "type": "message",
        "id": new_local_response_item_id(),
        "role": message.role,
        "content": [{
            "type": content_type,
            "text": portable_message_text(message),
        }],
    })
}

#[cfg(test)]
fn portable_message_text(message: &CodexSessionMessage) -> String {
    if !message.text.trim().is_empty() {
        return message.text.clone();
    }
    if message.attachments.is_empty() {
        String::new()
    } else {
        format!("[{} 个附件]", message.attachments.len())
    }
}

#[cfg(test)]
fn portable_timestamp_millis(timestamp: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .map(|value| value.timestamp_millis())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp_millis())
}

#[cfg(test)]
fn json_value_with_type(timestamp: &str, record_type: &str, payload: Value) -> Value {
    let timestamp = if timestamp.trim().is_empty() {
        chrono::Utc::now().to_rfc3339()
    } else {
        timestamp.to_string()
    };
    serde_json::json!({
        "timestamp": timestamp,
        "type": record_type,
        "payload": payload,
    })
}

#[cfg(test)]
fn append_rollout_value(
    output: &mut String,
    mut value: Value,
    next_ordinal: &mut Option<u64>,
) -> Result<(), String> {
    if let (Some(ordinal), Some(object)) = (*next_ordinal, value.as_object_mut()) {
        object.insert("ordinal".to_string(), Value::Number(ordinal.into()));
    }
    output.push_str(
        &serde_json::to_string(&value)
            .map_err(|error| format!("生成可见会话历史失败: {}", error))?,
    );
    output.push('\n');
    if let Some(ordinal) = next_ordinal.as_mut() {
        *ordinal = ordinal.saturating_add(1);
    }
    Ok(())
}

#[cfg(test)]
fn rollout_line_ordinal(line: &str) -> Option<u64> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("ordinal")?
        .as_u64()
}

#[cfg(test)]
fn align_rollout_line_ordinal(line: &str, ordinal: Option<u64>) -> Result<String, String> {
    let mut value = match serde_json::from_str::<Value>(line) {
        Ok(value) => value,
        Err(_) if ordinal.is_some() => {
            return Err("源会话包含无法写入分页记录的无效 JSON".to_string())
        }
        Err(_) => return Ok(line.to_string()),
    };
    let Some(object) = value.as_object_mut() else {
        if ordinal.is_some() {
            return Err("源会话包含非对象格式的分页记录".to_string());
        }
        return Ok(line.to_string());
    };
    let changed = match ordinal {
        Some(ordinal) => {
            if object.get("ordinal").and_then(Value::as_u64) == Some(ordinal) {
                false
            } else {
                object.insert("ordinal".to_string(), Value::Number(ordinal.into()));
                true
            }
        }
        None => object.remove("ordinal").is_some(),
    };
    if !changed {
        return Ok(line.to_string());
    }
    serde_json::to_string(&value).map_err(|error| format!("重写会话分页序号失败: {}", error))
}

fn normalize_paginated_rollout_ordinals(content: &str) -> Result<Option<String>, String> {
    let Some(first_record) = content.lines().find(|line| !line.trim().is_empty()) else {
        return Ok(None);
    };
    let Ok(first_value) = serde_json::from_str::<Value>(first_record) else {
        return Ok(None);
    };
    if first_value.get("type").and_then(Value::as_str) != Some("session_meta")
        || first_value.get("ordinal").and_then(Value::as_u64).is_none()
    {
        return Ok(None);
    }

    let mut output = String::with_capacity(content.len());
    let mut expected_ordinal = 0u64;
    let mut changed = false;
    for segment in content.split_inclusive('\n') {
        let (body, ending) = if let Some(body) = segment.strip_suffix("\r\n") {
            (body, "\r\n")
        } else if let Some(body) = segment.strip_suffix('\n') {
            (body, "\n")
        } else {
            (segment, "")
        };
        if body.trim().is_empty() {
            output.push_str(segment);
            continue;
        }
        let mut value = serde_json::from_str::<Value>(body).map_err(|error| {
            format!(
                "分页会话第 {} 条记录不是有效 JSON: {}",
                expected_ordinal, error
            )
        })?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| format!("分页会话第 {} 条记录不是 JSON 对象", expected_ordinal))?;
        if object.get("ordinal").and_then(Value::as_u64) == Some(expected_ordinal) {
            output.push_str(segment);
        } else {
            object.insert(
                "ordinal".to_string(),
                Value::Number(expected_ordinal.into()),
            );
            output.push_str(
                &serde_json::to_string(&value)
                    .map_err(|error| format!("序列化会话分页记录失败: {}", error))?,
            );
            output.push_str(ending);
            changed = true;
        }
        expected_ordinal = expected_ordinal.saturating_add(1);
    }
    Ok(changed.then_some(output))
}

#[cfg(test)]
fn rewrite_copied_history_line(
    line: &str,
    response_item_ids: &mut HashMap<String, String>,
) -> Result<Option<String>, String> {
    let Ok(mut value) = serde_json::from_str::<Value>(line) else {
        return Ok(Some(line.to_string()));
    };
    let mut changed = false;
    let mut removed_opaque_compaction = false;
    match value.get("type").and_then(Value::as_str) {
        Some("response_item") => {
            let Some(payload) = value.get_mut("payload") else {
                return Ok(Some(line.to_string()));
            };
            if copied_payload_is_opaque_compaction(payload) {
                return Ok(None);
            }
            changed |= rewrite_copied_payload(payload, response_item_ids);
        }
        Some("compacted") => {
            if let Some(history) = value
                .get_mut("payload")
                .and_then(|payload| payload.get_mut("replacement_history"))
                .and_then(Value::as_array_mut)
            {
                let original_len = history.len();
                history.retain_mut(|item| {
                    if copied_payload_is_opaque_compaction(item) {
                        return false;
                    }
                    changed |= rewrite_copied_payload(item, response_item_ids);
                    true
                });
                if history.len() != original_len {
                    changed = true;
                    removed_opaque_compaction = true;
                }
            }
        }
        _ => {}
    }
    if removed_opaque_compaction
        && value.get("type").and_then(Value::as_str) == Some("compacted")
        && value
            .get("payload")
            .and_then(|payload| payload.get("replacement_history"))
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    {
        return Ok(None);
    }
    if !changed {
        return Ok(Some(line.to_string()));
    }
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|error| format!("重写会话响应 ID 失败: {}", error))
}

#[cfg(test)]
fn copied_payload_is_opaque_compaction(payload: &Value) -> bool {
    payload.get("type").and_then(Value::as_str) == Some("compaction")
}

#[cfg(test)]
fn rewrite_copied_payload(
    payload: &mut Value,
    response_item_ids: &mut HashMap<String, String>,
) -> bool {
    let Some(payload) = payload.as_object_mut() else {
        return false;
    };
    // Encrypted reasoning is bound to the original response item and, for switched accounts,
    // may also be bound to the source account. Once the item receives a local ID, replaying the
    // original ciphertext makes the Responses API reject the entire copied conversation.
    let removed_encrypted_content = payload.remove("encrypted_content").is_some();
    let source_id = payload
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let item_type = payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if source_id.is_none() && !response_item_type_supports_id(item_type) {
        return removed_encrypted_content;
    }
    let local_id = match source_id {
        Some(source_id) => response_item_ids
            .entry(source_id)
            .or_insert_with(new_local_response_item_id)
            .clone(),
        None => new_local_response_item_id(),
    };
    payload.insert("id".to_string(), Value::String(local_id));
    true
}

#[cfg(test)]
fn new_local_response_item_id() -> String {
    // Codex assigns a prefixed ID when an item has no ID, then forwards prefixed IDs to the
    // Responses API. A non-prefixed local ID remains stable in history but is stripped from the
    // outbound request, so copied items cannot reference response objects owned by the source.
    format!("{:032x}", rand::random::<u128>())
}

#[cfg(test)]
fn response_item_type_supports_id(item_type: &str) -> bool {
    matches!(
        item_type,
        "additional_tools"
            | "message"
            | "agent_message"
            | "reasoning"
            | "local_shell_call"
            | "function_call"
            | "tool_search_call"
            | "function_call_output"
            | "custom_tool_call"
            | "custom_tool_call_output"
            | "tool_search_output"
            | "web_search_call"
            | "image_generation_call"
            | "compaction"
            | "compaction_summary"
            | "context_compaction"
    )
}

#[cfg(test)]
fn is_session_meta_line(line: &str) -> bool {
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .as_deref()
        == Some("session_meta")
}

fn normalize_custom_session_title(title: &str) -> Result<String, String> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Err("会话名称不能为空".to_string());
    }
    if title.chars().count() > 100 {
        return Err("会话名称不能超过 100 个字符".to_string());
    }
    Ok(title)
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

fn extract_project_path(content: &str) -> Option<String> {
    for line in content.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let cwd = find_string_key(&value, "cwd")
            .or_else(|| find_string_key(&value, "workspace"))
            .or_else(|| find_string_key(&value, "projectPath"))
            .or_else(|| find_string_key(&value, "workingDirectory"));
        let Some(cwd) = cwd else {
            continue;
        };
        let trimmed = cwd.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn project_name_for_path(project_path: &str) -> Option<String> {
    let project_path = project_path.trim();
    if project_path.is_empty() {
        return None;
    }
    Some(
        Path::new(project_path)
            .file_name()
            .and_then(|item| item.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| project_path.to_string()),
    )
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

fn desktop_project_id(root: &str) -> String {
    let mut bytes = Sha256::digest(format!("codex-switcher-project:{root}").as_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

const MAX_GENERATED_IMAGE_BYTES: usize = 100 * 1024 * 1024;

fn rewrite_local_image_attachments(
    content: &str,
    codex_home: &Path,
    session_id: &str,
) -> Result<(Option<String>, GeneratedImageRepairResult), String> {
    let mut result = GeneratedImageRepairResult::default();
    let mut pending_inline_images = Vec::<String>::new();
    let mut rewritten = String::with_capacity(content.len());
    let mut changed = false;

    for segment in content.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map_or((segment, ""), |line| (line, "\n"));
        let mut value = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(_) => {
                rewritten.push_str(segment);
                continue;
            }
        };
        if let Some(images) = inline_user_image_payloads(&value) {
            pending_inline_images = images;
        }

        let mut line_changed = false;
        if !pending_inline_images.is_empty()
            && value.get("type").and_then(Value::as_str) == Some("event_msg")
            && value
                .get("payload")
                .and_then(|payload| payload.get("type"))
                .and_then(Value::as_str)
                == Some("item_completed")
            && value
                .get("payload")
                .and_then(|payload| payload.get("item"))
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str)
                == Some("UserMessage")
        {
            let item_id = value
                .get("payload")
                .and_then(|payload| payload.get("item"))
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("user-message")
                .to_string();
            let image_key = if safe_generated_image_id(&item_id) {
                item_id
            } else {
                short_hash(&item_id)
            };
            if let Some(items) = value
                .get_mut("payload")
                .and_then(|payload| payload.get_mut("item"))
                .and_then(|item| item.get_mut("content"))
                .and_then(Value::as_array_mut)
            {
                let local_images = items
                    .iter_mut()
                    .filter(|item| {
                        matches!(
                            item.get("type").and_then(Value::as_str),
                            Some("local_image" | "localImage")
                        )
                    })
                    .collect::<Vec<_>>();
                for (index, (item, encoded)) in local_images
                    .into_iter()
                    .zip(pending_inline_images.iter())
                    .enumerate()
                {
                    let Some((bytes, extension)) = decode_generated_image(encoded) else {
                        result.invalid += 1;
                        continue;
                    };
                    let image_path = codex_home
                        .join("generated_images")
                        .join(session_id)
                        .join(format!("recovered-{image_key}-{index}.{extension}"));
                    if generated_image_extension_from_file(&image_path) != Some(extension) {
                        if let Some(parent) = image_path.parent() {
                            fs::create_dir_all(parent).map_err(|error| {
                                format!("创建历史图片恢复目录失败 ({}): {error}", parent.display())
                            })?;
                        }
                        write_bytes_atomic(&image_path, &bytes).map_err(|error| {
                            format!("恢复历史图片失败 ({}): {error}", image_path.display())
                        })?;
                        result.recreated += 1;
                    }
                    if generated_image_extension_from_file(&image_path) == Some(extension) {
                        result.verified += 1;
                    } else {
                        result.invalid += 1;
                        continue;
                    }
                    let stable_path = image_path.to_string_lossy().to_string();
                    if item.get("path").and_then(Value::as_str) != Some(stable_path.as_str()) {
                        item["path"] = Value::String(stable_path);
                        line_changed = true;
                    }
                }
            }
            pending_inline_images.clear();
        }

        if line_changed {
            rewritten.push_str(
                &serde_json::to_string(&value)
                    .map_err(|error| format!("序列化历史图片恢复记录失败: {error}"))?,
            );
            rewritten.push_str(newline);
            changed = true;
        } else {
            rewritten.push_str(segment);
        }
    }
    Ok((changed.then_some(rewritten), result))
}

fn inline_user_image_payloads(value: &Value) -> Option<Vec<String>> {
    if value.get("type").and_then(Value::as_str) != Some("response_item") {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("message")
        || payload.get("role").and_then(Value::as_str) != Some("user")
    {
        return None;
    }
    let images = payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| {
            matches!(
                item.get("type").and_then(Value::as_str),
                Some("input_image" | "image")
            )
        })
        .filter_map(|item| {
            item.get("image_url")
                .or_else(|| item.get("imageUrl"))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    (!images.is_empty()).then_some(images)
}

fn generated_image_payload(value: &Value) -> Option<(&str, &str)> {
    let record_type = value.get("type").and_then(Value::as_str)?;
    let payload = value.get("payload")?.as_object()?;
    let payload_type = payload.get("type").and_then(Value::as_str)?;
    let completed = payload
        .get("status")
        .and_then(Value::as_str)
        .is_none_or(|status| !matches!(status, "failed" | "cancelled" | "canceled"));
    if !completed {
        return None;
    }
    let image_id = match (record_type, payload_type) {
        ("event_msg", "image_generation_end") => payload.get("call_id"),
        ("response_item", "image_generation_call") => payload.get("id"),
        _ => return None,
    }
    .and_then(Value::as_str)?;
    let encoded = payload.get("result").and_then(Value::as_str)?;
    Some((image_id, encoded))
}

fn safe_generated_image_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn decode_generated_image(encoded: &str) -> Option<(Vec<u8>, &'static str)> {
    let encoded = encoded.trim();
    let payload = if encoded.starts_with("data:image/") {
        let (metadata, payload) = encoded.split_once(',')?;
        if !metadata.ends_with(";base64") {
            return None;
        }
        payload
    } else {
        encoded
    };
    if payload.is_empty() || payload.len() > MAX_GENERATED_IMAGE_BYTES.saturating_mul(2) {
        return None;
    }
    let bytes = general_purpose::STANDARD.decode(payload).ok()?;
    if bytes.len() > MAX_GENERATED_IMAGE_BYTES {
        return None;
    }
    let extension = generated_image_extension(&bytes)?;
    Some((bytes, extension))
}

fn encoded_generated_image_extension(encoded: &str) -> Option<&'static str> {
    let encoded = encoded.trim();
    let payload = if encoded.starts_with("data:image/") {
        let (metadata, payload) = encoded.split_once(',')?;
        if !metadata.ends_with(";base64") {
            return None;
        }
        payload
    } else {
        encoded
    };
    if payload.starts_with("iVBORw0KGgo") {
        Some("png")
    } else if payload.starts_with("/9j/") {
        Some("jpg")
    } else if payload.starts_with("R0lGOD") {
        Some("gif")
    } else if payload.starts_with("UklGR") {
        Some("webp")
    } else {
        None
    }
}

fn generated_image_extension_from_file(path: &Path) -> Option<&'static str> {
    let mut file = fs::File::open(path).ok()?;
    let mut header = [0_u8; 16];
    let read = file.read(&mut header).ok()?;
    generated_image_extension(&header[..read])
}

fn generated_image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::{
        archive_staged_thread_row, copy_history_onto_target, create_staged_fork_rollout,
        extract_title, find_task_started_offsets_reverse, fork_history_modes_are_compatible,
        format_copy_validation_failure, normalize_copy_target_directory,
        parse_codex_thread_fork_response, remove_staged_thread_rows, repair_sqlite_db,
        restore_session_file_if_unchanged, same_existing_directory, session_file_fingerprint,
        sql_quote, SessionRepairRecord, SessionStore,
    };
    use serde_json::{json, Value};
    use std::collections::HashSet;
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use std::process::Command;
    use tempfile::tempdir;

    fn write_jsonl(path: &Path, items: &[Value]) {
        let mut content = items
            .iter()
            .map(|item| serde_json::to_string(item).expect("serialize jsonl item"))
            .collect::<Vec<_>>()
            .join("\n");
        content.push('\n');
        fs::write(path, content).expect("write jsonl");
    }

    #[test]
    fn copy_target_directory_requires_an_existing_absolute_folder() {
        let target = tempdir().expect("target directory");
        assert_eq!(
            normalize_copy_target_directory(&target.path().to_string_lossy())
                .expect("valid target"),
            target.path().to_string_lossy()
        );
        assert!(normalize_copy_target_directory("relative/project")
            .expect_err("relative target should fail")
            .contains("绝对路径"));
        assert!(
            normalize_copy_target_directory(&target.path().join("missing").to_string_lossy())
                .expect_err("missing target should fail")
                .contains("不存在")
        );
    }

    #[test]
    fn copy_target_directory_must_differ_from_the_source_directory() {
        let source = tempdir().expect("source directory");
        let target = tempdir().expect("target directory");
        assert!(same_existing_directory(
            &source.path().to_string_lossy(),
            &source.path().join(".").to_string_lossy()
        ));
        assert!(!same_existing_directory(
            &source.path().to_string_lossy(),
            &target.path().to_string_lossy()
        ));
    }

    #[test]
    fn parses_the_official_codex_thread_fork_response() {
        let response = json!({
            "id": 2,
            "result": {
                "thread": {
                    "id": "01a03a00-0000-7000-8000-000000000001",
                    "historyMode": "paginated",
                    "cwd": "/tmp/target"
                }
            }
        });
        let result = parse_codex_thread_fork_response(&response).expect("fork response");
        assert_eq!(result.session_id, "01a03a00-0000-7000-8000-000000000001");
        assert_eq!(result.history_mode, "paginated");
        assert_eq!(result.project_path, "/tmp/target");
        assert!(
            parse_codex_thread_fork_response(&json!({ "id": 2, "result": {} }))
                .expect_err("missing id should fail")
                .contains("新会话 ID")
        );
    }

    #[test]
    fn imported_legacy_history_accepts_a_legacy_fork_but_paginated_history_does_not() {
        assert!(fork_history_modes_are_compatible("legacy", "legacy"));
        assert!(fork_history_modes_are_compatible("legacy", "paginated"));
        assert!(fork_history_modes_are_compatible("paginated", "paginated"));
        assert!(!fork_history_modes_are_compatible("paginated", "legacy"));
        assert!(!fork_history_modes_are_compatible("legacy", "unknown"));
    }

    #[test]
    fn failed_fork_validation_reports_whether_the_new_copy_was_rolled_back() {
        assert_eq!(
            format_copy_validation_failure("模式错误", true, None, None),
            "模式错误，本次新建副本已回滚"
        );
        let incomplete = format_copy_validation_failure(
            "模式错误",
            true,
            Some("文件仍被占用"),
            Some("临时索引仍被占用"),
        );
        assert!(incomplete.contains("回滚新建副本失败: 文件仍被占用"));
        assert!(incomplete.contains("清理临时复制数据失败: 临时索引仍被占用"));
        assert!(!incomplete.contains("已回滚"));
        assert_eq!(
            format_copy_validation_failure("ID 重复", false, None, None),
            "ID 重复"
        );
    }

    #[test]
    fn failed_fork_validation_removes_the_new_file_and_index_rows() {
        let codex = tempdir().expect("codex home");
        let sessions_dir = codex.path().join("sessions/2026/08/26");
        fs::create_dir_all(&sessions_dir).expect("sessions directory");
        let session_id = "01a03be2-4a51-7df1-82a0-396038f1b0c8";
        let session_path = sessions_dir.join(format!("rollout-{session_id}.jsonl"));
        fs::write(
            &session_path,
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":session_id}})
            ),
        )
        .expect("fork rollout");
        let state_db = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &state_db,
            &format!(
                "CREATE TABLE threads (id TEXT PRIMARY KEY);\
                 CREATE TABLE local_thread_catalog (thread_id TEXT PRIMARY KEY);\
                 INSERT INTO threads VALUES ('{session_id}');\
                 INSERT INTO local_thread_catalog VALUES ('{session_id}');"
            ),
        );
        let history_db = codex.path().join("thread_history_1.sqlite");
        run_sqlite_test(
            &history_db,
            &format!(
                "CREATE TABLE thread_items (thread_id TEXT, item_id TEXT);\
                 CREATE TABLE thread_turns (thread_id TEXT, turn_id TEXT);\
                 CREATE TABLE thread_history_projection_state (thread_id TEXT PRIMARY KEY);\
                 INSERT INTO thread_items VALUES ('{session_id}', 'item');\
                 INSERT INTO thread_turns VALUES ('{session_id}', 'turn');\
                 INSERT INTO thread_history_projection_state VALUES ('{session_id}');"
            ),
        );

        let store = SessionStore::new(codex.path().to_path_buf());
        store
            .cleanup_created_fork(session_id, &HashSet::new())
            .expect("rollback invalid fork");

        assert!(!session_path.exists());
        assert_eq!(
            run_sqlite_test_output(
                &state_db,
                &format!("SELECT COUNT(*) FROM threads WHERE id = '{session_id}';")
            )
            .trim(),
            "0"
        );
        assert_eq!(
            run_sqlite_test_output(
                &history_db,
                &format!(
                    "SELECT COUNT(*) FROM thread_history_projection_state WHERE thread_id = '{session_id}';"
                )
            )
            .trim(),
            "0"
        );
    }

    #[test]
    fn failed_fork_validation_never_removes_a_preexisting_session() {
        let codex = tempdir().expect("codex home");
        let sessions_dir = codex.path().join("sessions/2026/08/26");
        fs::create_dir_all(&sessions_dir).expect("sessions directory");
        let session_id = "01a03be2-4a51-7df1-82a0-396038f1b0c8";
        let session_path = sessions_dir.join(format!("rollout-{session_id}.jsonl"));
        fs::write(
            &session_path,
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":session_id}})
            ),
        )
        .expect("preexisting rollout");
        let state_db = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &state_db,
            &format!(
                "CREATE TABLE threads (id TEXT PRIMARY KEY);\
                 INSERT INTO threads VALUES ('{session_id}');"
            ),
        );

        let store = SessionStore::new(codex.path().to_path_buf());
        let error = store
            .cleanup_created_fork(session_id, &HashSet::from([session_path.clone()]))
            .expect_err("preexisting session must not be rolled back");

        assert!(error.contains("复制前已存在"));
        assert!(session_path.exists());
        assert_eq!(
            run_sqlite_test_output(
                &state_db,
                &format!("SELECT COUNT(*) FROM threads WHERE id = '{session_id}';")
            )
            .trim(),
            "1"
        );
    }

    #[test]
    fn stages_a_fork_with_a_fresh_identity_and_contiguous_ordinals() {
        let source_dir = tempdir().expect("source directory");
        let target_dir = tempdir().expect("target directory");
        let source_path = source_dir.path().join("source.jsonl");
        let staged_path = target_dir.path().join("staged.jsonl");
        fs::write(
            &source_path,
            concat!(
                r#"{"type":"session_meta","payload":{"id":"source","session_id":"source","thread_id":"source","cwd":"/old","model_provider":"old"},"ordinal":0}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"保留历史"}]},"ordinal":476}"#,
                "\n",
                "not-json\n",
                r#"{"type":"event_msg","payload":{"type":"task_complete"},"ordinal":627}"#,
                "\n"
            ),
        )
        .expect("write source");
        let created_at = chrono::DateTime::parse_from_rfc3339("2026-08-25T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&chrono::Utc);

        create_staged_fork_rollout(
            source_dir.path(),
            &source_path,
            &staged_path,
            "staged-id",
            &target_dir.path().to_string_lossy(),
            "target-provider",
            created_at,
        )
        .expect("stage rollout");

        let values = fs::read_to_string(staged_path)
            .expect("read staged")
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("valid staged line"))
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 3);
        assert_eq!(
            values
                .iter()
                .map(|value| value["ordinal"].as_u64().expect("ordinal"))
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(values[0]["payload"]["id"], "staged-id");
        assert_eq!(values[0]["payload"]["session_id"], "staged-id");
        assert_eq!(values[0]["payload"]["thread_id"], "staged-id");
        assert_eq!(values[0]["payload"]["model_provider"], "target-provider");
        assert_eq!(
            values[0]["payload"]["cwd"],
            target_dir.path().to_string_lossy().as_ref()
        );
        assert_eq!(values[1]["payload"]["type"], "message");
        assert_eq!(values[2]["payload"]["type"], "task_complete");
    }

    #[test]
    fn stages_the_complete_lineage_of_an_existing_paginated_fork() {
        let source_home = tempdir().expect("source home");
        let target_dir = tempdir().expect("target directory");
        let sessions_dir = source_home.path().join("sessions/2026/08/25");
        fs::create_dir_all(&sessions_dir).expect("sessions directory");
        let parent_path = sessions_dir.join("rollout-2026-08-25T10-00-00-parent-id.jsonl");
        let parent_meta = concat!(
            r#"{"type":"session_meta","payload":{"id":"parent-id"},"ordinal":0}"#,
            "\n"
        );
        let inherited = concat!(
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"继承内容"}]},"ordinal":1}"#,
            "\n"
        );
        let excluded = concat!(
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"截断后内容"}]},"ordinal":2}"#,
            "\n"
        );
        fs::write(&parent_path, format!("{parent_meta}{inherited}{excluded}"))
            .expect("write parent");
        let cutoff = (parent_meta.len() + inherited.len()) as u64;
        let child_path = sessions_dir.join("rollout-2026-08-25T11-00-00-child-id.jsonl");
        fs::write(
            &child_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session_meta",
                    "payload": {
                        "id": "child-id",
                        "history_base": {
                            "thread_id": "parent-id",
                            "end_ordinal_exclusive": 2,
                            "end_byte_offset": cutoff
                        }
                    },
                    "ordinal": 2
                }),
                serde_json::json!({
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "assistant",
                        "content": [{"text": "子会话内容"}]
                    },
                    "ordinal": 3
                })
            ),
        )
        .expect("write child");
        let staged_path = target_dir.path().join("staged.jsonl");

        create_staged_fork_rollout(
            source_home.path(),
            &child_path,
            &staged_path,
            "staged-id",
            &target_dir.path().to_string_lossy(),
            "openai",
            chrono::Utc::now(),
        )
        .expect("stage fork lineage");

        let staged = fs::read_to_string(staged_path).expect("read staged");
        assert!(staged.contains("继承内容"));
        assert!(!staged.contains("截断后内容"));
        assert!(staged.contains("子会话内容"));
        assert!(!staged.contains("history_base"));
        assert_eq!(
            staged
                .lines()
                .map(|line| {
                    serde_json::from_str::<Value>(line).expect("valid staged line")["ordinal"]
                        .as_u64()
                        .expect("ordinal")
                })
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn removes_only_the_staged_thread_rows() {
        let temp = tempdir().expect("temp directory");
        let db_path = temp.path().join("state_5.sqlite");
        run_sqlite_test(
            &db_path,
            r#"
                CREATE TABLE threads (id TEXT PRIMARY KEY);
                CREATE TABLE thread_items (thread_id TEXT NOT NULL, item_id TEXT NOT NULL);
                CREATE TABLE thread_turns (thread_id TEXT NOT NULL, turn_id TEXT NOT NULL);
                CREATE TABLE thread_history_projection_state (
                    thread_id TEXT PRIMARY KEY,
                    next_rollout_byte_offset INTEGER NOT NULL,
                    next_rollout_ordinal INTEGER NOT NULL
                );
                INSERT INTO threads (id) VALUES ('staged'), ('keep');
                INSERT INTO thread_items (thread_id, item_id) VALUES ('staged', 'drop'), ('keep', 'keep');
                INSERT INTO thread_turns (thread_id, turn_id) VALUES ('staged', 'drop'), ('keep', 'keep');
                INSERT INTO thread_history_projection_state VALUES ('staged', 1, 1), ('keep', 1, 1);
            "#,
        );

        remove_staged_thread_rows(&db_path, "staged").expect("remove staged rows");

        for table in [
            "threads",
            "thread_items",
            "thread_turns",
            "thread_history_projection_state",
        ] {
            assert_eq!(
                run_sqlite_test_output(
                    &db_path,
                    &format!(
                        "SELECT COUNT(*) FROM {table} WHERE {} = 'staged';",
                        if table == "threads" {
                            "id"
                        } else {
                            "thread_id"
                        }
                    ),
                )
                .trim(),
                "0"
            );
            assert_eq!(
                run_sqlite_test_output(
                    &db_path,
                    &format!(
                        "SELECT COUNT(*) FROM {table} WHERE {} = 'keep';",
                        if table == "threads" {
                            "id"
                        } else {
                            "thread_id"
                        }
                    ),
                )
                .trim(),
                "1"
            );
        }
    }

    #[test]
    fn archives_a_staged_fork_base_without_removing_its_history_projection() {
        let temp = tempdir().expect("temp directory");
        let db_path = temp.path().join("state_5.sqlite");
        let archived_path = temp.path().join("archived_sessions/staged.jsonl");
        run_sqlite_test(
            &db_path,
            r#"
                CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    rollout_path TEXT,
                    archived INTEGER NOT NULL DEFAULT 0,
                    archived_at INTEGER
                );
                CREATE TABLE thread_items (thread_id TEXT NOT NULL, item_id TEXT NOT NULL);
                CREATE TABLE local_thread_catalog (host_id TEXT NOT NULL, thread_id TEXT NOT NULL);
                INSERT INTO threads (id, rollout_path) VALUES ('staged', '/old/staged.jsonl');
                INSERT INTO thread_items (thread_id, item_id) VALUES ('staged', 'history');
                INSERT INTO local_thread_catalog (host_id, thread_id) VALUES ('local', 'staged');
            "#,
        );

        archive_staged_thread_row(&db_path, "staged", &archived_path).expect("archive staged row");

        let row = run_sqlite_test_output(
            &db_path,
            "SELECT rollout_path || '|' || archived || '|' || (archived_at IS NOT NULL) FROM threads WHERE id = 'staged';",
        );
        assert_eq!(
            row.trim(),
            format!("{}|1|1", archived_path.to_string_lossy())
        );
        assert_eq!(
            run_sqlite_test_output(
                &db_path,
                "SELECT COUNT(*) FROM thread_items WHERE thread_id = 'staged';",
            )
            .trim(),
            "1"
        );
        assert_eq!(
            run_sqlite_test_output(
                &db_path,
                "SELECT COUNT(*) FROM local_thread_catalog WHERE thread_id = 'staged';",
            )
            .trim(),
            "0"
        );
    }

    #[test]
    fn sqlite_visibility_repair_preserves_and_recovers_paginated_history_mode() {
        let codex = tempdir().expect("codex tempdir");
        let existing_path = codex.path().join("existing-paginated.jsonl");
        let missing_path = codex.path().join("missing-paginated.jsonl");
        let legacy_path = codex.path().join("legacy.jsonl");
        for path in [&existing_path, &missing_path] {
            fs::write(
                path,
                concat!(
                    r#"{"type":"session_meta","payload":{"id":"paginated"},"ordinal":0}"#,
                    "\n",
                    r#"{"type":"response_item","payload":{"type":"message"},"ordinal":1}"#,
                    "\n"
                ),
            )
            .expect("write paginated rollout");
        }
        fs::write(
            &legacy_path,
            concat!(r#"{"type":"session_meta","payload":{"id":"legacy"}}"#, "\n"),
        )
        .expect("write legacy rollout");
        let db_path = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &db_path,
            r#"
                CREATE TABLE threads (id TEXT PRIMARY KEY, history_mode TEXT);
                INSERT INTO threads (id, history_mode) VALUES ('existing', 'legacy');
            "#,
        );
        let records = vec![
            SessionRepairRecord {
                id: "existing".to_string(),
                title: "Existing".to_string(),
                path: existing_path,
                updated_at: 10,
            },
            SessionRepairRecord {
                id: "missing".to_string(),
                title: "Missing".to_string(),
                path: missing_path,
                updated_at: 10,
            },
            SessionRepairRecord {
                id: "legacy".to_string(),
                title: "Legacy".to_string(),
                path: legacy_path,
                updated_at: 10,
            },
        ];

        assert_eq!(
            repair_sqlite_db(&db_path, "openai", Some(&records)).expect("repair sqlite"),
            3
        );
        let rows = run_sqlite_test_output(
            &db_path,
            "SELECT id || '|' || history_mode FROM threads ORDER BY id;",
        );
        assert_eq!(
            rows.lines().collect::<Vec<_>>(),
            vec!["existing|paginated", "legacy|legacy", "missing|paginated"]
        );
    }

    fn task_started(turn_id: &str, timestamp: &str) -> Value {
        json!({
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": { "type": "task_started", "turn_id": turn_id }
        })
    }

    fn task_complete(turn_id: &str, timestamp: &str) -> Value {
        json!({
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": { "type": "task_complete", "turn_id": turn_id }
        })
    }

    fn response_message(id: &str, role: &str, timestamp: &str, content: Vec<Value>) -> Value {
        json!({
            "timestamp": timestamp,
            "type": "response_item",
            "payload": {
                "type": "message",
                "id": id,
                "role": role,
                "content": content
            }
        })
    }

    #[test]
    fn pages_session_turns_and_loads_inline_images_on_demand() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-content.jsonl");
        let local_image = codex.path().join("preview.png");
        fs::write(&local_image, b"image").expect("write local image");
        write_jsonl(
            &session_path,
            &[
                json!({"type":"session_meta","payload":{"id":"session-content"}}),
                task_started("turn-one", "2026-08-10T01:00:00Z"),
                response_message(
                    "message-one",
                    "user",
                    "2026-08-10T01:00:01Z",
                    vec![
                        json!({"type":"input_text","text":"第一轮问题"}),
                        json!({"type":"input_image","image_url":"data:image/png;base64,aGVsbG8="}),
                    ],
                ),
                json!({
                    "timestamp":"2026-08-10T01:00:01Z",
                    "type":"event_msg",
                    "payload":{
                        "type":"user_message",
                        "message":"第一轮问题",
                        "local_images":[local_image.to_string_lossy()]
                    }
                }),
                response_message(
                    "message-two",
                    "assistant",
                    "2026-08-10T01:00:02Z",
                    vec![json!({"type":"output_text","text":"第一轮回答"})],
                ),
                task_complete("turn-one", "2026-08-10T01:00:03Z"),
                task_started("turn-two", "2026-08-10T02:00:00Z"),
                response_message(
                    "message-three",
                    "user",
                    "2026-08-10T02:00:01Z",
                    vec![json!({"type":"input_text","text":"第二轮问题"})],
                ),
                task_complete("turn-two", "2026-08-10T02:00:02Z"),
            ],
        );
        let store = SessionStore::new(codex.path().to_path_buf());

        let first_page = store
            .list_session_content("session-content", None, Some(1), None)
            .expect("first page");
        assert_eq!(first_page.turns.len(), 1);
        assert_eq!(first_page.turns[0].id, "turn-one");
        assert_eq!(first_page.turns[0].messages.len(), 2);
        let image = &first_page.turns[0].messages[0].attachments[0];
        assert_eq!(image.kind, "image");
        assert!(image.inline);
        assert_eq!(
            image.source_path.as_deref(),
            Some(local_image.to_string_lossy().as_ref())
        );
        let asset = store
            .get_session_asset("session-content", &image.id)
            .expect("load image");
        assert_eq!(asset.mime_type, "image/png");
        assert_eq!(asset.data_url, "data:image/png;base64,aGVsbG8=");

        let second_page = store
            .list_session_content("session-content", first_page.next_cursor, Some(1), None)
            .expect("second page");
        assert_eq!(second_page.turns.len(), 1);
        assert_eq!(second_page.turns[0].id, "turn-two");
        assert!(second_page.next_cursor.is_none());
    }

    #[test]
    fn pages_session_turns_in_descending_order_without_returning_every_turn() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-desc-content.jsonl");
        let mut lines = vec![json!({
            "type":"session_meta",
            "payload":{"id":"session-desc-content"}
        })];
        for index in 1..=45 {
            let turn_id = format!("turn-{index:02}");
            let timestamp = format!("2026-08-10T{index:02}:00:00Z");
            lines.push(task_started(&turn_id, &timestamp));
            lines.push(response_message(
                &format!("message-{index:02}"),
                "user",
                &timestamp,
                vec![json!({"type":"input_text","text":format!("问题 {index}")})],
            ));
            lines.push(task_complete(&turn_id, &timestamp));
        }
        write_jsonl(&session_path, &lines);
        let store = SessionStore::new(codex.path().to_path_buf());

        let first = store
            .list_session_content("session-desc-content", None, Some(20), Some("desc"))
            .expect("first descending page");
        assert_eq!(first.turns.len(), 20);
        assert_eq!(
            first.turns.first().map(|turn| turn.id.as_str()),
            Some("turn-45")
        );
        assert_eq!(
            first.turns.last().map(|turn| turn.id.as_str()),
            Some("turn-26")
        );
        assert!(first.next_cursor.is_some());

        let second = store
            .list_session_content(
                "session-desc-content",
                first.next_cursor,
                Some(20),
                Some("desc"),
            )
            .expect("second descending page");
        assert_eq!(second.turns.len(), 20);
        assert_eq!(
            second.turns.first().map(|turn| turn.id.as_str()),
            Some("turn-25")
        );
        assert_eq!(
            second.turns.last().map(|turn| turn.id.as_str()),
            Some("turn-06")
        );
        assert!(second.next_cursor.is_some());

        let third = store
            .list_session_content(
                "session-desc-content",
                second.next_cursor,
                Some(20),
                Some("desc"),
            )
            .expect("third descending page");
        assert_eq!(third.turns.len(), 5);
        assert_eq!(
            third.turns.first().map(|turn| turn.id.as_str()),
            Some("turn-05")
        );
        assert_eq!(
            third.turns.last().map(|turn| turn.id.as_str()),
            Some("turn-01")
        );
        assert!(third.next_cursor.is_none());
    }

    #[test]
    fn descending_turn_scan_reads_from_the_tail_without_crossing_a_large_prefix() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-desc-large-prefix.jsonl");
        let mut lines = vec![json!({
            "type":"session_meta",
            "payload":{"id":"session-desc-large-prefix","padding":"x".repeat(300_000)}
        })];
        for index in 1..=25 {
            let turn_id = format!("turn-{index:02}");
            let timestamp = format!("2026-08-10T{index:02}:00:00Z");
            lines.push(task_started(&turn_id, &timestamp));
            lines.push(response_message(
                &format!("message-{index:02}"),
                "user",
                &timestamp,
                vec![json!({"type":"input_text","text":format!("问题 {index}")})],
            ));
            lines.push(task_complete(&turn_id, &timestamp));
        }
        write_jsonl(&session_path, &lines);
        let cursor = fs::metadata(&session_path).expect("session metadata").len();

        let scan = find_task_started_offsets_reverse(&session_path, cursor, 21)
            .expect("reverse turn start scan");

        assert_eq!(scan.offsets.len(), 21);
        assert!(!scan.reached_start);
        assert!(scan.offsets.iter().all(|offset| *offset > 300_000));
    }

    #[test]
    fn derives_stable_turn_id_when_legacy_markers_have_no_id() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("legacy-turn-id.jsonl");
        write_jsonl(
            &session_path,
            &[
                json!({"type":"session_meta","payload":{"id":"legacy-turn-id"}}),
                json!({
                    "timestamp":"2026-08-10T01:00:00Z",
                    "type":"event_msg",
                    "payload":{"type":"task_started"}
                }),
                response_message(
                    "legacy-message",
                    "user",
                    "2026-08-10T01:00:01Z",
                    vec![json!({"type":"input_text","text":"旧格式消息"})],
                ),
                json!({
                    "timestamp":"2026-08-10T01:00:02Z",
                    "type":"event_msg",
                    "payload":{"type":"task_complete"}
                }),
            ],
        );
        let store = SessionStore::new(codex.path().to_path_buf());

        let first = store
            .list_session_content("legacy-turn-id", None, Some(20), None)
            .expect("first read");
        let second = store
            .list_session_content("legacy-turn-id", None, Some(20), None)
            .expect("second read");

        assert_eq!(first.turns.len(), 1);
        assert_eq!(first.turns[0].id, second.turns[0].id);
        assert!(first.turns[0].id.starts_with("offset-"));
        assert!(first.turns[0].can_delete);
    }

    #[test]
    fn deletes_complete_turn_scrubs_compaction_and_restores_backup() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-delete-turn.jsonl");
        let deleted_message = json!({
            "type":"message",
            "id":"message-secret",
            "role":"user",
            "content":[{"type":"input_text","text":"需要删除的秘密内容"}]
        });
        let deleted_tool_call = json!({
            "type":"function_call",
            "id":"tool-call-secret",
            "name":"shell",
            "arguments":"{\"command\":\"private command\"}",
            "call_id":"secret-call"
        });
        let deleted_tool_output = json!({
            "type":"function_call_output",
            "id":"tool-output-secret",
            "call_id":"secret-call",
            "output":"private tool output"
        });
        write_jsonl(
            &session_path,
            &[
                json!({"type":"session_meta","payload":{"id":"session-delete-turn"}}),
                task_started("turn-secret", "2026-08-10T01:00:00Z"),
                json!({
                    "timestamp":"2026-08-10T01:00:01Z",
                    "type":"response_item",
                    "payload":deleted_message.clone()
                }),
                json!({
                    "timestamp":"2026-08-10T01:00:01Z",
                    "type":"response_item",
                    "payload":deleted_tool_call.clone()
                }),
                json!({
                    "timestamp":"2026-08-10T01:00:01Z",
                    "type":"response_item",
                    "payload":deleted_tool_output.clone()
                }),
                task_complete("turn-secret", "2026-08-10T01:00:02Z"),
                json!({
                    "timestamp":"2026-08-10T01:30:00Z",
                    "type":"compacted",
                    "payload":{
                        "replacement_history":[
                            deleted_message,
                            deleted_tool_call,
                            deleted_tool_output
                        ],
                        "message":""
                    }
                }),
                task_started("turn-keep", "2026-08-10T02:00:00Z"),
                response_message(
                    "message-keep",
                    "user",
                    "2026-08-10T02:00:01Z",
                    vec![json!({"type":"input_text","text":"需要保留的内容"})],
                ),
                task_complete("turn-keep", "2026-08-10T02:00:02Z"),
            ],
        );
        let original = fs::read_to_string(&session_path).expect("read original");
        let store = SessionStore::new(codex.path().to_path_buf());

        let result = store
            .delete_session_turn("session-delete-turn", "turn-secret")
            .expect("delete turn");
        let deleted = fs::read_to_string(&session_path).expect("read deleted session");
        assert!(!deleted.contains("需要删除的秘密内容"));
        assert!(!deleted.contains("private command"));
        assert!(!deleted.contains("private tool output"));
        assert!(!deleted.contains("tool-call-secret"));
        assert!(!deleted.contains("tool-output-secret"));
        assert!(!deleted.contains("turn-secret"));
        assert!(deleted.contains("需要保留的内容"));
        assert!(result.removed_bytes > 0);
        assert!(Path::new(&result.backup_path).is_file());

        store
            .restore_session_turn_backup("session-delete-turn", &result.backup_id)
            .expect("restore deleted turn");
        assert_eq!(
            fs::read_to_string(&session_path).expect("read restored session"),
            original
        );
    }

    #[test]
    fn deletes_selected_messages_and_duplicate_history_then_restores_backup() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-delete-message.jsonl");
        let deleted_payload = json!({
            "type":"message",
            "id":"message-delete",
            "role":"user",
            "content":[{"type":"input_text","text":"只删除这一条"}]
        });
        let kept_payload = json!({
            "type":"message",
            "id":"message-keep",
            "role":"assistant",
            "content":[{"type":"output_text","text":"这一条必须保留"}]
        });
        write_jsonl(
            &session_path,
            &[
                json!({"type":"session_meta","payload":{"id":"session-delete-message"}}),
                task_started("turn-message", "2026-08-10T01:00:00Z"),
                json!({
                    "timestamp":"2026-08-10T01:00:01Z",
                    "type":"event_msg",
                    "payload":{"type":"user_message","message":"只删除这一条"}
                }),
                json!({
                    "timestamp":"2026-08-10T01:00:01Z",
                    "type":"response_item",
                    "payload":deleted_payload.clone()
                }),
                json!({
                    "timestamp":"2026-08-10T01:00:02Z",
                    "type":"response_item",
                    "payload":kept_payload.clone()
                }),
                task_complete("turn-message", "2026-08-10T01:00:03Z"),
                json!({
                    "timestamp":"2026-08-10T01:30:00Z",
                    "type":"compacted",
                    "payload":{"replacement_history":[deleted_payload, kept_payload],"message":""}
                }),
            ],
        );
        let original = fs::read_to_string(&session_path).expect("read original");
        let store = SessionStore::new(codex.path().to_path_buf());

        let result = store
            .delete_session_messages(
                "session-delete-message",
                "turn-message",
                &["message-delete".to_string()],
            )
            .expect("delete selected message");
        let deleted = fs::read_to_string(&session_path).expect("read deleted session");
        assert!(!deleted.contains("只删除这一条"));
        assert!(!deleted.contains("message-delete"));
        assert!(deleted.contains("这一条必须保留"));
        assert!(deleted.contains("message-keep"));
        assert_eq!(result.deleted_message_ids, vec!["message-delete"]);
        assert!(result.removed_bytes > 0);

        store
            .restore_session_turn_backup("session-delete-message", &result.backup_id)
            .expect("restore message deletion");
        assert_eq!(
            fs::read_to_string(&session_path).expect("read restored session"),
            original
        );
    }

    #[test]
    fn restore_guard_preserves_session_when_it_changes_during_restore() {
        let directory = tempdir().expect("session tempdir");
        let session_path = directory.path().join("session.jsonl");
        let backup_path = directory.path().join("backup.jsonl");
        fs::write(&session_path, "original\n").expect("write session");
        fs::write(&backup_path, "backup\n").expect("write backup");
        let expected = session_file_fingerprint(&session_path).expect("session fingerprint");
        fs::write(&session_path, "updated while restoring\n").expect("update session");

        let error = restore_session_file_if_unchanged(&session_path, &backup_path, expected)
            .expect_err("changed session should not be replaced");

        assert!(error.contains("恢复过程中发生了更新"));
        assert_eq!(
            fs::read_to_string(&session_path).expect("read preserved session"),
            "updated while restoring\n"
        );
    }

    #[test]
    fn refuses_to_delete_turn_while_session_has_an_open_turn() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-active.jsonl");
        write_jsonl(
            &session_path,
            &[
                json!({"type":"session_meta","payload":{"id":"session-active"}}),
                task_started("turn-active", "2026-08-10T01:00:00Z"),
                response_message(
                    "message-active",
                    "user",
                    "2026-08-10T01:00:01Z",
                    vec![json!({"type":"input_text","text":"仍在执行"})],
                ),
            ],
        );
        let original = fs::read_to_string(&session_path).expect("read original");
        let store = SessionStore::new(codex.path().to_path_buf());

        let error = store
            .delete_session_turn("session-active", "turn-active")
            .expect_err("active turn should be protected");

        assert!(error.contains("仍在生成"));
        assert_eq!(
            fs::read_to_string(&session_path).expect("read unchanged session"),
            original
        );
    }

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
    fn moving_to_trash_hides_desktop_indexes_and_restore_reenables_them() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions").join("2026").join("08");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        fs::write(
            codex.path().join("config.toml"),
            "model_provider = \"openai\"\n",
        )
        .expect("write config");
        let session_id = "session-trash-index";
        let session_path = sessions_dir.join("session-trash-index.jsonl");
        let rollout = format!(
            "{}\n{}\n",
            json!({
                "type": "session_meta",
                "payload": {
                    "id": session_id,
                    "cwd": codex.path().to_string_lossy(),
                    "source": "vscode",
                    "timestamp": "2026-08-25T13:38:28Z"
                }
            }),
            json!({
                "type": "response_item",
                "payload": {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "需要回收的会话"}]
                }
            })
        );
        fs::write(&session_path, rollout).expect("write session");
        fs::write(
            codex.path().join("session_index.jsonl"),
            format!(
                "{}\n",
                json!({
                    "id": session_id,
                    "thread_name": "需要回收的会话",
                    "updated_at": "2026-08-25T13:38:28Z"
                })
            ),
        )
        .expect("write session index");

        let state_db = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &state_db,
            r#"
                CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    archived INTEGER NOT NULL DEFAULT 0,
                    archived_at INTEGER,
                    model_provider TEXT,
                    title TEXT,
                    first_user_message TEXT,
                    preview TEXT,
                    has_user_event INTEGER,
                    thread_source TEXT
                );
                INSERT INTO threads (
                    id, archived, model_provider, title, first_user_message,
                    preview, has_user_event, thread_source
                ) VALUES (
                    'session-trash-index', 0, 'openai', '需要回收的会话',
                    '需要回收的会话', '需要回收的会话', 1, 'user'
                );
                "#,
        );
        let catalog_db = codex.path().join("sqlite").join("codex-dev.db");
        fs::create_dir_all(catalog_db.parent().expect("catalog parent")).expect("catalog dir");
        run_sqlite_test(
            &catalog_db,
            &format!(
                r#"
                    CREATE TABLE local_thread_catalog (
                        host_id TEXT NOT NULL,
                        thread_id TEXT NOT NULL,
                        display_title TEXT NOT NULL,
                        source_created_at REAL NOT NULL,
                        source_updated_at REAL NOT NULL,
                        cwd TEXT,
                        source_kind TEXT NOT NULL,
                        model_provider TEXT,
                        observation_sequence INTEGER NOT NULL,
                        missing_candidate INTEGER NOT NULL DEFAULT 0,
                        source_recency_at REAL NOT NULL DEFAULT 0,
                        PRIMARY KEY (host_id, thread_id)
                    );
                    CREATE TABLE local_thread_catalog_metadata (
                        id INTEGER PRIMARY KEY,
                        catalog_revision INTEGER NOT NULL
                    );
                    INSERT INTO local_thread_catalog_metadata (id, catalog_revision) VALUES (1, 0);
                    INSERT INTO local_thread_catalog (
                        host_id, thread_id, display_title, source_created_at,
                        source_updated_at, cwd, source_kind, model_provider,
                        observation_sequence, missing_candidate, source_recency_at
                    ) VALUES (
                        'local', 'session-trash-index', '需要回收的会话', 1, 1,
                        {}, 'vscode', 'openai', 1, 0, 1
                    );
                    "#,
                sql_quote(&codex.path().to_string_lossy())
            ),
        );
        let store = SessionStore::new(codex.path().to_path_buf());
        assert!(store
            .codex_indexes_contain_visible_sessions(&[session_id.to_string()])
            .expect("detect visible Codex indexes"));

        let moved = store
            .move_to_trash(&[session_id.to_string()])
            .expect("move indexed session to trash");
        assert_eq!(moved.moved, 1);
        assert!(moved.failed.is_empty());
        assert_eq!(
            run_sqlite_test_output(
                &state_db,
                "SELECT archived FROM threads WHERE id = 'session-trash-index';"
            )
            .trim(),
            "1"
        );
        assert_eq!(
            run_sqlite_test_output(
                &catalog_db,
                "SELECT missing_candidate FROM local_thread_catalog WHERE host_id = 'local' AND thread_id = 'session-trash-index';"
            )
            .trim(),
            "1"
        );
        assert!(
            !fs::read_to_string(codex.path().join("session_index.jsonl"))
                .expect("read hidden session index")
                .contains(session_id)
        );
        assert!(!store
            .codex_indexes_contain_visible_sessions(&[session_id.to_string()])
            .expect("verify hidden Codex indexes"));

        run_sqlite_test(
            &state_db,
            "UPDATE threads SET archived = 0, archived_at = NULL WHERE id = 'session-trash-index';",
        );
        run_sqlite_test(
            &catalog_db,
            "UPDATE local_thread_catalog SET missing_candidate = 0 WHERE host_id = 'local' AND thread_id = 'session-trash-index';",
        );
        fs::write(
            codex.path().join("session_index.jsonl"),
            format!(
                "{}\n",
                json!({"id": session_id, "thread_name": "遗留可见记录"})
            ),
        )
        .expect("restore legacy visible index residue");
        let legacy_trashed_ids = store
            .list_trashed()
            .expect("list legacy trashed sessions")
            .into_iter()
            .map(|session| session.id)
            .collect::<Vec<_>>();
        assert!(store
            .codex_indexes_contain_visible_sessions(&legacy_trashed_ids)
            .expect("detect legacy visible residue"));
        store
            .hide_sessions_from_codex_indexes(&legacy_trashed_ids)
            .expect("reconcile legacy visible residue");
        assert!(!store
            .codex_indexes_contain_visible_sessions(&legacy_trashed_ids)
            .expect("verify reconciled legacy residue"));

        let restored = store
            .restore_from_trash(&[session_id.to_string()])
            .expect("restore indexed session");
        assert_eq!(restored.restored, 1);
        assert!(restored.failed.is_empty());
        assert_eq!(
            run_sqlite_test_output(
                &state_db,
                "SELECT archived FROM threads WHERE id = 'session-trash-index';"
            )
            .trim(),
            "0"
        );
        assert_eq!(
            run_sqlite_test_output(
                &catalog_db,
                "SELECT missing_candidate FROM local_thread_catalog WHERE host_id = 'local' AND thread_id = 'session-trash-index';"
            )
            .trim(),
            "0"
        );
        assert!(fs::read_to_string(codex.path().join("session_index.jsonl"))
            .expect("read restored session index")
            .contains(session_id));
        assert!(store
            .codex_indexes_contain_visible_sessions(&[session_id.to_string()])
            .expect("verify restored Codex indexes"));

        fs::write(
            store.trash_dir().join(format!("{}.json", session_id)),
            json!({
                "id": session_id,
                "title": "已失效的回收站元数据",
                "originalPath": session_path,
                "trashPath": store.trash_dir().join(format!("{}.jsonl", session_id))
            })
            .to_string(),
        )
        .expect("write stale trash metadata");
        assert!(store
            .list_trashed()
            .expect("ignore stale trash metadata")
            .is_empty());
    }

    #[test]
    fn renames_session_in_official_index_and_uses_custom_title_when_listing() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        fs::write(
            sessions_dir.join("session-rename.jsonl"),
            concat!(
                r#"{"type":"session_meta","payload":{"id":"session-rename"}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"role":"user","content":[{"text":"原始名称"}]}}"#,
                "\n"
            ),
        )
        .expect("write session");
        fs::write(
            codex.path().join("session_index.jsonl"),
            r#"{"id":"session-rename","thread_name":"旧索引名称","updated_at":"2026-08-01T00:00:00Z"}"#,
        )
        .expect("write session index");
        let db_path = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &db_path,
            r#"
                CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, name TEXT);
                INSERT INTO threads (id, title, name)
                VALUES ('session-rename', '旧 SQLite 名称', '旧 SQLite 名称');
                "#,
        );
        let store = SessionStore::new(codex.path().to_path_buf());

        assert_eq!(
            store.list_sessions(None, None).expect("list sessions")[0].title,
            "旧索引名称"
        );

        let result = store
            .rename_session("session-rename", "  新的 会话名称  ")
            .expect("rename session");

        assert_eq!(result.title, "新的 会话名称");
        assert_eq!(
            store.list_sessions(None, None).expect("list renamed")[0].title,
            "新的 会话名称"
        );
        let index = fs::read_to_string(codex.path().join("session_index.jsonl"))
            .expect("read session index");
        assert!(index.contains(r#""thread_name":"新的 会话名称""#));
        assert_eq!(
            run_sqlite_test_output(
                &db_path,
                "SELECT title || '|' || name FROM threads WHERE id = 'session-rename';"
            )
            .trim(),
            "新的 会话名称|新的 会话名称"
        );
    }

    #[test]
    fn updates_session_working_directory_in_rollout_and_sqlite_with_backup() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        let next_cwd = codex.path().join("workspaces").join("operator-end-cloud");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        fs::create_dir_all(&next_cwd).expect("next cwd");
        let session_path = sessions_dir.join("session-cwd.jsonl");
        let original = concat!(
            r#"{"type":"session_meta","payload":{"id":"session-cwd","cwd":"/old/project"}}"#,
            "\n",
            r#"{"type":"turn_context","payload":{"cwd":"/old/project"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"修改目录"}]}}"#,
            "\n"
        );
        fs::write(&session_path, original).expect("write session");
        let db_path = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &db_path,
            r#"
                CREATE TABLE threads (id TEXT PRIMARY KEY, cwd TEXT NOT NULL);
                INSERT INTO threads (id, cwd) VALUES ('session-cwd', '/old/project');
                "#,
        );
        let store = SessionStore::new(codex.path().to_path_buf());

        let result = store
            .update_session_working_directory("session-cwd", &next_cwd.to_string_lossy())
            .expect("update cwd");

        let updated = fs::read_to_string(&session_path).expect("read updated session");
        let lines = updated
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("valid jsonl"))
            .collect::<Vec<_>>();
        assert_eq!(
            lines[0]["payload"]["cwd"].as_str(),
            Some(next_cwd.to_string_lossy().as_ref())
        );
        assert_eq!(lines[1]["payload"]["cwd"].as_str(), Some("/old/project"));
        assert_eq!(
            run_sqlite_test_output(
                &db_path,
                "SELECT cwd FROM threads WHERE id = 'session-cwd';"
            )
            .trim(),
            next_cwd.to_string_lossy()
        );
        let listed = store.list_sessions(None, None).expect("list sessions");
        assert_eq!(listed[0].project_path, next_cwd.to_string_lossy());
        assert_eq!(listed[0].project_name, "operator-end-cloud");
        assert_eq!(
            result.project_path.as_deref(),
            Some(next_cwd.to_string_lossy().as_ref())
        );
        assert_eq!(
            fs::read_to_string(result.backup_path.expect("backup path")).expect("read backup"),
            original
        );
    }

    #[test]
    fn rejects_invalid_session_working_directory_without_mutating_rollout() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-cwd-invalid.jsonl");
        let original = concat!(
            r#"{"type":"session_meta","payload":{"id":"session-cwd-invalid","cwd":"/old/project"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"目录保持不变"}]}}"#,
            "\n"
        );
        fs::write(&session_path, original).expect("write session");
        let store = SessionStore::new(codex.path().to_path_buf());

        let relative_error = store
            .update_session_working_directory("session-cwd-invalid", "relative/project")
            .expect_err("relative cwd should fail");
        assert!(relative_error.contains("绝对路径"));
        let missing_error = store
            .update_session_working_directory(
                "session-cwd-invalid",
                &codex.path().join("missing-project").to_string_lossy(),
            )
            .expect_err("missing cwd should fail");
        assert!(missing_error.contains("不存在"));
        assert_eq!(
            fs::read_to_string(session_path).expect("read unchanged session"),
            original
        );
    }

    #[test]
    fn copies_history_into_a_new_session_without_overwriting_existing_sessions() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        fs::write(
            codex.path().join("config.toml"),
            "model_provider = \"current\"\n",
        )
        .expect("write config");
        let source_path = sessions_dir.join("source.jsonl");
        let target_path = sessions_dir.join("target.jsonl");
        let original_source = concat!(
            r#"{"type":"session_meta","payload":{"id":"source","session_id":"source","model_provider":"old","cwd":"/tmp/project","source":"vscode"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"reasoning","id":"rs_resp_stale_0","summary":[],"content":[],"encrypted_content":"ocx1:source-reasoning"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"compaction","id":"cmp_resp_stale_0","encrypted_content":"ocx1:source-compaction"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","id":"msg_resp_stale_0","role":"user","content":[{"text":"需要复制的历史"}]}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"function_call","id":"fc_call_stale_0","name":"shell","arguments":"{}","call_id":"call_pair"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"function_call_output","id":"fco_call_stale_0","call_id":"call_pair","output":"ok"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"future_response_item","id":"future_resp_stale_0","data":"future"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"历史回复"}]}}"#,
            "\n",
            r#"{"type":"compacted","payload":{"replacement_history":[{"type":"reasoning","id":"rs_resp_stale_0","summary":[],"content":[],"encrypted_content":"ocx1:nested-reasoning"},{"type":"message","id":"msg_resp_stale_0","role":"user","content":[{"text":"需要复制的历史"}]},{"type":"function_call","id":"fc_call_stale_0","name":"shell","arguments":"{}","call_id":"call_pair"},{"type":"function_call_output","id":"fco_call_stale_0","call_id":"call_pair","output":"ok"},{"type":"compaction","id":"cmp_nested_stale_0","encrypted_content":"ocx1:nested-compaction"}],"message":""}}"#,
            "\n"
        );
        fs::write(&source_path, original_source).expect("write source");
        let original_target = concat!(
            r#"{"type":"session_meta","payload":{"id":"target","session_id":"target","model_provider":"current"},"ordinal":0}"#,
            "\n",
            r#"{"type":"response_item","payload":{"role":"user","content":[{"text":"必须保留的现有内容"}]}}"#,
            "\n"
        );
        fs::write(&target_path, original_target).expect("write target");
        let history_db = codex.path().join("thread_history_1.sqlite");
        run_sqlite_test(
            &history_db,
            r#"
                CREATE TABLE thread_items (thread_id TEXT NOT NULL, item_id TEXT NOT NULL);
                CREATE TABLE thread_turns (thread_id TEXT NOT NULL, turn_id TEXT NOT NULL);
                CREATE TABLE thread_history_projection_state (
                    thread_id TEXT PRIMARY KEY,
                    next_rollout_byte_offset INTEGER NOT NULL,
                    next_rollout_ordinal INTEGER NOT NULL
                );
                INSERT INTO thread_items (thread_id, item_id) VALUES ('target', 'old-item');
                INSERT INTO thread_items (thread_id, item_id) VALUES ('unrelated', 'keep-item');
                INSERT INTO thread_turns (thread_id, turn_id) VALUES ('target', 'old-turn');
                INSERT INTO thread_history_projection_state
                    (thread_id, next_rollout_byte_offset, next_rollout_ordinal)
                VALUES ('target', 1234, 9);
            "#,
        );
        let store = SessionStore::new(codex.path().to_path_buf());

        let result = store
            .copy_session_history("source", "副本")
            .expect("copy session history");

        assert_ne!(result.session_id, "source");
        assert_ne!(result.session_id, "target");
        let copied_session = store
            .list_sessions(None, None)
            .expect("list copied sessions")
            .into_iter()
            .find(|session| session.id == result.session_id)
            .expect("new session record");
        let copied = fs::read_to_string(&copied_session.path).expect("read copied session");
        assert!(copied.contains(&format!(r#""id":"{}""#, result.session_id)));
        assert!(copied.contains(r#""model_provider":"current""#));
        assert!(copied.contains("需要复制的历史"));
        assert!(copied.contains("历史回复"));
        assert!(!copied.contains(r#""id":"source""#));
        assert!(!copied.contains("encrypted_content"));
        assert!(!copied.contains("ocx1:"));
        let copied_ordinals = copied
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).expect("valid copied record")
                    ["ordinal"]
                    .as_u64()
                    .expect("copied record ordinal")
            })
            .collect::<Vec<_>>();
        assert_eq!(copied_ordinals, (0..13).collect::<Vec<_>>());
        for stale_id in [
            "rs_resp_stale_0",
            "msg_resp_stale_0",
            "fc_call_stale_0",
            "fco_call_stale_0",
            "future_resp_stale_0",
            "cmp_resp_stale_0",
            "cmp_nested_stale_0",
        ] {
            assert!(!copied.contains(stale_id));
        }
        assert!(!copied.contains("必须保留的现有内容"));
        assert_eq!(
            fs::read_to_string(&source_path).expect("read source"),
            original_source
        );
        assert_eq!(
            fs::read_to_string(&target_path).expect("read target"),
            original_target
        );
        let copied_items = copied
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|value| {
                value.get("type").and_then(serde_json::Value::as_str) == Some("response_item")
            })
            .collect::<Vec<_>>();
        assert_eq!(copied_items.len(), 6);
        for item in &copied_items {
            let local_id = item
                .get("payload")
                .and_then(|payload| payload.get("id"))
                .and_then(serde_json::Value::as_str)
                .expect("copied item local id");
            assert!(!local_id.contains('_'));
            assert_eq!(local_id.len(), 32);
        }
        let call_ids = copied_items
            .iter()
            .filter_map(|item| item.get("payload"))
            .filter_map(|payload| payload.get("call_id"))
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        assert_eq!(call_ids, vec!["call_pair", "call_pair"]);
        let compacted = copied
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|value| {
                value.get("type").and_then(serde_json::Value::as_str) == Some("compacted")
            })
            .expect("copied compacted history");
        let compacted_items = compacted["payload"]["replacement_history"]
            .as_array()
            .expect("replacement history");
        assert_eq!(compacted_items.len(), 4);
        for item in compacted_items {
            let local_id = item["id"].as_str().expect("compacted item local id");
            assert!(!local_id.contains('_'));
            assert_eq!(local_id.len(), 32);
            let item_type = item["type"].as_str().expect("compacted item type");
            let top_level_id = copied_items
                .iter()
                .find(|candidate| candidate["payload"]["type"].as_str() == Some(item_type))
                .and_then(|candidate| candidate["payload"]["id"].as_str())
                .expect("matching top-level response item");
            assert_eq!(local_id, top_level_id);
        }
        let copied_values = copied
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("copied value"))
            .collect::<Vec<_>>();
        let final_compaction_index = copied_values
            .iter()
            .rposition(|value| value["type"].as_str() == Some("compacted"))
            .expect("portable final compaction");
        assert_eq!(
            copied_values[final_compaction_index]["payload"]["replacement_history"]
                .as_array()
                .expect("portable replacement history")
                .len(),
            2
        );
        let visible_items_after_compaction = copied_values[final_compaction_index + 1..]
            .iter()
            .filter(|value| {
                value["type"].as_str() == Some("event_msg")
                    && value["payload"]["type"].as_str() == Some("item_completed")
            })
            .collect::<Vec<_>>();
        assert_eq!(visible_items_after_compaction.len(), 2);
        assert_eq!(
            visible_items_after_compaction[0]["payload"]["item"]["type"].as_str(),
            Some("UserMessage")
        );
        assert_eq!(
            visible_items_after_compaction[1]["payload"]["item"]["type"].as_str(),
            Some("AgentMessage")
        );
        assert_eq!(result.title, "需要复制的历史 副本");
        assert!(result.backup_path.is_none());
        assert_eq!(result.project_path.as_deref(), Some("/tmp/project"));
        assert_eq!(
            store
                .list_sessions(None, None)
                .expect("list sessions")
                .len(),
            3
        );
        assert_eq!(
            run_sqlite_test_output(
                &history_db,
                "SELECT COUNT(*) FROM thread_items WHERE thread_id = 'target';"
            )
            .trim(),
            "1"
        );
        assert_eq!(
            run_sqlite_test_output(
                &history_db,
                "SELECT COUNT(*) FROM thread_turns WHERE thread_id = 'target';"
            )
            .trim(),
            "1"
        );
        assert_eq!(
            run_sqlite_test_output(
                &history_db,
                "SELECT COUNT(*) FROM thread_history_projection_state WHERE thread_id = 'target';"
            )
            .trim(),
            "1"
        );
        assert_eq!(
            run_sqlite_test_output(
                &history_db,
                "SELECT COUNT(*) FROM thread_items WHERE thread_id = 'unrelated';"
            )
            .trim(),
            "1"
        );
    }

    #[test]
    fn copy_history_keeps_the_new_session_when_index_sync_fails() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let source_path = sessions_dir.join("source-rollback.jsonl");
        let original_source = concat!(
            r#"{"type":"session_meta","payload":{"id":"source-rollback","source":"vscode"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"源历史"}]}}"#,
            "\n"
        );
        fs::write(&source_path, original_source).expect("write source");
        fs::write(codex.path().join("state_5.sqlite"), "not a sqlite database")
            .expect("write invalid database");
        let store = SessionStore::new(codex.path().to_path_buf());

        let result = store
            .copy_session_history("source-rollback", "副本")
            .expect("copy remains successful");

        assert!(!result.warnings.is_empty());
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("索引失败") || warning.contains("SQLite")));
        assert_eq!(
            fs::read_to_string(source_path).expect("read unchanged source"),
            original_source
        );
        let sessions = store.list_sessions(None, None).expect("list sessions");
        assert_eq!(sessions.len(), 2);
        assert!(sessions
            .iter()
            .any(|session| session.id == result.session_id));
    }

    #[test]
    fn copy_history_ignores_an_empty_opaque_compaction_snapshot() {
        let source = concat!(
            r#"{"type":"session_meta","payload":{"id":"source"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","id":"msg_source","role":"user","content":[{"text":"保留压缩前的可见历史"}]}}"#,
            "\n",
            r#"{"type":"compacted","payload":{"replacement_history":[{"type":"compaction","id":"cmp_source","encrypted_content":"ocx1:opaque"}],"message":""}}"#,
            "\n"
        );
        let target = concat!(
            r#"{"type":"session_meta","payload":{"id":"target"},"ordinal":0}"#,
            "\n"
        );

        let copied = copy_history_onto_target(source, target).expect("copy portable history");

        assert!(copied.contains("保留压缩前的可见历史"));
        assert_eq!(copied.matches(r#""type":"compacted""#).count(), 1);
        assert!(!copied.contains("encrypted_content"));
        let ordinals = copied
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).expect("valid copied record")
                    ["ordinal"]
                    .as_u64()
                    .expect("copied record ordinal")
            })
            .collect::<Vec<_>>();
        assert_eq!(ordinals, (0..6).collect::<Vec<_>>());
    }

    #[test]
    fn copy_history_materializes_every_visible_message_after_the_final_compaction() {
        let source = concat!(
            r#"{"timestamp":"2026-08-01T00:00:00Z","type":"session_meta","payload":{"id":"source"}}"#,
            "\n",
            r#"{"timestamp":"2026-08-01T00:00:01Z","type":"event_msg","payload":{"type":"task_started","turn_id":"source-turn-1"}}"#,
            "\n",
            r#"{"timestamp":"2026-08-01T00:00:02Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"第一问"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-08-01T00:00:03Z","type":"response_item","payload":{"type":"message","role":"assistant","phase":"final_answer","content":[{"type":"output_text","text":"第一答"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-08-01T00:00:04Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"source-turn-1"}}"#,
            "\n",
            r#"{"timestamp":"2026-08-01T00:00:05Z","type":"compacted","payload":{"replacement_history":[{"type":"message","role":"user","content":[{"type":"input_text","text":"第一问"}]}],"message":"旧压缩"}}"#,
            "\n",
            r#"{"timestamp":"2026-08-01T00:00:06Z","type":"event_msg","payload":{"type":"task_started","turn_id":"source-turn-2"}}"#,
            "\n",
            r#"{"timestamp":"2026-08-01T00:00:07Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"第二问"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-08-01T00:00:08Z","type":"response_item","payload":{"type":"message","role":"assistant","phase":"final_answer","content":[{"type":"output_text","text":"第二答"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-08-01T00:00:09Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"source-turn-2"}}"#,
            "\n"
        );
        let target = concat!(
            r#"{"timestamp":"2026-08-02T00:00:00Z","type":"session_meta","payload":{"id":"target"},"ordinal":0}"#,
            "\n"
        );

        let copied = copy_history_onto_target(source, target).expect("copy history");
        let values = copied
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("copied value"))
            .collect::<Vec<_>>();
        let final_compaction = values
            .iter()
            .rposition(|value| value["type"].as_str() == Some("compacted"))
            .expect("final compaction");
        assert_eq!(
            values[final_compaction]["payload"]["replacement_history"]
                .as_array()
                .expect("replacement history")
                .len(),
            4
        );
        assert!(values[final_compaction + 1..]
            .iter()
            .all(|value| value["type"].as_str() != Some("compacted")));
        let visible_text = values[final_compaction + 1..]
            .iter()
            .filter(|value| {
                value["type"].as_str() == Some("event_msg")
                    && value["payload"]["type"].as_str() == Some("item_completed")
            })
            .filter_map(|value| {
                value["payload"]["item"]["content"][0]["text"]
                    .as_str()
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        assert_eq!(visible_text, vec!["第一问", "第一答", "第二问", "第二答"]);
    }

    #[test]
    fn copy_history_rejects_a_source_with_only_opaque_compaction() {
        let source = concat!(
            r#"{"type":"session_meta","payload":{"id":"source"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"compaction","id":"cmp_source","encrypted_content":"ocx1:opaque"}}"#,
            "\n"
        );
        let target = concat!(
            r#"{"type":"session_meta","payload":{"id":"target"},"ordinal":0}"#,
            "\n"
        );

        let error = copy_history_onto_target(source, target).expect_err("reject opaque history");

        assert!(error.contains("不可跨会话使用的加密历史"));
    }

    #[test]
    fn refuses_to_copy_a_source_with_an_open_turn() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let source_path = sessions_dir.join("source-open-guard.jsonl");
        write_jsonl(
            &source_path,
            &[
                json!({"type":"session_meta","payload":{"id":"source-open-guard"}}),
                task_started("source-active-turn", "2026-08-10T12:00:00Z"),
                response_message(
                    "source-message",
                    "user",
                    "2026-08-10T12:00:00Z",
                    vec![json!({"type":"input_text","text":"仍在生成"})],
                ),
            ],
        );
        let before = fs::read_to_string(&source_path).expect("read source before copy");
        let store = SessionStore::new(codex.path().to_path_buf());

        let error = store
            .copy_session_history("source-open-guard", "副本")
            .expect_err("open source should reject copy");

        assert!(error.contains("源会话仍在生成"));
        assert_eq!(
            fs::read_to_string(&source_path).expect("read unchanged source"),
            before
        );
        assert_eq!(
            store
                .list_sessions(None, None)
                .expect("list sessions")
                .len(),
            1
        );
    }

    #[test]
    fn repeated_copying_always_adds_distinct_sessions() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let source_path = sessions_dir.join("repeat-source.jsonl");
        let source = concat!(
            r#"{"type":"session_meta","payload":{"id":"repeat-source","source":"vscode"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"重复复制测试"}]}}"#,
            "\n"
        );
        fs::write(&source_path, source).expect("write source");
        let store = SessionStore::new(codex.path().to_path_buf());

        let first = store
            .copy_session_history("repeat-source", "副本")
            .expect("first copy");
        let second = store
            .copy_session_history("repeat-source", "副本")
            .expect("second copy");

        assert_ne!(first.session_id, second.session_id);
        assert_ne!(first.session_id, "repeat-source");
        assert_ne!(second.session_id, "repeat-source");
        assert_eq!(
            store
                .list_sessions(None, None)
                .expect("list sessions")
                .len(),
            3
        );
        assert_eq!(
            fs::read_to_string(source_path).expect("read source"),
            source
        );
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
    fn extracts_project_name_and_path_from_session_metadata_cwd() {
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
        assert_eq!(
            sessions[0].project_path,
            "/Users/dalong/Documents/codeDesign/operator-end-cloud"
        );
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
    fn repair_visibility_rebuilds_and_verifies_desktop_catalog() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let primary_session_path = sessions_dir.join("session-catalog.jsonl");
        fs::write(
            &primary_session_path,
            concat!(
                r#"{"timestamp":"2026-07-01T08:30:00Z","type":"session_meta","payload":{"id":"session-catalog","timestamp":"2026-07-01T08:30:00Z","cwd":"/tmp/demo","cli_version":"0.130.0","source":"vscode","model_provider":"openai"}}"#,
                "\n",
                r#"{"timestamp":"2026-07-01T08:31:00Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"恢复桌面列表"}]}}"#,
                "\n"
            ),
        )
        .expect("write session");
        fs::write(
            sessions_dir.join("session-subagent.jsonl"),
            concat!(
                r#"{"timestamp":"2026-07-01T08:32:00Z","type":"session_meta","payload":{"id":"session-subagent","timestamp":"2026-07-01T08:32:00Z","cwd":"/tmp/demo","cli_version":"0.130.0","source":{"subagent":{"other":"guardian"}},"model_provider":"openai"}}"#,
                "\n",
                r#"{"timestamp":"2026-07-01T08:33:00Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"内部检查"}]}}"#,
                "\n"
            ),
        )
        .expect("write subagent session");
        fs::write(
            codex.path().join("config.toml"),
            "model_provider = \"openai\"\n",
        )
        .expect("write config");
        let state_db = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &state_db,
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
                    has_user_event INTEGER NOT NULL DEFAULT 0,
                    first_user_message TEXT NOT NULL DEFAULT '',
                    preview TEXT NOT NULL DEFAULT '',
                    archived INTEGER NOT NULL DEFAULT 0,
                    archived_at INTEGER,
                    thread_source TEXT
                );
            "#,
        );
        run_sqlite_test(
            &state_db,
            &format!(
                "INSERT INTO threads (id, rollout_path, created_at, updated_at, source, model_provider, cwd, title, sandbox_policy, approval_mode, has_user_event, first_user_message, preview, archived, archived_at) VALUES ('session-catalog', {}, 1, 1, 'vscode', 'legacy', '/tmp/demo', '恢复桌面列表', 'danger-full-access', 'never', 0, '', '', 1, 123);",
                sql_quote(&primary_session_path.to_string_lossy())
            ),
        );
        let catalog_db = codex.path().join("sqlite").join("codex-dev.db");
        fs::create_dir_all(catalog_db.parent().expect("catalog parent")).expect("catalog dir");
        run_sqlite_test(
            &catalog_db,
            r#"
                CREATE TABLE local_thread_catalog (
                    host_id TEXT NOT NULL,
                    thread_id TEXT NOT NULL,
                    display_title TEXT NOT NULL,
                    source_created_at REAL NOT NULL,
                    source_updated_at REAL NOT NULL,
                    cwd TEXT NOT NULL,
                    source_kind TEXT NOT NULL,
                    source_detail TEXT,
                    model_provider TEXT NOT NULL,
                    git_branch TEXT,
                    observation_sequence INTEGER NOT NULL,
                    missing_candidate INTEGER NOT NULL DEFAULT 0,
                    thread_source TEXT,
                    source_recency_at REAL NOT NULL DEFAULT 0,
                    pending_observed_title INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (host_id, thread_id)
                );
                CREATE TABLE local_thread_catalog_hosts (
                    host_id TEXT PRIMARY KEY,
                    host_kind TEXT NOT NULL
                );
                CREATE TABLE local_thread_catalog_metadata (
                    id INTEGER PRIMARY KEY,
                    catalog_revision INTEGER NOT NULL DEFAULT 0
                );
                INSERT INTO local_thread_catalog_metadata (id, catalog_revision) VALUES (1, 0);
                CREATE TABLE local_thread_catalog_sync_state (
                    host_id TEXT PRIMARY KEY,
                    observation_sequence INTEGER NOT NULL DEFAULT 0
                );
            "#,
        );
        let store = SessionStore::new(codex.path().to_path_buf());

        let summary = store.repair_visibility().expect("repair");

        let catalog_row = run_sqlite_test_output(
            &catalog_db,
            "SELECT display_title || '|' || source_kind || '|' || model_provider || '|' || missing_candidate FROM local_thread_catalog WHERE host_id = 'local' AND thread_id = 'session-catalog';",
        );
        assert_eq!(catalog_row.trim(), "恢复桌面列表|vscode|openai|0");
        assert_eq!(summary.verified_visible_session_count, 1);
        assert_eq!(summary.skipped_non_sidebar_session_count, 1);
        assert!(summary.desktop_reload_required);
        let state_row = run_sqlite_test_output(
            &state_db,
            "SELECT model_provider || '|' || first_user_message || '|' || preview || '|' || has_user_event || '|' || archived || '|' || (archived_at IS NULL) || '|' || thread_source FROM threads WHERE id = 'session-catalog';",
        );
        assert_eq!(
            state_row.trim(),
            "openai|恢复桌面列表|恢复桌面列表|1|0|1|user"
        );
        let subagent_count = run_sqlite_test_output(
            &catalog_db,
            "SELECT COUNT(*) FROM local_thread_catalog WHERE thread_id = 'session-subagent';",
        );
        assert_eq!(subagent_count.trim(), "0");
    }

    #[test]
    fn account_switch_syncs_persisted_thread_and_catalog_provider_without_rewriting_rollout() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("sessions dir");
        let rollout_path = sessions_dir.join("session-provider.jsonl");
        let rollout = concat!(
            r#"{"type":"session_meta","payload":{"id":"session-provider","model_provider":"openai","cwd":"/tmp/demo","source":"vscode"}}"#,
            "\n"
        );
        fs::write(&rollout_path, rollout).expect("write rollout");

        let state_db = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &state_db,
            r#"
                CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    model_provider TEXT NOT NULL
                );
                INSERT INTO threads (id, model_provider) VALUES ('session-provider', 'openai');
            "#,
        );
        let catalog_db = codex.path().join("sqlite/codex-dev.db");
        fs::create_dir_all(catalog_db.parent().expect("catalog parent")).expect("catalog dir");
        run_sqlite_test(
            &catalog_db,
            r#"
                CREATE TABLE local_thread_catalog (
                    host_id TEXT NOT NULL,
                    thread_id TEXT NOT NULL,
                    model_provider TEXT NOT NULL,
                    PRIMARY KEY (host_id, thread_id)
                );
                INSERT INTO local_thread_catalog (host_id, thread_id, model_provider)
                VALUES ('local', 'session-provider', 'openai');
                INSERT INTO local_thread_catalog (host_id, thread_id, model_provider)
                VALUES ('remote-host', 'remote-session', 'openai');
            "#,
        );
        let store = SessionStore::new(codex.path().to_path_buf());

        let updated = store
            .synchronize_model_provider("cliproxyapi")
            .expect("synchronize provider");

        assert_eq!(updated, 2);
        assert_eq!(
            run_sqlite_test_output(
                &state_db,
                "SELECT model_provider FROM threads WHERE id = 'session-provider';"
            )
            .trim(),
            "cliproxyapi"
        );
        assert_eq!(
            run_sqlite_test_output(
                &catalog_db,
                "SELECT model_provider FROM local_thread_catalog WHERE host_id = 'local' AND thread_id = 'session-provider';"
            )
            .trim(),
            "cliproxyapi"
        );
        assert_eq!(
            run_sqlite_test_output(
                &catalog_db,
                "SELECT model_provider FROM local_thread_catalog WHERE host_id = 'remote-host' AND thread_id = 'remote-session';"
            )
            .trim(),
            "openai"
        );
        assert_eq!(
            fs::read_to_string(rollout_path).expect("read rollout"),
            rollout
        );
    }

    #[test]
    fn model_compatibility_repair_resets_local_thread_choices_and_preserves_history() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("sessions dir");
        let rollout_path = sessions_dir.join("session-model.jsonl");
        let rollout = concat!(
            r#"{"type":"session_meta","payload":{"id":"session-model","model_provider":"zbc","cwd":"/tmp/demo","source":"vscode"}}"#,
            "\n",
            r#"{"type":"turn_context","payload":{"model":"zbc/gpt-5.6-sol","effort":"high","collaboration_mode":{"settings":{"model":"zhangbo-codex/gpt-5.6-sol"}}}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"thread_settings","thread_settings":{"model":"zbc/gpt-5.6-sol","collaboration_mode":{"settings":{"model":"zbc/gpt-5.6-sol"}}}}}"#,
            "\n",
            r#"{"type":"world_state","payload":{"state":{"model":"zbc/gpt-5.6-sol","collaboration_mode":{"model":"zhangbo-codex/gpt-5.6-sol"},"personality":{"model":"zbc/gpt-5.6-sol"}}}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"保留正文中的 zbc/gpt-5.6-sol，不要全局替换"}]}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"reasoning","id":"rs_old-account","summary":[{"type":"summary_text","text":"旧账号推理摘要"}],"encrypted_content":"old-account-ciphertext"}}"#,
            "\n",
            r#"{"type":"compacted","payload":{"message":"保留可读压缩摘要","replacement_history":[{"type":"message","id":"msg_summary","role":"user","content":[{"type":"input_text","text":"保留可读压缩摘要"}]},{"type":"compaction","id":"cmp_old-account","encrypted_content":"old-account-ciphertext"}]}}"#,
            "\n",
            r#"{"type":"turn_context","payload":{"model":"gpt-5.5","effort":"medium"}}"#,
            "\n"
        );
        fs::write(&rollout_path, rollout).expect("write rollout");

        let state_db = codex.path().join("state_5.sqlite");
        run_sqlite_test(
            &state_db,
            r#"
                CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    model_provider TEXT NOT NULL,
                    model TEXT,
                    reasoning_effort TEXT
                );
                INSERT INTO threads (id, model_provider, model, reasoning_effort)
                VALUES ('session-model', 'zbc', 'gpt-5.6-sol', 'high');
                INSERT INTO threads (id, model_provider, model, reasoning_effort)
                VALUES ('session-current-choice', 'openai', 'gpt-5.5', 'medium');
                INSERT INTO threads (id, model_provider, model, reasoning_effort)
                VALUES ('session-current-default', 'openai', NULL, NULL);
            "#,
        );
        let catalog_db = codex.path().join("sqlite/codex-dev.db");
        fs::create_dir_all(catalog_db.parent().expect("catalog parent")).expect("catalog dir");
        run_sqlite_test(
            &catalog_db,
            r#"
                CREATE TABLE local_thread_catalog (
                    host_id TEXT NOT NULL,
                    thread_id TEXT NOT NULL,
                    model_provider TEXT NOT NULL,
                    PRIMARY KEY (host_id, thread_id)
                );
                CREATE TABLE local_thread_catalog_metadata (
                    id INTEGER PRIMARY KEY,
                    catalog_revision INTEGER NOT NULL
                );
                INSERT INTO local_thread_catalog_metadata (id, catalog_revision) VALUES (1, 0);
                INSERT INTO local_thread_catalog (host_id, thread_id, model_provider)
                VALUES ('local', 'session-model', 'zbc');
                INSERT INTO local_thread_catalog (host_id, thread_id, model_provider)
                VALUES ('remote-host', 'remote-session', 'zbc');
            "#,
        );
        let store = SessionStore::new(codex.path().to_path_buf());

        let summary = store
            .repair_model_compatibility("openai")
            .expect("repair model compatibility");

        assert_eq!(summary.target_provider, "openai");
        assert_eq!(summary.repaired_rollout_file_count, 1);
        assert_eq!(summary.rewritten_rollout_model_field_count, 7);
        assert_eq!(summary.synchronized_rollout_provider_count, 1);
        assert_eq!(summary.removed_encrypted_reasoning_item_count, 1);
        assert_eq!(summary.removed_encrypted_compaction_item_count, 1);
        assert_eq!(summary.repaired_thread_count, 2);
        assert_eq!(summary.synchronized_catalog_row_count, 1);
        assert_eq!(summary.repaired_database_count, 2);
        assert!(!summary.backup_dirs.is_empty());
        assert_eq!(
            run_sqlite_test_output(
                &state_db,
                "SELECT model_provider || '|' || (model IS NULL) || '|' || (reasoning_effort IS NULL) FROM threads WHERE id = 'session-model';"
            )
            .trim(),
            "openai|1|1"
        );
        assert_eq!(
            run_sqlite_test_output(
                &state_db,
                "SELECT model_provider || '|' || (model IS NULL) || '|' || (reasoning_effort IS NULL) FROM threads WHERE id = 'session-current-choice';"
            )
            .trim(),
            "openai|1|1"
        );
        assert_eq!(
            run_sqlite_test_output(
                &catalog_db,
                "SELECT model_provider FROM local_thread_catalog WHERE host_id = 'local' AND thread_id = 'session-model';"
            )
            .trim(),
            "openai"
        );
        assert_eq!(
            run_sqlite_test_output(
                &catalog_db,
                "SELECT model_provider FROM local_thread_catalog WHERE host_id = 'remote-host' AND thread_id = 'remote-session';"
            )
            .trim(),
            "zbc"
        );
        assert_eq!(
            run_sqlite_test_output(
                &catalog_db,
                "SELECT catalog_revision FROM local_thread_catalog_metadata WHERE id = 1;"
            )
            .trim(),
            "1"
        );
        let repaired_rollout = fs::read_to_string(&rollout_path).expect("read rollout");
        assert!(repaired_rollout.contains(r#""model_provider":"openai""#));
        assert_eq!(
            repaired_rollout.matches(r#""model":"gpt-5.6-sol""#).count(),
            7
        );
        assert!(repaired_rollout.contains(r#""model":"gpt-5.5""#));
        assert!(repaired_rollout.contains(r#""text":"保留正文中的 zbc/gpt-5.6-sol，不要全局替换""#));
        assert!(repaired_rollout.contains(r#""text":"保留可读压缩摘要""#));
        assert!(!repaired_rollout.contains("rs_old-account"));
        assert!(!repaired_rollout.contains("cmp_old-account"));
        assert!(!repaired_rollout.contains("old-account-ciphertext"));
        assert!(!repaired_rollout.contains(r#""model":"zbc/gpt-5.6-sol""#));
        assert!(!repaired_rollout.contains(r#""model":"zhangbo-codex/gpt-5.6-sol""#));

        let mut rollout_file = fs::OpenOptions::new()
            .append(true)
            .open(&rollout_path)
            .expect("open rollout for API-provider history");
        writeln!(
            rollout_file,
            r#"{{"type":"response_item","payload":{{"type":"reasoning","id":"rs_official-account","summary":[],"encrypted_content":"official-account-ciphertext"}}}}"#
        )
        .expect("append official reasoning item");
        writeln!(
            rollout_file,
            r#"{{"type":"compacted","payload":{{"message":"保留第二次压缩摘要","replacement_history":[{{"type":"message","id":"msg_second_summary","role":"user","content":[{{"type":"input_text","text":"保留第二次压缩摘要"}}]}},{{"type":"compaction","id":"cmp_official-account","encrypted_content":"official-account-ciphertext"}}]}}}}"#
        )
        .expect("append official compaction item");
        drop(rollout_file);

        let api_provider_summary = store
            .repair_model_compatibility("api_service")
            .expect("repair after switching to API provider");
        assert_eq!(api_provider_summary.repaired_rollout_file_count, 1);
        assert_eq!(api_provider_summary.rewritten_rollout_model_field_count, 8);
        assert_eq!(api_provider_summary.synchronized_rollout_provider_count, 1);
        assert_eq!(
            api_provider_summary.removed_encrypted_reasoning_item_count,
            1
        );
        assert_eq!(
            api_provider_summary.removed_encrypted_compaction_item_count,
            1
        );
        assert_eq!(api_provider_summary.repaired_thread_count, 3);
        assert_eq!(api_provider_summary.synchronized_catalog_row_count, 1);

        let api_provider_rollout = fs::read_to_string(&rollout_path).expect("read API rollout");
        assert!(api_provider_rollout.contains(r#""model_provider":"api_service""#));
        assert_eq!(
            api_provider_rollout
                .matches(r#""model":"api_service/gpt-5.6-sol""#)
                .count(),
            7
        );
        assert!(api_provider_rollout.contains(r#""model":"api_service/gpt-5.5""#));
        assert!(api_provider_rollout.contains(r#""text":"保留第二次压缩摘要""#));
        assert!(!api_provider_rollout.contains("rs_official-account"));
        assert!(!api_provider_rollout.contains("cmp_official-account"));
        assert!(!api_provider_rollout.contains("official-account-ciphertext"));

        let mut api_rollout_file = fs::OpenOptions::new()
            .append(true)
            .open(&rollout_path)
            .expect("open rollout for official return history");
        writeln!(
            api_rollout_file,
            r#"{{"type":"response_item","payload":{{"type":"reasoning","id":"rs_api-service-account","summary":[],"encrypted_content":"api-service-account-ciphertext"}}}}"#
        )
        .expect("append API-provider reasoning item");
        writeln!(
            api_rollout_file,
            r#"{{"type":"compacted","payload":{{"message":"保留 API 服务压缩摘要","replacement_history":[{{"type":"message","id":"msg_api_summary","role":"user","content":[{{"type":"input_text","text":"保留 API 服务压缩摘要"}}]}},{{"type":"compaction","id":"cmp_api-service-account","encrypted_content":"api-service-account-ciphertext"}}]}}}}"#
        )
        .expect("append API-provider compaction item");
        drop(api_rollout_file);

        let official_return_summary = store
            .repair_model_compatibility("openai")
            .expect("repair after switching back to official account");
        assert_eq!(official_return_summary.repaired_rollout_file_count, 1);
        assert_eq!(
            official_return_summary.rewritten_rollout_model_field_count,
            8
        );
        assert_eq!(
            official_return_summary.synchronized_rollout_provider_count,
            1
        );
        assert_eq!(
            official_return_summary.removed_encrypted_reasoning_item_count,
            1
        );
        assert_eq!(
            official_return_summary.removed_encrypted_compaction_item_count,
            1
        );
        assert_eq!(official_return_summary.repaired_thread_count, 3);
        assert_eq!(official_return_summary.synchronized_catalog_row_count, 1);

        let official_return_rollout =
            fs::read_to_string(&rollout_path).expect("read returned official rollout");
        assert!(official_return_rollout.contains(r#""model_provider":"openai""#));
        assert_eq!(
            official_return_rollout
                .matches(r#""model":"gpt-5.6-sol""#)
                .count(),
            7
        );
        assert!(official_return_rollout.contains(r#""model":"gpt-5.5""#));
        assert!(official_return_rollout.contains(r#""text":"保留 API 服务压缩摘要""#));
        assert!(!official_return_rollout.contains(r#""model":"api_service/gpt-5.6-sol""#));
        assert!(!official_return_rollout.contains("rs_api-service-account"));
        assert!(!official_return_rollout.contains("cmp_api-service-account"));
        assert!(!official_return_rollout.contains("api-service-account-ciphertext"));

        let no_op_summary = store
            .repair_model_compatibility("openai")
            .expect("repeat compatibility repair");
        assert_eq!(no_op_summary.repaired_rollout_file_count, 0);
        assert_eq!(no_op_summary.repaired_thread_count, 0);
        assert_eq!(no_op_summary.synchronized_catalog_row_count, 0);
        assert_eq!(no_op_summary.removed_encrypted_reasoning_item_count, 0);
        assert_eq!(no_op_summary.removed_encrypted_compaction_item_count, 0);
        assert!(no_op_summary.backup_dirs.is_empty());
    }

    #[test]
    fn repair_visibility_rebuilds_local_projects_and_thread_assignments() {
        let codex = tempdir().expect("codex tempdir");
        let alpha = codex.path().join("alpha");
        let beta = codex.path().join("beta");
        fs::create_dir_all(&alpha).expect("alpha project");
        fs::create_dir_all(&beta).expect("beta project");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("sessions dir");
        let alpha_session = sessions_dir.join("alpha.jsonl");
        let beta_session = sessions_dir.join("beta.jsonl");
        let subagent_session = sessions_dir.join("subagent.jsonl");
        write_jsonl(
            &alpha_session,
            &[json!({
                "type": "session_meta",
                "payload": {
                    "id": "session-alpha",
                    "cwd": alpha,
                    "source": "vscode",
                    "model_provider": "openai"
                }
            })],
        );
        write_jsonl(
            &beta_session,
            &[json!({
                "type": "session_meta",
                "payload": {
                    "id": "session-beta",
                    "cwd": beta,
                    "source": "vscode",
                    "model_provider": "openai"
                }
            })],
        );
        write_jsonl(
            &subagent_session,
            &[json!({
                "type": "session_meta",
                "payload": {
                    "id": "session-subagent",
                    "cwd": beta,
                    "source": { "subagent": { "other": "guardian" } },
                    "model_provider": "openai"
                }
            })],
        );
        let existing_project_id = "existing-alpha";
        fs::write(
            codex.path().join(".codex-global-state.json"),
            serde_json::to_vec(&json!({
                "unrelated-setting": true,
                "local-projects": {
                    existing_project_id: {
                        "id": existing_project_id,
                        "name": "Alpha custom name",
                        "rootPaths": [alpha],
                        "createdAt": 10,
                        "updatedAt": 10
                    }
                },
                "project-order": [existing_project_id],
                "thread-project-assignments": {
                    "unrelated-thread": {
                        "projectKind": "local",
                        "projectId": existing_project_id
                    }
                },
                "projectless-thread-ids": ["session-alpha", "session-beta", "keep-projectless"]
            }))
            .expect("serialize global state"),
        )
        .expect("write global state");
        let records = vec![
            SessionRepairRecord {
                id: "session-alpha".to_string(),
                title: "Alpha task".to_string(),
                path: alpha_session,
                updated_at: 10,
            },
            SessionRepairRecord {
                id: "session-beta".to_string(),
                title: "Beta task".to_string(),
                path: beta_session,
                updated_at: 20,
            },
            SessionRepairRecord {
                id: "session-subagent".to_string(),
                title: "Internal".to_string(),
                path: subagent_session,
                updated_at: 30,
            },
        ];
        let store = SessionStore::new(codex.path().to_path_buf());

        let result = store
            .repair_desktop_projects(&records)
            .expect("repair desktop projects");

        assert_eq!(result.created, 1);
        assert_eq!(result.verified_projects, 2);
        assert_eq!(result.verified_assignments, 2);
        assert_eq!(result.skipped, 1);
        let state: Value = serde_json::from_slice(
            &fs::read(codex.path().join(".codex-global-state.json"))
                .expect("read repaired global state"),
        )
        .expect("parse repaired global state");
        assert_eq!(state["unrelated-setting"], json!(true));
        assert_eq!(
            state["local-projects"]
                .as_object()
                .expect("local projects")
                .len(),
            2
        );
        assert_eq!(
            state["thread-project-assignments"]["session-alpha"]["projectId"],
            existing_project_id
        );
        let beta_project_id = state["thread-project-assignments"]["session-beta"]["projectId"]
            .as_str()
            .expect("beta project id");
        assert_eq!(
            state["local-projects"][beta_project_id]["rootPaths"][0],
            beta.to_string_lossy().as_ref()
        );
        assert!(state["thread-project-assignments"]
            .get("session-subagent")
            .is_none());
        assert_eq!(state["projectless-thread-ids"], json!(["keep-projectless"]));
        assert!(codex.path().join(".codex-global-state.json.bak").is_file());
    }

    #[test]
    fn repair_visibility_recreates_missing_generated_images_from_rollout() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("sessions dir");
        let session_path = sessions_dir.join("generated-images.jsonl");
        let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
        write_jsonl(
            &session_path,
            &[
                json!({
                    "type": "session_meta",
                    "payload": {
                        "id": "session-images",
                        "cwd": codex.path(),
                        "source": "vscode",
                        "model_provider": "openai"
                    }
                }),
                json!({
                    "type": "event_msg",
                    "payload": {
                        "type": "image_generation_end",
                        "call_id": "exec-11111111-2222-4333-8444-555555555555",
                        "status": "completed",
                        "result": png_base64
                    }
                }),
                json!({
                    "type": "event_msg",
                    "payload": {
                        "type": "image_generation_end",
                        "call_id": "../unsafe",
                        "status": "completed",
                        "result": png_base64
                    }
                }),
            ],
        );
        let records = vec![SessionRepairRecord {
            id: "session-images".to_string(),
            title: "Generated images".to_string(),
            path: session_path,
            updated_at: 10,
        }];
        let store = SessionStore::new(codex.path().to_path_buf());

        let result = store
            .repair_generated_images(&records)
            .expect("repair generated images");

        assert_eq!(result.recreated, 1);
        assert_eq!(result.verified, 1);
        assert_eq!(result.invalid, 1);
        let image_path = codex
            .path()
            .join("generated_images/session-images/exec-11111111-2222-4333-8444-555555555555.png");
        assert_eq!(
            &fs::read(image_path).expect("read recreated image")[..8],
            b"\x89PNG\r\n\x1a\n"
        );
        assert!(!codex.path().join("unsafe.png").exists());

        let unsafe_records = vec![SessionRepairRecord {
            id: "../unsafe-session".to_string(),
            title: "Unsafe generated images".to_string(),
            path: records[0].path.clone(),
            updated_at: 10,
        }];
        let unsafe_result = store
            .repair_generated_images(&unsafe_records)
            .expect("reject unsafe session id");
        assert_eq!(unsafe_result.recreated, 0);
        assert_eq!(unsafe_result.verified, 0);
        assert_eq!(unsafe_result.invalid, 1);
    }

    #[test]
    fn deep_repair_persists_inline_user_images_and_rewrites_local_image_paths() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("sessions dir");
        let session_path = sessions_dir.join("local-images.jsonl");
        let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
        write_jsonl(
            &session_path,
            &[
                json!({
                    "type": "session_meta",
                    "payload": {"id": "session-images", "model_provider": "openai"}
                }),
                json!({
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [
                            {"type": "input_text", "text": "look at this"},
                            {"type": "input_image", "image_url": format!("data:image/png;base64,{png_base64}")}
                        ]
                    }
                }),
                json!({
                    "type": "event_msg",
                    "payload": {
                        "type": "item_completed",
                        "item": {
                            "type": "UserMessage",
                            "id": "user-message-1",
                            "content": [
                                {"type": "text", "text": "look at this"},
                                {"type": "local_image", "path": "/var/folders/demo/codex-clipboard-missing.png"}
                            ]
                        }
                    }
                }),
            ],
        );
        let records = vec![SessionRepairRecord {
            id: "session-images".to_string(),
            title: "Local images".to_string(),
            path: session_path.clone(),
            updated_at: 10,
        }];
        let store = SessionStore::new(codex.path().to_path_buf());

        let result = store
            .repair_local_image_attachments(&records)
            .expect("repair local images");

        assert_eq!(result.changed_rollout_files, 1);
        assert_eq!(
            result.changed_session_ids,
            HashSet::from(["session-images".to_string()])
        );
        assert_eq!(result.images.recreated, 1);
        assert_eq!(result.images.verified, 1);
        assert_eq!(result.images.invalid, 0);
        assert_eq!(result.backup_dirs.len(), 1);

        let repaired = fs::read_to_string(&session_path).expect("read repaired rollout");
        assert!(repaired.contains(&format!("data:image/png;base64,{png_base64}")));
        let local_path = repaired
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find_map(|value| {
                value
                    .pointer("/payload/item/content/1/path")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .expect("rewritten local image path");
        assert!(local_path.starts_with(
            codex
                .path()
                .join("generated_images/session-images")
                .to_string_lossy()
                .as_ref()
        ));
        assert_eq!(
            &fs::read(&local_path).expect("read recovered local image")[..8],
            b"\x89PNG\r\n\x1a\n"
        );

        let second = store
            .repair_local_image_attachments(&records)
            .expect("verify repaired local images");
        assert_eq!(second.changed_rollout_files, 0);
        assert_eq!(second.images.recreated, 0);
        assert_eq!(second.images.verified, 1);
        assert!(second.backup_dirs.is_empty());
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

    #[test]
    fn repair_visibility_fills_missing_paginated_rollout_ordinals() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-paginated.jsonl");
        fs::write(
            &session_path,
            concat!(
                r#"{"type":"session_meta","payload":{"id":"session-paginated","model_provider":"openai","cwd":"/tmp/demo"},"ordinal":0}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"missing ordinal"}]}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"task_complete"},"ordinal":99}"#,
                "\n"
            ),
        )
        .expect("write paginated session");
        let records = vec![SessionRepairRecord {
            id: "session-paginated".to_string(),
            title: "Paginated".to_string(),
            path: session_path.clone(),
            updated_at: 10,
        }];
        let store = SessionStore::new(codex.path().to_path_buf());

        let (changed, _) = store
            .repair_rollout_visibility("openai", &records, false)
            .expect("repair paginated rollout");

        assert_eq!(changed, 1);
        let ordinals = fs::read_to_string(session_path)
            .expect("read repaired rollout")
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).expect("valid repaired record")
                    ["ordinal"]
                    .as_u64()
                    .expect("repaired record ordinal")
            })
            .collect::<Vec<_>>();
        assert_eq!(ordinals, vec![0, 1, 2]);
    }

    #[test]
    fn deep_repair_resets_only_stale_paginated_history_projection() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-stale-projection.jsonl");
        fs::write(
            &session_path,
            concat!(
                r#"{"type":"session_meta","payload":{"id":"target"},"ordinal":0}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1"},"ordinal":1}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1"},"ordinal":2}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"turn-2"},"ordinal":3}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-2"},"ordinal":4}"#,
                "\n"
            ),
        )
        .expect("write paginated session");
        let history_db = codex.path().join("thread_history_1.sqlite");
        run_sqlite_test(
            &history_db,
            r#"
                CREATE TABLE thread_items (thread_id TEXT NOT NULL, item_id TEXT NOT NULL);
                CREATE TABLE thread_turns (thread_id TEXT NOT NULL, turn_id TEXT NOT NULL);
                CREATE TABLE thread_history_projection_state (
                    thread_id TEXT PRIMARY KEY,
                    next_rollout_byte_offset INTEGER NOT NULL,
                    next_rollout_ordinal INTEGER NOT NULL
                );
                INSERT INTO thread_items VALUES ('target', 'old-item'), ('unrelated', 'keep-item');
                INSERT INTO thread_turns VALUES ('target', 'old-turn'), ('unrelated', 'keep-turn');
                INSERT INTO thread_history_projection_state VALUES
                    ('target', 120, 3),
                    ('unrelated', 500, 9);
            "#,
        );
        let records = vec![SessionRepairRecord {
            id: "target".to_string(),
            title: "Target".to_string(),
            path: session_path,
            updated_at: 10,
        }];
        let store = SessionStore::new(codex.path().to_path_buf());

        let (reset, backup_dirs) = store
            .reset_stale_thread_history_projections(&records, &HashSet::new())
            .expect("reset stale history projection");

        assert_eq!(reset, 1);
        assert_eq!(backup_dirs.len(), 1);
        for table in [
            "thread_items",
            "thread_turns",
            "thread_history_projection_state",
        ] {
            assert_eq!(
                run_sqlite_test_output(
                    &history_db,
                    &format!("SELECT COUNT(*) FROM {table} WHERE thread_id = 'target';")
                )
                .trim(),
                "0"
            );
            assert_eq!(
                run_sqlite_test_output(
                    &history_db,
                    &format!("SELECT COUNT(*) FROM {table} WHERE thread_id = 'unrelated';")
                )
                .trim(),
                "1"
            );
        }
    }

    #[test]
    fn deep_repair_preserves_complete_paginated_history_projection() {
        let codex = tempdir().expect("codex tempdir");
        let sessions_dir = codex.path().join("sessions");
        fs::create_dir_all(&sessions_dir).expect("session dir");
        let session_path = sessions_dir.join("session-complete-projection.jsonl");
        fs::write(
            &session_path,
            concat!(
                r#"{"type":"session_meta","payload":{"id":"target"},"ordinal":0}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1"},"ordinal":1}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1"},"ordinal":2}"#,
                "\n"
            ),
        )
        .expect("write paginated session");
        let file_size = fs::metadata(&session_path).expect("session metadata").len();
        let history_db = codex.path().join("thread_history_1.sqlite");
        run_sqlite_test(
            &history_db,
            &format!(
                r#"
                    CREATE TABLE thread_items (thread_id TEXT NOT NULL, item_id TEXT NOT NULL);
                    CREATE TABLE thread_turns (thread_id TEXT NOT NULL, turn_id TEXT NOT NULL);
                    CREATE TABLE thread_history_projection_state (
                        thread_id TEXT PRIMARY KEY,
                        next_rollout_byte_offset INTEGER NOT NULL,
                        next_rollout_ordinal INTEGER NOT NULL
                    );
                    INSERT INTO thread_items VALUES ('target', 'item');
                    INSERT INTO thread_turns VALUES ('target', 'turn-1');
                    INSERT INTO thread_history_projection_state VALUES ('target', {file_size}, 3);
                "#
            ),
        );
        let records = vec![SessionRepairRecord {
            id: "target".to_string(),
            title: "Target".to_string(),
            path: session_path,
            updated_at: 10,
        }];
        let store = SessionStore::new(codex.path().to_path_buf());

        let (reset, backup_dirs) = store
            .reset_stale_thread_history_projections(&records, &HashSet::new())
            .expect("preserve complete history projection");

        assert_eq!(reset, 0);
        assert!(backup_dirs.is_empty());
        assert_eq!(
            run_sqlite_test_output(
                &history_db,
                "SELECT COUNT(*) FROM thread_turns WHERE thread_id = 'target';"
            )
            .trim(),
            "1"
        );

        let (forced_reset, forced_backup_dirs) = store
            .reset_stale_thread_history_projections(
                &records,
                &HashSet::from(["target".to_string()]),
            )
            .expect("force reset changed image projection");
        assert_eq!(forced_reset, 1);
        assert_eq!(forced_backup_dirs.len(), 1);
        assert_eq!(
            run_sqlite_test_output(
                &history_db,
                "SELECT COUNT(*) FROM thread_turns WHERE thread_id = 'target';"
            )
            .trim(),
            "0"
        );
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
