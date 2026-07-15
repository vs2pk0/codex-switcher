use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, TimeZone,
    Timelike,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageDashboard {
    pub summary: CodexUsageSummary,
    pub trends: Vec<CodexUsageTrendPoint>,
    pub logs: Vec<CodexUsageLog>,
    pub total_logs: usize,
    pub provider_stats: Vec<CodexUsageProviderStat>,
    pub model_stats: Vec<CodexUsageModelStat>,
    pub files_scanned: usize,
    pub errors: Vec<String>,
    pub cache_path: String,
    pub pricing_configs: Vec<CodexUsagePricingConfig>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageSummary {
    pub total_requests: usize,
    pub total_cost: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub real_total_tokens: u64,
    pub cache_hit_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageActivity {
    pub summary: CodexUsageActivitySummary,
    pub days: Vec<CodexUsageActivityDay>,
    pub hours: Vec<CodexUsageActivityHour>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageActivitySummary {
    pub total_tokens: u64,
    pub peak_day_tokens: u64,
    pub longest_task_seconds: i64,
    pub current_streak_days: usize,
    pub longest_streak_days: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageActivityDay {
    pub date: String,
    pub timestamp: i64,
    pub tokens: u64,
    pub requests: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageActivityHour {
    pub hour: u32,
    pub label: String,
    pub timestamp: i64,
    pub tokens: u64,
    pub requests: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageTrendPoint {
    pub timestamp: i64,
    pub label: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub total_cost: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageLog {
    pub request_id: String,
    pub provider_name: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub total_cost: String,
    pub status_code: u16,
    pub created_at: i64,
    pub data_source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageModelStat {
    pub model: String,
    pub request_count: usize,
    pub total_tokens: u64,
    pub total_cost: String,
    pub avg_cost_per_request: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageProviderStat {
    pub provider_id: String,
    pub provider_name: String,
    pub request_count: usize,
    pub total_tokens: u64,
    pub total_cost: String,
    pub success_rate: f64,
    pub avg_latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsagePricing {
    pub model_id: String,
    pub display_name: String,
    pub input_cost_per_million: String,
    pub output_cost_per_million: String,
    pub cache_read_cost_per_million: String,
    pub cache_creation_cost_per_million: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsagePricingConfig {
    pub app: String,
    pub multiplier: String,
    pub pricing_model_source: String,
}

#[derive(Debug, Clone, Default)]
struct CumulativeTokens {
    input: u64,
    cached_input: u64,
    output: u64,
}

#[derive(Debug, Clone, Default)]
struct DeltaTokens {
    input: u64,
    cached_input: u64,
    output: u64,
}

impl DeltaTokens {
    fn is_zero(&self) -> bool {
        self.input == 0 && self.cached_input == 0 && self.output == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParsedUsageLog {
    request_id: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    rollout_id: String,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    created_at: i64,
    #[serde(default)]
    source_kind: UsageLogSourceKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum UsageLogSourceKind {
    #[default]
    Legacy,
    Unknown,
    Fork,
    CanonicalRoot,
}

impl UsageLogSourceKind {
    fn as_db_value(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Unknown => "unknown",
            Self::Fork => "fork",
            Self::CanonicalRoot => "canonical_root",
        }
    }

    fn from_db_value(value: &str) -> Self {
        match value {
            "unknown" => Self::Unknown,
            "fork" => Self::Fork,
            "canonical_root" => Self::CanonicalRoot,
            _ => Self::Legacy,
        }
    }

    fn priority(self) -> u8 {
        match self {
            Self::Legacy => 0,
            Self::Unknown => 1,
            Self::Fork => 2,
            Self::CanonicalRoot => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageCache {
    version: u32,
    updated_at: i64,
    files: Vec<UsageCacheFile>,
    logs: Vec<ParsedUsageLog>,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct UsageCacheFile {
    path: String,
    modified_ms: u128,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
struct FileParseState {
    has_session_meta: bool,
    session_id: Option<String>,
    rollout_id: Option<String>,
    parent_rollout_id: Option<String>,
    is_fork: bool,
    is_canonical_root: bool,
    session_started_at: Option<i64>,
    last_valid_event_at: Option<i64>,
    owned_task_started: bool,
    current_model: String,
    prev_total: Option<CumulativeTokens>,
    event_index: u32,
}

#[derive(Debug, Clone)]
struct ParsedUsageFile {
    source_path: String,
    rollout_id: String,
    parent_rollout_id: Option<String>,
    events: Vec<ParsedUsageEvent>,
    is_fork: bool,
}

#[derive(Debug, Clone)]
struct ParsedUsageEvent {
    fingerprint: String,
    log: Option<ParsedUsageLog>,
    after_owned_task_start: bool,
}

const USAGE_CACHE_VERSION: u32 = 4;
const PRICING_DEFAULTS_VERSION: u32 = 1;
const GPT_56_DEFAULT_MODEL_IDS: [&str; 3] = ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"];

#[derive(Debug, Clone, Copy)]
struct ModelPricing {
    input_per_million: f64,
    output_per_million: f64,
    cache_read_per_million: f64,
    cache_creation_per_million: f64,
}

pub fn get_codex_usage_dashboard(
    start_date: Option<i64>,
    end_date: Option<i64>,
    page: Option<usize>,
    page_size: Option<usize>,
    refresh: Option<bool>,
) -> Result<CodexUsageDashboard, String> {
    let (conn, cache) = ensure_usage_cache_db(refresh.unwrap_or(false))?;
    let pricing = load_pricing()?;
    let pricing_configs = load_pricing_configs()?;
    let cost_multiplier = pricing_config_multiplier(&pricing_configs);
    let errors = cache.errors;

    let start = start_date.unwrap_or(i64::MIN);
    let end = end_date.unwrap_or(i64::MAX);
    let mut logs = read_usage_logs_db_range(&conn, start, end)?;
    logs.sort_by_key(|log| std::cmp::Reverse(log.created_at));

    let total_logs = logs.len();
    let safe_page_size = page_size.unwrap_or(20).clamp(1, 200);
    let safe_page = page.unwrap_or(1).max(1);
    let start_index = (safe_page - 1).saturating_mul(safe_page_size);
    let page_logs = logs
        .iter()
        .skip(start_index)
        .take(safe_page_size)
        .map(|log| to_usage_log(log, &pricing, cost_multiplier))
        .collect();

    Ok(CodexUsageDashboard {
        summary: summarize_logs(&logs, &pricing, cost_multiplier),
        trends: build_trends(&logs, start_date, end_date, &pricing, cost_multiplier),
        provider_stats: build_provider_stats(&logs, &pricing, cost_multiplier),
        model_stats: build_model_stats(&logs, &pricing, cost_multiplier),
        logs: page_logs,
        total_logs,
        files_scanned: cache.files.len(),
        errors,
        cache_path: usage_db_path().to_string_lossy().to_string(),
        pricing_configs,
    })
}

pub fn get_codex_usage_activity(refresh: Option<bool>) -> Result<CodexUsageActivity, String> {
    let (conn, _) = ensure_usage_cache_db(refresh.unwrap_or(false))?;
    let start = local_day_start_timestamp(usage_activity_start_date(Local::now().date_naive()));
    let logs = read_usage_logs_db_range(&conn, start, i64::MAX)?;
    Ok(build_usage_activity(&logs))
}

pub fn list_model_pricing() -> Result<Vec<CodexUsagePricing>, String> {
    load_pricing()
}

pub fn update_model_pricing(input: CodexUsagePricing) -> Result<(), String> {
    validate_pricing(&input)?;
    let mut pricing = load_pricing()?;
    let model_id = clean_model_id_for_pricing(&input.model_id);
    let mut next = input;
    next.model_id = model_id.clone();
    next.display_name = next.display_name.trim().to_string();
    if let Some(existing) = pricing.iter_mut().find(|item| item.model_id == model_id) {
        *existing = next;
    } else {
        pricing.push(next);
    }
    write_pricing(&pricing)
}

pub fn delete_model_pricing(model_id: &str) -> Result<(), String> {
    let model_id = clean_model_id_for_pricing(model_id);
    let mut pricing = load_pricing()?;
    pricing.retain(|item| item.model_id != model_id);
    write_pricing(&pricing)
}

pub fn reset_model_pricing() -> Result<Vec<CodexUsagePricing>, String> {
    let pricing = default_pricing();
    write_pricing(&pricing)?;
    Ok(pricing)
}

pub fn get_pricing_config() -> Result<Vec<CodexUsagePricingConfig>, String> {
    load_pricing_configs()
}

pub fn update_pricing_config(
    configs: Vec<CodexUsagePricingConfig>,
) -> Result<Vec<CodexUsagePricingConfig>, String> {
    let next = normalize_pricing_configs(configs)?;
    Ok(next)
}

fn parse_codex_usage_file(path: &Path) -> Result<ParsedUsageFile, String> {
    let file = fs::File::open(path).map_err(|error| format!("打开会话文件失败: {}", error))?;
    let reader = BufReader::new(file);
    let filename_timestamp = rollout_filename_timestamp(path);
    let path_rollout_id = rollout_identity_from_path(path);
    let mut state = FileParseState {
        has_session_meta: false,
        session_id: None,
        rollout_id: None,
        parent_rollout_id: None,
        is_fork: false,
        is_canonical_root: false,
        session_started_at: None,
        last_valid_event_at: None,
        owned_task_started: false,
        current_model: "unknown".to_string(),
        prev_total: None,
        event_index: 0,
    };
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }
        let is_event_msg = line.contains("\"event_msg\"");
        let is_turn_context = line.contains("\"turn_context\"");
        let is_session_meta = line.contains("\"session_meta\"");
        if !is_event_msg && !is_turn_context && !is_session_meta {
            continue;
        }
        if is_event_msg && !line.contains("\"token_count\"") && !line.contains("\"task_started\"") {
            continue;
        }

        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") if state.session_id.is_none() => {
                let payload = value.get("payload");
                state.session_started_at = parse_event_timestamp(&value).or_else(|| {
                    payload
                        .and_then(|payload| payload.get("timestamp"))
                        .and_then(Value::as_str)
                        .and_then(parse_rfc3339_timestamp)
                });
                let raw_session_id = payload
                    .and_then(|payload| {
                        payload
                            .get("session_id")
                            .or_else(|| payload.get("sessionId"))
                    })
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                state.rollout_id = payload
                    .and_then(|payload| payload.get("id"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                state.parent_rollout_id = payload
                    .and_then(|payload| payload.get("forked_from_id"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let has_fork_parent = state.parent_rollout_id.is_some();
                let has_distinct_rollout_id = match (&raw_session_id, &state.rollout_id) {
                    (Some(session_id), Some(rollout_id)) => session_id != rollout_id,
                    _ => false,
                };
                state.is_fork = has_fork_parent || has_distinct_rollout_id;
                let effective_session_id = raw_session_id.or_else(|| state.rollout_id.clone());
                state.has_session_meta = effective_session_id.is_some();
                state.is_canonical_root = !has_fork_parent
                    && matches!(
                        (&effective_session_id, &state.rollout_id),
                        (Some(session_id), Some(rollout_id)) if session_id == rollout_id
                    );
                state.session_id = effective_session_id;
            }
            Some("turn_context") => {
                if let Some(model) = value
                    .get("payload")
                    .and_then(|payload| {
                        payload
                            .get("model")
                            .or_else(|| payload.get("info").and_then(|info| info.get("model")))
                    })
                    .and_then(Value::as_str)
                {
                    state.current_model = normalize_codex_model(model);
                }
            }
            Some("event_msg") => {
                let Some(payload) = value.get("payload") else {
                    continue;
                };
                if payload.get("type").and_then(Value::as_str) == Some("task_started") {
                    if state.is_fork
                        && !state.owned_task_started
                        && task_started_at(payload)
                            .zip(state.session_started_at)
                            .is_some_and(|(task_at, session_at)| {
                                (session_at..=session_at.saturating_add(1)).contains(&task_at)
                            })
                    {
                        state.owned_task_started = true;
                    }
                    continue;
                }
                if payload.get("type").and_then(Value::as_str) != Some("token_count") {
                    continue;
                }
                let Some(info) = payload.get("info").filter(|info| !info.is_null()) else {
                    continue;
                };
                if let Some(model) = info
                    .get("model")
                    .or_else(|| info.get("model_name"))
                    .or_else(|| payload.get("model"))
                    .and_then(Value::as_str)
                {
                    state.current_model = normalize_codex_model(model);
                }
                let total = info
                    .get("total_token_usage")
                    .and_then(parse_cumulative_tokens);
                let last = info
                    .get("last_token_usage")
                    .and_then(parse_cumulative_tokens);
                let event_timestamp = parse_event_timestamp(&value);
                if let Some(timestamp) = event_timestamp {
                    state.last_valid_event_at = Some(timestamp);
                }
                let fingerprint = token_usage_fingerprint(total.as_ref(), last.as_ref());
                let delta = if let Some(total) = total {
                    // Fork/replay rollouts start with the parent's cumulative total.  Only
                    // those rollouts use last_token_usage for their first event; a canonical
                    // root still owns its complete initial cumulative total.
                    let delta = match state.prev_total.as_ref() {
                        Some(_) => compute_delta(&state.prev_total, &total),
                        None if state.is_fork => {
                            compute_delta(&None, last.as_ref().unwrap_or(&total))
                        }
                        None => compute_delta(&None, &total),
                    };
                    state.prev_total = Some(total);
                    delta
                } else if let Some(last) = last {
                    compute_delta(&None, &last)
                } else {
                    continue;
                };
                let delta = DeltaTokens {
                    cached_input: delta.cached_input.min(delta.input),
                    ..delta
                };
                state.event_index += 1;
                let session_id = state.session_id.as_deref().unwrap_or("unknown");
                let rollout_id = state
                    .rollout_id
                    .as_deref()
                    .or(path_rollout_id.as_deref())
                    .unwrap_or(session_id);
                let source_kind = if state.is_canonical_root {
                    UsageLogSourceKind::CanonicalRoot
                } else if state.is_fork {
                    UsageLogSourceKind::Fork
                } else {
                    UsageLogSourceKind::Unknown
                };
                let log = (!delta.is_zero()).then(|| ParsedUsageLog {
                    request_id: format!("codex_rollout:{}:{}", rollout_id, state.event_index),
                    session_id: session_id.to_string(),
                    rollout_id: rollout_id.to_string(),
                    model: state.current_model.clone(),
                    input_tokens: delta.input,
                    output_tokens: delta.output,
                    cache_read_tokens: delta.cached_input,
                    created_at: event_timestamp
                        .or(state.last_valid_event_at)
                        .or(state.session_started_at)
                        .or(filename_timestamp)
                        .unwrap_or_default(),
                    source_kind,
                });
                events.push(ParsedUsageEvent {
                    fingerprint,
                    log,
                    after_owned_task_start: state.owned_task_started,
                });
            }
            _ => {}
        }
    }

    if !events.is_empty() && !state.has_session_meta {
        return Err("会话文件包含用量事件但缺少有效 session_meta，已保留上次可信缓存".to_string());
    }
    let rollout_id = state
        .rollout_id
        .or(path_rollout_id)
        .or_else(|| state.session_id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    Ok(ParsedUsageFile {
        source_path: path.to_string_lossy().to_string(),
        rollout_id,
        parent_rollout_id: state.parent_rollout_id,
        events,
        is_fork: state.is_fork,
    })
}

fn collect_codex_session_files(codex_home: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_jsonl_recursive(&codex_home.join("sessions"), &mut files, 0, 4);
    collect_jsonl_recursive(&codex_home.join("archived_sessions"), &mut files, 0, 1);
    files.sort();
    files
}

fn collect_jsonl_recursive(dir: &Path, files: &mut Vec<PathBuf>, depth: u32, max_depth: u32) {
    if !dir.is_dir() || depth > max_depth {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_recursive(&path, files, depth + 1, max_depth);
        } else if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
}

fn parse_cumulative_tokens(total_usage: &Value) -> Option<CumulativeTokens> {
    if total_usage.is_null() || !total_usage.is_object() {
        return None;
    }
    Some(CumulativeTokens {
        input: read_u64(total_usage, &["input_tokens", "inputTokens", "input"]),
        cached_input: read_u64(
            total_usage,
            &[
                "cached_input_tokens",
                "cache_read_input_tokens",
                "cache_read_tokens",
                "cacheReadTokens",
                "cachedInput",
            ],
        ),
        output: read_u64(total_usage, &["output_tokens", "outputTokens", "output"]),
    })
}

fn compute_delta(prev: &Option<CumulativeTokens>, current: &CumulativeTokens) -> DeltaTokens {
    match prev {
        None => DeltaTokens {
            input: current.input,
            cached_input: current.cached_input,
            output: current.output,
        },
        Some(prev) => DeltaTokens {
            input: current.input.saturating_sub(prev.input),
            cached_input: current.cached_input.saturating_sub(prev.cached_input),
            output: current.output.saturating_sub(prev.output),
        },
    }
}

fn ensure_usage_cache_db(force_refresh: bool) -> Result<(Connection, UsageCache), String> {
    let mut conn = open_usage_db()?;
    migrate_usage_json_cache_if_needed(&mut conn)?;
    let previous_cache_meta = read_usage_cache_meta_db(&conn)?;
    if let Some(cache) = previous_cache_meta.as_ref() {
        let files_match = cache.files == current_usage_cache_files();
        if !should_rebuild_usage_cache(force_refresh, cache.version, files_match) {
            return Ok((conn, cache.clone()));
        }
    }
    // A rebuild must retain records whose source JSONL has already rotated away.  Current
    // files replace matching request IDs below, which also repairs v2 fork-overwrite rows.
    let previous_cache = read_usage_cache_db(&conn)?;
    let cache = rebuild_usage_cache_db(&mut conn, previous_cache)?;
    Ok((conn, cache))
}

fn should_rebuild_usage_cache(force_refresh: bool, cache_version: u32, files_match: bool) -> bool {
    force_refresh || cache_version != USAGE_CACHE_VERSION || !files_match
}

fn current_usage_cache_files() -> Vec<UsageCacheFile> {
    collect_codex_session_files(&default_codex_home())
        .iter()
        .filter_map(|path| usage_cache_file(path))
        .collect()
}

fn read_usage_json_cache(path: &Path) -> Result<UsageCache, String> {
    let content =
        fs::read_to_string(path).map_err(|error| format!("读取统计缓存失败: {}", error))?;
    serde_json::from_str(&content).map_err(|error| format!("解析统计缓存失败: {}", error))
}

fn rebuild_usage_cache_db(
    conn: &mut Connection,
    previous_cache: Option<UsageCache>,
) -> Result<UsageCache, String> {
    fs::create_dir_all(statistics_dir()).map_err(|error| format!("创建统计目录失败: {}", error))?;
    let codex_home = default_codex_home();
    let files = collect_codex_session_files(&codex_home);
    let file_meta = files
        .iter()
        .filter_map(|path| usage_cache_file(path))
        .collect();
    let (current_logs, parsed_rollout_ids, errors) = parse_codex_usage_files(&files);
    let previous_version = previous_cache.as_ref().map(|cache| cache.version);
    let previous_logs = previous_cache.map(|cache| cache.logs);
    let logs = merge_usage_logs(
        previous_logs,
        previous_version,
        current_logs,
        &parsed_rollout_ids,
    );
    let cache = UsageCache {
        version: USAGE_CACHE_VERSION,
        updated_at: Local::now().timestamp(),
        files: file_meta,
        logs,
        errors,
    };
    write_usage_cache_db(conn, &cache)?;
    Ok(cache)
}

fn parse_codex_usage_files(
    files: &[PathBuf],
) -> (Vec<ParsedUsageLog>, HashSet<String>, Vec<String>) {
    let mut parsed_files = Vec::new();
    let mut errors = Vec::new();
    for file in files {
        match parse_codex_usage_file(file) {
            Ok(parsed) => parsed_files.push(parsed),
            Err(error) => errors.push(format!("{}: {}", file.display(), error)),
        }
    }

    let mut rollout_indexes = HashMap::<String, usize>::new();
    for (index, file) in parsed_files.iter().enumerate() {
        rollout_indexes
            .entry(file.rollout_id.clone())
            .and_modify(|current| {
                let selected = &parsed_files[*current];
                if selected.events.len() < file.events.len()
                    || (selected.events.len() == file.events.len()
                        && selected.source_path > file.source_path)
                {
                    *current = index;
                }
            })
            .or_insert(index);
    }

    let mut selected_indexes = rollout_indexes.values().copied().collect::<Vec<_>>();
    selected_indexes.sort_by(|left, right| {
        parsed_files[*left]
            .rollout_id
            .cmp(&parsed_files[*right].rollout_id)
    });
    let mut parsed_rollout_ids = HashSet::new();
    let mut logs = Vec::new();
    for index in selected_indexes {
        let file = &parsed_files[index];
        if file.events.is_empty() {
            // A newly-created or concurrently truncated JSONL is not proof that previously
            // parsed owned events disappeared. Leave its trusted v4 cache untouched.
            continue;
        }
        let explicit_boundary = file
            .events
            .iter()
            .position(|event| event.after_owned_task_start);
        let inherited_count = if !file.is_fork {
            Some(0)
        } else if let Some(boundary) = explicit_boundary {
            // A task_started stamped at this rollout's own creation second is the strongest
            // ownership signal. Forks can replay only a suffix of the parent (LCP = 0), and
            // an owned call can coincidentally share the parent's next token fingerprint.
            Some(boundary)
        } else if let Some(parent) = file
            .parent_rollout_id
            .as_ref()
            .and_then(|parent_id| rollout_indexes.get(parent_id))
            .map(|index| &parsed_files[*index])
            .filter(|parent| !parent.events.is_empty())
        {
            let lcp = common_event_prefix_len(&parent.events, &file.events);
            if lcp > 0 {
                Some(lcp)
            } else {
                errors.push(format!(
                    "{}: fork 与父 rollout 无公共回放前缀且缺少可信自有事件边界，已保留上次可信缓存",
                    file.rollout_id
                ));
                None
            }
        } else {
            // If the parent JSONL has rotated away, only an explicit task_started whose
            // started_at belongs to this rollout is a safe ownership boundary. Otherwise
            // the copied history cannot be distinguished and is conservatively ignored.
            errors.push(format!(
                "{}: 缺少父 rollout 且无法确认 fork 自有事件边界，已保留上次可信缓存",
                file.rollout_id
            ));
            None
        };
        let Some(inherited_count) = inherited_count else {
            continue;
        };
        parsed_rollout_ids.insert(file.rollout_id.clone());
        logs.extend(
            file.events
                .iter()
                .skip(inherited_count)
                .filter_map(|event| event.log.clone()),
        );
    }
    (
        dedupe_usage_logs_preferred(logs),
        parsed_rollout_ids,
        errors,
    )
}

fn common_event_prefix_len(parent: &[ParsedUsageEvent], child: &[ParsedUsageEvent]) -> usize {
    parent
        .iter()
        .zip(child)
        .take_while(|(parent_event, child_event)| {
            parent_event.fingerprint == child_event.fingerprint
        })
        .count()
}

fn open_usage_db() -> Result<Connection, String> {
    fs::create_dir_all(statistics_dir()).map_err(|error| format!("创建统计目录失败: {}", error))?;
    let conn = Connection::open(usage_db_path())
        .map_err(|error| format!("打开统计数据库失败: {}", error))?;
    init_usage_db(&conn)?;
    Ok(conn)
}

fn init_usage_db(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        CREATE TABLE IF NOT EXISTS usage_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS usage_files (
            path TEXT PRIMARY KEY,
            modified_ms TEXT NOT NULL,
            size_bytes INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS usage_errors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS usage_logs (
            request_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL DEFAULT '',
            rollout_id TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            cache_read_tokens INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            source_kind TEXT NOT NULL DEFAULT 'legacy'
        );
        CREATE INDEX IF NOT EXISTS idx_usage_logs_created_at ON usage_logs(created_at);
        CREATE INDEX IF NOT EXISTS idx_usage_logs_model ON usage_logs(model);
        "#,
    )
    .map_err(|error| format!("初始化统计数据库失败: {}", error))?;
    let columns = usage_log_columns(conn)?;
    for (name, statement) in [
        (
            "session_id",
            "ALTER TABLE usage_logs ADD COLUMN session_id TEXT NOT NULL DEFAULT ''",
        ),
        (
            "rollout_id",
            "ALTER TABLE usage_logs ADD COLUMN rollout_id TEXT NOT NULL DEFAULT ''",
        ),
        (
            "source_kind",
            "ALTER TABLE usage_logs ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'legacy'",
        ),
    ] {
        if !columns.contains(name) {
            conn.execute(statement, [])
                .map_err(|error| format!("迁移统计数据库字段失败: {}", error))?;
        }
    }
    Ok(())
}

fn usage_log_columns(conn: &Connection) -> Result<HashSet<String>, String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(usage_logs)")
        .map_err(|error| format!("读取统计数据库字段失败: {}", error))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("读取统计数据库字段失败: {}", error))?;
    let mut columns = HashSet::new();
    for row in rows {
        columns.insert(row.map_err(|error| format!("读取统计数据库字段失败: {}", error))?);
    }
    Ok(columns)
}

fn read_usage_cache_db(conn: &Connection) -> Result<Option<UsageCache>, String> {
    let Some(mut cache) = read_usage_cache_meta_db(conn)? else {
        return Ok(None);
    };
    cache.logs = read_usage_logs_db(conn)?;
    Ok(Some(cache))
}

fn read_usage_cache_meta_db(conn: &Connection) -> Result<Option<UsageCache>, String> {
    let Some(version_value) = read_usage_meta(conn, "version")? else {
        return Ok(None);
    };
    let version = version_value.parse::<u32>().unwrap_or_default();
    let updated_at = read_usage_meta(conn, "updatedAt")?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_default();
    let files = read_usage_files_db(conn)?;
    let errors = read_usage_errors_db(conn)?;
    Ok(Some(UsageCache {
        version,
        updated_at,
        files,
        logs: Vec::new(),
        errors,
    }))
}

fn read_usage_meta(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM usage_meta WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| format!("读取统计元数据失败: {}", error))
}

fn read_usage_files_db(conn: &Connection) -> Result<Vec<UsageCacheFile>, String> {
    let mut stmt = conn
        .prepare("SELECT path, modified_ms, size_bytes FROM usage_files ORDER BY path ASC")
        .map_err(|error| format!("读取统计文件指纹失败: {}", error))?;
    let rows = stmt
        .query_map([], |row| {
            let modified_ms = row.get::<_, String>(1)?.parse::<u128>().unwrap_or_default();
            Ok(UsageCacheFile {
                path: row.get(0)?,
                modified_ms,
                size_bytes: row.get::<_, i64>(2)?.max(0) as u64,
            })
        })
        .map_err(|error| format!("读取统计文件指纹失败: {}", error))?;
    collect_sqlite_rows(rows, "读取统计文件指纹失败")
}

fn read_usage_logs_db(conn: &Connection) -> Result<Vec<ParsedUsageLog>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT request_id, session_id, rollout_id, model, input_tokens, output_tokens, \
             cache_read_tokens, created_at, source_kind \
             FROM usage_logs ORDER BY created_at ASC, request_id ASC",
        )
        .map_err(|error| format!("读取统计日志失败: {}", error))?;
    let rows = stmt
        .query_map([], |row| {
            let request_id = row.get::<_, String>(0)?;
            let stored_session_id = row.get::<_, String>(1)?;
            let session_id = if stored_session_id.is_empty() {
                legacy_session_id_from_request_id(&request_id)
                    .unwrap_or_default()
                    .to_string()
            } else {
                stored_session_id
            };
            Ok(ParsedUsageLog {
                request_id,
                session_id,
                rollout_id: row.get(2)?,
                model: row.get(3)?,
                input_tokens: row.get::<_, i64>(4)?.max(0) as u64,
                output_tokens: row.get::<_, i64>(5)?.max(0) as u64,
                cache_read_tokens: row.get::<_, i64>(6)?.max(0) as u64,
                created_at: row.get(7)?,
                source_kind: UsageLogSourceKind::from_db_value(&row.get::<_, String>(8)?),
            })
        })
        .map_err(|error| format!("读取统计日志失败: {}", error))?;
    collect_sqlite_rows(rows, "读取统计日志失败")
}

fn read_usage_logs_db_range(
    conn: &Connection,
    start: i64,
    end: i64,
) -> Result<Vec<ParsedUsageLog>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT request_id, session_id, rollout_id, model, input_tokens, output_tokens, \
             cache_read_tokens, created_at, source_kind \
             FROM usage_logs WHERE created_at >= ?1 AND created_at <= ?2 \
             ORDER BY created_at ASC, request_id ASC",
        )
        .map_err(|error| format!("读取统计日志失败: {}", error))?;
    let rows = stmt
        .query_map(params![start, end], |row| {
            let request_id = row.get::<_, String>(0)?;
            let stored_session_id = row.get::<_, String>(1)?;
            let session_id = if stored_session_id.is_empty() {
                legacy_session_id_from_request_id(&request_id)
                    .unwrap_or_default()
                    .to_string()
            } else {
                stored_session_id
            };
            Ok(ParsedUsageLog {
                request_id,
                session_id,
                rollout_id: row.get(2)?,
                model: row.get(3)?,
                input_tokens: row.get::<_, i64>(4)?.max(0) as u64,
                output_tokens: row.get::<_, i64>(5)?.max(0) as u64,
                cache_read_tokens: row.get::<_, i64>(6)?.max(0) as u64,
                created_at: row.get(7)?,
                source_kind: UsageLogSourceKind::from_db_value(&row.get::<_, String>(8)?),
            })
        })
        .map_err(|error| format!("读取统计日志失败: {}", error))?;
    collect_sqlite_rows(rows, "读取统计日志失败")
}

fn read_usage_errors_db(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT message FROM usage_errors ORDER BY id ASC")
        .map_err(|error| format!("读取统计错误失败: {}", error))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("读取统计错误失败: {}", error))?;
    collect_sqlite_rows(rows, "读取统计错误失败")
}

fn collect_sqlite_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
    message: &str,
) -> Result<Vec<T>, String> {
    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|error| format!("{}: {}", message, error))?);
    }
    Ok(items)
}

fn write_usage_cache_db(conn: &mut Connection, cache: &UsageCache) -> Result<(), String> {
    let tx = conn
        .transaction()
        .map_err(|error| format!("写入统计数据库失败: {}", error))?;
    tx.execute(
        "INSERT OR REPLACE INTO usage_meta (key, value) VALUES ('version', ?1)",
        params![cache.version.to_string()],
    )
    .map_err(|error| format!("写入统计版本失败: {}", error))?;
    tx.execute(
        "INSERT OR REPLACE INTO usage_meta (key, value) VALUES ('updatedAt', ?1)",
        params![cache.updated_at.to_string()],
    )
    .map_err(|error| format!("写入统计更新时间失败: {}", error))?;
    tx.execute("DELETE FROM usage_files", [])
        .map_err(|error| format!("清理统计文件指纹失败: {}", error))?;
    tx.execute("DELETE FROM usage_errors", [])
        .map_err(|error| format!("清理统计错误失败: {}", error))?;
    tx.execute("DELETE FROM usage_logs", [])
        .map_err(|error| format!("清理统计日志失败: {}", error))?;

    {
        let mut stmt = tx
            .prepare("INSERT INTO usage_files (path, modified_ms, size_bytes) VALUES (?1, ?2, ?3)")
            .map_err(|error| format!("写入统计文件指纹失败: {}", error))?;
        for file in &cache.files {
            stmt.execute(params![
                file.path,
                file.modified_ms.to_string(),
                file.size_bytes as i64,
            ])
            .map_err(|error| format!("写入统计文件指纹失败: {}", error))?;
        }
    }
    {
        let mut stmt = tx
            .prepare("INSERT INTO usage_errors (message) VALUES (?1)")
            .map_err(|error| format!("写入统计错误失败: {}", error))?;
        for error in &cache.errors {
            stmt.execute(params![error])
                .map_err(|error| format!("写入统计错误失败: {}", error))?;
        }
    }
    {
        let mut stmt = tx
            .prepare(
                "INSERT OR IGNORE INTO usage_logs \
                 (request_id, session_id, rollout_id, model, input_tokens, output_tokens, \
                  cache_read_tokens, created_at, source_kind) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .map_err(|error| format!("写入统计日志失败: {}", error))?;
        for log in &cache.logs {
            stmt.execute(params![
                log.request_id,
                log.session_id,
                log.rollout_id,
                log.model,
                log.input_tokens as i64,
                log.output_tokens as i64,
                log.cache_read_tokens as i64,
                log.created_at,
                log.source_kind.as_db_value(),
            ])
            .map_err(|error| format!("写入统计日志失败: {}", error))?;
        }
    }

    tx.commit()
        .map_err(|error| format!("提交统计数据库失败: {}", error))?;
    Ok(())
}

fn migrate_usage_json_cache_if_needed(conn: &mut Connection) -> Result<(), String> {
    let path = usage_cache_path();
    migrate_usage_json_cache_from_path_if_needed(conn, &path)
}

fn migrate_usage_json_cache_from_path_if_needed(
    conn: &mut Connection,
    path: &Path,
) -> Result<(), String> {
    let Some(marker) = usage_json_cache_marker(path) else {
        return Ok(());
    };
    if read_usage_meta(conn, "jsonMigrated")?.as_deref() == Some(marker.as_str()) {
        return Ok(());
    }
    let existing_version =
        read_usage_meta(conn, "version")?.and_then(|value| value.parse::<u32>().ok());
    if existing_version.is_some_and(|version| version >= USAGE_CACHE_VERSION) {
        // A v4 database already uses provenance-aware rollout IDs. Re-importing a changed
        // legacy JSON cache would add non-conflicting codex_session IDs and double-count it.
        write_usage_meta(conn, "jsonMigrated", &marker)?;
        return Ok(());
    }
    let has_existing_cache = existing_version.is_some();
    let cache = match read_usage_json_cache(path) {
        Ok(cache) => cache,
        Err(_) if has_existing_cache => return Ok(()),
        Err(error) => return Err(error),
    };
    import_usage_cache_into_db(conn, &cache)?;
    write_usage_meta(conn, "jsonMigrated", &marker)?;
    Ok(())
}

fn usage_json_cache_marker(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    Some(format!("{}:{}", metadata.len(), modified_ms))
}

fn write_usage_meta(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO usage_meta (key, value) VALUES (?1, ?2)",
        params![key, value],
    )
    .map_err(|error| format!("写入统计元数据失败: {}", error))?;
    Ok(())
}

fn import_usage_cache_into_db(conn: &mut Connection, cache: &UsageCache) -> Result<(), String> {
    let has_version = read_usage_meta(conn, "version")?.is_some();
    let tx = conn
        .transaction()
        .map_err(|error| format!("迁移统计缓存失败: {}", error))?;
    if !has_version {
        tx.execute(
            "INSERT OR REPLACE INTO usage_meta (key, value) VALUES ('version', ?1)",
            params![cache.version.to_string()],
        )
        .map_err(|error| format!("迁移统计版本失败: {}", error))?;
        tx.execute(
            "INSERT OR REPLACE INTO usage_meta (key, value) VALUES ('updatedAt', ?1)",
            params![cache.updated_at.to_string()],
        )
        .map_err(|error| format!("迁移统计更新时间失败: {}", error))?;
        tx.execute("DELETE FROM usage_files", [])
            .map_err(|error| format!("迁移统计文件指纹失败: {}", error))?;
        tx.execute("DELETE FROM usage_errors", [])
            .map_err(|error| format!("迁移统计错误失败: {}", error))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO usage_files (path, modified_ms, size_bytes) VALUES (?1, ?2, ?3)",
                )
                .map_err(|error| format!("迁移统计文件指纹失败: {}", error))?;
            for file in &cache.files {
                stmt.execute(params![
                    file.path,
                    file.modified_ms.to_string(),
                    file.size_bytes as i64,
                ])
                .map_err(|error| format!("迁移统计文件指纹失败: {}", error))?;
            }
        }
        {
            let mut stmt = tx
                .prepare("INSERT INTO usage_errors (message) VALUES (?1)")
                .map_err(|error| format!("迁移统计错误失败: {}", error))?;
            for error in &cache.errors {
                stmt.execute(params![error])
                    .map_err(|error| format!("迁移统计错误失败: {}", error))?;
            }
        }
    }
    {
        let mut stmt = tx
            .prepare(
                "INSERT OR IGNORE INTO usage_logs \
                 (request_id, session_id, rollout_id, model, input_tokens, output_tokens, \
                  cache_read_tokens, created_at, source_kind) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .map_err(|error| format!("迁移统计日志失败: {}", error))?;
        for log in &cache.logs {
            stmt.execute(params![
                log.request_id,
                log.session_id,
                log.rollout_id,
                log.model,
                log.input_tokens as i64,
                log.output_tokens as i64,
                log.cache_read_tokens as i64,
                log.created_at,
                log.source_kind.as_db_value(),
            ])
            .map_err(|error| format!("迁移统计日志失败: {}", error))?;
        }
    }
    tx.commit()
        .map_err(|error| format!("提交统计迁移失败: {}", error))?;
    Ok(())
}

fn merge_usage_logs(
    previous_logs: Option<Vec<ParsedUsageLog>>,
    previous_version: Option<u32>,
    current_logs: Vec<ParsedUsageLog>,
    parsed_rollout_ids: &HashSet<String>,
) -> Vec<ParsedUsageLog> {
    let current_logs = dedupe_usage_logs_preferred(current_logs);
    if previous_version != Some(USAGE_CACHE_VERSION) {
        // Versions before v4 keyed every rollout by session_id:event_index and did not
        // persist provenance. Previous-only rows therefore cannot be distinguished from
        // fork replay pollution. Rebuild exclusively from source JSONL once during the
        // migration instead of making unverifiable rows permanent.
        return current_logs;
    }

    let mut merged = dedupe_usage_logs_preferred(previous_logs.unwrap_or_default())
        .into_iter()
        .filter(|log| log.rollout_id.is_empty() || !parsed_rollout_ids.contains(&log.rollout_id))
        .map(|log| (log.request_id.clone(), log))
        .collect::<HashMap<_, _>>();
    for current in current_logs {
        match merged.get(&current.request_id) {
            Some(previous) if previous.source_kind.priority() > current.source_kind.priority() => {}
            _ => {
                merged.insert(current.request_id.clone(), current);
            }
        }
    }
    let mut logs = merged.into_values().collect::<Vec<_>>();
    logs.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.request_id.cmp(&right.request_id))
    });
    logs
}

fn dedupe_usage_logs_preferred(mut logs: Vec<ParsedUsageLog>) -> Vec<ParsedUsageLog> {
    logs.sort_by(|left, right| {
        left.request_id
            .cmp(&right.request_id)
            .then_with(|| {
                right
                    .source_kind
                    .priority()
                    .cmp(&left.source_kind.priority())
            })
            .then_with(|| left.created_at.cmp(&right.created_at))
            .then_with(|| left.model.cmp(&right.model))
            .then_with(|| left.input_tokens.cmp(&right.input_tokens))
            .then_with(|| left.output_tokens.cmp(&right.output_tokens))
            .then_with(|| left.cache_read_tokens.cmp(&right.cache_read_tokens))
    });
    logs.dedup_by(|left, right| left.request_id == right.request_id);
    logs.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.request_id.cmp(&right.request_id))
    });
    logs
}

