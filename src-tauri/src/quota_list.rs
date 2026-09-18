//! 额度列表查询：汇总本地 API 服务 / OpenCodex 内未过期 Codex 账号的额度窗口，
//! 结果缓存 2 分钟；远程 Base URL 则探测对端的 checkListStatus / quotaList 接口。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::account::CodexAccount;

/// 额度列表缓存时长（秒）。
const QUOTA_LIST_CACHE_SECONDS: i64 = 120;
/// 远程探测超时（秒）。
const REMOTE_PROBE_TIMEOUT_SECONDS: u64 = 5;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaListWindow {
    pub key: String,
    pub label: String,
    pub percentage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaListAccount {
    pub account_id: String,
    pub label: String,
    pub windows: Vec<QuotaListWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaListResult {
    /// "apiService" | "openCodex" | "remote"。
    pub scope: String,
    pub available: bool,
    pub cached_at: i64,
    pub accounts: Vec<QuotaListAccount>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaListStatus {
    pub scope: String,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum QuotaScope {
    ApiService,
    OpenCodex,
    Remote(String),
}

impl QuotaScope {
    fn label(&self) -> &'static str {
        match self {
            QuotaScope::ApiService => "apiService",
            QuotaScope::OpenCodex => "openCodex",
            QuotaScope::Remote(_) => "remote",
        }
    }

    fn cache_key(&self) -> String {
        match self {
            QuotaScope::ApiService => "api-service".to_string(),
            QuotaScope::OpenCodex => "open-codex".to_string(),
            QuotaScope::Remote(base) => format!("remote:{base}"),
        }
    }
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or_default()
}

/// 去掉 Base URL 的 /v1 后缀与末尾斜杠，得到探测根地址。
fn probe_base(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Base URL 为空，无法探测额度列表接口".to_string());
    }
    let url = reqwest::Url::parse(trimmed).map_err(|error| format!("Base URL 无效: {error}"))?;
    let mut base = url.to_string();
    while base.ends_with('/') {
        base.pop();
    }
    if base.ends_with("/v1") {
        base.truncate(base.len() - 3);
    }
    Ok(base)
}

fn resolve_scope(
    base_url: &str,
    opencodex: &crate::opencodex::OpenCodexBackend,
) -> Result<QuotaScope, String> {
    let base = probe_base(base_url)?;
    let url = reqwest::Url::parse(&base).map_err(|error| format!("Base URL 无效: {error}"))?;
    let local = matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"));
    if let (true, Some(port)) = (local, url.port_or_known_default()) {
        if crate::api_service::configured_port() == Some(port) {
            return Ok(QuotaScope::ApiService);
        }
        if opencodex.known_ports().contains(&port) {
            return Ok(QuotaScope::OpenCodex);
        }
    }
    Ok(QuotaScope::Remote(base))
}

fn account_is_expired(account: &CodexAccount) -> bool {
    let Some(iso) = crate::account::resolve_access_token_expiry(account) else {
        return false;
    };
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&iso) else {
        return false;
    };
    parsed.timestamp() <= now_seconds()
}

fn account_label(account: &CodexAccount) -> String {
    let email = account.email.trim();
    if !email.is_empty() {
        return email.to_string();
    }
    account
        .account_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(account.id.as_str())
        .to_string()
}

/// 本地 API 服务中未过期的 Codex 账号（auth 目录里已绑定的账号）。
fn api_service_account_ids() -> Result<Vec<String>, String> {
    let dirs = crate::api_service::ApiServiceDirs::new()?;
    let bound = crate::api_service::list_all_bound_accounts(&dirs)?;
    let store = crate::account::AccountStore::default();
    let accounts = store.list_accounts()?;
    let mut ids = Vec::new();
    for item in bound {
        let Some(account_id) = item.account_id.as_deref() else {
            continue;
        };
        let Some(account) = accounts.iter().find(|account| account.id == account_id) else {
            continue;
        };
        if account_is_expired(account) {
            continue;
        }
        ids.push(account.id.clone());
    }
    Ok(ids)
}

/// OpenCodex 中已导入且未过期的 Codex 账号。
fn open_codex_account_ids(
    opencodex: &crate::opencodex::OpenCodexBackend,
) -> Result<Vec<String>, String> {
    let scan = opencodex.scan_codex_switcher_accounts()?;
    let store = crate::account::AccountStore::default();
    let accounts = store.list_accounts()?;
    let mut ids = Vec::new();
    for summary in scan.accounts {
        if summary.status != "already_imported" {
            continue;
        };
        let Some(account) = accounts
            .iter()
            .find(|account| account.id == summary.target_account_id)
        else {
            continue;
        };
        if account_is_expired(account) {
            continue;
        }
        ids.push(account.id.clone());
    }
    Ok(ids)
}

fn local_accounts(
    scope: &QuotaScope,
    opencodex: &crate::opencodex::OpenCodexBackend,
) -> Result<Vec<CodexAccount>, String> {
    let ids = match scope {
        QuotaScope::ApiService => api_service_account_ids()?,
        QuotaScope::OpenCodex => open_codex_account_ids(opencodex)?,
        QuotaScope::Remote(_) => return Ok(Vec::new()),
    };
    let store = crate::account::AccountStore::default();
    let accounts = store.list_accounts()?;
    Ok(ids
        .into_iter()
        .filter_map(|id| accounts.iter().find(|account| account.id == id).cloned())
        .collect())
}

fn round1(value: f64) -> f64 {
    (value.clamp(0.0, 100.0) * 10.0).round() / 10.0
}

fn window_short_label(minutes: i64) -> String {
    if minutes > 0 && minutes % (60 * 24) == 0 {
        format!("{} 天", minutes / 60 / 24)
    } else if minutes > 0 && minutes % 60 == 0 {
        format!("{} 小时", minutes / 60)
    } else {
        format!("{minutes} 分钟")
    }
}

fn record<'a>(value: &'a Value, snake: &str, camel: &str) -> Option<&'a Value> {
    let object = value.as_object()?;
    object
        .get(snake)
        .or_else(|| object.get(camel))
        .filter(|item| !item.is_null())
}