fn usage_cache_file(path: &Path) -> Option<UsageCacheFile> {
    let metadata = fs::metadata(path).ok()?;
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    Some(UsageCacheFile {
        path: path.to_string_lossy().to_string(),
        modified_ms,
        size_bytes: metadata.len(),
    })
}

fn summarize_logs(
    logs: &[ParsedUsageLog],
    pricing: &[CodexUsagePricing],
    cost_multiplier: f64,
) -> CodexUsageSummary {
    let mut summary = CodexUsageSummary::default();
    let mut total_cost = 0.0;
    for log in logs {
        summary.total_requests += 1;
        summary.total_input_tokens += fresh_input_tokens(log);
        summary.total_output_tokens += log.output_tokens;
        summary.total_cache_read_tokens += log.cache_read_tokens;
        total_cost += calculate_cost(log, pricing, cost_multiplier);
    }
    summary.real_total_tokens = summary.total_input_tokens
        + summary.total_output_tokens
        + summary.total_cache_creation_tokens
        + summary.total_cache_read_tokens;
    let cacheable_input = summary.total_input_tokens
        + summary.total_cache_creation_tokens
        + summary.total_cache_read_tokens;
    summary.cache_hit_rate = if cacheable_input > 0 {
        summary.total_cache_read_tokens as f64 / cacheable_input as f64
    } else {
        0.0
    };
    summary.total_cost = format_cost(total_cost);
    summary
}

fn build_usage_activity(logs: &[ParsedUsageLog]) -> CodexUsageActivity {
    let today = Local::now().date_naive();
    let start_date = usage_activity_start_date(today);
    let day_count = 53 * 7;
    let mut day_tokens: HashMap<NaiveDate, (u64, usize)> = HashMap::new();
    let mut hour_tokens: HashMap<u32, (u64, usize)> = HashMap::new();
    let mut session_ranges: HashMap<String, (i64, i64)> = HashMap::new();

    for log in logs {
        let local_time = Local
            .timestamp_opt(log.created_at, 0)
            .single()
            .unwrap_or_else(Local::now);
        let date = local_time.date_naive();
        let entry = day_tokens.entry(date).or_default();
        entry.0 += log_total_tokens(log);
        entry.1 += 1;

        if date == today {
            let hour_entry = hour_tokens.entry(local_time.hour()).or_default();
            hour_entry.0 += log_total_tokens(log);
            hour_entry.1 += 1;
        }

        if !log.session_id.is_empty() {
            let range = session_ranges
                .entry(log.session_id.clone())
                .or_insert((log.created_at, log.created_at));
            range.0 = range.0.min(log.created_at);
            range.1 = range.1.max(log.created_at);
        }
    }

    let mut hours = Vec::with_capacity(24);
    for hour in 0..24 {
        let (tokens, requests) = hour_tokens.get(&hour).copied().unwrap_or_default();
        let timestamp = Local
            .with_ymd_and_hms(today.year(), today.month(), today.day(), hour, 0, 0)
            .earliest()
            .map(|date_time| date_time.timestamp())
            .unwrap_or_else(|| Local::now().timestamp());
        hours.push(CodexUsageActivityHour {
            hour,
            label: format!("{:02}:00", hour),
            timestamp,
            tokens,
            requests,
        });
    }

    let mut days = Vec::with_capacity(day_count);
    for index in 0..day_count {
        let date = start_date + ChronoDuration::days(index as i64);
        let (tokens, requests) = day_tokens.get(&date).copied().unwrap_or_default();
        days.push(CodexUsageActivityDay {
            date: date.format("%Y-%m-%d").to_string(),
            timestamp: local_day_start_timestamp(date),
            tokens,
            requests,
        });
    }

    let total_tokens = days.iter().map(|day| day.tokens).sum::<u64>();
    let peak_day_tokens = days.iter().map(|day| day.tokens).max().unwrap_or_default();
    let longest_task_seconds = session_ranges
        .values()
        .map(|(start, end)| end.saturating_sub(*start))
        .max()
        .unwrap_or_default();
    let active_days = days
        .iter()
        .map(|day| (day.date.clone(), day.tokens > 0))
        .collect::<HashMap<_, _>>();

    CodexUsageActivity {
        summary: CodexUsageActivitySummary {
            total_tokens,
            peak_day_tokens,
            longest_task_seconds,
            current_streak_days: current_usage_streak(today, &active_days),
            longest_streak_days: longest_usage_streak(&days),
        },
        days,
        hours,
    }
}

fn usage_activity_start_date(today: NaiveDate) -> NaiveDate {
    let current_week_start =
        today - ChronoDuration::days(today.weekday().num_days_from_monday() as i64);
    current_week_start - ChronoDuration::weeks(52)
}

fn local_day_start_timestamp(date: NaiveDate) -> i64 {
    let Some(naive) = date.and_hms_opt(0, 0, 0) else {
        return Local::now().timestamp();
    };
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|date_time| date_time.timestamp())
        .unwrap_or_else(|| Local::now().timestamp())
}

fn legacy_session_id_from_request_id(request_id: &str) -> Option<&str> {
    request_id
        .strip_prefix("codex_session:")
        .and_then(|rest| rest.rsplit_once(':').map(|(session_id, _)| session_id))
        .filter(|session_id| !session_id.is_empty())
}

fn log_total_tokens(log: &ParsedUsageLog) -> u64 {
    fresh_input_tokens(log) + log.output_tokens + log.cache_read_tokens
}

fn current_usage_streak(today: NaiveDate, active_days: &HashMap<String, bool>) -> usize {
    let mut streak = 0;
    let mut cursor = today;
    loop {
        let key = cursor.format("%Y-%m-%d").to_string();
        if !active_days.get(&key).copied().unwrap_or(false) {
            break;
        }
        streak += 1;
        cursor -= ChronoDuration::days(1);
    }
    streak
}