fn finite_i64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Number(number)) => number
            .as_f64()
            .filter(|item| item.is_finite())
            .map(|item| item as i64),
        Some(Value::String(text)) => text.trim().parse::<f64>().ok().map(|item| item as i64),
        _ => None,
    }
}

fn finite_f64(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64().filter(|item| item.is_finite()),
        Some(Value::String(text)) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// 与前端 hasQuotaWindow 对齐：显式 false 视为无；否则要有窗口时长或重置时间。
fn quota_window_present(
    present: Option<bool>,
    minutes: Option<i64>,
    reset_time: Option<i64>,
) -> bool {
    if present == Some(false) {
        return false;
    }
    if present == Some(true) {
        return true;
    }
    minutes.is_some_and(|value| value > 0) || reset_time.is_some_and(|value| value > 0)
}

/// 从 CodexQuota 提取额度窗口列表：主 5h / 7d 窗口 + raw_data 中的附加限额。
fn windows_from_quota(quota: &crate::account::CodexQuota) -> Vec<QuotaListWindow> {
    let mut windows = Vec::new();
    if quota_window_present(
        quota.hourly_window_present,
        quota.hourly_window_minutes,
        quota.hourly_reset_time,
    ) {
        let minutes = quota.hourly_window_minutes;
        windows.push(QuotaListWindow {
            key: "hourly".to_string(),
            label: window_short_label(minutes.unwrap_or(300)),
            percentage: round1(quota.hourly_percentage as f64),
            reset_time: quota.hourly_reset_time,
            window_minutes: minutes,
        });
    }
    if quota_window_present(
        quota.weekly_window_present,
        quota.weekly_window_minutes,
        quota.weekly_reset_time,
    ) {
        let minutes = quota.weekly_window_minutes;
        windows.push(QuotaListWindow {
            key: "weekly".to_string(),
            label: window_short_label(minutes.unwrap_or(10080)),
            percentage: round1(quota.weekly_percentage as f64),
            reset_time: quota.weekly_reset_time,
            window_minutes: minutes,
        });
    }

    let raw = quota.raw_data.as_ref().cloned().unwrap_or(Value::Null);
    let nested_data = record(&raw, "data", "data");
    let limits_source =
        record(&raw, "additional_rate_limits", "additionalRateLimits").or_else(|| {
            nested_data
                .and_then(|nested| record(nested, "additional_rate_limits", "additionalRateLimits"))
        });
    let Some(Value::Array(limits)) = limits_source else {
        return windows;
    };
    for (limit_index, limit) in limits.iter().enumerate() {
        let label = record(limit, "limit_name", "limitName")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().replace(['-', '_'], " "))
            .filter(|value| !value.is_empty())
            .unwrap_or_default();
        if label.is_empty() {
            continue;
        }
        let Some(rate_limit) = record(limit, "rate_limit", "rateLimit") else {
            continue;
        };
        for slot in ["primary", "secondary"] {
            let Some(window) = record(
                rate_limit,
                &format!("{slot}_window"),
                &format!("{slot}Window"),
            ) else {
                continue;
            };
            if !window.is_object() {
                continue;
            }
            let remaining = finite_f64(record(window, "remaining_percent", "remainingPercent"));
            let used = finite_f64(record(window, "used_percent", "usedPercent"));
            let Some(percentage) = remaining.or_else(|| used.map(|value| 100.0 - value)) else {
                continue;
            };
            let window_seconds =
                finite_i64(record(window, "limit_window_seconds", "limitWindowSeconds"));
            let reset_time = finite_i64(record(window, "reset_at", "resetAt"));
            let window_minutes = window_seconds
                .filter(|value| *value > 0)
                .map(|value| (value as f64 / 60.0).round().max(1.0) as i64);
            windows.push(QuotaListWindow {
                key: format!(
                    "{limit_index}:{label}:{slot}:{}",
                    window_seconds
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ),
                label: label.clone(),
                percentage: round1(percentage),
                reset_time,
                window_minutes,
            });
        }
    }
    windows
}