fn longest_usage_streak(days: &[CodexUsageActivityDay]) -> usize {
    let mut current = 0;
    let mut longest = 0;
    for day in days {
        if day.tokens > 0 {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn build_trends(
    logs: &[ParsedUsageLog],
    start_date: Option<i64>,
    end_date: Option<i64>,
    pricing: &[CodexUsagePricing],
    cost_multiplier: f64,
) -> Vec<CodexUsageTrendPoint> {
    let now = Local::now().timestamp();
    let end = end_date.unwrap_or(now);
    let start = start_date.unwrap_or(end - 24 * 60 * 60);
    let same_local_day = Local
        .timestamp_opt(start, 0)
        .single()
        .zip(Local.timestamp_opt(end, 0).single())
        .is_some_and(|(start, end)| start.date_naive() == end.date_naive());
    let hourly = same_local_day || end.saturating_sub(start) <= 24 * 60 * 60;
    if !hourly {
        return build_daily_trends(logs, start, end, pricing, cost_multiplier);
    }
    let bucket_seconds = 60 * 60;
    let bucket_count = (end.saturating_sub(start) / bucket_seconds + 1).clamp(1, 60) as usize;
    let mut buckets = Vec::with_capacity(bucket_count);
    for index in 0..bucket_count {
        let timestamp = start + index as i64 * bucket_seconds;
        buckets.push(CodexUsageTrendPoint {
            timestamp,
            label: format_trend_label(timestamp, hourly),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            total_cost: "0.000000".to_string(),
        });
    }
    let mut costs = vec![0.0; bucket_count];
    for log in logs {
        if log.created_at < start || log.created_at > end {
            continue;
        }
        let index =
            ((log.created_at - start) / bucket_seconds).clamp(0, bucket_count as i64 - 1) as usize;
        buckets[index].input_tokens += fresh_input_tokens(log);
        buckets[index].output_tokens += log.output_tokens;
        buckets[index].cache_read_tokens += log.cache_read_tokens;
        costs[index] += calculate_cost(log, pricing, cost_multiplier);
    }
    for (bucket, cost) in buckets.iter_mut().zip(costs) {
        bucket.total_cost = format_cost(cost);
    }
    buckets
}

fn build_daily_trends(
    logs: &[ParsedUsageLog],
    start: i64,
    end: i64,
    pricing: &[CodexUsagePricing],
    cost_multiplier: f64,
) -> Vec<CodexUsageTrendPoint> {
    let start_day = Local
        .timestamp_opt(start, 0)
        .single()
        .map(|datetime| datetime.date_naive())
        .unwrap_or_else(|| Local::now().date_naive());
    let end_day = Local
        .timestamp_opt(end, 0)
        .single()
        .map(|datetime| datetime.date_naive())
        .unwrap_or(start_day);
    let bucket_count = (end_day
        .signed_duration_since(start_day)
        .num_days()
        .saturating_add(1))
    .clamp(1, 60) as usize;
    let mut buckets = Vec::with_capacity(bucket_count);
    for index in 0..bucket_count {
        let date = start_day + ChronoDuration::days(index as i64);
        let timestamp = local_day_start_timestamp(date);
        buckets.push(CodexUsageTrendPoint {
            timestamp,
            label: date.format("%m/%d").to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            total_cost: "0.000000".to_string(),
        });
    }
    let mut costs = vec![0.0; bucket_count];
    for log in logs {
        if log.created_at < start || log.created_at > end {
            continue;
        }
        let Some(log_day) = Local
            .timestamp_opt(log.created_at, 0)
            .single()
            .map(|datetime| datetime.date_naive())
        else {
            continue;
        };
        let index = log_day.signed_duration_since(start_day).num_days();
        if index < 0 || index >= bucket_count as i64 {
            continue;
        }
        let index = index as usize;
        buckets[index].input_tokens += fresh_input_tokens(log);
        buckets[index].output_tokens += log.output_tokens;
        buckets[index].cache_read_tokens += log.cache_read_tokens;
        costs[index] += calculate_cost(log, pricing, cost_multiplier);
    }
    for (bucket, cost) in buckets.iter_mut().zip(costs) {
        bucket.total_cost = format_cost(cost);
    }
    buckets
}

fn build_provider_stats(
    logs: &[ParsedUsageLog],
    pricing: &[CodexUsagePricing],
    cost_multiplier: f64,
) -> Vec<CodexUsageProviderStat> {
    let total_cost = logs
        .iter()
        .map(|log| calculate_cost(log, pricing, cost_multiplier))
        .sum::<f64>();
    let total_tokens = logs
        .iter()
        .map(|log| fresh_input_tokens(log) + log.output_tokens + log.cache_read_tokens)
        .sum::<u64>();
    if logs.is_empty() {
        return Vec::new();
    }
    vec![CodexUsageProviderStat {
        provider_id: "codex_session".to_string(),
        provider_name: "Codex (Session)".to_string(),
        request_count: logs.len(),
        total_tokens,
        total_cost: format_cost(total_cost),
        success_rate: 100.0,
        avg_latency_ms: 0,
    }]
}

fn build_model_stats(
    logs: &[ParsedUsageLog],
    pricing: &[CodexUsagePricing],
    cost_multiplier: f64,
) -> Vec<CodexUsageModelStat> {
    let mut grouped: HashMap<String, (usize, u64, f64)> = HashMap::new();
    for log in logs {
        let entry = grouped.entry(log.model.clone()).or_default();
        entry.0 += 1;
        entry.1 += fresh_input_tokens(log) + log.output_tokens + log.cache_read_tokens;
        entry.2 += calculate_cost(log, pricing, cost_multiplier);
    }
    let mut stats = grouped
        .into_iter()
        .map(
            |(model, (request_count, total_tokens, cost))| CodexUsageModelStat {
                model,
                request_count,
                total_tokens,
                total_cost: format_cost(cost),
                avg_cost_per_request: format_cost(if request_count > 0 {
                    cost / request_count as f64
                } else {
                    0.0
                }),
            },
        )
        .collect::<Vec<_>>();
    stats.sort_by_key(|item| std::cmp::Reverse(item.total_tokens));
    stats
}

fn to_usage_log(
    log: &ParsedUsageLog,
    pricing: &[CodexUsagePricing],
    cost_multiplier: f64,
) -> CodexUsageLog {
    CodexUsageLog {
        request_id: log.request_id.clone(),
        provider_name: "Codex (Session)".to_string(),
        model: log.model.clone(),
        input_tokens: fresh_input_tokens(log),
        output_tokens: log.output_tokens,
        cache_read_tokens: log.cache_read_tokens,
        cache_creation_tokens: 0,
        total_cost: format_cost(calculate_cost(log, pricing, cost_multiplier)),
        status_code: 200,
        created_at: log.created_at,
        data_source: "codex_session".to_string(),
    }
}

fn calculate_cost(
    log: &ParsedUsageLog,
    pricing_list: &[CodexUsagePricing],
    cost_multiplier: f64,
) -> f64 {
    let pricing = model_pricing(&log.model, pricing_list);
    ((fresh_input_tokens(log) as f64 * pricing.input_per_million
        + log.output_tokens as f64 * pricing.output_per_million
        + log.cache_read_tokens as f64 * pricing.cache_read_per_million
        + 0.0 * pricing.cache_creation_per_million)
        / 1_000_000.0)
        * cost_multiplier
}

fn model_pricing(model: &str, pricing_list: &[CodexUsagePricing]) -> ModelPricing {
    for candidate in model_pricing_candidates(model) {
        if let Some(pricing) = pricing_list.iter().find(|item| item.model_id == candidate) {
            return pricing_to_numbers(pricing);
        }
    }
    for candidate in model_pricing_candidates(model) {
        if should_try_pricing_prefix_match(&candidate) {
            if let Some(pricing) = pricing_list
                .iter()
                .filter(|item| item.model_id.starts_with(&format!("{}-", candidate)))
                .min_by_key(|item| item.model_id.len())
            {
                return pricing_to_numbers(pricing);
            }
        }
    }
    ModelPricing {
        input_per_million: 0.0,
        output_per_million: 0.0,
        cache_read_per_million: 0.0,
        cache_creation_per_million: 0.0,
    }
}

fn load_pricing() -> Result<Vec<CodexUsagePricing>, String> {
    let path = pricing_path();
    if path.exists() {
        let content =
            fs::read_to_string(&path).map_err(|error| format!("读取成本定价失败: {}", error))?;
        let pricing: Vec<CodexUsagePricing> = serde_json::from_str(&content)
            .map_err(|error| format!("解析成本定价失败: {}", error))?;
        let applied_version = read_pricing_defaults_version()?;
        let (pricing, migrated) = migrate_pricing_defaults(pricing, applied_version);
        let mut pricing = filter_supported_pricing(pricing);
        pricing.sort_by(|left, right| left.model_id.cmp(&right.model_id));
        write_pricing(&pricing)?;
        if migrated {
            write_pricing_defaults_version()?;
        }
        return Ok(pricing);
    }
    let pricing = default_pricing();
    write_pricing(&pricing)?;
    write_pricing_defaults_version()?;
    Ok(pricing)
}

fn migrate_pricing_defaults(
    mut pricing: Vec<CodexUsagePricing>,
    applied_version: u32,
) -> (Vec<CodexUsagePricing>, bool) {
    if applied_version >= PRICING_DEFAULTS_VERSION {
        return (pricing, false);
    }

    for default in default_pricing()
        .into_iter()
        .filter(|item| GPT_56_DEFAULT_MODEL_IDS.contains(&item.model_id.as_str()))
    {
        if !pricing.iter().any(|item| item.model_id == default.model_id) {
            pricing.push(default);
        }
    }
    (pricing, true)
}

fn read_pricing_defaults_version() -> Result<u32, String> {
    let path = pricing_defaults_version_path();
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("读取默认定价版本失败: {error}")),
    };
    content
        .trim()
        .parse::<u32>()
        .map_err(|error| format!("解析默认定价版本失败: {error}"))
}

fn write_pricing_defaults_version() -> Result<(), String> {
    fs::create_dir_all(statistics_dir()).map_err(|error| format!("创建统计目录失败: {error}"))?;
    fs::write(
        pricing_defaults_version_path(),
        PRICING_DEFAULTS_VERSION.to_string(),
    )
    .map_err(|error| format!("写入默认定价版本失败: {error}"))
}

fn write_pricing(pricing: &[CodexUsagePricing]) -> Result<(), String> {
    fs::create_dir_all(statistics_dir()).map_err(|error| format!("创建统计目录失败: {}", error))?;
    let mut next = filter_supported_pricing(pricing.to_vec());
    next.sort_by(|left, right| left.model_id.cmp(&right.model_id));
    let content = serde_json::to_string_pretty(&next)
        .map_err(|error| format!("序列化成本定价失败: {}", error))?;
    fs::write(pricing_path(), content).map_err(|error| format!("写入成本定价失败: {}", error))
}

fn validate_pricing(pricing: &CodexUsagePricing) -> Result<(), String> {
    let model_id = clean_model_id_for_pricing(&pricing.model_id);
    if model_id.is_empty() {
        return Err("模型 ID 不能为空".to_string());
    }
    if !is_supported_pricing_model(&model_id) {
        return Err("当前只支持配置 GPT/Codex 相关模型".to_string());
    }
    if pricing.display_name.trim().is_empty() {
        return Err("显示名称不能为空".to_string());
    }
    for (label, value) in [
        ("输入成本", &pricing.input_cost_per_million),
        ("输出成本", &pricing.output_cost_per_million),
        ("缓存命中", &pricing.cache_read_cost_per_million),
        ("缓存创建", &pricing.cache_creation_cost_per_million),
    ] {
        let parsed = value
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("{}必须是非负数字", label))?;
        if parsed < 0.0 || !parsed.is_finite() {
            return Err(format!("{}必须是非负数字", label));
        }
    }
    Ok(())
}

fn pricing_to_numbers(pricing: &CodexUsagePricing) -> ModelPricing {
    ModelPricing {
        input_per_million: pricing.input_cost_per_million.parse().unwrap_or(0.0),
        output_per_million: pricing.output_cost_per_million.parse().unwrap_or(0.0),
        cache_read_per_million: pricing.cache_read_cost_per_million.parse().unwrap_or(0.0),
        cache_creation_per_million: pricing
            .cache_creation_cost_per_million
            .parse()
            .unwrap_or(0.0),
    }
}

fn filter_supported_pricing(pricing: Vec<CodexUsagePricing>) -> Vec<CodexUsagePricing> {
    pricing
        .into_iter()
        .filter(|item| is_supported_pricing_model(&item.model_id))
        .collect()
}

fn is_supported_pricing_model(model_id: &str) -> bool {
    let cleaned = clean_model_id_for_pricing(model_id);
    let normalized = strip_known_model_namespace(&cleaned).unwrap_or(cleaned);
    normalized.starts_with("gpt-") || normalized.starts_with("codex-")
}

fn load_pricing_configs() -> Result<Vec<CodexUsagePricingConfig>, String> {
    let path = pricing_config_path();
    if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("读取计费默认配置失败: {}", error))?;
        let configs = serde_json::from_str::<Vec<CodexUsagePricingConfig>>(&content)
            .or_else(|_| {
                serde_json::from_str::<CodexUsagePricingConfig>(&content).map(|config| vec![config])
            })
            .map_err(|error| format!("解析计费默认配置失败: {}", error))?;
        return normalize_pricing_configs(configs);
    }
    let configs = default_pricing_configs();
    write_pricing_config(&configs)?;
    Ok(configs)
}

fn write_pricing_config(configs: &[CodexUsagePricingConfig]) -> Result<(), String> {
    fs::create_dir_all(statistics_dir()).map_err(|error| format!("创建统计目录失败: {}", error))?;
    let content = serde_json::to_string_pretty(configs)
        .map_err(|error| format!("序列化计费默认配置失败: {}", error))?;
    fs::write(pricing_config_path(), content)
        .map_err(|error| format!("写入计费默认配置失败: {}", error))
}

fn default_pricing_configs() -> Vec<CodexUsagePricingConfig> {
    vec![CodexUsagePricingConfig {
        app: "Codex".to_string(),
        multiplier: "1".to_string(),
        pricing_model_source: "response".to_string(),
    }]
}

fn normalize_pricing_configs(
    configs: Vec<CodexUsagePricingConfig>,
) -> Result<Vec<CodexUsagePricingConfig>, String> {
    let mut merged = default_pricing_configs();
    for config in configs {
        let app = normalize_pricing_app(&config.app);
        if app != "Codex" {
            continue;
        }
        validate_pricing_config(&config)?;
        if let Some(existing) = merged.iter_mut().find(|item| item.app == app) {
            existing.multiplier = config.multiplier.trim().to_string();
            existing.pricing_model_source =
                normalize_pricing_model_source(&config.pricing_model_source);
        }
    }
    write_pricing_config(&merged)?;
    Ok(merged)
}

fn validate_pricing_config(config: &CodexUsagePricingConfig) -> Result<(), String> {
    let parsed = config
        .multiplier
        .trim()
        .parse::<f64>()
        .map_err(|_| "默认倍率必须是非负数字".to_string())?;
    if parsed < 0.0 || !parsed.is_finite() {
        return Err("默认倍率必须是非负数字".to_string());
    }
    match normalize_pricing_model_source(&config.pricing_model_source).as_str() {
        "response" | "request" => Ok(()),
        _ => Err("计费模式必须是 response 或 request".to_string()),
    }
}

fn normalize_pricing_model_source(source: &str) -> String {
    match source.trim().to_ascii_lowercase().as_str() {
        "request" => "request".to_string(),
        _ => "response".to_string(),
    }
}

fn normalize_pricing_app(app: &str) -> String {
    match app.trim().to_ascii_lowercase().as_str() {
        "claude" => "Claude".to_string(),
        "gemini" => "Gemini".to_string(),
        "codex" => "Codex".to_string(),
        other => other.to_string(),
    }
}

fn pricing_config_multiplier(configs: &[CodexUsagePricingConfig]) -> f64 {
    configs
        .iter()
        .find(|config| normalize_pricing_app(&config.app) == "Codex")
        .and_then(|config| config.multiplier.trim().parse::<f64>().ok())
        .unwrap_or(1.0)
        .max(0.0)
}

fn default_pricing() -> Vec<CodexUsagePricing> {
    [
        (
            "claude-fable-5",
            "Claude Fable 5",
            "10",
            "50",
            "1.00",
            "12.50",
        ),
        (
            "claude-mythos-5",
            "Claude Mythos 5",
            "10",
            "50",
            "1.00",
            "12.50",
        ),
        (
            "claude-opus-4-8",
            "Claude Opus 4.8",
            "5",
            "25",
            "0.50",
            "6.25",
        ),
        (
            "claude-opus-4-7",
            "Claude Opus 4.7",
            "5",
            "25",
            "0.50",
            "6.25",
        ),
        (
            "claude-opus-4-6-20260206",
            "Claude Opus 4.6",
            "5",
            "25",
            "0.50",
            "6.25",
        ),
        (
            "claude-sonnet-4-6-20260217",
            "Claude Sonnet 4.6",
            "3",
            "15",
            "0.30",
            "3.75",
        ),
        (
            "claude-opus-4-5-20251101",
            "Claude Opus 4.5",
            "5",
            "25",
            "0.50",
            "6.25",
        ),
        (
            "claude-sonnet-4-5-20250929",
            "Claude Sonnet 4.5",
            "3",
            "15",
            "0.30",
            "3.75",
        ),
        (
            "claude-haiku-4-5-20251001",
            "Claude Haiku 4.5",
            "1",
            "5",
            "0.10",
            "1.25",
        ),
        (
            "claude-opus-4-20250514",
            "Claude Opus 4",
            "15",
            "75",
            "1.50",
            "18.75",
        ),
        (
            "claude-opus-4-1-20250805",
            "Claude Opus 4.1",
            "15",
            "75",
            "1.50",
            "18.75",
        ),
        (
            "claude-sonnet-4-20250514",
            "Claude Sonnet 4",
            "3",
            "15",
            "0.30",
            "3.75",
        ),
        (
            "claude-3-5-haiku-20241022",
            "Claude 3.5 Haiku",
            "0.80",
            "4",
            "0.08",
            "1",
        ),
        (
            "claude-3-5-sonnet-20241022",
            "Claude 3.5 Sonnet",
            "3",
            "15",
            "0.30",
            "3.75",
        ),
        ("gpt-5.5", "GPT-5.5", "5", "30", "0.50", "0"),
        ("gpt-5.5-low", "GPT-5.5", "5", "30", "0.50", "0"),
        ("gpt-5.5-medium", "GPT-5.5", "5", "30", "0.50", "0"),
        ("gpt-5.5-high", "GPT-5.5", "5", "30", "0.50", "0"),
        ("gpt-5.5-xhigh", "GPT-5.5", "5", "30", "0.50", "0"),
        ("gpt-5.5-minimal", "GPT-5.5", "5", "30", "0.50", "0"),
        ("gpt-5.6-sol", "GPT-5.6 Sol", "5", "30", "0.50", "6.25"),
        (
            "gpt-5.6-terra",
            "GPT-5.6 Terra",
            "2.50",
            "15",
            "0.25",
            "3.125",
        ),
        ("gpt-5.6-luna", "GPT-5.6 Luna", "1", "6", "0.10", "1.25"),
        ("gpt-5.4", "GPT-5.4", "2.50", "15", "0.25", "0"),
        ("gpt-5.4-mini", "GPT-5.4 Mini", "0.75", "4.50", "0.075", "0"),
        ("gpt-5.4-nano", "GPT-5.4 Nano", "0.20", "1.25", "0.02", "0"),
        ("gpt-5.2", "GPT-5.2", "1.75", "14", "0.175", "0"),
        ("gpt-5.2-low", "GPT-5.2", "1.75", "14", "0.175", "0"),
        ("gpt-5.2-medium", "GPT-5.2", "1.75", "14", "0.175", "0"),
        ("gpt-5.2-high", "GPT-5.2", "1.75", "14", "0.175", "0"),
        ("gpt-5.2-xhigh", "GPT-5.2", "1.75", "14", "0.175", "0"),
        ("gpt-5.2-codex", "GPT-5.2 Codex", "1.75", "14", "0.175", "0"),
        (
            "gpt-5.2-codex-low",
            "GPT-5.2 Codex",
            "1.75",
            "14",
            "0.175",
            "0",
        ),
        (
            "gpt-5.2-codex-medium",
            "GPT-5.2 Codex",
            "1.75",
            "14",
            "0.175",
            "0",
        ),
        (
            "gpt-5.2-codex-high",
            "GPT-5.2 Codex",
            "1.75",
            "14",
            "0.175",
            "0",
        ),
        (
            "gpt-5.2-codex-xhigh",
            "GPT-5.2 Codex",
            "1.75",
            "14",
            "0.175",
            "0",
        ),
        ("gpt-5.3-codex", "GPT-5.3 Codex", "1.75", "14", "0.175", "0"),
        (
            "gpt-5.3-codex-low",
            "GPT-5.3 Codex",
            "1.75",
            "14",
            "0.175",
            "0",
        ),
        (
            "gpt-5.3-codex-medium",
            "GPT-5.3 Codex",
            "1.75",
            "14",
            "0.175",
            "0",
        ),
        (
            "gpt-5.3-codex-high",
            "GPT-5.3 Codex",
            "1.75",
            "14",
            "0.175",
            "0",
        ),
        (
            "gpt-5.3-codex-xhigh",
            "GPT-5.3 Codex",
            "1.75",
            "14",
            "0.175",
            "0",
        ),
        ("gpt-5.1", "GPT-5.1", "1.25", "10", "0.125", "0"),
        ("gpt-5.1-low", "GPT-5.1", "1.25", "10", "0.125", "0"),
        ("gpt-5.1-medium", "GPT-5.1", "1.25", "10", "0.125", "0"),
        ("gpt-5.1-high", "GPT-5.1", "1.25", "10", "0.125", "0"),
        ("gpt-5.1-minimal", "GPT-5.1", "1.25", "10", "0.125", "0"),
        ("gpt-5.1-codex", "GPT-5.1 Codex", "1.25", "10", "0.125", "0"),
        (
            "gpt-5.1-codex-mini",
            "GPT-5.1 Codex",
            "1.25",
            "10",
            "0.125",
            "0",
        ),
        (
            "gpt-5.1-codex-max",
            "GPT-5.1 Codex",
            "1.25",
            "10",
            "0.125",
            "0",
        ),
        (
            "gpt-5.1-codex-max-high",
            "GPT-5.1 Codex",
            "1.25",
            "10",
            "0.125",
            "0",
        ),
        (
            "gpt-5.1-codex-max-xhigh",
            "GPT-5.1 Codex",
            "1.25",
            "10",
            "0.125",
            "0",
        ),
        ("gpt-5", "GPT-5", "1.25", "10", "0.125", "0"),
        ("gpt-5-low", "GPT-5", "1.25", "10", "0.125", "0"),
        ("gpt-5-medium", "GPT-5", "1.25", "10", "0.125", "0"),
        ("gpt-5-high", "GPT-5", "1.25", "10", "0.125", "0"),
        ("gpt-5-minimal", "GPT-5", "1.25", "10", "0.125", "0"),
        ("gpt-5-codex", "GPT-5 Codex", "1.25", "10", "0.125", "0"),
        ("gpt-5-codex-low", "GPT-5 Codex", "1.25", "10", "0.125", "0"),
        (
            "gpt-5-codex-medium",
            "GPT-5 Codex",
            "1.25",
            "10",
            "0.125",
            "0",
        ),
        (
            "gpt-5-codex-high",
            "GPT-5 Codex",
            "1.25",
            "10",
            "0.125",
            "0",
        ),
        (
            "gpt-5-codex-mini",
            "GPT-5 Codex",
            "1.25",
            "10",
            "0.125",
            "0",
        ),
        (
            "gpt-5-codex-mini-medium",
            "GPT-5 Codex",
            "1.25",
            "10",
            "0.125",
            "0",
        ),
        (
            "gpt-5-codex-mini-high",
            "GPT-5 Codex",
            "1.25",
            "10",
            "0.125",
            "0",
        ),
        ("o3", "OpenAI o3", "2", "8", "0.50", "0"),
        ("o4-mini", "OpenAI o4-mini", "1.10", "4.40", "0.275", "0"),
        ("gpt-4.1", "GPT-4.1", "2", "8", "0.50", "0"),
        ("gpt-4.1-mini", "GPT-4.1 Mini", "0.40", "1.60", "0.10", "0"),
        ("gpt-4.1-nano", "GPT-4.1 Nano", "0.10", "0.40", "0.025", "0"),
        (
            "gemini-3.5-flash",
            "Gemini 3.5 Flash",
            "1.50",
            "9.00",
            "0.15",
            "0",
        ),
        (
            "gemini-3.1-pro-preview",
            "Gemini 3.1 Pro Preview",
            "2",
            "12",
            "0.20",
            "0",
        ),
        (
            "gemini-3.1-flash-lite",
            "Gemini 3.1 Flash Lite",
            "0.25",
            "1.50",
            "0.025",
            "0",
        ),
        (
            "gemini-3.1-flash-lite-preview",
            "Gemini 3.1 Flash Lite Preview",
            "0.25",
            "1.50",
            "0.025",
            "0",
        ),
        (
            "gemini-3-pro-preview",
            "Gemini 3 Pro Preview",
            "2",
            "12",
            "0.2",
            "0",
        ),
        (
            "gemini-3-flash-preview",
            "Gemini 3 Flash Preview",
            "0.5",
            "3",
            "0.05",
            "0",
        ),
        (
            "gemini-2.5-pro",
            "Gemini 2.5 Pro",
            "1.25",
            "10",
            "0.125",
            "0",
        ),
        (
            "gemini-2.5-flash",
            "Gemini 2.5 Flash",
            "0.3",
            "2.5",
            "0.03",
            "0",
        ),
        (
            "gemini-2.5-flash-lite",
            "Gemini 2.5 Flash Lite",
            "0.10",
            "0.40",
            "0.01",
            "0",
        ),
        (
            "gemini-2.0-flash",
            "Gemini 2.0 Flash",
            "0.10",
            "0.40",
            "0.025",
            "0",
        ),
        (
            "step-3.7-flash",
            "Step 3.7 Flash",
            "0.19",
            "1.13",
            "0.04",
            "0",
        ),
        (
            "step-3.5-flash",
            "Step 3.5 Flash",
            "0.10",
            "0.30",
            "0.02",
            "0",
        ),
        (
            "step-3.5-flash-2603",
            "Step 3.5 Flash 2603",
            "0.10",
            "0.30",
            "0.02",
            "0",
        ),
        (
            "doubao-seed-2-1-pro",
            "Doubao Seed 2.1 Pro",
            "0.84",
            "4.2",
            "0.17",
            "0",
        ),
        (
            "doubao-seed-2-1-turbo",
            "Doubao Seed 2.1 Turbo",
            "0.42",
            "2.1",
            "0.08",
            "0",
        ),
        (
            "doubao-seed-code",
            "Doubao Seed Code",
            "0.17",
            "1.11",
            "0.02",
            "0",
        ),
        (
            "doubao-seed-2-0-pro",
            "Doubao Seed 2.0 Pro",
            "0.47",
            "2.37",
            "0.09",
            "0",
        ),
        (
            "doubao-seed-2-0-code",
            "Doubao Seed 2.0 Code",
            "0.47",
            "2.37",
            "0.09",
            "0",
        ),
        (
            "doubao-seed-2-0-code-preview-latest",
            "Doubao Seed 2.0 Code Preview",
            "0.47",
            "2.37",
            "0.09",
            "0",
        ),
        (
            "doubao-seed-2-0-lite",
            "Doubao Seed 2.0 Lite",
            "0.08",
            "0.50",
            "0.017",
            "0",
        ),
        (
            "doubao-seed-2-0-mini",
            "Doubao Seed 2.0 Mini",
            "0.03",
            "0.31",
            "0.0056",
            "0",
        ),
        (
            "deepseek-v3.2",
            "DeepSeek V3.2",
            "0.28",
            "0.42",
            "0.028",
            "0",
        ),
        (
            "deepseek-v3.1",
            "DeepSeek V3.1",
            "0.55",
            "1.67",
            "0.055",
            "0",
        ),
        ("deepseek-v3", "DeepSeek V3", "0.28", "1.11", "0.028", "0"),
        (
            "deepseek-chat",
            "DeepSeek Chat",
            "0.27",
            "1.10",
            "0.07",
            "0",
        ),
        (
            "deepseek-reasoner",
            "DeepSeek Reasoner",
            "0.55",
            "2.19",
            "0.14",
            "0",
        ),
        (
            "deepseek-v4-flash",
            "DeepSeek V4 Flash",
            "0.14",
            "0.28",
            "0.0028",
            "0",
        ),
        (
            "deepseek-v4-pro",
            "DeepSeek V4 Pro",
            "0.435",
            "0.87",
            "0.003625",
            "0",
        ),
        (
            "kimi-k2-thinking",
            "Kimi K2 Thinking",
            "0.55",
            "2.20",
            "0.10",
            "0",
        ),
        ("kimi-k2-0905", "Kimi K2", "0.55", "2.20", "0.10", "0"),
        (
            "kimi-k2-turbo",
            "Kimi K2 Turbo",
            "1.11",
            "8.06",
            "0.14",
            "0",
        ),
        ("kimi-k2.5", "Kimi K2.5", "0.60", "3.00", "0.10", "0"),
        ("kimi-k2.6", "Kimi K2.6", "0.95", "4.00", "0.16", "0"),
        (
            "kimi-k2.7-code",
            "Kimi K2.7 Code",
            "0.95",
            "4.00",
            "0.19",
            "0",
        ),
        ("minimax-m2.1", "MiniMax M2.1", "0.27", "0.95", "0.03", "0"),
        (
            "minimax-m2.1-lightning",
            "MiniMax M2.1 Lightning",
            "0.27",
            "2.33",
            "0.03",
            "0",
        ),
        ("minimax-m2", "MiniMax M2", "0.27", "0.95", "0.03", "0"),
        ("minimax-m2.5", "MiniMax M2.5", "0.15", "0.95", "0.03", "0"),
        (
            "minimax-m2.5-lightning",
            "MiniMax M2.5 Lightning",
            "0.30",
            "2.40",
            "0.03",
            "0",
        ),
        (
            "minimax-m2.7",
            "MiniMax M2.7",
            "0.30",
            "1.20",
            "0.06",
            "0.375",
        ),
        (
            "minimax-m2.7-highspeed",
            "MiniMax M2.7 Highspeed",
            "0.60",
            "2.40",
            "0.06",
            "0.375",
        ),
        ("minimax-m3", "MiniMax M3", "0.60", "2.40", "0.12", "0"),
        ("glm-4.7", "GLM-4.7", "0.6", "2.2", "0.11", "0"),
        ("glm-4.6", "GLM-4.6", "0.6", "2.2", "0.11", "0"),
        ("glm-5", "GLM-5", "1", "3.2", "0.2", "0"),
        ("glm-5.1", "GLM-5.1", "1.4", "4.4", "0.26", "0"),
        ("glm-5.2", "GLM-5.2", "1.4", "4.4", "0.26", "0"),
        (
            "mimo-v2-flash",
            "MiMo V2 Flash",
            "0.09",
            "0.29",
            "0.009",
            "0",
        ),
        ("mimo-v2-pro", "MiMo V2 Pro", "0.435", "0.87", "0.0036", "0"),
        ("mimo-v2.5", "MiMo V2.5", "0.14", "0.29", "0.0028", "0"),
        (
            "mimo-v2.5-pro",
            "MiMo V2.5 Pro",
            "0.435",
            "0.87",
            "0.0036",
            "0",
        ),
        ("qwen3.7-max", "Qwen3.7 Max", "2.50", "7.50", "0.25", "0"),
        ("qwen3.7-plus", "Qwen3.7 Plus", "0.40", "1.60", "0.08", "0"),
        (
            "qwen3.6-plus",
            "Qwen3.6 Plus",
            "0.325",
            "1.95",
            "0.065",
            "0",
        ),
        ("qwen3.5-plus", "Qwen3.5 Plus", "0.26", "1.56", "0.052", "0"),
        ("qwen3-max", "Qwen3 Max", "0.78", "3.90", "0", "0"),
        (
            "qwen3-235b-a22b",
            "Qwen3 235B-A22B",
            "0.70",
            "8.40",
            "0",
            "0",
        ),
        (
            "qwen3-coder-plus",
            "Qwen3 Coder Plus",
            "0.65",
            "3.25",
            "0.13",
            "0",
        ),
        (
            "qwen3-coder-480b",
            "Qwen3 Coder 480B",
            "0.65",
            "3.25",
            "0",
            "0",
        ),
        (
            "qwen3-coder-480b-a35b-instruct",
            "Qwen3 Coder 480B-A35B Instruct",
            "0.65",
            "3.25",
            "0",
            "0",
        ),
        (
            "qwen3-coder-flash",
            "Qwen3 Coder Flash",
            "0.195",
            "0.975",
            "0.039",
            "0",
        ),
        (
            "qwen3-coder-next",
            "Qwen3 Coder Next",
            "0.12",
            "0.75",
            "0",
            "0",
        ),
        ("qwq-plus", "QwQ Plus", "0.80", "2.40", "0", "0"),
        ("qwq-32b", "QwQ 32B", "0.20", "0.60", "0", "0"),
        ("qwen3-32b", "Qwen3 32B", "0.16", "0.64", "0", "0"),
        ("grok-4.3", "Grok 4.3", "1.25", "2.50", "0.20", "0"),
        (
            "grok-4.20-0309-reasoning",
            "Grok 4.20 Reasoning",
            "1.25",
            "2.50",
            "0.20",
            "0",
        ),
        (
            "grok-4.20-0309-non-reasoning",
            "Grok 4.20",
            "1.25",
            "2.50",
            "0.20",
            "0",
        ),
        (
            "grok-4-1-fast-reasoning",
            "Grok 4.1 Fast Reasoning",
            "0.20",
            "0.50",
            "0.05",
            "0",
        ),
        (
            "grok-4-1-fast-non-reasoning",
            "Grok 4.1 Fast",
            "0.20",
            "0.50",
            "0.05",
            "0",
        ),
        ("grok-4", "Grok 4", "3", "15", "0.75", "0"),
        (
            "grok-code-fast-1",
            "Grok Build 0.1 (Code Fast Alias)",
            "1",
            "2",
            "0.20",
            "0",
        ),
        ("grok-build-0.1", "Grok Build 0.1", "1", "2", "0.20", "0"),
        ("grok-3", "Grok 3", "3", "15", "0.75", "0"),
        ("grok-3-mini", "Grok 3 Mini", "0.25", "0.50", "0.075", "0"),
        (
            "mistral-medium-3.5",
            "Mistral Medium 3.5",
            "1.50",
            "7.50",
            "0",
            "0",
        ),
        (
            "mistral-small-4",
            "Mistral Small 4",
            "0.10",
            "0.30",
            "0.01",
            "0",
        ),
        (
            "devstral-small-2-2512",
            "Devstral Small 2",
            "0.10",
            "0.30",
            "0.01",
            "0",
        ),
        (
            "magistral-small",
            "Magistral Small",
            "0.50",
            "1.50",
            "0",
            "0",
        ),
        ("codestral-2508", "Codestral", "0.30", "0.90", "0.03", "0"),
        (
            "devstral-small-1.1",
            "Devstral Small 1.1",
            "0.07",
            "0.28",
            "0.01",
            "0",
        ),
        ("devstral-2-2512", "Devstral 2", "0.40", "2", "0.04", "0"),
        (
            "devstral-medium",
            "Devstral Medium",
            "0.40",
            "2",
            "0.04",
            "0",
        ),
        (
            "mistral-large-3-2512",
            "Mistral Large 3",
            "0.50",
            "1.50",
            "0.05",
            "0",
        ),
        (
            "mistral-medium-3.1",
            "Mistral Medium 3.1",
            "0.40",
            "2",
            "0.04",
            "0",
        ),
        (
            "mistral-small-3.2-24b",
            "Mistral Small 3.2",
            "0.075",
            "0.20",
            "0.01",
            "0",
        ),
        ("magistral-medium", "Magistral Medium", "2", "5", "0", "0"),
        ("command-a", "Cohere Command A", "2.50", "10", "0", "0"),
        (
            "command-r-plus",
            "Cohere Command R+",
            "2.50",
            "10",
            "0",
            "0",
        ),
        ("command-r", "Cohere Command R", "0.15", "0.60", "0", "0"),
        ("o3-pro", "OpenAI o3-pro", "20", "80", "0", "0"),
        ("o3-mini", "OpenAI o3-mini", "0.55", "2.20", "0.55", "0"),
        ("o1", "OpenAI o1", "15", "60", "7.50", "0"),
        ("o1-mini", "OpenAI o1-mini", "0.55", "2.20", "0.55", "0"),
        ("codex-mini", "Codex Mini", "0.75", "3", "0.025", "0"),
        ("gpt-5-mini", "GPT-5 Mini", "0.25", "2", "0.025", "0"),
        ("gpt-5-nano", "GPT-5 Nano", "0.05", "0.40", "0.005", "0"),
    ]
    .into_iter()
    .map(
        |(model_id, display_name, input, output, cache_read, cache_creation)| CodexUsagePricing {
            model_id: model_id.to_string(),
            display_name: display_name.to_string(),
            input_cost_per_million: input.to_string(),
            output_cost_per_million: output.to_string(),
            cache_read_cost_per_million: cache_read.to_string(),
            cache_creation_cost_per_million: cache_creation.to_string(),
        },
    )
    .filter(|item| is_supported_pricing_model(&item.model_id))
    .collect()
}