struct CacheEntry {
    cached_at: i64,
    result: QuotaListResult,
}

fn quota_list_cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn cached_result(key: &str) -> Option<QuotaListResult> {
    let cache = quota_list_cache().lock().ok()?;
    let entry = cache.get(key)?;
    if now_seconds() - entry.cached_at > QUOTA_LIST_CACHE_SECONDS {
        return None;
    }
    Some(entry.result.clone())
}

fn store_result(key: &str, result: QuotaListResult) {
    if let Ok(mut cache) = quota_list_cache().lock() {
        cache.insert(
            key.to_string(),
            CacheEntry {
                cached_at: result.cached_at,
                result,
            },
        );
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(REMOTE_PROBE_TIMEOUT_SECONDS))
        .build()
        .map_err(|error| format!("初始化额度列表请求失败: {error}"))
}

fn parse_remote_status(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    match value {
        Value::Bool(flag) => flag,
        Value::Object(_) => ["available", "status", "ok", "enabled"]
            .iter()
            .find_map(|key| value.get(*key))
            .is_some_and(|value| match value {
                Value::Bool(flag) => *flag,
                Value::String(text) => text.eq_ignore_ascii_case("true") || text == "1",
                _ => false,
            }),
        _ => false,
    }
}

fn parse_remote_window(value: &Value) -> Option<QuotaListWindow> {
    let percentage = finite_f64(record(value, "percentage", "percentage"))
        .or_else(|| finite_f64(record(value, "remaining_percent", "remainingPercent")))
        .or_else(|| finite_f64(record(value, "percent", "percent")))?;
    let label = record(value, "label", "label")
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .to_string();
    let window_minutes = finite_i64(record(value, "window_minutes", "windowMinutes"));
    let key = record(value, "key", "key")
        .and_then(|item| item.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}:{:?}", label, window_minutes));
    Some(QuotaListWindow {
        key,
        label: if label.is_empty() {
            window_short_label(window_minutes.unwrap_or(0))
        } else {
            label
        },
        percentage: round1(percentage),
        reset_time: finite_i64(record(value, "reset_time", "resetTime"))
            .or_else(|| finite_i64(record(value, "reset_at", "resetAt"))),
        window_minutes,
    })
}