fn model_pricing_candidates(model_id: &str) -> Vec<String> {
    let cleaned = clean_model_id_for_pricing(model_id);
    if cleaned.is_empty() || matches!(cleaned.as_str(), "unknown" | "null" | "none") {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let mut queue = vec![cleaned];
    while let Some(candidate) = queue.pop() {
        if candidate.is_empty() || candidates.iter().any(|item| item == &candidate) {
            continue;
        }
        candidates.push(candidate.clone());
        if let Some(stripped) = strip_known_model_namespace(&candidate) {
            queue.push(stripped);
        }
        if let Some(stripped) = strip_claude_desktop_non_anthropic_prefix(&candidate) {
            queue.push(stripped);
        }
        if let Some(stripped) = strip_model_date_suffix(&candidate) {
            queue.push(stripped);
        }
        if let Some(stripped) = strip_reasoning_effort_suffix(&candidate) {
            queue.push(stripped);
        }
        if candidate.starts_with("claude-") && candidate.contains('.') {
            queue.push(candidate.replace('.', "-"));
        }
    }
    candidates
}

fn clean_model_id_for_pricing(model_id: &str) -> String {
    let cleaned = model_id
        .rsplit_once('/')
        .map_or(model_id, |(_, rest)| rest)
        .split(':')
        .next()
        .unwrap_or(model_id)
        .trim()
        .replace([' ', '_'], "-")
        .replace('@', "-")
        .to_ascii_lowercase();
    normalize_gpt_56_display_model(&cleaned).unwrap_or(cleaned)
}

fn strip_known_model_namespace(model_id: &str) -> Option<String> {
    if let Some(pos) = model_id.rfind("claude-") {
        if pos > 0 {
            return Some(model_id[pos..].to_string());
        }
    }
    for marker in [
        "openai.",
        "anthropic.",
        "google.",
        "moonshot.",
        "moonshotai.",
        "bedrock.",
        "global.",
    ] {
        if let Some(stripped) = model_id.strip_prefix(marker) {
            return Some(stripped.to_string());
        }
    }
    None
}

fn strip_claude_desktop_non_anthropic_prefix(model_id: &str) -> Option<String> {
    const MARKERS: &[&str] = &[
        "codex", "deepseek", "gemini", "glm", "gpt", "grok", "kimi", "minimax", "mistral",
        "moonshot", "openai", "qwen",
    ];
    let rest = model_id.strip_prefix("claude-")?;
    MARKERS
        .iter()
        .any(|marker| rest.starts_with(marker))
        .then(|| rest.to_string())
}

fn strip_model_date_suffix(model_id: &str) -> Option<String> {
    let bytes = model_id.as_bytes();
    if bytes.len() > 11 {
        let start = bytes.len() - 11;
        let suffix = &bytes[start..];
        if suffix[0] == b'-'
            && suffix[1..5].iter().all(|b| b.is_ascii_digit())
            && suffix[5] == b'-'
            && suffix[6..8].iter().all(|b| b.is_ascii_digit())
            && suffix[8] == b'-'
            && suffix[9..11].iter().all(|b| b.is_ascii_digit())
        {
            return Some(model_id[..start].to_string());
        }
    }
    let (base, suffix) = model_id.rsplit_once('-')?;
    (!base.is_empty() && suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_digit()))
        .then(|| base.to_string())
}

fn strip_reasoning_effort_suffix(model_id: &str) -> Option<String> {
    for suffix in ["-minimal", "-low", "-medium", "-high", "-xhigh"] {
        if let Some(stripped) = model_id.strip_suffix(suffix) {
            if !stripped.is_empty() {
                return Some(stripped.to_string());
            }
        }
    }
    None
}

fn should_try_pricing_prefix_match(model_id: &str) -> bool {
    let dash_count = model_id.matches('-').count();
    if model_id.starts_with("claude-") {
        return dash_count >= 3;
    }
    if ["o1", "o3", "o4", "o5"]
        .iter()
        .any(|prefix| model_id.starts_with(prefix))
    {
        return dash_count >= 1;
    }
    [
        "gpt-",
        "gemini-",
        "deepseek-",
        "qwen-",
        "glm-",
        "kimi-",
        "minimax-",
    ]
    .iter()
    .any(|prefix| model_id.starts_with(prefix))
        && dash_count >= 2
}

fn fresh_input_tokens(log: &ParsedUsageLog) -> u64 {
    log.input_tokens.saturating_sub(log.cache_read_tokens)
}

fn normalize_codex_model(raw: &str) -> String {
    let mut name = raw.trim().to_lowercase();
    if let Some(position) = name.rfind('/') {
        name = name[position + 1..].to_string();
    }
    if name.len() > 11 {
        let suffix = &name[name.len() - 11..];
        if suffix.as_bytes().first() == Some(&b'-')
            && suffix[1..5].chars().all(|c| c.is_ascii_digit())
            && suffix.as_bytes().get(5) == Some(&b'-')
            && suffix[6..8].chars().all(|c| c.is_ascii_digit())
            && suffix.as_bytes().get(8) == Some(&b'-')
            && suffix[9..11].chars().all(|c| c.is_ascii_digit())
        {
            name.truncate(name.len() - 11);
        }
    }
    if name.len() > 9 {
        let parts = name.rsplitn(2, '-').collect::<Vec<_>>();
        if parts.len() == 2 && parts[0].len() == 8 && parts[0].chars().all(|c| c.is_ascii_digit()) {
            name = parts[1].to_string();
        }
    }
    let name = name.replace([' ', '_'], "-").replace('@', "-");
    normalize_gpt_56_display_model(&name).unwrap_or(name)
}

fn normalize_gpt_56_display_model(model_id: &str) -> Option<String> {
    match model_id.trim().to_ascii_lowercase().as_str() {
        "5.6-sol" | "gpt-5.6-sol" => Some("gpt-5.6-sol".to_string()),
        "5.6-terra" | "gpt-5.6-terra" => Some("gpt-5.6-terra".to_string()),
        "5.6-luna" | "gpt-5.6-luna" => Some("gpt-5.6-luna".to_string()),
        _ => None,
    }
}

fn parse_event_timestamp(value: &Value) -> Option<i64> {
    value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_rfc3339_timestamp)
}

fn parse_rfc3339_timestamp(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|datetime| datetime.timestamp())
}

fn task_started_at(payload: &Value) -> Option<i64> {
    let value = payload.get("started_at")?;
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(parse_rfc3339_timestamp))
}

fn token_usage_fingerprint(
    total: Option<&CumulativeTokens>,
    last: Option<&CumulativeTokens>,
) -> String {
    fn part(value: Option<&CumulativeTokens>) -> String {
        value
            .map(|value| format!("{}:{}:{}", value.input, value.cached_input, value.output))
            .unwrap_or_else(|| "-".to_string())
    }
    format!("{}|{}", part(total), part(last))
}

fn rollout_identity_from_path(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn rollout_filename_timestamp(path: &Path) -> Option<i64> {
    rollout_filename_timestamp_in_timezone(path, &Local)
}

fn rollout_filename_timestamp_in_timezone<Tz: TimeZone>(path: &Path, timezone: &Tz) -> Option<i64> {
    let file_name = path.file_name()?.to_str()?;
    let value = file_name.strip_prefix("rollout-")?.get(..19)?;
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H-%M-%S").ok()?;
    timezone
        .from_local_datetime(&naive)
        .earliest()
        .map(|datetime| datetime.timestamp())
}

fn format_trend_label(timestamp: i64, hourly: bool) -> String {
    let Some(datetime) = Local.timestamp_opt(timestamp, 0).single() else {
        return "--".to_string();
    };
    if hourly {
        datetime.format("%m/%d %H:%M").to_string()
    } else {
        datetime.format("%m/%d").to_string()
    }
}

fn read_u64(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or(0)
}

fn format_cost(value: f64) -> String {
    format!("{:.6}", value.max(0.0))
}

fn default_codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
        .unwrap_or_else(|| PathBuf::from(".codex"))
}

fn switcher_root_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".codex_switcher"))
        .unwrap_or_else(|| PathBuf::from(".codex_switcher"))
}

fn statistics_dir() -> PathBuf {
    switcher_root_dir().join("statistics")
}

fn usage_cache_path() -> PathBuf {
    statistics_dir().join("usage_logs.json")
}

fn usage_db_path() -> PathBuf {
    statistics_dir().join("usage.sqlite")
}

fn pricing_path() -> PathBuf {
    statistics_dir().join("pricing.json")
}

fn pricing_config_path() -> PathBuf {
    statistics_dir().join("pricing_config.json")
}

fn pricing_defaults_version_path() -> PathBuf {
    statistics_dir().join("pricing-defaults.version")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_delta_from_cumulative_usage() {
        let prev = Some(CumulativeTokens {
            input: 100,
            cached_input: 40,
            output: 10,
        });
        let current = CumulativeTokens {
            input: 180,
            cached_input: 70,
            output: 35,
        };
        let delta = compute_delta(&prev, &current);
        assert_eq!(delta.input, 80);
        assert_eq!(delta.cached_input, 30);
        assert_eq!(delta.output, 25);
    }

    fn file_logs(parsed: &ParsedUsageFile) -> Vec<ParsedUsageLog> {
        parsed
            .events
            .iter()
            .filter_map(|event| event.log.clone())
            .collect()
    }

    fn sample_log(
        request_id: &str,
        session_id: &str,
        rollout_id: &str,
        source_kind: UsageLogSourceKind,
        input_tokens: u64,
    ) -> ParsedUsageLog {
        ParsedUsageLog {
            request_id: request_id.to_string(),
            session_id: session_id.to_string(),
            rollout_id: rollout_id.to_string(),
            model: "gpt-5.5".to_string(),
            input_tokens,
            output_tokens: 10,
            cache_read_tokens: 20,
            created_at: 1_782_662_400,
            source_kind,
        }
    }

    fn usage_meta(
        timestamp: &str,
        session_id: &str,
        rollout_id: &str,
        parent: Option<&str>,
    ) -> String {
        let mut payload = serde_json::json!({
            "session_id": session_id,
            "id": rollout_id,
        });
        if let Some(parent) = parent {
            payload["forked_from_id"] = Value::String(parent.to_string());
        }
        serde_json::json!({
            "timestamp": timestamp,
            "type": "session_meta",
            "payload": payload,
        })
        .to_string()
    }

    fn token_event(
        timestamp: Option<&str>,
        total: (u64, u64, u64),
        last: (u64, u64, u64),
    ) -> String {
        let mut value = serde_json::json!({
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": total.0,
                        "cached_input_tokens": total.1,
                        "output_tokens": total.2,
                    },
                    "last_token_usage": {
                        "input_tokens": last.0,
                        "cached_input_tokens": last.1,
                        "output_tokens": last.2,
                    },
                },
            },
        });
        if let Some(timestamp) = timestamp {
            value["timestamp"] = Value::String(timestamp.to_string());
        }
        value.to_string()
    }

    fn task_started(timestamp: &str, started_at: i64) -> String {
        serde_json::json!({
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": { "type": "task_started", "started_at": started_at },
        })
        .to_string()
    }

    #[test]
    fn parses_codex_token_count_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("session.jsonl");
        fs::write(
            &file,
            [
                r#"{"type":"session_meta","payload":{"id":"s1"}}"#,
                r#"{"type":"turn_context","payload":{"model":"OpenAI/GPT-5.5-2026-05-14"}}"#,
                r#"{"type":"event_msg","timestamp":"2026-06-29T01:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":5}}}}"#,
                r#"{"type":"event_msg","timestamp":"2026-06-29T01:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":140,"cached_input_tokens":30,"output_tokens":10}}}}"#,
            ]
            .join("\n"),
        )
        .expect("write session");
        let parsed = parse_codex_usage_file(&file).expect("parse");
        let logs = file_logs(&parsed);
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].request_id, "codex_rollout:s1:1");
        assert_eq!(logs[0].session_id, "s1");
        assert_eq!(logs[0].rollout_id, "s1");
        assert_eq!(logs[0].source_kind, UsageLogSourceKind::CanonicalRoot);
        assert_eq!(logs[0].model, "gpt-5.5");
        assert_eq!(logs[0].input_tokens, 100);
        assert_eq!(logs[0].cache_read_tokens, 20);
        assert_eq!(logs[1].input_tokens, 40);
        assert_eq!(logs[1].cache_read_tokens, 10);
    }

    #[test]
    fn parses_first_fork_event_from_last_usage() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("fork.jsonl");
        fs::write(
            &file,
            [
                r#"{"type":"session_meta","payload":{"session_id":"shared","id":"fork"}}"#,
                r#"{"type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
                r#"{"type":"event_msg","timestamp":"2026-07-11T01:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":800,"output_tokens":100},"last_token_usage":{"input_tokens":20,"cached_input_tokens":10,"output_tokens":5}}}}"#,
                r#"{"type":"event_msg","timestamp":"2026-07-11T01:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1030,"cached_input_tokens":815,"output_tokens":107},"last_token_usage":{"input_tokens":30,"cached_input_tokens":15,"output_tokens":7}}}}"#,
            ]
            .join("\n"),
        )
        .expect("write fork");

        let parsed = parse_codex_usage_file(&file).expect("parse fork");
        let logs = file_logs(&parsed);

        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].input_tokens, 20);
        assert_eq!(logs[0].cache_read_tokens, 10);
        assert_eq!(logs[0].output_tokens, 5);
        assert_eq!(logs[1].input_tokens, 30);
        assert_eq!(logs[1].cache_read_tokens, 15);
        assert_eq!(logs[1].output_tokens, 7);
    }

    #[test]
    fn root_first_event_uses_total_usage_even_when_last_is_present() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("root.jsonl");
        fs::write(
            &file,
            [
                r#"{"type":"session_meta","payload":{"session_id":"root","id":"root"}}"#,
                r#"{"type":"event_msg","timestamp":"2026-07-11T01:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":10},"last_token_usage":{"input_tokens":20,"cached_input_tokens":5,"output_tokens":2}}}}"#,
            ]
            .join("\n"),
        )
        .expect("write root");

        let parsed = parse_codex_usage_file(&file).expect("parse root");

        let logs = file_logs(&parsed);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].source_kind, UsageLogSourceKind::CanonicalRoot);
        assert_eq!(logs[0].input_tokens, 100);
        assert_eq!(logs[0].cache_read_tokens, 40);
        assert_eq!(logs[0].output_tokens, 10);
    }

    #[test]
    fn canonical_root_precedes_earlier_named_fork_and_fork_extends_coverage() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fork_file = temp.path().join("rollout-01-fork.jsonl");
        let root_file = temp.path().join("rollout-02-root.jsonl");
        fs::write(
            &root_file,
            [
                r#"{"type":"session_meta","payload":{"session_id":"shared","id":"shared"}}"#,
                r#"{"type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
                r#"{"type":"event_msg","timestamp":"2026-07-11T01:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":5},"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":5}}}}"#,
                r#"{"type":"event_msg","timestamp":"2026-07-11T01:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":150,"cached_input_tokens":30,"output_tokens":10},"last_token_usage":{"input_tokens":50,"cached_input_tokens":10,"output_tokens":5}}}}"#,
            ]
            .join("\n"),
        )
        .expect("write root");
        fs::write(
            &fork_file,
            [
                r#"{"timestamp":"2026-07-11T02:00:00Z","type":"session_meta","payload":{"session_id":"shared","id":"fork","forked_from_id":"shared"}}"#,
                r#"{"type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
                r#"{"timestamp":"2026-07-11T02:00:00Z","type":"event_msg","payload":{"type":"task_started","started_at":1783735200}}"#,
                r#"{"type":"event_msg","timestamp":"2026-07-11T02:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":800,"output_tokens":100},"last_token_usage":{"input_tokens":25,"cached_input_tokens":15,"output_tokens":5}}}}"#,
                r#"{"type":"event_msg","timestamp":"2026-07-11T02:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1040,"cached_input_tokens":820,"output_tokens":108},"last_token_usage":{"input_tokens":40,"cached_input_tokens":20,"output_tokens":8}}}}"#,
                r#"{"type":"event_msg","timestamp":"2026-07-11T02:02:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1060,"cached_input_tokens":830,"output_tokens":112},"last_token_usage":{"input_tokens":20,"cached_input_tokens":10,"output_tokens":4}}}}"#,
            ]
            .join("\n"),
        )
        .expect("write fork");

        let files = vec![fork_file, root_file];
        let (current, _, errors) = parse_codex_usage_files(&files);

        assert!(errors.is_empty());
        assert_eq!(current.len(), 5);
        assert_eq!(current[0].request_id, "codex_rollout:shared:1");
        assert_eq!(current[0].model, "gpt-5.5");
        assert_eq!(current[0].input_tokens, 100);
        assert_eq!(current[0].created_at, 1_783_731_600);
        assert_eq!(current[1].input_tokens, 50);
        assert_eq!(current[2].request_id, "codex_rollout:fork:1");
        assert_eq!(current[2].model, "gpt-5.6-sol");
        assert_eq!(current[2].input_tokens, 25);

        let mut conn = Connection::open_in_memory().expect("open sqlite");
        init_usage_db(&conn).expect("init sqlite");
        let cache = UsageCache {
            version: USAGE_CACHE_VERSION,
            updated_at: 1_784_000_000,
            files: Vec::new(),
            logs: current,
            errors: Vec::new(),
        };
        write_usage_cache_db(&mut conn, &cache).expect("write cache");
        let stored = read_usage_logs_db(&conn).expect("read cache");
        assert_eq!(stored.len(), 5);
        assert_eq!(stored[0].model, "gpt-5.5");
        assert_eq!(stored[0].input_tokens, 100);
        assert_eq!(stored[1].input_tokens, 50);
        assert_eq!(stored[2].model, "gpt-5.6-sol");
    }

    #[test]
    fn id_only_root_precedes_earlier_named_fork() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fork_file = temp.path().join("rollout-01-fork.jsonl");
        let root_file = temp.path().join("rollout-02-root.jsonl");
        fs::write(
            &fork_file,
            [
                r#"{"timestamp":"2026-07-11T02:00:00Z","type":"session_meta","payload":{"session_id":"legacy","id":"fork","forked_from_id":"legacy"}}"#,
                r#"{"type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
                r#"{"timestamp":"2026-07-11T02:00:00Z","type":"event_msg","payload":{"type":"task_started","started_at":1783735200}}"#,
                r#"{"type":"event_msg","timestamp":"2026-07-11T02:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":800,"output_tokens":100},"last_token_usage":{"input_tokens":20,"cached_input_tokens":10,"output_tokens":5}}}}"#,
            ]
            .join("\n"),
        )
        .expect("write fork");
        fs::write(
            &root_file,
            [
                r#"{"type":"session_meta","payload":{"id":"legacy"}}"#,
                r#"{"type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
                r#"{"type":"event_msg","timestamp":"2026-07-11T01:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":5}}}}"#,
            ]
            .join("\n"),
        )
        .expect("write root");

        let (current, _, errors) = parse_codex_usage_files(&[fork_file, root_file]);

        assert!(errors.is_empty());
        assert_eq!(current.len(), 2);
        assert_eq!(current[0].request_id, "codex_rollout:legacy:1");
        assert_eq!(current[0].model, "gpt-5.5");
        assert_eq!(current[0].input_tokens, 100);
        assert_eq!(current[0].created_at, 1_783_731_600);
    }

    #[test]
    fn legacy_migration_drops_unverifiable_previous_only_rows() {
        let historical = sample_log(
            "codex_session:deleted:1",
            "deleted",
            "",
            UsageLogSourceKind::Legacy,
            100,
        );
        let updated = sample_log(
            "codex_rollout:active:1",
            "active",
            "active",
            UsageLogSourceKind::CanonicalRoot,
            200,
        );

        let logs = merge_usage_logs(
            Some(vec![historical]),
            Some(USAGE_CACHE_VERSION - 1),
            vec![updated.clone()],
            &HashSet::from(["active".to_string()]),
        );

        assert_eq!(logs, vec![updated]);
    }

    #[test]
    fn v2_polluted_record_is_replaced_by_current_root() {
        let stale = sample_log(
            "codex_session:active:1",
            "active",
            "",
            UsageLogSourceKind::Legacy,
            100,
        );
        let fresh = sample_log(
            "codex_rollout:active:1",
            "active",
            "active",
            UsageLogSourceKind::CanonicalRoot,
            200,
        );

        let logs = merge_usage_logs(
            Some(vec![stale]),
            Some(USAGE_CACHE_VERSION - 1),
            vec![fresh],
            &HashSet::from(["active".to_string()]),
        );

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].input_tokens, 200);
        assert_eq!(logs[0].cache_read_tokens, 20);
    }

    #[test]
    fn current_rollout_replaces_stale_v4_rows_but_missing_rollouts_survive() {
        let stale = sample_log(
            "codex_rollout:active:2",
            "session",
            "active",
            UsageLogSourceKind::Fork,
            25,
        );
        let missing = sample_log(
            "codex_rollout:rotated:1",
            "session",
            "rotated",
            UsageLogSourceKind::Fork,
            50,
        );
        let current = sample_log(
            "codex_rollout:active:1",
            "session",
            "active",
            UsageLogSourceKind::CanonicalRoot,
            100,
        );

        let logs = merge_usage_logs(
            Some(vec![stale, missing.clone()]),
            Some(USAGE_CACHE_VERSION),
            vec![current.clone()],
            &HashSet::from(["active".to_string()]),
        );

        assert_eq!(logs.len(), 2);
        assert!(logs.contains(&current));
        assert!(logs.contains(&missing));
        assert!(!logs.iter().any(|log| log.request_id.ends_with(":2")));
    }

    #[test]
    fn sibling_forks_preserve_distinct_tails_independent_of_file_order() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root.jsonl");
        let fork_a = temp.path().join("fork-a.jsonl");
        let fork_b = temp.path().join("fork-b.jsonl");
        let replay_one = token_event(Some("2026-07-11T01:00:00Z"), (100, 20, 5), (100, 20, 5));
        let replay_two = token_event(Some("2026-07-11T01:01:00Z"), (150, 30, 10), (50, 10, 5));
        fs::write(
            &root,
            [
                usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
                replay_one.clone(),
                replay_two.clone(),
                token_event(Some("2026-07-11T01:02:00Z"), (190, 35, 14), (40, 5, 4)),
            ]
            .join("\n"),
        )
        .expect("write root");
        fs::write(
            &fork_a,
            [
                usage_meta("2026-07-11T02:00:00Z", "session", "fork-a", Some("session")),
                replay_one.clone(),
                replay_two.clone(),
                token_event(Some("2026-07-11T02:01:00Z"), (230, 50, 22), (80, 20, 12)),
            ]
            .join("\n"),
        )
        .expect("write fork a");
        fs::write(
            &fork_b,
            [
                usage_meta("2026-07-11T03:00:00Z", "session", "fork-b", Some("session")),
                replay_one,
                replay_two,
                // Same token tuple and raw index as fork-a is still an independent call.
                token_event(Some("2026-07-11T03:01:00Z"), (230, 50, 22), (80, 20, 12)),
                token_event(Some("2026-07-11T03:02:00Z"), (260, 55, 27), (30, 5, 5)),
            ]
            .join("\n"),
        )
        .expect("write fork b");

        let (forward, _, forward_errors) =
            parse_codex_usage_files(&[root.clone(), fork_a.clone(), fork_b.clone()]);
        let (reverse, _, reverse_errors) = parse_codex_usage_files(&[fork_b, fork_a, root]);

        assert!(forward_errors.is_empty());
        assert!(reverse_errors.is_empty());
        assert_eq!(forward, reverse);
        assert_eq!(forward.len(), 6);
        let ids = forward
            .iter()
            .map(|log| log.request_id.as_str())
            .collect::<HashSet<_>>();
        assert!(ids.contains("codex_rollout:session:3"));
        assert!(ids.contains("codex_rollout:fork-a:3"));
        assert!(ids.contains("codex_rollout:fork-b:3"));
        assert!(ids.contains("codex_rollout:fork-b:4"));
    }

    #[test]
    fn fork_replaying_parent_suffix_uses_explicit_boundary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let parent = temp.path().join("parent.jsonl");
        let child = temp.path().join("child.jsonl");
        let first = token_event(None, (100, 20, 5), (100, 20, 5));
        let second = token_event(None, (150, 30, 10), (50, 10, 5));
        let third = token_event(None, (190, 35, 14), (40, 5, 4));
        let session_at = parse_rfc3339_timestamp("2026-07-11T02:00:00Z").unwrap();
        fs::write(
            &parent,
            [
                usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
                first,
                second.clone(),
                third.clone(),
            ]
            .join("\n"),
        )
        .expect("write parent");
        fs::write(
            &child,
            [
                usage_meta("2026-07-11T02:00:00Z", "session", "child", Some("session")),
                second,
                third,
                task_started("2026-07-11T02:00:01Z", session_at),
                token_event(None, (230, 40, 20), (40, 5, 6)),
            ]
            .join("\n"),
        )
        .expect("write child");

        let (logs, _, errors) = parse_codex_usage_files(&[parent, child]);
        assert!(errors.is_empty());
        assert_eq!(logs.len(), 4);
        assert!(logs
            .iter()
            .any(|log| log.request_id == "codex_rollout:child:3"));
        assert!(!logs.iter().any(|log| {
            log.request_id == "codex_rollout:child:1" || log.request_id == "codex_rollout:child:2"
        }));
    }

    #[test]
    fn explicit_boundary_wins_when_lcp_is_zero() {
        let temp = tempfile::tempdir().expect("tempdir");
        let parent = temp.path().join("parent.jsonl");
        let child = temp.path().join("child.jsonl");
        let session_at = parse_rfc3339_timestamp("2026-07-11T02:00:00Z").unwrap();
        fs::write(
            &parent,
            [
                usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
                token_event(None, (100, 20, 5), (100, 20, 5)),
            ]
            .join("\n"),
        )
        .expect("write parent");
        fs::write(
            &child,
            [
                usage_meta("2026-07-11T02:00:00Z", "session", "child", Some("session")),
                token_event(None, (500, 400, 40), (50, 40, 4)),
                task_started("2026-07-11T02:00:01Z", session_at),
                token_event(None, (550, 440, 45), (50, 40, 5)),
            ]
            .join("\n"),
        )
        .expect("write child");

        let (logs, replace_ids, errors) = parse_codex_usage_files(&[parent, child]);
        assert!(errors.is_empty());
        assert!(replace_ids.contains("child"));
        assert!(logs
            .iter()
            .any(|log| log.request_id == "codex_rollout:child:2"));
        assert!(!logs
            .iter()
            .any(|log| log.request_id == "codex_rollout:child:1"));
    }

    #[test]
    fn explicit_boundary_wins_when_lcp_accidentally_extends_past_boundary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let parent = temp.path().join("parent.jsonl");
        let child = temp.path().join("child.jsonl");
        let first = token_event(None, (100, 20, 5), (100, 20, 5));
        let second = token_event(None, (150, 30, 10), (50, 10, 5));
        let coincident = token_event(None, (190, 35, 14), (40, 5, 4));
        let session_at = parse_rfc3339_timestamp("2026-07-11T02:00:00Z").unwrap();
        fs::write(
            &parent,
            [
                usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
                first.clone(),
                second.clone(),
                coincident.clone(),
            ]
            .join("\n"),
        )
        .expect("write parent");
        fs::write(
            &child,
            [
                usage_meta("2026-07-11T02:00:00Z", "session", "child", Some("session")),
                first,
                second,
                task_started("2026-07-11T02:00:01Z", session_at),
                coincident,
            ]
            .join("\n"),
        )
        .expect("write child");

        let (logs, _, errors) = parse_codex_usage_files(&[parent, child]);
        assert!(errors.is_empty());
        assert!(logs
            .iter()
            .any(|log| log.request_id == "codex_rollout:child:3"));
        assert!(!logs.iter().any(|log| {
            log.request_id == "codex_rollout:child:1" || log.request_id == "codex_rollout:child:2"
        }));
    }

    #[test]
    fn no_marker_lcp_zero_preserves_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let parent = temp.path().join("parent.jsonl");
        let child = temp.path().join("child.jsonl");
        fs::write(
            &parent,
            [
                usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
                token_event(None, (100, 20, 5), (100, 20, 5)),
            ]
            .join("\n"),
        )
        .expect("write parent");
        fs::write(
            &child,
            [
                usage_meta("2026-07-11T02:00:00Z", "session", "child", Some("session")),
                token_event(None, (500, 400, 40), (50, 40, 4)),
            ]
            .join("\n"),
        )
        .expect("write child");

        let (current, replace_ids, errors) = parse_codex_usage_files(&[parent, child]);
        assert_eq!(errors.len(), 1);
        assert!(!replace_ids.contains("child"));
        assert!(!current.iter().any(|log| log.rollout_id.as_str() == "child"));
        let previous = sample_log(
            "codex_rollout:child:1",
            "session",
            "child",
            UsageLogSourceKind::Fork,
            75,
        );
        let merged = merge_usage_logs(
            Some(vec![previous.clone()]),
            Some(USAGE_CACHE_VERSION),
            current,
            &replace_ids,
        );
        assert!(merged.contains(&previous));
    }

    #[test]
    fn equal_length_duplicate_parent_selection_is_order_independent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let preferred_parent = temp.path().join("a-parent.jsonl");
        let other_parent = temp.path().join("z-parent.jsonl");
        let child = temp.path().join("child.jsonl");
        let preferred_event = token_event(None, (100, 20, 5), (100, 20, 5));
        fs::write(
            &preferred_parent,
            [
                usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
                preferred_event.clone(),
            ]
            .join("\n"),
        )
        .expect("write preferred parent");
        fs::write(
            &other_parent,
            [
                usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
                token_event(None, (999, 900, 90), (999, 900, 90)),
            ]
            .join("\n"),
        )
        .expect("write other parent");
        fs::write(
            &child,
            [
                usage_meta("2026-07-11T02:00:00Z", "session", "child", Some("session")),
                preferred_event,
                token_event(None, (150, 30, 10), (50, 10, 5)),
            ]
            .join("\n"),
        )
        .expect("write child");

        let (forward, _, forward_errors) = parse_codex_usage_files(&[
            preferred_parent.clone(),
            other_parent.clone(),
            child.clone(),
        ]);
        let (reverse, _, reverse_errors) =
            parse_codex_usage_files(&[child, other_parent, preferred_parent]);
        assert!(forward_errors.is_empty());
        assert!(reverse_errors.is_empty());
        assert_eq!(forward, reverse);
        assert_eq!(forward.len(), 2);
        assert!(forward
            .iter()
            .any(|log| log.request_id == "codex_rollout:child:2"));
    }

    #[test]
    fn nested_fork_replay_is_counted_once_per_rollout_tail() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root.jsonl");
        let parent = temp.path().join("parent.jsonl");
        let child = temp.path().join("child.jsonl");
        let first = token_event(None, (100, 20, 5), (100, 20, 5));
        let second = token_event(None, (150, 30, 10), (50, 10, 5));
        let parent_tail = token_event(None, (225, 45, 20), (75, 15, 10));
        fs::write(
            &root,
            [
                usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
                first.clone(),
                second.clone(),
            ]
            .join("\n"),
        )
        .expect("write root");
        fs::write(
            &parent,
            [
                usage_meta("2026-07-11T02:00:00Z", "session", "parent", Some("session")),
                first.clone(),
                second.clone(),
                parent_tail.clone(),
            ]
            .join("\n"),
        )
        .expect("write parent");
        fs::write(
            &child,
            [
                usage_meta("2026-07-11T03:00:00Z", "session", "child", Some("parent")),
                first,
                second,
                parent_tail,
                token_event(None, (260, 50, 26), (35, 5, 6)),
            ]
            .join("\n"),
        )
        .expect("write child");

        let (logs, _, errors) = parse_codex_usage_files(&[child, root, parent]);
        assert!(errors.is_empty());
        assert_eq!(logs.len(), 4);
        assert!(logs
            .iter()
            .any(|log| log.request_id == "codex_rollout:parent:3"));
        assert!(logs
            .iter()
            .any(|log| log.request_id == "codex_rollout:child:4"));
    }

    #[test]
    fn missing_parent_uses_owned_task_boundary_and_unresolved_cache_survives() {
        let temp = tempfile::tempdir().expect("tempdir");
        let resolved = temp.path().join("resolved.jsonl");
        let unresolved = temp.path().join("unresolved.jsonl");
        let session_at = parse_rfc3339_timestamp("2026-07-11T02:00:00Z").expect("timestamp");
        fs::write(
            &resolved,
            [
                usage_meta(
                    "2026-07-11T02:00:00Z",
                    "session",
                    "resolved",
                    Some("missing"),
                ),
                token_event(None, (100, 20, 5), (100, 20, 5)),
                task_started("2026-07-11T02:00:01Z", session_at),
                token_event(None, (150, 30, 10), (50, 10, 5)),
            ]
            .join("\n"),
        )
        .expect("write resolved");
        fs::write(
            &unresolved,
            [
                usage_meta(
                    "2026-07-11T03:00:00Z",
                    "session",
                    "unresolved",
                    Some("missing"),
                ),
                token_event(None, (100, 20, 5), (100, 20, 5)),
            ]
            .join("\n"),
        )
        .expect("write unresolved");

        let (logs, replace_ids, errors) = parse_codex_usage_files(&[resolved, unresolved]);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].request_id, "codex_rollout:resolved:2");
        assert!(replace_ids.contains("resolved"));
        assert!(!replace_ids.contains("unresolved"));
        assert_eq!(errors.len(), 1);

        let previous = sample_log(
            "codex_rollout:unresolved:1",
            "session",
            "unresolved",
            UsageLogSourceKind::Fork,
            75,
        );
        let merged = merge_usage_logs(
            Some(vec![previous.clone()]),
            Some(USAGE_CACHE_VERSION),
            logs,
            &replace_ids,
        );
        assert!(merged.contains(&previous));
    }

    #[test]
    fn missing_event_timestamps_use_session_then_previous_event_not_file_mtime() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("rollout-2026-07-12T23-59-59-root.jsonl");
        fs::write(
            &file,
            [
                usage_meta("2026-07-10T01:00:00Z", "session", "root", None),
                token_event(None, (100, 20, 5), (100, 20, 5)),
                token_event(Some("2026-07-10T01:05:00Z"), (150, 30, 10), (50, 10, 5)),
                token_event(None, (190, 35, 14), (40, 5, 4)),
            ]
            .join("\n"),
        )
        .expect("write root");

        let logs = file_logs(&parse_codex_usage_file(&file).expect("parse"));
        assert_eq!(
            logs[0].created_at,
            parse_rfc3339_timestamp("2026-07-10T01:00:00Z").unwrap()
        );
        assert_eq!(
            logs[2].created_at,
            parse_rfc3339_timestamp("2026-07-10T01:05:00Z").unwrap()
        );
    }

    #[test]
    fn rollout_filename_local_time_matches_session_meta_utc() {
        let path = Path::new("rollout-2026-07-11T10-47-54-example.jsonl");
        let utc8 = chrono::FixedOffset::east_opt(8 * 60 * 60).unwrap();
        let timestamp = rollout_filename_timestamp_in_timezone(path, &utc8)
            .expect("filename timestamp in UTC+8");
        assert_eq!(
            timestamp,
            parse_rfc3339_timestamp("2026-07-11T02:47:54Z").unwrap()
        );
    }

    #[test]
    fn v3_cached_fork_is_discarded_when_current_root_rebuilds() {
        let polluted = sample_log(
            "codex_session:session:1",
            "session",
            "",
            UsageLogSourceKind::Legacy,
            25,
        );
        let root = sample_log(
            "codex_rollout:root:1",
            "session",
            "root",
            UsageLogSourceKind::CanonicalRoot,
            100,
        );
        let logs = merge_usage_logs(
            Some(vec![polluted]),
            Some(USAGE_CACHE_VERSION - 1),
            vec![root.clone()],
            &HashSet::from(["root".to_string()]),
        );
        assert_eq!(logs, vec![root]);
    }

    #[test]
    fn daily_trends_use_local_calendar_boundaries() {
        let first_day = NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
        let second_day = first_day + ChronoDuration::days(1);
        let start = local_day_start_timestamp(first_day);
        let end = local_day_start_timestamp(second_day + ChronoDuration::days(1)) - 1;
        let mut first = sample_log(
            "codex_rollout:root:1",
            "session",
            "root",
            UsageLogSourceKind::CanonicalRoot,
            100,
        );
        first.created_at = start + 60;
        let mut second = sample_log(
            "codex_rollout:root:2",
            "session",
            "root",
            UsageLogSourceKind::CanonicalRoot,
            200,
        );
        second.created_at = local_day_start_timestamp(second_day) + 60;

        let trends = build_trends(&[first, second], Some(start), Some(end), &[], 1.0);
        assert_eq!(trends.len(), 2);
        assert_eq!(trends[0].timestamp, local_day_start_timestamp(first_day));
        assert_eq!(trends[1].timestamp, local_day_start_timestamp(second_day));
        assert_eq!(trends[0].input_tokens, 80);
        assert_eq!(trends[1].input_tokens, 180);
    }

    #[test]
    fn explicit_boundary_prevents_truncated_parent_from_exposing_replay() {
        let temp = tempfile::tempdir().expect("tempdir");
        let parent = temp.path().join("parent.jsonl");
        let child = temp.path().join("child.jsonl");
        let first = token_event(None, (100, 20, 5), (100, 20, 5));
        let second = token_event(None, (150, 30, 10), (50, 10, 5));
        let session_at = parse_rfc3339_timestamp("2026-07-11T02:00:00Z").unwrap();
        fs::write(
            &parent,
            [
                usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
                first.clone(),
            ]
            .join("\n"),
        )
        .expect("write truncated parent");
        fs::write(
            &child,
            [
                usage_meta("2026-07-11T02:00:00Z", "session", "child", Some("session")),
                first,
                second,
                task_started("2026-07-11T02:00:01Z", session_at),
                token_event(None, (190, 35, 14), (40, 5, 4)),
            ]
            .join("\n"),
        )
        .expect("write child");

        let (logs, _, errors) = parse_codex_usage_files(&[parent, child]);
        assert!(errors.is_empty());
        assert_eq!(logs.len(), 2);
        assert!(logs
            .iter()
            .any(|log| log.request_id == "codex_rollout:child:3"));
        assert!(!logs
            .iter()
            .any(|log| log.request_id == "codex_rollout:child:2"));
    }

    #[test]
    fn later_task_marker_is_not_mistaken_for_initial_fork_boundary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let parent = temp.path().join("parent.jsonl");
        let child = temp.path().join("child.jsonl");
        let first = token_event(None, (100, 20, 5), (100, 20, 5));
        let session_at = parse_rfc3339_timestamp("2026-07-11T02:00:00Z").unwrap();
        fs::write(
            &parent,
            [
                usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
                first.clone(),
            ]
            .join("\n"),
        )
        .expect("write parent");
        fs::write(
            &child,
            [
                usage_meta("2026-07-11T02:00:00Z", "session", "child", Some("session")),
                first,
                token_event(None, (150, 30, 10), (50, 10, 5)),
                task_started("2026-07-11T02:01:00Z", session_at + 60),
                token_event(None, (190, 35, 14), (40, 5, 4)),
            ]
            .join("\n"),
        )
        .expect("write child");

        let (logs, _, errors) = parse_codex_usage_files(&[parent, child]);
        assert!(errors.is_empty());
        assert!(logs
            .iter()
            .any(|log| log.request_id == "codex_rollout:child:2"));
        assert!(logs
            .iter()
            .any(|log| log.request_id == "codex_rollout:child:3"));
    }

    #[test]
    fn empty_current_rollout_does_not_delete_trusted_v4_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("empty.jsonl");
        fs::write(
            &file,
            usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
        )
        .expect("write empty rollout");
        let (current, replace_ids, errors) = parse_codex_usage_files(&[file]);
        assert!(current.is_empty());
        assert!(replace_ids.is_empty());
        assert!(errors.is_empty());

        let previous = sample_log(
            "codex_rollout:session:1",
            "session",
            "session",
            UsageLogSourceKind::CanonicalRoot,
            100,
        );
        let merged = merge_usage_logs(
            Some(vec![previous.clone()]),
            Some(USAGE_CACHE_VERSION),
            current,
            &replace_ids,
        );
        assert_eq!(merged, vec![previous]);
    }

    #[test]
    fn empty_parent_is_not_used_as_evidence_for_child_ownership() {
        let temp = tempfile::tempdir().expect("tempdir");
        let parent = temp.path().join("parent.jsonl");
        let child = temp.path().join("child.jsonl");
        fs::write(
            &parent,
            usage_meta("2026-07-11T01:00:00Z", "session", "session", None),
        )
        .expect("write empty parent");
        fs::write(
            &child,
            [
                usage_meta("2026-07-11T02:00:00Z", "session", "child", Some("session")),
                token_event(None, (100, 20, 5), (100, 20, 5)),
            ]
            .join("\n"),
        )
        .expect("write child replay");

        let (logs, replace_ids, errors) = parse_codex_usage_files(&[parent, child]);
        assert!(logs.is_empty());
        assert!(replace_ids.is_empty());
        assert_eq!(errors.len(), 1);
        let previous = sample_log(
            "codex_rollout:child:2",
            "session",
            "child",
            UsageLogSourceKind::Fork,
            50,
        );
        let merged = merge_usage_logs(
            Some(vec![previous.clone()]),
            Some(USAGE_CACHE_VERSION),
            logs,
            &replace_ids,
        );
        assert_eq!(merged, vec![previous]);
    }

    #[test]
    fn v4_database_never_reimports_changed_legacy_json_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let json_path = temp.path().join("usage_logs.json");
        let mut conn = Connection::open_in_memory().expect("open sqlite");
        init_usage_db(&conn).expect("init sqlite");
        let current = sample_log(
            "codex_rollout:session:1",
            "session",
            "session",
            UsageLogSourceKind::CanonicalRoot,
            100,
        );
        write_usage_cache_db(
            &mut conn,
            &UsageCache {
                version: USAGE_CACHE_VERSION,
                updated_at: 1,
                files: Vec::new(),
                logs: vec![current.clone()],
                errors: Vec::new(),
            },
        )
        .expect("write v4 cache");
        let legacy = sample_log(
            "codex_session:session:1",
            "session",
            "",
            UsageLogSourceKind::Legacy,
            25,
        );
        fs::write(
            &json_path,
            serde_json::to_string(&UsageCache {
                version: USAGE_CACHE_VERSION - 1,
                updated_at: 0,
                files: Vec::new(),
                logs: vec![legacy],
                errors: Vec::new(),
            })
            .unwrap(),
        )
        .expect("write legacy json");

        migrate_usage_json_cache_from_path_if_needed(&mut conn, &json_path)
            .expect("skip legacy import");
        assert_eq!(read_usage_logs_db(&conn).unwrap(), vec![current]);
        assert!(read_usage_meta(&conn, "jsonMigrated").unwrap().is_some());
    }

    #[test]
    fn token_events_without_valid_session_meta_are_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("rollout-unknown.jsonl");
        fs::write(&file, token_event(None, (100, 20, 5), (100, 20, 5)))
            .expect("write malformed rollout");
        assert!(parse_codex_usage_file(&file).is_err());
    }

    #[test]
    fn force_version_or_file_changes_trigger_a_rebuild() {
        assert!(!should_rebuild_usage_cache(
            false,
            USAGE_CACHE_VERSION,
            true
        ));
        assert!(should_rebuild_usage_cache(true, USAGE_CACHE_VERSION, true));
        assert!(should_rebuild_usage_cache(
            false,
            USAGE_CACHE_VERSION - 1,
            true
        ));
        assert!(should_rebuild_usage_cache(
            false,
            USAGE_CACHE_VERSION,
            false
        ));
    }

    #[test]
    fn sqlite_usage_cache_round_trips_imported_logs() {
        let mut conn = Connection::open_in_memory().expect("open sqlite");
        init_usage_db(&conn).expect("init sqlite");
        let cache = UsageCache {
            version: USAGE_CACHE_VERSION,
            updated_at: 1_782_880_000,
            files: vec![UsageCacheFile {
                path: "/tmp/session.jsonl".to_string(),
                modified_ms: 123,
                size_bytes: 456,
            }],
            logs: vec![ParsedUsageLog {
                request_id: "codex_rollout:imported:1".to_string(),
                session_id: "session".to_string(),
                rollout_id: "imported".to_string(),
                model: "gpt-5.5".to_string(),
                input_tokens: 10,
                output_tokens: 2,
                cache_read_tokens: 4,
                created_at: 1_782_880_001,
                source_kind: UsageLogSourceKind::Fork,
            }],
            errors: vec!["sample error".to_string()],
        };

        import_usage_cache_into_db(&mut conn, &cache).expect("import cache");
        let restored = read_usage_cache_db(&conn)
            .expect("read cache")
            .expect("cache exists");

        assert_eq!(restored.version, USAGE_CACHE_VERSION);
        assert_eq!(restored.files, cache.files);
        assert_eq!(restored.logs.len(), 1);
        assert_eq!(restored.logs[0].request_id, "codex_rollout:imported:1");
        assert_eq!(restored.logs[0].session_id, "session");
        assert_eq!(restored.logs[0].rollout_id, "imported");
        assert_eq!(restored.logs[0].source_kind, UsageLogSourceKind::Fork);
        assert_eq!(restored.errors, cache.errors);
    }

    #[test]
    fn default_pricing_configs_only_include_codex() {
        let configs = default_pricing_configs();

        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].app, "Codex");
    }

    #[test]
    fn supported_pricing_only_keeps_gpt_and_codex_models() {
        let pricing = default_pricing();

        assert!(!pricing.is_empty());
        assert!(pricing
            .iter()
            .all(|item| is_supported_pricing_model(&item.model_id)));
        assert!(pricing.iter().any(|item| item.model_id == "gpt-5.5"));
        assert!(pricing.iter().any(|item| item.model_id == "gpt-5.6-sol"));
        assert!(pricing.iter().any(|item| item.model_id == "gpt-5.6-terra"));
        assert!(pricing.iter().any(|item| item.model_id == "gpt-5.6-luna"));
        assert!(!pricing
            .iter()
            .any(|item| item.model_id.starts_with("claude-")));
        assert!(!pricing
            .iter()
            .any(|item| item.model_id.starts_with("gemini-")));
        assert!(!pricing
            .iter()
            .any(|item| item.model_id.starts_with("doubao-")));
    }

    #[test]
    fn gpt_56_display_names_use_default_pricing() {
        let pricing = default_pricing();
        let log = ParsedUsageLog {
            request_id: "codex_rollout:gpt56:1".to_string(),
            session_id: "session".to_string(),
            rollout_id: "gpt56".to_string(),
            model: normalize_codex_model("5.6 Terra"),
            input_tokens: 1_200_000,
            output_tokens: 100_000,
            cache_read_tokens: 200_000,
            created_at: 1_782_880_001,
            source_kind: UsageLogSourceKind::CanonicalRoot,
        };

        assert_eq!(log.model, "gpt-5.6-terra");
        assert_eq!(format_cost(calculate_cost(&log, &pricing, 1.0)), "4.050000");
    }

    #[test]
    fn pricing_defaults_migration_adds_gpt_56_once_and_preserves_custom_prices() {
        let custom_sol = CodexUsagePricing {
            model_id: "gpt-5.6-sol".to_string(),
            display_name: "Custom Sol".to_string(),
            input_cost_per_million: "99".to_string(),
            output_cost_per_million: "98".to_string(),
            cache_read_cost_per_million: "97".to_string(),
            cache_creation_cost_per_million: "96".to_string(),
        };
        let existing = vec![
            CodexUsagePricing {
                model_id: "gpt-5.5".to_string(),
                display_name: "GPT-5.5".to_string(),
                input_cost_per_million: "5".to_string(),
                output_cost_per_million: "30".to_string(),
                cache_read_cost_per_million: "0.50".to_string(),
                cache_creation_cost_per_million: "0".to_string(),
            },
            custom_sol.clone(),
        ];

        let (migrated, changed) = migrate_pricing_defaults(existing, 0);
        assert!(changed);
        assert_eq!(
            migrated
                .iter()
                .filter(|item| item.model_id == "gpt-5.6-sol")
                .count(),
            1
        );
        assert_eq!(
            migrated
                .iter()
                .find(|item| item.model_id == "gpt-5.6-sol")
                .expect("custom Sol pricing")
                .input_cost_per_million,
            custom_sol.input_cost_per_million
        );
        assert!(migrated.iter().any(|item| item.model_id == "gpt-5.6-terra"));
        assert!(migrated.iter().any(|item| item.model_id == "gpt-5.6-luna"));

        let without_luna = migrated
            .into_iter()
            .filter(|item| item.model_id != "gpt-5.6-luna")
            .collect::<Vec<_>>();
        let (after_current_version, changed) =
            migrate_pricing_defaults(without_luna, PRICING_DEFAULTS_VERSION);
        assert!(!changed);
        assert!(!after_current_version
            .iter()
            .any(|item| item.model_id == "gpt-5.6-luna"));
    }

    #[test]
    fn filter_supported_pricing_removes_non_gpt_entries() {
        let pricing = filter_supported_pricing(vec![
            CodexUsagePricing {
                model_id: "gpt-5.5".to_string(),
                display_name: "GPT-5.5".to_string(),
                input_cost_per_million: "1".to_string(),
                output_cost_per_million: "1".to_string(),
                cache_read_cost_per_million: "0".to_string(),
                cache_creation_cost_per_million: "0".to_string(),
            },
            CodexUsagePricing {
                model_id: "gemini-2.5-pro".to_string(),
                display_name: "Gemini 2.5 Pro".to_string(),
                input_cost_per_million: "1".to_string(),
                output_cost_per_million: "1".to_string(),
                cache_read_cost_per_million: "0".to_string(),
                cache_creation_cost_per_million: "0".to_string(),
            },
        ]);

        assert_eq!(pricing.len(), 1);
        assert_eq!(pricing[0].model_id, "gpt-5.5");
    }
}