fn parse_remote_accounts(value: &Value) -> Vec<QuotaListAccount> {
    let accounts = if value.is_array() {
        value
    } else {
        record(value, "accounts", "accounts").unwrap_or(&Value::Null)
    };
    let Some(list) = accounts.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| {
            let account_id = record(item, "account_id", "accountId")
                .or_else(|| record(item, "id", "id"))
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            if account_id.is_empty() {
                return None;
            }
            let label = record(item, "label", "label")
                .or_else(|| record(item, "email", "email"))
                .and_then(|value| value.as_str())
                .unwrap_or(account_id.as_str())
                .to_string();
            let windows = record(item, "windows", "windows")
                .and_then(|value| value.as_array())
                .map(|items| items.iter().filter_map(parse_remote_window).collect())
                .unwrap_or_default();
            let error = record(item, "error", "error")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            Some(QuotaListAccount {
                account_id,
                label,
                windows,
                error,
            })
        })
        .collect()
}

async fn remote_status(base: &str) -> bool {
    let Ok(client) = http_client() else {
        return false;
    };
    let Ok(response) = client.get(format!("{base}/checkListStatus")).send().await else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    let Ok(body) = response.text().await else {
        return false;
    };
    parse_remote_status(&body)
}

async fn remote_quota_list(base: &str) -> Result<QuotaListResult, String> {
    let client = http_client()?;
    let response = client
        .get(format!("{base}/quotaList"))
        .send()
        .await
        .map_err(|error| format!("请求额度列表失败: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取额度列表响应失败: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "额度列表接口返回 {}: {}",
            status,
            crate::compact_http_body(&body)
        ));
    }
    let value: Value =
        serde_json::from_str(&body).map_err(|error| format!("解析额度列表 JSON 失败: {error}"))?;
    let accounts = parse_remote_accounts(&value);
    Ok(QuotaListResult {
        scope: QuotaScope::Remote(base.to_string()).label().to_string(),
        available: !accounts.is_empty(),
        cached_at: now_seconds(),
        accounts,
    })
}

async fn compute_local_quota_list(
    scope: &QuotaScope,
    opencodex: &crate::opencodex::OpenCodexBackend,
) -> Result<QuotaListResult, String> {
    let accounts = local_accounts(scope, opencodex)?;
    let available = !accounts.is_empty();
    let mut entries = Vec::new();
    for account in accounts {
        let label = account_label(&account);
        match crate::fetch_codex_quota_for_account(&account.id).await {
            Ok(quota) => entries.push(QuotaListAccount {
                account_id: account.id.clone(),
                label,
                windows: windows_from_quota(&quota),
                error: None,
            }),
            Err(error) => entries.push(QuotaListAccount {
                account_id: account.id.clone(),
                label,
                windows: Vec::new(),
                error: Some(error),
            }),
        }
    }
    Ok(QuotaListResult {
        scope: scope.label().to_string(),
        available,
        cached_at: now_seconds(),
        accounts: entries,
    })
}

#[tauri::command]
pub(crate) async fn check_quota_list_status(
    base_url: String,
    opencodex: tauri::State<'_, Arc<crate::opencodex::OpenCodexBackend>>,
) -> Result<QuotaListStatus, String> {
    let scope = resolve_scope(&base_url, &opencodex)?;
    if let QuotaScope::Remote(base) = &scope {
        let base = base.clone();
        return Ok(QuotaListStatus {
            scope: scope.label().to_string(),
            available: remote_status(&base).await,
        });
    }
    if let Some(cached) = cached_result(&scope.cache_key()) {
        return Ok(QuotaListStatus {
            scope: scope.label().to_string(),
            available: cached.available,
        });
    }
    let available = !local_accounts(&scope, &opencodex)?.is_empty();
    Ok(QuotaListStatus {
        scope: scope.label().to_string(),
        available,
    })
}

#[tauri::command]
pub(crate) async fn fetch_quota_list(
    base_url: String,
    force: bool,
    opencodex: tauri::State<'_, Arc<crate::opencodex::OpenCodexBackend>>,
) -> Result<QuotaListResult, String> {
    let scope = resolve_scope(&base_url, &opencodex)?;
    let key = scope.cache_key();
    if !force {
        if let Some(cached) = cached_result(&key) {
            return Ok(cached);
        }
    }
    let result = match &scope {
        QuotaScope::Remote(base) => remote_quota_list(base).await?,
        local => compute_local_quota_list(local, &opencodex).await?,
    };
    store_result(&key, result.clone());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::CodexQuota;

    fn quota_with_additional_limits() -> CodexQuota {
        let raw = serde_json::json!({
            "additional_rate_limits": [
                {
                    "limit_name": "gpt-5.3-codex-spark",
                    "rate_limit": {
                        "primary_window": {
                            "used_percent": 25.0,
                            "limit_window_seconds": 18000,
                            "reset_at": 1760000000
                        }
                    }
                }
            ]
        });
        CodexQuota {
            hourly_percentage: 40,
            hourly_reset_time: Some(1760000111),
            hourly_window_minutes: Some(300),
            hourly_window_present: Some(true),
            weekly_percentage: 7,
            weekly_reset_time: None,
            weekly_window_minutes: Some(10080),
            weekly_window_present: Some(true),
            reset_credits_available: None,
            reset_credits: Vec::new(),
            reset_credits_next_expires_at: None,
            raw_data: Some(raw),
        }
    }

    #[test]
    fn windows_include_base_and_additional_limits() {
        let windows = windows_from_quota(&quota_with_additional_limits());
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].key, "hourly");
        assert_eq!(windows[0].label, "5 小时");
        assert_eq!(windows[0].percentage, 40.0);
        assert_eq!(windows[1].key, "weekly");
        assert_eq!(windows[1].label, "7 天");
        assert_eq!(windows[2].label, "gpt 5.3 codex spark");
        assert_eq!(windows[2].percentage, 75.0);
        assert_eq!(windows[2].window_minutes, Some(300));
        assert_eq!(windows[2].reset_time, Some(1760000000));
    }

    #[test]
    fn probe_base_strips_v1_and_trailing_slash() {
        assert_eq!(
            probe_base("http://127.0.0.1:17877/v1/").unwrap(),
            "http://127.0.0.1:17877"
        );
        assert_eq!(
            probe_base("https://lu.sb996.cn:18787/v1").unwrap(),
            "https://lu.sb996.cn:18787"
        );
        assert!(probe_base("   ").is_err());
    }

    #[test]
    fn remote_status_accepts_bool_and_object_shapes() {
        assert!(parse_remote_status("true"));
        assert!(parse_remote_status("{\"available\":true}"));
        assert!(parse_remote_status("{\"status\":\"true\"}"));
        assert!(!parse_remote_status("{\"available\":false}"));
        assert!(!parse_remote_status("nope"));
    }

    #[test]
    fn remote_accounts_parse_camel_and_snake_shapes() {
        let value = serde_json::json!({
            "accounts": [
                {
                    "accountId": "a1",
                    "label": "one@example.com",
                    "windows": [ { "label": "5 小时", "percentage": 88, "windowMinutes": 300 } ]
                },
                {
                    "id": "a2",
                    "email": "two@example.com",
                    "windows": [ { "label": "7 天", "remaining_percent": 12 } ]
                }
            ]
        });
        let accounts = parse_remote_accounts(&value);
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].account_id, "a1");
        assert_eq!(accounts[0].windows[0].percentage, 88.0);
        assert_eq!(accounts[1].label, "two@example.com");
        assert_eq!(accounts[1].windows[0].percentage, 12.0);
    }

    #[test]
    fn cache_returns_fresh_entry_and_drops_stale() {
        let key = "unit-test-cache";
        let result = QuotaListResult {
            scope: "apiService".to_string(),
            available: true,
            cached_at: now_seconds(),
            accounts: Vec::new(),
        };
        store_result(key, result.clone());
        assert_eq!(cached_result(key), Some(result.clone()));
        store_result(
            key,
            QuotaListResult {
                cached_at: now_seconds() - QUOTA_LIST_CACHE_SECONDS - 1,
                ..result
            },
        );
        assert_eq!(cached_result(key), None);
    }
}
