mod account;
mod api_service;
mod app_update;
mod oauth;
mod push;
mod session;
mod subscription;
mod token_keeper;
mod usage;

use account::{
    write_bytes_atomic, write_reader_atomic, AccountStore, ApiKeyAccountBindingInput, CodexAccount,
    CodexQuota, CodexResetCredit,
};
use oauth::CodexOAuthLoginStartResponse;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use session::{
    CodexSessionRecord, CodexSessionTokenStats, CodexSessionTrashSummary,
    CodexSessionVisibilityRepairInstanceList, CodexSessionVisibilityRepairProviderList,
    CodexSessionVisibilityRepairSummary, CodexTrashedSessionRecord, SessionStore,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tempfile::TempDir;
use usage::{CodexUsageActivity, CodexUsageDashboard, CodexUsagePricing, CodexUsagePricingConfig};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexOAuthCallbackEvent {
    login_id: String,
    ok: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexApiKeyModel {
    pub(crate) id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) owned_by: Option<String>,
}

const MAX_API_MODEL_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_API_MODELS: usize = 500;
const MAX_API_MODEL_ID_CHARS: usize = 256;
const MAX_API_BALANCE_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct CodexApiKeyBalance {
    provider: String,
    balance_kind: String,
    available_amount: Option<f64>,
    used_amount: Option<f64>,
    total_amount: Option<f64>,
    currency: String,
    unlimited: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_name: Option<String>,
}

#[derive(Debug, Clone)]
struct CodexRelayBalanceEndpoints {
    new_api_status: reqwest::Url,
    new_api_usage: reqwest::Url,
    new_api_billing_subscription: reqwest::Url,
    new_api_billing_usage: reqwest::Url,
    sub2api_usage: reqwest::Url,
    insecure_http_origin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexSwitcherSettings {
    #[serde(default = "default_monitor_quota")]
    monitor_quota: bool,
    quota_refresh_minutes: u64,
    current_account_refresh_minutes: u64,
    #[serde(default)]
    quota_next_refresh_at: u64,
    #[serde(default)]
    current_account_next_refresh_at: u64,
    sort_mode: String,
    #[serde(default = "default_sort_direction")]
    sort_direction: String,
    custom_order: Vec<String>,
    #[serde(default)]
    pinned_account_ids: Vec<String>,
    #[serde(default = "default_account_type_filter")]
    account_type_filter: String,
    #[serde(default = "default_page_size")]
    page_size: u64,
    #[serde(default = "default_account_view_mode")]
    account_view_mode: String,
    #[serde(default = "default_sidebar_enabled")]
    sidebar_enabled: bool,
    #[serde(default = "default_show_quota_countdowns")]
    show_quota_countdowns: bool,
    #[serde(default = "default_show_additional_quota_windows")]
    show_additional_quota_windows: bool,
    #[serde(default = "default_badge_style")]
    badge_style: String,
    #[serde(default = "default_badge_styles")]
    badge_styles: HashMap<String, String>,
    #[serde(default = "default_max_columns")]
    max_columns: u64,
    #[serde(default = "default_language")]
    language: String,
}

const MAX_REFRESH_MINUTES: u64 = 1440;
static SWITCHER_SETTINGS_LOCK: Mutex<()> = Mutex::new(());

impl Default for CodexSwitcherSettings {
    fn default() -> Self {
        Self {
            monitor_quota: true,
            quota_refresh_minutes: 10,
            current_account_refresh_minutes: 10,
            quota_next_refresh_at: 0,
            current_account_next_refresh_at: 0,
            sort_mode: "created_at".to_string(),
            sort_direction: default_sort_direction(),
            custom_order: Vec::new(),
            pinned_account_ids: Vec::new(),
            account_type_filter: default_account_type_filter(),
            page_size: default_page_size(),
            account_view_mode: default_account_view_mode(),
            sidebar_enabled: default_sidebar_enabled(),
            show_quota_countdowns: default_show_quota_countdowns(),
            show_additional_quota_windows: default_show_additional_quota_windows(),
            badge_style: default_badge_style(),
            badge_styles: default_badge_styles(),
            max_columns: default_max_columns(),
            language: default_language(),
        }
    }
}

impl CodexSwitcherSettings {
    fn normalized(mut self) -> Self {
        self.monitor_quota = true;
        self.quota_refresh_minutes = clamp_refresh_minutes(self.quota_refresh_minutes);
        self.current_account_refresh_minutes =
            clamp_refresh_minutes(self.current_account_refresh_minutes);
        self.quota_next_refresh_at =
            normalize_next_refresh_at(self.quota_next_refresh_at, self.quota_refresh_minutes);
        self.current_account_next_refresh_at = normalize_next_refresh_at(
            self.current_account_next_refresh_at,
            self.current_account_refresh_minutes,
        );
        self.account_view_mode = normalize_account_view_mode(&self.account_view_mode);
        self.language = normalize_language(&self.language);
        self
    }
}

fn default_monitor_quota() -> bool {
    true
}

fn clamp_refresh_minutes(value: u64) -> u64 {
    value.clamp(1, MAX_REFRESH_MINUTES)
}

fn normalize_next_refresh_at(value: u64, interval_minutes: u64) -> u64 {
    if value == 0 {
        return 0;
    }
    let Ok(now_ms) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return 0;
    };
    let now = now_ms.as_millis() as u64;
    let interval_ms = clamp_refresh_minutes(interval_minutes) * 60_000;
    if value > now && value - now <= interval_ms {
        value
    } else {
        0
    }
}

fn default_badge_style() -> String {
    "classic".to_string()
}

fn default_sort_direction() -> String {
    "desc".to_string()
}

fn default_account_type_filter() -> String {
    "all".to_string()
}

fn default_page_size() -> u64 {
    50
}

fn default_account_view_mode() -> String {
    "card".to_string()
}

fn default_sidebar_enabled() -> bool {
    true
}

fn normalize_account_view_mode(value: &str) -> String {
    match value {
        "compact" | "table" => value.to_string(),
        _ => default_account_view_mode(),
    }
}

fn default_show_quota_countdowns() -> bool {
    true
}

fn default_show_additional_quota_windows() -> bool {
    true
}

fn default_badge_styles() -> HashMap<String, String> {
    HashMap::from([
        ("free".to_string(), "gold".to_string()),
        ("plus".to_string(), "amber".to_string()),
        ("proLite".to_string(), "violet".to_string()),
        ("proMax".to_string(), "cyan".to_string()),
        ("team".to_string(), "emerald".to_string()),
        ("api".to_string(), "stamp".to_string()),
    ])
}

fn default_max_columns() -> u64 {
    3
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn normalize_language(value: &str) -> String {
    match value {
        "zh-TW" | "en" | "ru" => value.to_string(),
        _ => default_language(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexSwitcherPaths {
    app_dir: String,
    accounts_json: String,
    settings_json: String,
    backup_dir: String,
    account_dir: String,
    session_dir: String,
    statistics_dir: String,
    data_dir: String,
    codex_home: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CodexConfigFileContent {
    kind: String,
    name: String,
    path: String,
    content: String,
    exists: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodexConfigFileKind {
    AuthJson,
    ConfigToml,
}

impl CodexConfigFileKind {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auth" | "auth.json" => Ok(Self::AuthJson),
            "config" | "config.toml" => Ok(Self::ConfigToml),
            _ => Err("只允许编辑 auth.json 或 config.toml".to_string()),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::AuthJson => "auth",
            Self::ConfigToml => "config",
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Self::AuthJson => "auth.json",
            Self::ConfigToml => "config.toml",
        }
    }

    fn default_content(self) -> &'static str {
        match self {
            Self::AuthJson => "{}\n",
            Self::ConfigToml => "",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexSwitcherBackupFile {
    name: String,
    path: String,
    created_at: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexSwitcherBackupProgressEvent {
    task_id: String,
    status: String,
    progress: u8,
    message: String,
    backup_file: Option<CodexSwitcherBackupFile>,
}

#[tauri::command]
fn list_codex_accounts() -> Result<Vec<CodexAccount>, String> {
    AccountStore::default().list_accounts()
}

#[tauri::command]
fn get_current_codex_account() -> Result<Option<CodexAccount>, String> {
    AccountStore::default().current_account()
}

#[tauri::command]
fn detect_current_codex_account() -> Result<Option<CodexAccount>, String> {
    AccountStore::default().detect_current_account_from_codex_config()
}

#[tauri::command]
fn import_codex_from_json(json_content: String) -> Result<Vec<CodexAccount>, String> {
    AccountStore::default().import_from_json(&json_content)
}

#[tauri::command]
fn import_codex_from_local() -> Result<Vec<CodexAccount>, String> {
    AccountStore::default().import_from_local()
}

#[tauri::command]
fn add_codex_account_with_api_key(
    api_key: String,
    api_base_url: Option<String>,
    api_provider_name: Option<String>,
    api_official_url: Option<String>,
    account_name: Option<String>,
    bound_oauth_account_id: Option<String>,
    bound_oauth_use_local_gateway: Option<bool>,
) -> Result<CodexAccount, String> {
    AccountStore::default().add_api_key_account_with_binding(ApiKeyAccountBindingInput {
        api_key,
        api_base_url,
        api_provider_name,
        api_official_url,
        account_name,
        bound_oauth_account_id,
        bound_oauth_use_local_gateway: bound_oauth_use_local_gateway.unwrap_or(false),
    })
}

#[tauri::command]
fn update_codex_api_key_credentials(
    account_id: String,
    api_key: String,
    api_base_url: Option<String>,
    api_provider_name: Option<String>,
    api_official_url: Option<String>,
) -> Result<CodexAccount, String> {
    AccountStore::default().update_api_key_credentials(
        &account_id,
        api_key,
        api_base_url,
        api_provider_name,
        api_official_url,
    )
}

fn codex_models_endpoint(base_url: Option<&str>) -> Result<String, String> {
    let base_url = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("https://api.openai.com/v1");
    let mut url =
        reqwest::Url::parse(base_url).map_err(|error| format!("Base URL 无效: {}", error))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Base URL 必须是有效的 http:// 或 https:// 地址".to_string());
    }
    let path = url.path().trim_end_matches('/');
    if !path.ends_with("/models") {
        let models_path = if path.is_empty() {
            "/models".to_string()
        } else {
            format!("{}/models", path)
        };
        url.set_path(&models_path);
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

fn parse_codex_api_key_models(body: &str) -> Result<Vec<CodexApiKeyModel>, String> {
    let payload = serde_json::from_str::<Value>(body)
        .map_err(|error| format!("解析模型列表 JSON 失败: {}", error))?;
    let items = payload
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| payload.get("models").and_then(Value::as_array))
        .or_else(|| payload.as_array())
        .ok_or_else(|| "模型列表响应中缺少 data 或 models 数组".to_string())?;

    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for item in items {
        let Some(id) = (match item {
            Value::String(id) => Some(id.as_str()),
            Value::Object(fields) => fields
                .get("id")
                .or_else(|| fields.get("model"))
                .or_else(|| fields.get("name"))
                .and_then(Value::as_str),
            _ => None,
        })
        .map(str::trim) else {
            continue;
        };
        if id.is_empty()
            || id.chars().count() > MAX_API_MODEL_ID_CHARS
            || id.chars().any(char::is_control)
            || !seen.insert(id.to_ascii_lowercase())
        {
            continue;
        }
        let owned_by = item.as_object().and_then(|fields| {
            fields
                .get("owned_by")
                .or_else(|| fields.get("ownedBy"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .filter(|value| {
                    value.chars().count() <= MAX_API_MODEL_ID_CHARS
                        && !value.chars().any(char::is_control)
                })
                .map(ToOwned::to_owned)
        });
        models.push(CodexApiKeyModel {
            id: id.to_string(),
            owned_by,
        });
        if models.len() >= MAX_API_MODELS {
            break;
        }
    }
    models.sort_by(|left, right| {
        left.id
            .to_ascii_lowercase()
            .cmp(&right.id.to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(models)
}

#[tauri::command]
async fn fetch_codex_api_key_models(account_id: String) -> Result<Vec<CodexApiKeyModel>, String> {
    let account = AccountStore::default()
        .list_accounts()?
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| "账号不存在".to_string())?;
    fetch_codex_api_key_models_for_account(&account).await
}

pub(crate) async fn fetch_codex_api_key_models_for_account(
    account: &CodexAccount,
) -> Result<Vec<CodexApiKeyModel>, String> {
    if account.auth_mode.as_deref() != Some("apikey") {
        return Err("只有 API Key 账号可以获取模型列表".to_string());
    }
    let api_key = account
        .openai_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "API Key 账号缺少 OPENAI_API_KEY".to_string())?;
    let endpoint = codex_models_endpoint(account.api_base_url.as_deref())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("创建模型列表请求失败: {}", error))?;
    let mut response = client
        .get(&endpoint)
        .bearer_auth(api_key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| format!("请求模型列表失败: {}", error))?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_API_MODEL_RESPONSE_BYTES as u64)
    {
        return Err(format!(
            "模型列表响应超过 {} MiB 限制",
            MAX_API_MODEL_RESPONSE_BYTES / 1024 / 1024
        ));
    }
    let status = response.status();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("读取模型列表响应失败: {}", error))?
    {
        if body.len() + chunk.len() > MAX_API_MODEL_RESPONSE_BYTES {
            return Err(format!(
                "模型列表响应超过 {} MiB 限制",
                MAX_API_MODEL_RESPONSE_BYTES / 1024 / 1024
            ));
        }
        body.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&body).into_owned();
    if !status.is_success() {
        return Err(format!(
            "模型列表接口返回 {}: {}",
            status,
            compact_http_body(&body)
        ));
    }
    parse_codex_api_key_models(&body)
}

fn relay_balance_url(root: &reqwest::Url, suffix: &str) -> reqwest::Url {
    let mut url = root.clone();
    let root_path = root.path().trim_end_matches('/');
    let suffix = suffix.trim_start_matches('/');
    let path = if root_path.is_empty() {
        format!("/{}", suffix)
    } else {
        format!("{}/{}", root_path, suffix)
    };
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    url
}

fn codex_relay_balance_endpoints(
    base_url: Option<&str>,
) -> Result<CodexRelayBalanceEndpoints, String> {
    let base_url = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "API Key 账号未设置 Base URL，无法查询中转站余额".to_string())?;
    let mut root =
        reqwest::Url::parse(base_url).map_err(|error| format!("Base URL 无效: {}", error))?;
    if !matches!(root.scheme(), "http" | "https") || root.host_str().is_none() {
        return Err("Base URL 必须是有效的 http:// 或 https:// 地址".to_string());
    }
    let mut root_path = root.path().trim_end_matches('/').to_string();
    if root_path == "/v1" {
        root_path.clear();
    } else if let Some(prefix) = root_path.strip_suffix("/v1") {
        root_path = prefix.to_string();
    }
    root.set_path(if root_path.is_empty() {
        "/"
    } else {
        &root_path
    });
    root.set_query(None);
    root.set_fragment(None);

    let insecure_http_origin = (root.scheme() == "http" && !balance_http_host_is_loopback(&root))
        .then(|| root.origin().ascii_serialization());

    Ok(CodexRelayBalanceEndpoints {
        new_api_status: relay_balance_url(&root, "api/status"),
        new_api_usage: relay_balance_url(&root, "api/usage/token/"),
        new_api_billing_subscription: relay_balance_url(&root, "dashboard/billing/subscription"),
        new_api_billing_usage: relay_balance_url(&root, "dashboard/billing/usage"),
        sub2api_usage: relay_balance_url(&root, "v1/usage"),
        insecure_http_origin,
    })
}

fn balance_http_host_is_loopback(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

fn ensure_balance_transport_allowed(
    endpoints: &CodexRelayBalanceEndpoints,
    approved_insecure_http_origin: Option<&str>,
) -> Result<(), String> {
    if let Some(required_origin) = endpoints.insecure_http_origin.as_deref() {
        if approved_insecure_http_origin != Some(required_origin) {
            return Err(
                "INSECURE_HTTP_CONFIRM_REQUIRED: 远程 HTTP 中转站会以明文传输 API Key，请确认风险后重试"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn json_f64(value: Option<&Value>) -> Option<f64> {
    let number = value.and_then(|value| {
        value.as_f64().or_else(|| {
            value
                .as_str()
                .and_then(|text| text.trim().parse::<f64>().ok())
        })
    })?;
    number.is_finite().then_some(number)
}

fn json_trimmed_string(value: Option<&Value>) -> Option<String> {
    let value = value?.as_str()?.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some(value.chars().take(120).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewApiQuotaDisplayType {
    Usd,
    Cny,
    Tokens,
}

#[derive(Debug, Clone, Copy)]
struct NewApiQuotaSettings {
    quota_per_unit: f64,
    usd_exchange_rate: f64,
    display_type: NewApiQuotaDisplayType,
}

impl NewApiQuotaSettings {
    fn convert_quota(self, quota: f64) -> Option<f64> {
        let amount = match self.display_type {
            NewApiQuotaDisplayType::Usd => quota / self.quota_per_unit,
            NewApiQuotaDisplayType::Cny => quota / self.quota_per_unit * self.usd_exchange_rate,
            NewApiQuotaDisplayType::Tokens => quota,
        };
        amount.is_finite().then_some(amount)
    }

    fn unit(self) -> &'static str {
        match self.display_type {
            NewApiQuotaDisplayType::Usd => "USD",
            NewApiQuotaDisplayType::Cny => "CNY",
            NewApiQuotaDisplayType::Tokens => "TOKENS",
        }
    }
}

fn parse_new_api_quota_settings(body: &str) -> Option<NewApiQuotaSettings> {
    let payload = serde_json::from_str::<Value>(body).ok()?;
    if payload.get("success").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let data = payload.get("data")?;
    let quota_per_unit = json_f64(data.get("quota_per_unit")).filter(|value| *value > 0.0)?;
    let usd_exchange_rate = json_f64(data.get("usd_exchange_rate"))
        .filter(|value| *value > 0.0)
        .unwrap_or(1.0);
    let display_type = match data
        .get("quota_display_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        Some("CNY") => NewApiQuotaDisplayType::Cny,
        Some("TOKENS") => NewApiQuotaDisplayType::Tokens,
        _ => NewApiQuotaDisplayType::Usd,
    };
    Some(NewApiQuotaSettings {
        quota_per_unit,
        usd_exchange_rate,
        display_type,
    })
}

fn parse_new_api_balance(
    body: &str,
    settings: NewApiQuotaSettings,
) -> Result<CodexApiKeyBalance, String> {
    let payload = serde_json::from_str::<Value>(body)
        .map_err(|error| format!("解析 NewAPI 余额响应失败: {}", error))?;
    if payload.get("code").and_then(Value::as_bool) == Some(false) {
        return Err("NewAPI 余额接口返回失败".to_string());
    }
    let data = payload
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| "NewAPI 余额响应缺少 data".to_string())?;
    if data.get("object").and_then(Value::as_str) != Some("token_usage") {
        return Err("NewAPI 余额响应格式不受支持".to_string());
    }
    let unlimited = data
        .get("unlimited_quota")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let convert =
        |key: &str| json_f64(data.get(key)).and_then(|value| settings.convert_quota(value));
    let available_amount = convert("total_available");
    if !unlimited && available_amount.is_none() {
        return Err("NewAPI 余额响应缺少 total_available".to_string());
    }
    Ok(CodexApiKeyBalance {
        provider: "new_api".to_string(),
        balance_kind: if unlimited { "unlimited" } else { "key_quota" }.to_string(),
        available_amount: (!unlimited).then_some(available_amount).flatten(),
        used_amount: convert("total_used"),
        total_amount: convert("total_granted"),
        currency: settings.unit().to_string(),
        unlimited,
        plan_name: json_trimmed_string(data.get("name")),
    })
}

fn parse_new_api_account_balance(
    subscription_body: &str,
    usage_body: &str,
    settings: NewApiQuotaSettings,
) -> Result<CodexApiKeyBalance, String> {
    let subscription = serde_json::from_str::<Value>(subscription_body)
        .map_err(|error| format!("解析 NewAPI 账户总额响应失败: {error}"))?;
    let usage = serde_json::from_str::<Value>(usage_body)
        .map_err(|error| format!("解析 NewAPI 账户用量响应失败: {error}"))?;
    let total_amount = json_f64(subscription.get("hard_limit_usd"))
        .ok_or_else(|| "NewAPI 账户总额响应缺少 hard_limit_usd".to_string())?;
    let used_amount = json_f64(usage.get("total_usage"))
        .map(|value| value / 100.0)
        .ok_or_else(|| "NewAPI 账户用量响应缺少 total_usage".to_string())?;
    let unlimited_sentinel = settings.display_type != NewApiQuotaDisplayType::Tokens
        && (total_amount - 100_000_000.0).abs() < 0.000_001;
    if total_amount < 0.0 || used_amount < 0.0 || unlimited_sentinel {
        return Err("NewAPI 账户账单响应不包含可显示的真实余额".to_string());
    }

    Ok(CodexApiKeyBalance {
        provider: "new_api".to_string(),
        balance_kind: "wallet".to_string(),
        available_amount: Some((total_amount - used_amount).max(0.0)),
        used_amount: Some(used_amount),
        total_amount: Some(total_amount),
        currency: settings.unit().to_string(),
        unlimited: false,
        plan_name: None,
    })
}

fn parse_sub2api_balance(body: &str) -> Result<CodexApiKeyBalance, String> {
    let payload = serde_json::from_str::<Value>(body)
        .map_err(|error| format!("解析 Sub2API 余额响应失败: {}", error))?;
    let data = payload
        .as_object()
        .ok_or_else(|| "Sub2API 余额响应格式不受支持".to_string())?;
    if data.get("isValid").and_then(Value::as_bool) == Some(false) {
        return Err("Sub2API API Key 当前不可用".to_string());
    }

    let mode = data.get("mode").and_then(Value::as_str).unwrap_or_default();
    let quota = data.get("quota").and_then(Value::as_object);
    let wallet_balance = json_f64(data.get("balance"));
    let remaining = json_f64(data.get("remaining"))
        .or_else(|| quota.and_then(|value| json_f64(value.get("remaining"))));
    let unlimited = mode == "unrestricted"
        && wallet_balance.is_none()
        && remaining.is_some_and(|value| value < 0.0);
    let available_amount = if unlimited {
        None
    } else {
        wallet_balance.or(remaining)
    };
    if available_amount.is_none() && !unlimited {
        return Err("Sub2API 未返回可显示的余额或额度".to_string());
    }

    let balance_kind = if unlimited {
        "unlimited"
    } else if wallet_balance.is_some() {
        "wallet"
    } else if mode == "quota_limited" {
        "key_quota"
    } else {
        "subscription"
    };
    let currency = json_trimmed_string(data.get("unit"))
        .or_else(|| quota.and_then(|value| json_trimmed_string(value.get("unit"))))
        .unwrap_or_else(|| "USD".to_string());

    Ok(CodexApiKeyBalance {
        provider: "sub2api".to_string(),
        balance_kind: balance_kind.to_string(),
        available_amount,
        used_amount: quota.and_then(|value| json_f64(value.get("used"))),
        total_amount: quota.and_then(|value| json_f64(value.get("limit"))),
        currency,
        unlimited,
        plan_name: json_trimmed_string(data.get("planName")),
    })
}

async fn read_balance_response(
    mut response: reqwest::Response,
    label: &str,
) -> Result<(reqwest::StatusCode, String), String> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_API_BALANCE_RESPONSE_BYTES as u64)
    {
        return Err(format!("{}响应超过 256 KiB 限制", label));
    }
    let status = response.status();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("读取{}响应失败: {}", label, error))?
    {
        if body.len() + chunk.len() > MAX_API_BALANCE_RESPONSE_BYTES {
            return Err(format!("{}响应超过 256 KiB 限制", label));
        }
        body.extend_from_slice(&chunk);
    }
    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}

async fn fetch_new_api_account_balance(
    client: &reqwest::Client,
    endpoints: &CodexRelayBalanceEndpoints,
    api_key: &str,
    settings: NewApiQuotaSettings,
) -> Result<CodexApiKeyBalance, String> {
    let subscription = client
        .get(endpoints.new_api_billing_subscription.clone())
        .bearer_auth(api_key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| format!("请求 NewAPI 账户总额失败: {error}"))?;
    let (subscription_status, subscription_body) =
        read_balance_response(subscription, "NewAPI 账户总额接口").await?;
    if !subscription_status.is_success() {
        return Err(balance_http_error(subscription_status));
    }

    let usage = client
        .get(endpoints.new_api_billing_usage.clone())
        .bearer_auth(api_key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| format!("请求 NewAPI 账户用量失败: {error}"))?;
    let (usage_status, usage_body) = read_balance_response(usage, "NewAPI 账户用量接口").await?;
    if !usage_status.is_success() {
        return Err(balance_http_error(usage_status));
    }

    parse_new_api_account_balance(&subscription_body, &usage_body, settings)
}

fn balance_http_error(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 => "余额查询鉴权失败，请检查 API Key".to_string(),
        403 => "余额查询被中转站拒绝，请检查账号或访问限制".to_string(),
        404 | 405 => "当前中转站不支持 NewAPI/Sub2API 余额接口".to_string(),
        _ => format!("余额接口返回 HTTP {}", status.as_u16()),
    }
}

#[tauri::command]
async fn fetch_codex_api_key_balance(
    account_id: String,
    approved_insecure_http_origin: Option<String>,
) -> Result<CodexApiKeyBalance, String> {
    let account = AccountStore::default()
        .list_accounts()?
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| "账号不存在".to_string())?;
    if account.auth_mode.as_deref() != Some("apikey") {
        return Err("只有 API Key 账号可以查询中转站余额".to_string());
    }
    let api_key = account
        .openai_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "API Key 账号缺少 OPENAI_API_KEY".to_string())?;
    let endpoints = codex_relay_balance_endpoints(account.api_base_url.as_deref())?;
    ensure_balance_transport_allowed(&endpoints, approved_insecure_http_origin.as_deref())?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let same_origin = attempt.previous().last().is_some_and(|previous| {
                previous.scheme() == attempt.url().scheme()
                    && previous.host_str() == attempt.url().host_str()
                    && previous.port_or_known_default() == attempt.url().port_or_known_default()
            });
            if same_origin && attempt.previous().len() < 3 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|error| format!("创建余额查询请求失败: {}", error))?;

    let status_response = client
        .get(endpoints.new_api_status.clone())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await;
    if let Ok(response) = status_response {
        if let Ok((status, body)) = read_balance_response(response, "NewAPI 状态接口").await {
            if status.is_success() {
                if let Some(quota_settings) = parse_new_api_quota_settings(&body) {
                    let response = client
                        .get(endpoints.new_api_usage.clone())
                        .bearer_auth(api_key)
                        .header(reqwest::header::ACCEPT, "application/json")
                        .send()
                        .await
                        .map_err(|error| format!("请求 NewAPI 余额失败: {}", error))?;
                    let (status, body) = read_balance_response(response, "NewAPI 余额接口").await?;
                    if status.is_success() {
                        if let Ok(balance) = parse_new_api_balance(&body, quota_settings) {
                            if balance.unlimited {
                                if let Ok(account_balance) = fetch_new_api_account_balance(
                                    &client,
                                    &endpoints,
                                    api_key,
                                    quota_settings,
                                )
                                .await
                                {
                                    return Ok(account_balance);
                                }
                            }
                            return Ok(balance);
                        }
                    } else if !matches!(status.as_u16(), 404 | 405) {
                        return Err(balance_http_error(status));
                    }
                }
            }
        }
    }

    let response = client
        .get(endpoints.sub2api_usage)
        .bearer_auth(api_key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| format!("请求 Sub2API 余额失败: {}", error))?;
    let (status, body) = read_balance_response(response, "Sub2API 余额接口").await?;
    if !status.is_success() {
        return Err(balance_http_error(status));
    }
    parse_sub2api_balance(&body)
}

#[tauri::command]
fn check_codex_api_key_model_access(account_id: String) -> Result<bool, String> {
    AccountStore::default().check_api_key_model_access(&account_id)
}

#[tauri::command]
fn set_codex_api_key_default_model(
    account_id: String,
    model_id: String,
) -> Result<CodexAccount, String> {
    AccountStore::default().update_api_key_default_model(&account_id, model_id)
}

#[tauri::command]
fn update_codex_account_profile(
    account_id: String,
    account_name: Option<String>,
) -> Result<CodexAccount, String> {
    AccountStore::default().update_account_profile(&account_id, account_name)
}

#[tauri::command]
fn update_codex_api_key_bound_oauth_account(
    account_id: String,
    bound_oauth_account_id: Option<String>,
    bound_oauth_use_local_gateway: Option<bool>,
) -> Result<CodexAccount, String> {
    AccountStore::default().update_api_key_bound_oauth_account(
        &account_id,
        bound_oauth_account_id,
        bound_oauth_use_local_gateway.unwrap_or(false),
    )
}

#[tauri::command]
fn update_codex_account_phone(account_id: String, phone: String) -> Result<CodexAccount, String> {
    AccountStore::default().update_account_phone(&account_id, phone)
}

#[tauri::command]
fn update_codex_account_from_json(
    account_id: String,
    json_content: String,
) -> Result<CodexAccount, String> {
    AccountStore::default().update_account_from_json(&account_id, &json_content)
}

#[tauri::command]
fn export_codex_accounts(
    account_ids: Vec<String>,
    format: Option<String>,
) -> Result<String, String> {
    AccountStore::default().export_accounts(&account_ids, format.as_deref())
}

#[tauri::command]
fn codex_oauth_login_start(app_handle: AppHandle) -> Result<CodexOAuthLoginStartResponse, String> {
    let response = oauth::start_oauth_login()?;
    start_oauth_callback_listener(app_handle, response.login_id.clone())?;
    Ok(response)
}

#[tauri::command]
fn codex_oauth_submit_callback_url(login_id: String, callback_url: String) -> Result<(), String> {
    oauth::submit_callback_url(&login_id, &callback_url)
}

#[tauri::command]
async fn codex_oauth_login_completed(login_id: String) -> Result<CodexAccount, String> {
    let tokens = oauth::complete_oauth_login(&login_id).await?;
    AccountStore::default().save_oauth_tokens(
        tokens.id_token,
        tokens.access_token,
        tokens.refresh_token,
    )
}

#[tauri::command]
fn codex_oauth_login_cancel(login_id: Option<String>) -> Result<(), String> {
    oauth::cancel_oauth_login(login_id.as_deref())
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err("只能打开 http/https 链接".to_string());
    }
    let status = open_url_with_system(trimmed)?;
    if status.success() {
        Ok(())
    } else {
        Err("系统浏览器打开失败".to_string())
    }
}

#[tauri::command]
fn open_path_in_file_manager(path: String) -> Result<(), String> {
    let input = PathBuf::from(path.trim());
    let target = if input.is_dir() {
        input
    } else {
        input
            .parent()
            .map(PathBuf::from)
            .ok_or_else(|| "无法定位文件夹".to_string())?
    };
    open_path_with_system(&target)
}

fn start_oauth_callback_listener(app_handle: AppHandle, login_id: String) -> Result<(), String> {
    let listener = bind_oauth_callback_listener()?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("OAuth 回调监听配置失败: {}", error))?;

    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10 * 60);
        let mut callback_received = false;
        let mut close_deadline: Option<Instant> = None;
        while Instant::now() < deadline
            && (oauth::is_login_active(&login_id) || close_deadline.is_some())
        {
            if close_deadline.is_some_and(|target| Instant::now() >= target) {
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0_u8; 4096];
                    let size = stream.read(&mut buffer).unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..size]);
                    let request_path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");

                    if request_path.starts_with("/close-tab") {
                        let html = oauth_close_tab_html();
                        let body = html.as_bytes();
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(body);
                        close_oauth_callback_tab_soon();
                        close_deadline = Some(Instant::now() + Duration::from_secs(2));
                        continue;
                    }

                    if request_path == "/favicon.ico" {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                        continue;
                    }

                    if !request_path.starts_with("/auth/callback") {
                        let body = b"Not Found";
                        let _ = write!(
                            stream,
                            "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(body);
                        continue;
                    }

                    let should_emit = !callback_received;
                    let callback_result = if callback_received {
                        Ok(())
                    } else {
                        oauth::submit_callback_url(
                            &login_id,
                            &format!(
                                "{}{}",
                                oauth::redirect_uri().trim_end_matches("/auth/callback"),
                                request_path
                            ),
                        )
                    };
                    let (ok, message, html) = match callback_result {
                        Ok(()) => {
                            callback_received = true;
                            close_deadline = Some(Instant::now() + Duration::from_secs(45));
                            (
                                true,
                                "已收到 OAuth 回调，正在保存账号".to_string(),
                                oauth_callback_html(
                                    true,
                                    "授权回调已收到",
                                    "正在回到 Codex Switcher 保存账号，本页面将自动关闭。",
                                ),
                            )
                        }
                        Err(error) => {
                            close_deadline = Some(Instant::now() + Duration::from_secs(60));
                            let html = oauth_callback_html(
                                false,
                                "授权回调失败",
                                "请回到 Codex Switcher 重试，或复制地址栏里的完整回调地址手动完成。",
                            );
                            (false, error, html)
                        }
                    };
                    let body = html.as_bytes();
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(body);
                    if should_emit {
                        let _ = app_handle.emit(
                            "codex-oauth-callback-received",
                            CodexOAuthCallbackEvent {
                                login_id: login_id.clone(),
                                ok,
                                message,
                            },
                        );
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(200));
                }
                Err(error) => {
                    let _ = app_handle.emit(
                        "codex-oauth-callback-received",
                        CodexOAuthCallbackEvent {
                            login_id: login_id.clone(),
                            ok: false,
                            message: format!("OAuth 回调监听失败: {}", error),
                        },
                    );
                    break;
                }
            }
        }
    });
    Ok(())
}

fn oauth_close_tab_html() -> String {
    r#"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <title>Codex OAuth</title>
  </head>
  <body>
    <script>
      try { window.close(); } catch (_) {}
    </script>
  </body>
</html>"#
        .to_string()
}

fn close_oauth_callback_tab_soon() {
    thread::spawn(|| {
        thread::sleep(Duration::from_millis(220));
        close_oauth_callback_tab();
    });
}

#[cfg(target_os = "macos")]
fn close_oauth_callback_tab() {
    let script = r#"
tell application "System Events"
  set browserNames to {"Safari", "Google Chrome", "Chromium", "Microsoft Edge", "Brave Browser", "Arc", "Firefox"}
  set frontApp to name of first application process whose frontmost is true
  if browserNames contains frontApp then
    keystroke "w" using command down
  end if
end tell
"#;
    let _ = Command::new("osascript").arg("-e").arg(script).status();
}

#[cfg(not(target_os = "macos"))]
fn close_oauth_callback_tab() {}

fn oauth_callback_html(success: bool, title: &str, description: &str) -> String {
    let status_class = if success { "success" } else { "error" };
    let icon = if success { "✓" } else { "!" };
    let countdown_html = if success {
        r#"<div class="countdown">正在尝试关闭页面，<span id="countdown">3</span> 秒后如未关闭，可直接关闭此标签页。</div>"#
    } else {
        ""
    };
    let auto_close_script = if success {
        r#"
        <script>
          (function () {
            var seconds = 3;
            var countdown = document.getElementById('countdown');
            var closeButton = document.getElementById('closeButton');
            var closeHint = document.getElementById('closeHint');
            var closeStatus = document.getElementById('closeStatus');
            var nativeCloseRequested = false;
            function requestNativeClose() {
              if (nativeCloseRequested) return;
              nativeCloseRequested = true;
              try {
                fetch('/close-tab', { method: 'POST', keepalive: true }).catch(function () {});
              } catch (_) {}
            }
            function markBlocked() {
              if (document.hidden) return;
              if (closeButton) closeButton.textContent = '关闭标签页';
              if (closeStatus) closeStatus.textContent = '浏览器限制脚本关闭当前标签页，请使用标签页关闭按钮或快捷键关闭。';
              if (closeHint) closeHint.className = 'hint blocked';
            }
            function tryClose() {
              requestNativeClose();
              try { window.close(); } catch (_) {}
            }
            closeButton && closeButton.addEventListener('click', function () {
              tryClose();
              window.setTimeout(markBlocked, 280);
            });
            var timer = window.setInterval(function () {
              seconds -= 1;
              if (countdown) countdown.textContent = String(Math.max(seconds, 0));
              tryClose();
              if (seconds <= 0) {
                window.clearInterval(timer);
                markBlocked();
              }
            }, 1000);
            window.setTimeout(tryClose, 250);
          })();
        </script>"#
    } else {
        ""
    };
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Codex OAuth</title>
    <style>
      :root {{
        color-scheme: light;
        font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", sans-serif;
        background: #f5f7fb;
        color: #111827;
      }}
      * {{ box-sizing: border-box; }}
      body {{
        min-height: 100vh;
        margin: 0;
        display: grid;
        place-items: center;
        padding: 28px;
        background:
          radial-gradient(circle at 16% 12%, rgba(37, 99, 235, 0.16), transparent 34%),
          radial-gradient(circle at 84% 18%, rgba(14, 165, 168, 0.18), transparent 30%),
          linear-gradient(180deg, #f8fbff 0%, #eef4fb 100%);
      }}
      .card {{
        width: min(480px, 100%);
        padding: 34px 32px 30px;
        border: 1px solid #dbe4ef;
        border-radius: 18px;
        background: rgba(255, 255, 255, 0.92);
        text-align: center;
      }}
      .icon {{
        width: 64px;
        height: 64px;
        margin: 0 auto 18px;
        display: grid;
        place-items: center;
        border-radius: 999px;
        font-size: 34px;
        font-weight: 800;
      }}
      .success .icon {{
        color: #047857;
        background: linear-gradient(135deg, #d1fae5, #ecfdf5);
      }}
      .error .icon {{
        color: #dc2626;
        background: linear-gradient(135deg, #fee2e2, #fff1f2);
      }}
      h1 {{
        margin: 0;
        font-size: 28px;
        line-height: 1.25;
        letter-spacing: -0.02em;
      }}
      p {{
        margin: 14px 0 0;
        color: #526174;
        font-size: 16px;
        line-height: 1.7;
      }}
      .actions {{
        margin-top: 24px;
        display: flex;
        justify-content: center;
        gap: 12px;
      }}
      button {{
        border: 0;
        border-radius: 10px;
        padding: 11px 18px;
        cursor: pointer;
        color: #fff;
        font-weight: 700;
        background: linear-gradient(135deg, #2563eb, #0ea5a8);
      }}
      .hint {{
        margin-top: 16px;
        color: #8a97a8;
        font-size: 13px;
      }}
      .hint.blocked {{
        padding: 10px 12px;
        border: 1px solid #fed7aa;
        border-radius: 10px;
        color: #9a3412;
        background: #fff7ed;
      }}
      .countdown {{
        margin-top: 14px;
        color: #64748b;
        font-size: 14px;
      }}
    </style>
  </head>
  <body>
    <main class="card {status_class}">
      <div class="icon">{icon}</div>
      <h1>{title}</h1>
      <p>{description}</p>
      {countdown_html}
      <div class="actions">
        <button id="closeButton" type="button">关闭页面</button>
      </div>
      <div id="closeHint" class="hint"><span id="closeStatus">如果浏览器拦截自动关闭，可以直接关闭此标签页。</span></div>
    </main>
    {auto_close_script}
  </body>
</html>"#,
    )
}

fn bind_oauth_callback_listener() -> Result<TcpListener, String> {
    let mut last_error = None;
    for _ in 0..20 {
        match TcpListener::bind(("127.0.0.1", oauth::CALLBACK_PORT)) {
            Ok(listener) => return Ok(listener),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                return Err(format!(
                    "OAuth 回调端口 {} 启动失败: {}",
                    oauth::CALLBACK_PORT,
                    error
                ));
            }
        }
    }
    Err(format!(
        "OAuth 回调端口 {} 启动失败: {}",
        oauth::CALLBACK_PORT,
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "端口被占用".to_string())
    ))
}

fn open_path_with_system(path: &PathBuf) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .arg(path)
            .status()
            .map_err(|error| format!("打开文件夹失败: {}", error))?;
        if status.success() {
            Ok(())
        } else {
            Err("打开文件夹失败".to_string())
        }
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer.exe");
        command.arg(path);
        hide_command_window(&mut command);
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("打开文件夹失败: {}", error))
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let status = Command::new("xdg-open")
            .arg(path)
            .status()
            .map_err(|error| format!("打开文件夹失败: {}", error))?;
        if status.success() {
            Ok(())
        } else {
            Err("打开文件夹失败".to_string())
        }
    }
}

fn open_url_with_system(url: &str) -> Result<std::process::ExitStatus, String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .status()
            .map_err(|error| format!("打开浏览器失败: {}", error))
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("powershell");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "Start-Process -FilePath $args[0]",
            ])
            .arg(url);
        hide_command_window(&mut command);
        command
            .status()
            .map_err(|error| format!("打开浏览器失败: {}", error))
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .status()
            .map_err(|error| format!("打开浏览器失败: {}", error))
    }
}

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[tauri::command]
fn delete_codex_account(account_id: String) -> Result<(), String> {
    AccountStore::default().delete_account(&account_id)
}

#[tauri::command]
async fn switch_codex_account(account_id: String) -> Result<CodexAccount, String> {
    let accounts = AccountStore::default().list_accounts()?;
    let target = accounts
        .iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| "账号不存在".to_string())?;
    let oauth_account_id = if target.auth_mode.as_deref() == Some("apikey") {
        target.bound_oauth_account_id.clone()
    } else {
        Some(target.id.clone())
    };
    if let Some(oauth_account_id) = oauth_account_id {
        token_keeper::ensure_fresh_access_token(&oauth_account_id, "切换账号前 Token 需要续期")
            .await?;
    }
    AccountStore::default().switch_account(&account_id)
}

#[tauri::command]
fn restart_codex_app() -> Result<String, String> {
    AccountStore::default().restart_codex_app()
}

#[tauri::command]
fn get_codex_switcher_settings() -> Result<CodexSwitcherSettings, String> {
    let _settings_guard = lock_switcher_settings()?;
    read_switcher_settings()
}

#[tauri::command]
fn update_codex_switcher_settings(
    settings: CodexSwitcherSettings,
) -> Result<CodexSwitcherSettings, String> {
    let _settings_guard = lock_switcher_settings()?;
    let settings = settings.normalized();
    write_switcher_settings(&settings)?;
    Ok(settings)
}

fn codex_config_file_path(codex_home: &Path, kind: CodexConfigFileKind) -> PathBuf {
    codex_home.join(kind.file_name())
}

fn read_codex_config_file_from(
    codex_home: &Path,
    kind: CodexConfigFileKind,
) -> Result<CodexConfigFileContent, String> {
    let path = codex_config_file_path(codex_home, kind);
    let exists = path.is_file();
    let content = if exists {
        std::fs::read_to_string(&path)
            .map_err(|error| format!("读取 Codex {} 失败: {}", kind.file_name(), error))?
    } else {
        kind.default_content().to_string()
    };
    Ok(CodexConfigFileContent {
        kind: kind.key().to_string(),
        name: kind.file_name().to_string(),
        path: path.to_string_lossy().to_string(),
        content,
        exists,
    })
}

fn parse_auth_json_object(content: &str) -> Result<Value, String> {
    let value = serde_json::from_str::<Value>(content)
        .map_err(|error| format!("auth.json JSON 格式错误: {}", error))?;
    if !value.is_object() {
        return Err("auth.json 顶层必须是 JSON 对象".to_string());
    }
    Ok(value)
}

fn validate_codex_config_content(kind: CodexConfigFileKind, content: &str) -> Result<(), String> {
    match kind {
        CodexConfigFileKind::AuthJson => {
            parse_auth_json_object(content)?;
        }
        CodexConfigFileKind::ConfigToml => {
            if !content.trim().is_empty() {
                content
                    .parse::<toml_edit::Document>()
                    .map_err(|error| format!("config.toml TOML 格式错误: {}", error))?;
            }
        }
    }
    Ok(())
}

fn format_codex_config_content(kind: CodexConfigFileKind, content: &str) -> Result<String, String> {
    match kind {
        CodexConfigFileKind::AuthJson => {
            let value = parse_auth_json_object(content)?;
            serde_json::to_string_pretty(&value)
                .map(|formatted| format!("{}\n", formatted))
                .map_err(|error| format!("格式化 auth.json 失败: {}", error))
        }
        CodexConfigFileKind::ConfigToml => {
            if content.trim().is_empty() {
                return Ok(String::new());
            }
            let document = content
                .parse::<toml_edit::Document>()
                .map_err(|error| format!("config.toml TOML 格式错误: {}", error))?;
            let formatted = document.to_string();
            Ok(if formatted.ends_with('\n') {
                formatted
            } else {
                format!("{}\n", formatted)
            })
        }
    }
}

fn read_file_snapshot(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    std::fs::read(path)
        .map(Some)
        .map_err(|error| format!("读取回滚快照失败 ({}): {}", path.display(), error))
}

fn restore_file_snapshot(path: &Path, snapshot: Option<&[u8]>) -> Result<(), String> {
    if let Some(content) = snapshot {
        return write_bytes_atomic(path, content);
    }
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|error| format!("移除新增配置失败 ({}): {}", path.display(), error))?;
    }
    Ok(())
}

fn rollback_file_on_error<T>(
    result: Result<T, String>,
    path: &Path,
    snapshot: Option<&[u8]>,
) -> Result<T, String> {
    match result {
        Ok(value) => Ok(value),
        Err(state_error) => match restore_file_snapshot(path, snapshot) {
            Ok(()) => Err(state_error),
            Err(rollback_error) => Err(format!(
                "{}；回滚 {} 失败: {}",
                state_error,
                path.display(),
                rollback_error
            )),
        },
    }
}

fn write_codex_config_file_to(
    codex_home: &Path,
    kind: CodexConfigFileKind,
    content: &str,
) -> Result<CodexConfigFileContent, String> {
    validate_codex_config_content(kind, content)?;
    std::fs::create_dir_all(codex_home)
        .map_err(|error| format!("创建 Codex 目录失败: {}", error))?;
    let path = codex_config_file_path(codex_home, kind);
    let backup_path = if path.is_file() {
        let backup_path = codex_config_backup_path(codex_home, kind);
        if let Some(parent) = backup_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("创建 Codex 配置备份目录失败: {}", error))?;
        }
        std::fs::copy(&path, &backup_path).map_err(|error| {
            format!(
                "备份 Codex {} 失败 ({}): {}",
                kind.file_name(),
                backup_path.display(),
                error
            )
        })?;
        Some(backup_path)
    } else {
        None
    };
    if let Err(error) = write_bytes_atomic(&path, content.as_bytes()) {
        if let Some(backup_path) = backup_path {
            let _ = std::fs::copy(backup_path, &path);
        }
        return Err(format!("保存 Codex {} 失败: {}", kind.file_name(), error));
    }
    read_codex_config_file_from(codex_home, kind)
}

#[tauri::command]
fn read_codex_config_file(file_kind: String) -> Result<CodexConfigFileContent, String> {
    let kind = CodexConfigFileKind::parse(&file_kind)?;
    read_codex_config_file_from(&default_codex_home(), kind)
}

#[tauri::command]
fn format_codex_config_file(file_kind: String, content: String) -> Result<String, String> {
    let kind = CodexConfigFileKind::parse(&file_kind)?;
    format_codex_config_content(kind, &content)
}

#[tauri::command]
fn write_codex_config_file(
    file_kind: String,
    content: String,
) -> Result<CodexConfigFileContent, String> {
    let kind = CodexConfigFileKind::parse(&file_kind)?;
    let codex_home = default_codex_home();
    let path = codex_config_file_path(&codex_home, kind);
    let snapshot = (kind == CodexConfigFileKind::ConfigToml)
        .then(|| read_file_snapshot(&path))
        .transpose()?
        .flatten();
    let written = write_codex_config_file_to(&codex_home, kind, &content)?;
    if kind == CodexConfigFileKind::ConfigToml {
        rollback_file_on_error(
            AccountStore::default().release_current_api_key_default_model(),
            &path,
            snapshot.as_deref(),
        )?;
    }
    Ok(written)
}

#[tauri::command]
fn reset_codex_config_toml() -> Result<bool, String> {
    let path = default_codex_home().join("config.toml");
    let snapshot = read_file_snapshot(&path)?;
    let removed = if path.exists() {
        std::fs::remove_file(&path).map_err(|error| {
            format!(
                "删除 Codex config.toml 失败 ({}): {}",
                path.display(),
                error
            )
        })?;
        true
    } else {
        false
    };
    rollback_file_on_error(
        AccountStore::default().release_current_api_key_default_model(),
        &path,
        snapshot.as_deref(),
    )?;
    Ok(removed)
}

#[tauri::command]
async fn codex_get_usage_dashboard(
    start_date: Option<i64>,
    end_date: Option<i64>,
    page: Option<usize>,
    page_size: Option<usize>,
    refresh: Option<bool>,
) -> Result<CodexUsageDashboard, String> {
    tauri::async_runtime::spawn_blocking(move || {
        usage::get_codex_usage_dashboard(start_date, end_date, page, page_size, refresh)
    })
    .await
    .map_err(|error| format!("统计任务执行失败: {}", error))?
}

#[tauri::command]
async fn codex_get_usage_activity(refresh: Option<bool>) -> Result<CodexUsageActivity, String> {
    tauri::async_runtime::spawn_blocking(move || usage::get_codex_usage_activity(refresh))
        .await
        .map_err(|error| format!("活动统计任务执行失败: {}", error))?
}

#[tauri::command]
fn codex_list_model_pricing() -> Result<Vec<CodexUsagePricing>, String> {
    usage::list_model_pricing()
}

#[tauri::command]
fn codex_update_model_pricing(pricing: CodexUsagePricing) -> Result<(), String> {
    usage::update_model_pricing(pricing)
}

#[tauri::command]
fn codex_delete_model_pricing(model_id: String) -> Result<(), String> {
    usage::delete_model_pricing(&model_id)
}

#[tauri::command]
fn codex_reset_model_pricing() -> Result<Vec<CodexUsagePricing>, String> {
    usage::reset_model_pricing()
}

#[tauri::command]
fn codex_get_pricing_config() -> Result<Vec<CodexUsagePricingConfig>, String> {
    usage::get_pricing_config()
}

#[tauri::command]
fn codex_update_pricing_config(
    configs: Vec<CodexUsagePricingConfig>,
) -> Result<Vec<CodexUsagePricingConfig>, String> {
    usage::update_pricing_config(configs)
}

#[tauri::command]
fn get_codex_switcher_paths() -> Result<CodexSwitcherPaths, String> {
    ensure_switcher_data_dirs()?;
    let app_dir = switcher_data_dir();
    let account_dir = switcher_account_dir();
    let session_dir = switcher_session_dir();
    let statistics_dir = switcher_statistics_dir();
    let data_dir = switcher_config_data_dir();
    let backup_dir = switcher_backup_dir();
    Ok(CodexSwitcherPaths {
        app_dir: app_dir.to_string_lossy().to_string(),
        accounts_json: account_dir
            .join("accounts.json")
            .to_string_lossy()
            .to_string(),
        settings_json: data_dir.join("settings.json").to_string_lossy().to_string(),
        backup_dir: backup_dir.to_string_lossy().to_string(),
        account_dir: account_dir.to_string_lossy().to_string(),
        session_dir: session_dir.to_string_lossy().to_string(),
        statistics_dir: statistics_dir.to_string_lossy().to_string(),
        data_dir: data_dir.to_string_lossy().to_string(),
        codex_home: default_codex_home().to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn export_codex_switcher_backup(app_handle: AppHandle) -> Result<CodexSwitcherBackupFile, String> {
    export_codex_switcher_backup_with_progress(&app_handle, |_, _| {})
}

#[tauri::command]
fn start_codex_switcher_backup(app_handle: AppHandle, task_id: String) -> Result<String, String> {
    let task_id = task_id.trim().to_string();
    if task_id.is_empty() {
        return Err("备份任务 ID 不能为空".to_string());
    }
    let emit_task_id = task_id.clone();
    thread::spawn(move || {
        emit_backup_progress(
            &app_handle,
            &emit_task_id,
            "running",
            1,
            "正在准备备份任务...",
            None,
        );
        let result =
            export_codex_switcher_backup_with_progress(&app_handle, |progress, message| {
                emit_backup_progress(
                    &app_handle,
                    &emit_task_id,
                    "running",
                    progress,
                    message,
                    None,
                );
            });
        match result {
            Ok(backup_file) => emit_backup_progress(
                &app_handle,
                &emit_task_id,
                "completed",
                100,
                "备份完成",
                Some(backup_file),
            ),
            Err(error) => {
                emit_backup_progress(&app_handle, &emit_task_id, "failed", 100, &error, None)
            }
        }
    });
    Ok(task_id)
}

#[tauri::command]
fn start_codex_switcher_session_backup(
    app_handle: AppHandle,
    task_id: String,
) -> Result<String, String> {
    let task_id = task_id.trim().to_string();
    if task_id.is_empty() {
        return Err("备份任务 ID 不能为空".to_string());
    }
    let emit_task_id = task_id.clone();
    thread::spawn(move || {
        emit_backup_progress(
            &app_handle,
            &emit_task_id,
            "running",
            1,
            "正在准备会话备份任务...",
            None,
        );
        let result = export_codex_switcher_session_backup_with_progress(|progress, message| {
            emit_backup_progress(
                &app_handle,
                &emit_task_id,
                "running",
                progress,
                message,
                None,
            );
        });
        match result {
            Ok(backup_file) => emit_backup_progress(
                &app_handle,
                &emit_task_id,
                "completed",
                100,
                "会话备份完成",
                Some(backup_file),
            ),
            Err(error) => {
                emit_backup_progress(&app_handle, &emit_task_id, "failed", 100, &error, None)
            }
        }
    });
    Ok(task_id)
}

fn emit_backup_progress(
    app_handle: &AppHandle,
    task_id: &str,
    status: &str,
    progress: u8,
    message: &str,
    backup_file: Option<CodexSwitcherBackupFile>,
) {
    let _ = app_handle.emit(
        "codex-switcher-backup-progress",
        CodexSwitcherBackupProgressEvent {
            task_id: task_id.to_string(),
            status: status.to_string(),
            progress: progress.min(100),
            message: message.to_string(),
            backup_file,
        },
    );
}

fn with_switcher_data_maintenance<T>(
    app_handle: &AppHandle,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let runtime = app_handle.state::<push::PushRuntimeState>();
    // Backup and restore must acquire shared data locks in this single order.
    let _run_guard = push::begin_run(&runtime)?;
    let _push_settings_guard = push::lock_settings(&runtime)?;
    let _settings_guard = lock_switcher_settings()?;
    let _account_guard = account::lock_account_database_mutation()?;
    let _push_database_guard = push::lock_push_database()?;
    let _usage_guard = usage::lock_usage_data()?;
    push::checkpoint_push_database_for_backup()?;
    usage::checkpoint_usage_database_for_backup()?;
    operation()
}

fn export_codex_switcher_backup_with_progress<F>(
    app_handle: &AppHandle,
    mut progress: F,
) -> Result<CodexSwitcherBackupFile, String>
where
    F: FnMut(u8, &str),
{
    with_switcher_data_maintenance(app_handle, || {
        progress(5, "正在读取账号、设置与会话信息...");
        let codex_session_summary = codex_session_backup_summary(&default_codex_home());
        let statistics_summary = switcher_statistics_backup_summary();
        let backup = serde_json::json!({
            "app": "Codex Switcher",
            "version": 2,
            "exportedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "accounts": list_codex_accounts()?,
            "currentAccount": get_current_codex_account()?,
            "settings": read_switcher_settings()?,
            "codexSessions": codex_session_summary,
            "statistics": statistics_summary,
        });
        progress(15, "正在生成备份清单...");
        let content = serde_json::to_string_pretty(&backup)
            .map_err(|error| format!("序列化备份失败: {}", error))?;
        let backup_dir = switcher_backup_dir();
        std::fs::create_dir_all(&backup_dir)
            .map_err(|error| format!("创建备份目录失败: {}", error))?;
        let filename = format!(
            "codex-switcher-backup-{}.zip",
            chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")
        );
        let backup_path = backup_dir.join(filename);
        write_switcher_backup_zip(&backup_path, &content, &mut progress)?;
        progress(98, "正在刷新备份文件信息...");
        backup_file_info(&backup_path)
    })
}

fn export_codex_switcher_session_backup_with_progress<F>(
    mut progress: F,
) -> Result<CodexSwitcherBackupFile, String>
where
    F: FnMut(u8, &str),
{
    progress(5, "正在读取 Codex 会话信息...");
    let codex_home = default_codex_home();
    let backup = serde_json::json!({
        "app": "Codex Switcher",
        "kind": "codexSessions",
        "version": 1,
        "exportedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "codexSessions": codex_session_backup_summary(&codex_home),
    });
    progress(15, "正在生成会话备份清单...");
    let content = serde_json::to_string_pretty(&backup)
        .map_err(|error| format!("序列化会话备份失败: {}", error))?;
    let backup_dir = switcher_session_dir();
    std::fs::create_dir_all(&backup_dir)
        .map_err(|error| format!("创建会话备份目录失败: {}", error))?;
    let filename = format!(
        "codex-session-backup-{}.zip",
        chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")
    );
    let backup_path = backup_dir.join(filename);
    write_session_backup_zip(&backup_path, &content, &mut progress)?;
    progress(98, "正在刷新会话备份文件信息...");
    backup_file_info(&backup_path)
}

#[tauri::command]
fn list_codex_switcher_backups() -> Result<Vec<CodexSwitcherBackupFile>, String> {
    list_backup_files_in_dir(&switcher_backup_dir(), "备份")
}

#[tauri::command]
fn list_codex_switcher_session_backups() -> Result<Vec<CodexSwitcherBackupFile>, String> {
    list_backup_files_in_dir(&switcher_session_dir(), "会话备份")
}

fn list_backup_files_in_dir(
    backup_dir: &Path,
    label: &str,
) -> Result<Vec<CodexSwitcherBackupFile>, String> {
    std::fs::create_dir_all(backup_dir)
        .map_err(|error| format!("创建{}目录失败: {}", label, error))?;
    let mut backups = Vec::new();
    for entry in std::fs::read_dir(backup_dir)
        .map_err(|error| format!("读取{}目录失败: {}", label, error))?
    {
        let path = entry
            .map_err(|error| format!("读取备份文件失败: {}", error))?
            .path();
        if path.extension().and_then(|value| value.to_str()) != Some("zip") {
            continue;
        }
        backups.push(backup_file_info(&path)?);
    }
    backups.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(backups)
}

#[tauri::command]
fn restore_codex_switcher_backup(
    app_handle: AppHandle,
    backup_path: String,
) -> Result<Vec<CodexAccount>, String> {
    let path = validate_backup_zip_path(&backup_path)?;
    let file = std::fs::File::open(&path).map_err(|error| format!("打开备份失败: {}", error))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("读取 ZIP 备份失败: {}", error))?;
    let mut backup_json = String::new();
    archive
        .by_name("backup.json")
        .map_err(|error| format!("备份中缺少 backup.json: {}", error))?
        .read_to_string(&mut backup_json)
        .map_err(|error| format!("读取备份 JSON 失败: {}", error))?;
    let backup_value: Value = serde_json::from_str(&backup_json)
        .map_err(|error| format!("备份 JSON 解析失败: {}", error))?;
    let restored_settings = backup_value
        .get("settings")
        .cloned()
        .and_then(|value| serde_json::from_value::<CodexSwitcherSettings>(value).ok());
    let accounts_value = backup_value
        .get("accounts")
        .cloned()
        .unwrap_or_else(|| backup_value.clone());
    let accounts_json = serde_json::to_string(&accounts_value)
        .map_err(|error| format!("备份账号解析失败: {}", error))?;
    let mut prepared =
        prepare_backup_archive_files_with(&mut archive, backup_entry_restore_target)?;
    validate_prepared_switcher_backup(&mut prepared)?;
    let restores_account_database = prepared
        .entries
        .iter()
        .any(|entry| entry.target == switcher_account_dir().join("accounts.json"));

    with_switcher_data_maintenance(&app_handle, || {
        remove_restored_database_sidecars(&prepared)?;
        apply_prepared_backup_archive(&prepared)?;
        if let Some(settings) = restored_settings.as_ref() {
            write_switcher_settings(settings)?;
        }
        let store = AccountStore::default();
        if restores_account_database {
            store.list_accounts()
        } else {
            store.import_from_json_while_locked(&accounts_json)
        }
    })
}

#[tauri::command]
fn restore_codex_switcher_session_backup(backup_path: String) -> Result<(), String> {
    let path = validate_session_backup_zip_path(&backup_path)
        .or_else(|_| validate_backup_zip_path(&backup_path))?;
    let file = std::fs::File::open(&path).map_err(|error| format!("打开备份失败: {}", error))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("读取 ZIP 备份失败: {}", error))?;
    let prepared =
        prepare_backup_archive_files_with(&mut archive, backup_session_entry_restore_target)?;
    apply_prepared_backup_archive(&prepared)
}

#[tauri::command]
fn delete_codex_switcher_backup(backup_path: String) -> Result<(), String> {
    let path = validate_backup_zip_path(&backup_path)?;
    std::fs::remove_file(&path).map_err(|error| format!("删除备份失败: {}", error))?;
    Ok(())
}

#[tauri::command]
fn import_codex_switcher_backup(json_content: String) -> Result<Vec<CodexAccount>, String> {
    let value: Value = serde_json::from_str(&json_content)
        .map_err(|error| format!("备份 JSON 解析失败: {}", error))?;
    if let Some(settings_value) = value.get("settings") {
        if let Ok(settings) =
            serde_json::from_value::<CodexSwitcherSettings>(settings_value.clone())
        {
            let _settings_guard = lock_switcher_settings()?;
            write_switcher_settings(&settings)?;
        }
    }
    let accounts_value = value.get("accounts").cloned().unwrap_or(value);
    import_codex_from_json(
        serde_json::to_string(&accounts_value)
            .map_err(|error| format!("备份账号解析失败: {}", error))?,
    )
}

#[tauri::command]
async fn refresh_codex_quota(account_id: String) -> Result<CodexAccount, String> {
    match fetch_codex_quota_for_account(&account_id).await {
        Ok(quota) => AccountStore::default().update_account_quota(&account_id, quota),
        Err(error) => {
            let _ = AccountStore::default().update_account_quota_error(&account_id, error.clone());
            Err(error)
        }
    }
}

#[tauri::command]
async fn refresh_all_codex_quotas() -> Result<i32, String> {
    let accounts = AccountStore::default().list_accounts()?;
    let mut count = 0;
    for account in accounts {
        if account.auth_mode.as_deref() == Some("apikey")
            || account
                .openai_api_key
                .as_deref()
                .is_some_and(|key| !key.trim().is_empty())
        {
            continue;
        }
        match fetch_codex_quota_for_account(&account.id).await {
            Ok(quota) => {
                if AccountStore::default()
                    .update_account_quota(&account.id, quota)
                    .is_ok()
                {
                    count += 1;
                }
            }
            Err(error) => {
                let _ = AccountStore::default().update_account_quota_error(&account.id, error);
            }
        }
    }
    Ok(count)
}

#[tauri::command]
async fn consume_codex_reset_credit(account_id: String) -> Result<CodexAccount, String> {
    let source = refreshed_quota_source_account(&account_id).await?;
    let access_token = source.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err("OAuth access_token 为空，无法重置额度".to_string());
    }
    let redeem_request_id = format!(
        "{}-{}",
        chrono::Utc::now().timestamp_millis(),
        rand::thread_rng().gen::<u64>()
    );
    let response = reqwest::Client::new()
        .post("https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume")
        .bearer_auth(access_token)
        .header("OpenAI-Beta", "codex-1")
        .header("Referer", "https://chatgpt.com/")
        .header("User-Agent", "Mozilla/5.0")
        .json(&serde_json::json!({ "redeem_request_id": redeem_request_id }))
        .send()
        .await
        .map_err(|error| format!("请求重置额度失败: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取重置额度响应失败: {}", error))?;
    if !status.is_success() {
        return Err(format!(
            "重置额度接口返回 {}: {}",
            status,
            compact_http_body(&body)
        ));
    }
    refresh_codex_quota(account_id).await
}

#[tauri::command]
fn codex_list_sessions_across_instances(
    title_query: Option<String>,
    content_query: Option<String>,
) -> Result<Vec<CodexSessionRecord>, String> {
    SessionStore::default().list_sessions(title_query, content_query)
}

#[tauri::command]
fn codex_get_session_token_stats_across_instances(
    session_ids: Vec<String>,
) -> Result<Vec<CodexSessionTokenStats>, String> {
    SessionStore::default().token_stats(&session_ids)
}

#[tauri::command]
fn codex_move_sessions_to_trash_across_instances(
    session_ids: Vec<String>,
) -> Result<CodexSessionTrashSummary, String> {
    SessionStore::default().move_to_trash(&session_ids)
}

#[tauri::command]
fn codex_list_trashed_sessions_across_instances() -> Result<Vec<CodexTrashedSessionRecord>, String>
{
    SessionStore::default().list_trashed()
}

#[tauri::command]
fn codex_restore_sessions_from_trash_across_instances(
    session_ids: Vec<String>,
) -> Result<CodexSessionTrashSummary, String> {
    SessionStore::default().restore_from_trash(&session_ids)
}

#[tauri::command]
fn codex_repair_session_visibility_across_instances(
    mode: Option<String>,
    run_id: Option<String>,
    target_provider: Option<String>,
    target_instance_id: Option<String>,
    repair_instance_ids: Option<Vec<String>>,
    session_ids: Option<Vec<String>>,
) -> Result<CodexSessionVisibilityRepairSummary, String> {
    let _ = run_id;
    SessionStore::default().repair_visibility_with_options(
        mode.as_deref(),
        target_provider,
        target_instance_id,
        repair_instance_ids,
        session_ids,
    )
}

#[tauri::command]
fn codex_list_session_visibility_repair_instances(
) -> Result<CodexSessionVisibilityRepairInstanceList, String> {
    SessionStore::default().list_visibility_repair_instances()
}

#[tauri::command]
fn codex_list_session_visibility_repair_providers(
) -> Result<CodexSessionVisibilityRepairProviderList, String> {
    SessionStore::default().list_visibility_repair_providers()
}

fn switcher_data_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("CODEX_SWITCHER_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return path;
    }
    default_switcher_data_dir()
}

#[cfg(test)]
fn default_switcher_data_dir() -> PathBuf {
    test_switcher_data_dir()
}

#[cfg(not(test))]
fn default_switcher_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join(".codex_switcher")
}

#[cfg(test)]
fn test_switcher_data_dir() -> PathBuf {
    let thread_id = format!("{:?}", std::thread::current().id())
        .chars()
        .filter(|value| value.is_ascii_alphanumeric())
        .collect::<String>();
    std::env::temp_dir()
        .join("codex-switcher-tests")
        .join(format!("{}-{}", std::process::id(), thread_id))
}

fn switcher_account_dir() -> PathBuf {
    switcher_data_dir().join("account")
}

fn switcher_session_dir() -> PathBuf {
    switcher_data_dir().join("session")
}

fn switcher_session_trash_dir() -> PathBuf {
    switcher_data_dir().join("session-trash")
}

fn codex_session_trash_dir(codex_home: &Path) -> PathBuf {
    switcher_session_trash_dir().join(short_hash(&codex_home.to_string_lossy()))
}

fn switcher_statistics_dir() -> PathBuf {
    switcher_data_dir().join("statistics")
}

fn switcher_config_data_dir() -> PathBuf {
    switcher_data_dir().join("data")
}

fn switcher_config_backup_dir() -> PathBuf {
    switcher_data_dir().join("config-backups")
}

fn codex_config_backup_path(codex_home: &Path, kind: CodexConfigFileKind) -> PathBuf {
    switcher_config_backup_dir()
        .join(short_hash(&codex_home.to_string_lossy()))
        .join(format!("{}.codex-switcher.bak", kind.file_name()))
}

fn switcher_backup_dir() -> PathBuf {
    switcher_data_dir().join("backup")
}

fn short_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .take(12)
        .collect()
}

fn ensure_switcher_data_dirs() -> Result<(), String> {
    for dir in [
        switcher_data_dir(),
        switcher_account_dir(),
        switcher_session_dir(),
        switcher_statistics_dir(),
        switcher_config_data_dir(),
        switcher_backup_dir(),
    ] {
        std::fs::create_dir_all(&dir).map_err(|error| {
            format!(
                "创建 Codex Switcher 数据目录失败 ({}): {}",
                dir.display(),
                error
            )
        })?;
    }
    Ok(())
}

fn write_switcher_backup_zip<F>(
    backup_path: &Path,
    backup_json: &str,
    progress: &mut F,
) -> Result<(), String>
where
    F: FnMut(u8, &str),
{
    progress(20, "正在创建 ZIP 文件...");
    let file = std::fs::File::create(backup_path)
        .map_err(|error| format!("创建 ZIP 备份失败: {}", error))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    zip.start_file("backup.json", options)
        .map_err(|error| format!("写入备份 JSON 失败: {}", error))?;
    zip.write_all(backup_json.as_bytes())
        .map_err(|error| format!("写入备份 JSON 失败: {}", error))?;
    progress(30, "正在写入 Codex Switcher 数据...");
    let root = switcher_data_dir();
    let excluded_dirs = vec![switcher_backup_dir(), switcher_session_dir()];
    add_directory_to_backup_zip(&mut zip, &root, &root, "data", &excluded_dirs, options)?;
    progress(50, "正在写入 Codex 会话记录...");
    add_codex_sessions_to_backup_zip(&mut zip, options, progress)?;
    progress(92, "正在压缩并完成 ZIP...");
    zip.finish()
        .map_err(|error| format!("完成 ZIP 备份失败: {}", error))?;
    Ok(())
}

fn write_session_backup_zip<F>(
    backup_path: &Path,
    backup_json: &str,
    progress: &mut F,
) -> Result<(), String>
where
    F: FnMut(u8, &str),
{
    progress(20, "正在创建会话 ZIP 文件...");
    let file = std::fs::File::create(backup_path)
        .map_err(|error| format!("创建会话 ZIP 备份失败: {}", error))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    zip.start_file("backup.json", options)
        .map_err(|error| format!("写入会话备份 JSON 失败: {}", error))?;
    zip.write_all(backup_json.as_bytes())
        .map_err(|error| format!("写入会话备份 JSON 失败: {}", error))?;
    progress(35, "正在写入 Codex 会话数据...");
    add_codex_sessions_to_backup_zip(&mut zip, options, progress)?;
    progress(92, "正在压缩并完成会话 ZIP...");
    zip.finish()
        .map_err(|error| format!("完成会话 ZIP 备份失败: {}", error))?;
    Ok(())
}

fn add_directory_to_backup_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    current: &Path,
    archive_prefix: &str,
    excluded_dirs: &[PathBuf],
    options: zip::write::FileOptions,
) -> Result<(), String> {
    if !current.exists() || excluded_dirs.iter().any(|dir| current.starts_with(dir)) {
        return Ok(());
    }
    for entry in std::fs::read_dir(current)
        .map_err(|error| format!("读取备份数据目录失败 ({}): {}", current.display(), error))?
    {
        let path = entry
            .map_err(|error| format!("读取备份数据文件失败: {}", error))?
            .path();
        if excluded_dirs.iter().any(|dir| path.starts_with(dir)) {
            continue;
        }
        if path.is_dir() {
            add_directory_to_backup_zip(zip, root, &path, archive_prefix, excluded_dirs, options)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| format!("计算备份相对路径失败: {}", error))?;
        let name = format!(
            "{}/{}",
            archive_prefix.trim_matches('/'),
            relative.to_string_lossy().replace('\\', "/")
        );
        add_file_to_backup_zip(zip, &path, &name, options)?;
    }
    Ok(())
}

fn add_directory_to_backup_zip_skipping_shadow(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    current: &Path,
    shadow_root: &Path,
    archive_prefix: &str,
    options: zip::write::FileOptions,
) -> Result<(), String> {
    if !current.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(current)
        .map_err(|error| format!("读取备份数据目录失败 ({}): {}", current.display(), error))?
    {
        let path = entry
            .map_err(|error| format!("读取备份数据文件失败: {}", error))?
            .path();
        let relative = path
            .strip_prefix(root)
            .map_err(|error| format!("计算备份相对路径失败: {}", error))?;
        if path.is_dir() {
            add_directory_to_backup_zip_skipping_shadow(
                zip,
                root,
                &path,
                shadow_root,
                archive_prefix,
                options,
            )?;
            continue;
        }
        if shadow_root.join(relative).exists() {
            continue;
        }
        let name = format!(
            "{}/{}",
            archive_prefix.trim_matches('/'),
            relative.to_string_lossy().replace('\\', "/")
        );
        add_file_to_backup_zip(zip, &path, &name, options)?;
    }
    Ok(())
}

fn add_file_to_backup_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    path: &Path,
    archive_name: &str,
    options: zip::write::FileOptions,
) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    zip.start_file(archive_name, options)
        .map_err(|error| format!("写入备份文件失败: {}", error))?;
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("读取备份文件失败: {}", error))?;
    std::io::copy(&mut file, zip).map_err(|error| format!("写入备份文件失败: {}", error))?;
    Ok(())
}

fn add_codex_sessions_to_backup_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    options: zip::write::FileOptions,
    progress: &mut impl FnMut(u8, &str),
) -> Result<(), String> {
    let codex_home = default_codex_home();
    migrate_legacy_session_trash(&codex_home);
    progress(55, "正在备份会话文件...");
    add_directory_to_backup_zip(
        zip,
        &codex_home.join("sessions"),
        &codex_home.join("sessions"),
        "codex/sessions",
        &[],
        options,
    )?;
    progress(82, "正在备份会话回收站...");
    let session_trash_dir = codex_session_trash_dir(&codex_home);
    add_directory_to_backup_zip(
        zip,
        &session_trash_dir,
        &session_trash_dir,
        "codex/session-trash",
        &[],
        options,
    )?;
    add_directory_to_backup_zip_skipping_shadow(
        zip,
        &legacy_session_trash_dir(&codex_home),
        &legacy_session_trash_dir(&codex_home),
        &session_trash_dir,
        "codex/session-trash",
        options,
    )?;
    progress(88, "正在备份会话索引...");
    for filename in ["session_index.jsonl", "session_index.jsonl.bak"] {
        add_file_to_backup_zip(
            zip,
            &codex_home.join(filename),
            &format!("codex/{}", filename),
            options,
        )?;
    }
    Ok(())
}

fn codex_session_backup_summary(codex_home: &Path) -> Value {
    migrate_legacy_session_trash(codex_home);
    serde_json::json!({
        "sessionsDir": codex_home.join("sessions").to_string_lossy().to_string(),
        "sessionFiles": count_files_under(&codex_home.join("sessions")),
        "trashedSessionFiles": count_session_trash_backup_files(codex_home),
        "includesSessionIndex": codex_home.join("session_index.jsonl").is_file(),
    })
}

fn legacy_session_trash_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(".codex-switcher").join("session-trash")
}

fn migrate_legacy_session_trash(codex_home: &Path) {
    migrate_directory_files(
        &legacy_session_trash_dir(codex_home),
        &codex_session_trash_dir(codex_home),
    );
}

fn migrate_directory_files(from: &Path, to: &Path) {
    if !from.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    if std::fs::create_dir_all(to).is_err() {
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
        if std::fs::rename(&source, &target).is_err() && std::fs::copy(&source, &target).is_ok() {
            let _ = std::fs::remove_file(&source);
        }
    }
}

fn count_session_trash_backup_files(codex_home: &Path) -> usize {
    let primary = codex_session_trash_dir(codex_home);
    let legacy = legacy_session_trash_dir(codex_home);
    count_files_under(&primary) + count_files_under_skipping_shadow(&legacy, &legacy, &primary)
}

fn switcher_statistics_backup_summary() -> Value {
    let statistics_dir = switcher_statistics_dir();
    let includes_usage_json_cache = statistics_dir.join("usage_logs.json").is_file();
    let includes_usage_database = statistics_dir.join("usage.sqlite").is_file();
    serde_json::json!({
        "statisticsDir": statistics_dir.to_string_lossy().to_string(),
        "files": count_files_under(&statistics_dir),
        "includesUsageCache": includes_usage_json_cache || includes_usage_database,
        "includesUsageJsonCache": includes_usage_json_cache,
        "includesUsageDatabase": includes_usage_database,
        "includesPricing": statistics_dir.join("pricing.json").is_file(),
        "includesPricingConfig": statistics_dir.join("pricing_config.json").is_file(),
    })
}

fn count_files_under(path: &Path) -> usize {
    if !path.exists() {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                count_files_under(&path)
            } else if path.is_file() {
                1
            } else {
                0
            }
        })
        .sum()
}

fn count_files_under_skipping_shadow(root: &Path, current: &Path, shadow_root: &Path) -> usize {
    if !current.exists() {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(current) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                return count_files_under_skipping_shadow(root, &path, shadow_root);
            }
            if !path.is_file() {
                return 0;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                return 0;
            };
            if shadow_root.join(relative).exists() {
                0
            } else {
                1
            }
        })
        .sum()
}

struct PreparedBackupEntry {
    target: PathBuf,
    staged_path: PathBuf,
}

struct PreparedBackupArchive {
    _staging_dir: TempDir,
    entries: Vec<PreparedBackupEntry>,
}

fn prepare_backup_archive_files_with(
    archive: &mut zip::ZipArchive<std::fs::File>,
    restore_target: fn(&str) -> Option<PathBuf>,
) -> Result<PreparedBackupArchive, String> {
    let staging_dir =
        tempfile::tempdir().map_err(|error| format!("创建恢复临时目录失败: {}", error))?;
    let mut entries = Vec::new();
    let mut targets = HashSet::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("读取 ZIP 文件条目失败: {}", error))?;
        if file.is_dir() {
            continue;
        }
        let Some(target) = restore_target(file.name()) else {
            continue;
        };
        if !targets.insert(target.clone()) {
            return Err(format!("备份中存在重复恢复目标: {}", target.display()));
        }
        let staged_path = staging_dir.path().join(format!("{index:08x}.restore"));
        let mut output = std::fs::File::create(&staged_path)
            .map_err(|error| format!("创建恢复临时文件失败: {}", error))?;
        std::io::copy(&mut file, &mut output)
            .and_then(|_| output.sync_all())
            .map_err(|error| format!("校验备份文件失败 ({}): {}", target.display(), error))?;
        entries.push(PreparedBackupEntry {
            target,
            staged_path,
        });
    }
    Ok(PreparedBackupArchive {
        _staging_dir: staging_dir,
        entries,
    })
}

fn validate_prepared_switcher_backup(prepared: &mut PreparedBackupArchive) -> Result<(), String> {
    let account_database = switcher_account_dir().join("accounts.json");
    let app_settings = switcher_config_data_dir().join("settings.json");
    let push_settings = push::settings_path();
    for entry in &prepared.entries {
        if entry.target == account_database {
            account::validate_account_database_backup(&entry.staged_path)?;
        } else if entry.target == app_settings {
            let file = std::fs::File::open(&entry.staged_path)
                .map_err(|error| format!("打开备份应用设置失败: {}", error))?;
            serde_json::from_reader::<_, CodexSwitcherSettings>(file)
                .map_err(|error| format!("备份应用设置无效: {}", error))?;
        } else if entry.target == push_settings {
            push::validate_settings_backup(&entry.staged_path)?;
        }
    }
    normalize_prepared_sqlite_databases(prepared)
}

fn normalize_prepared_sqlite_databases(prepared: &mut PreparedBackupArchive) -> Result<(), String> {
    let database_targets = prepared
        .entries
        .iter()
        .filter(|entry| entry.target.extension().and_then(|value| value.to_str()) == Some("sqlite"))
        .map(|entry| entry.target.clone())
        .collect::<Vec<_>>();
    for target in database_targets {
        let staged_database = prepared
            .entries
            .iter()
            .find(|entry| entry.target == target)
            .map(|entry| entry.staged_path.clone())
            .ok_or_else(|| format!("备份数据库条目不存在: {}", target.display()))?;
        let wal_target = path_with_suffix(&target, "-wal");
        if let Some(wal_path) = prepared
            .entries
            .iter()
            .find(|entry| entry.target == wal_target)
            .map(|entry| entry.staged_path.clone())
        {
            std::fs::rename(&wal_path, path_with_suffix(&staged_database, "-wal"))
                .map_err(|error| format!("准备备份数据库 WAL 失败: {}", error))?;
        }
        let connection = rusqlite::Connection::open(&staged_database)
            .map_err(|error| format!("打开备份数据库失败 ({}): {}", target.display(), error))?;
        let quick_check = connection
            .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
            .map_err(|error| format!("校验备份数据库失败 ({}): {}", target.display(), error))?;
        if quick_check != "ok" {
            return Err(format!(
                "备份数据库校验失败 ({}): {}",
                target.display(),
                quick_check
            ));
        }
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|error| format!("整理备份数据库失败 ({}): {}", target.display(), error))?;
    }
    prepared
        .entries
        .retain(|entry| !is_sqlite_sidecar(&entry.target));
    Ok(())
}

fn apply_prepared_backup_archive(prepared: &PreparedBackupArchive) -> Result<(), String> {
    for entry in &prepared.entries {
        let input = std::fs::File::open(&entry.staged_path)
            .map_err(|error| format!("打开恢复临时文件失败: {}", error))?;
        write_reader_atomic(&entry.target, input).map_err(|error| {
            format!(
                "原子恢复备份文件失败 ({}): {}",
                entry.target.display(),
                error
            )
        })?;
    }
    Ok(())
}

fn remove_restored_database_sidecars(prepared: &PreparedBackupArchive) -> Result<(), String> {
    for entry in prepared
        .entries
        .iter()
        .filter(|entry| entry.target.extension().and_then(|value| value.to_str()) == Some("sqlite"))
    {
        for suffix in ["-wal", "-shm"] {
            let sidecar = path_with_suffix(&entry.target, suffix);
            match std::fs::remove_file(&sidecar) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "清理数据库临时文件失败 ({}): {}",
                        sidecar.display(),
                        error
                    ));
                }
            }
        }
    }
    Ok(())
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn is_sqlite_sidecar(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.ends_with(".sqlite-wal") || name.ends_with(".sqlite-shm"))
}

fn backup_entry_restore_target(name: &str) -> Option<PathBuf> {
    backup_session_entry_restore_target(name).or_else(|| backup_data_entry_restore_target(name))
}

fn backup_session_entry_restore_target(name: &str) -> Option<PathBuf> {
    let normalized = name.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute()
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    let codex_home = default_codex_home();
    normalized
        .strip_prefix("codex/sessions/")
        .map(|relative| codex_home.join("sessions").join(relative))
        .or_else(|| {
            normalized
                .strip_prefix("codex/session-trash/")
                .map(|relative| codex_session_trash_dir(&codex_home).join(relative))
        })
        .or_else(|| {
            ["session_index.jsonl", "session_index.jsonl.bak"]
                .into_iter()
                .find_map(|filename| {
                    (normalized == format!("codex/{}", filename)).then(|| codex_home.join(filename))
                })
        })
}

fn backup_data_entry_restore_target(name: &str) -> Option<PathBuf> {
    let normalized = name.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute()
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    normalized
        .strip_prefix("data/")
        .map(|relative| switcher_data_dir().join(relative))
}

fn backup_file_info(path: &Path) -> Result<CodexSwitcherBackupFile, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("读取备份文件信息失败 ({}): {}", path.display(), error))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| {
            chrono::DateTime::<chrono::Local>::from(std::time::UNIX_EPOCH + duration)
                .format("%Y/%m/%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "未知时间".to_string());
    Ok(CodexSwitcherBackupFile {
        name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("backup.zip")
            .to_string(),
        path: path.to_string_lossy().to_string(),
        created_at: modified,
        size_bytes: metadata.len(),
    })
}

fn validate_backup_zip_path(path: &str) -> Result<PathBuf, String> {
    validate_backup_zip_path_in_dir(path, &switcher_backup_dir(), "备份")
}

fn validate_session_backup_zip_path(path: &str) -> Result<PathBuf, String> {
    validate_backup_zip_path_in_dir(path, &switcher_session_dir(), "会话备份")
}

fn validate_backup_zip_path_in_dir(
    path: &str,
    base_dir: &Path,
    label: &str,
) -> Result<PathBuf, String> {
    let backup_dir = base_dir
        .canonicalize()
        .map_err(|error| format!("读取{}目录失败: {}", label, error))?;
    let backup_path = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| format!("读取备份文件失败: {}", error))?;
    if !backup_path.starts_with(&backup_dir)
        || backup_path.extension().and_then(|value| value.to_str()) != Some("zip")
    {
        return Err(format!("只能操作{}目录内的 ZIP 文件", label));
    }
    Ok(backup_path)
}

fn switcher_settings_path() -> PathBuf {
    migrate_legacy_settings_path();
    switcher_config_data_dir().join("settings.json")
}

fn migrate_legacy_settings_path() {
    let next = switcher_config_data_dir().join("settings.json");
    if next.exists() {
        return;
    }
    let legacy_roots = [
        dirs::data_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join("codex-switcher"),
        switcher_data_dir(),
    ];
    if let Some(source) = legacy_roots
        .iter()
        .map(|root| root.join("settings.json"))
        .find(|path| path.exists())
    {
        let _ = std::fs::create_dir_all(switcher_config_data_dir());
        let _ = std::fs::copy(source, next);
    }
}

fn default_codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
        .unwrap_or_else(|| PathBuf::from(".codex"))
}

fn read_switcher_settings() -> Result<CodexSwitcherSettings, String> {
    let path = switcher_settings_path();
    if !path.exists() {
        return Ok(CodexSwitcherSettings::default());
    }
    let content =
        std::fs::read_to_string(&path).map_err(|error| format!("读取设置失败: {}", error))?;
    serde_json::from_str::<CodexSwitcherSettings>(&content)
        .map(CodexSwitcherSettings::normalized)
        .map_err(|error| format!("解析设置失败: {}", error))
}

fn lock_switcher_settings() -> Result<MutexGuard<'static, ()>, String> {
    SWITCHER_SETTINGS_LOCK
        .lock()
        .map_err(|_| "应用设置锁已损坏".to_string())
}

fn write_switcher_settings(settings: &CodexSwitcherSettings) -> Result<(), String> {
    let settings = settings.clone().normalized();
    std::fs::create_dir_all(switcher_config_data_dir())
        .map_err(|error| format!("创建设置目录失败: {}", error))?;
    let content = serde_json::to_vec_pretty(&settings)
        .map_err(|error| format!("序列化设置失败: {}", error))?;
    write_bytes_atomic(&switcher_settings_path(), &content)
        .map_err(|error| format!("写入设置失败: {}", error))
}

async fn fetch_codex_quota_for_account(account_id: &str) -> Result<CodexQuota, String> {
    let source = refreshed_quota_source_account(account_id).await?;
    let accounts = AccountStore::default().list_accounts()?;
    let target = accounts
        .iter()
        .find(|item| item.id == account_id)
        .cloned()
        .ok_or_else(|| "账号不存在".to_string())?;
    let access_token = source.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err("OAuth access_token 为空，无法查询额度".to_string());
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("初始化额度请求失败: {}", error))?;
    let response = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .bearer_auth(access_token)
        .header("OpenAI-Beta", "codex-1")
        .send()
        .await
        .map_err(|error| format!("请求额度失败: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取额度响应失败: {}", error))?;
    if !status.is_success() {
        return Err(format!(
            "额度接口返回 {}: {}",
            status,
            compact_http_body(&body)
        ));
    }
    let raw: Value =
        serde_json::from_str(&body).map_err(|error| format!("解析额度 JSON 失败: {}", error))?;
    let mut quota = parse_codex_quota(raw);
    hydrate_reset_credits_once(
        &client,
        &source,
        target.quota.as_ref(),
        source.quota.as_ref(),
        &mut quota,
    )
    .await;
    if let Err(error) = subscription::refresh_account_subscription(&source.id, false).await {
        eprintln!(
            "刷新 Codex 额度后同步订阅状态失败: account_id={}, error={error}",
            source.id
        );
    }
    Ok(quota)
}

fn quota_source_account(account_id: &str) -> Result<CodexAccount, String> {
    let accounts = AccountStore::default().list_accounts()?;
    let account = accounts
        .iter()
        .find(|item| item.id == account_id)
        .cloned()
        .ok_or_else(|| "账号不存在".to_string())?;
    quota_source_account_from_accounts(&accounts, &account)
}

async fn refreshed_quota_source_account(account_id: &str) -> Result<CodexAccount, String> {
    let source = quota_source_account(account_id)?;
    token_keeper::ensure_fresh_access_token(&source.id, "额度刷新前 Token 需要续期").await
}

fn quota_source_account_from_accounts(
    accounts: &[CodexAccount],
    account: &CodexAccount,
) -> Result<CodexAccount, String> {
    let source = if account.auth_mode.as_deref() == Some("apikey") {
        let bound_id = account
            .bound_oauth_account_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "API Key 账号未绑定 OAuth，无法查询 Codex 额度".to_string())?;
        accounts
            .iter()
            .find(|item| item.id == bound_id)
            .cloned()
            .ok_or_else(|| "绑定的 OAuth 账号不存在".to_string())?
    } else {
        account.clone()
    };
    Ok(source)
}

async fn hydrate_reset_credits_once(
    client: &reqwest::Client,
    source: &CodexAccount,
    target_cached_quota: Option<&CodexQuota>,
    source_cached_quota: Option<&CodexQuota>,
    quota: &mut CodexQuota,
) {
    if !quota.reset_credits.is_empty() {
        quota.reset_credits_next_expires_at = next_reset_credit_expires_at(&quota.reset_credits);
        return;
    }
    if quota.reset_credits_available == Some(0) {
        return;
    }

    let cached = reset_credits_from_cached_quota(target_cached_quota)
        .or_else(|| reset_credits_from_cached_quota(source_cached_quota));
    if let Some(credits) = cached {
        if !reset_credits_match_available_count(&credits, quota.reset_credits_available) {
            if let Ok((available_count, fetched_credits)) =
                fetch_codex_reset_credits(client, source).await
            {
                if !fetched_credits.is_empty() {
                    quota.reset_credits = fetched_credits;
                    quota.reset_credits_next_expires_at =
                        next_reset_credit_expires_at(&quota.reset_credits);
                    if quota.reset_credits_available.is_none() {
                        quota.reset_credits_available = available_count.or_else(|| {
                            Some(
                                quota
                                    .reset_credits
                                    .iter()
                                    .filter(|credit| is_available_reset_credit(credit))
                                    .count() as i64,
                            )
                        });
                    }
                }
            }
            return;
        }
        quota.reset_credits = credits;
        quota.reset_credits_next_expires_at = next_reset_credit_expires_at(&quota.reset_credits);
        if quota.reset_credits_available.is_none() {
            quota.reset_credits_available = Some(
                quota
                    .reset_credits
                    .iter()
                    .filter(|credit| is_available_reset_credit(credit))
                    .count() as i64,
            );
        }
        return;
    }
    if quota.reset_credits_available.is_none()
        && reset_credits_available_from_cached_quota(target_cached_quota)
            .or_else(|| reset_credits_available_from_cached_quota(source_cached_quota))
            == Some(0)
    {
        quota.reset_credits_available = Some(0);
        return;
    }

    if let Ok((available_count, credits)) = fetch_codex_reset_credits(client, source).await {
        if credits.is_empty() {
            if quota.reset_credits_available.is_none() {
                quota.reset_credits_available = available_count;
            }
            return;
        }
        quota.reset_credits = credits;
        quota.reset_credits_next_expires_at = next_reset_credit_expires_at(&quota.reset_credits);
        if quota.reset_credits_available.is_none() {
            quota.reset_credits_available = available_count.or_else(|| {
                Some(
                    quota
                        .reset_credits
                        .iter()
                        .filter(|credit| is_available_reset_credit(credit))
                        .count() as i64,
                )
            });
        }
    }
}

fn reset_credits_from_cached_quota(quota: Option<&CodexQuota>) -> Option<Vec<CodexResetCredit>> {
    let quota = quota?;
    if !quota.reset_credits.is_empty() {
        return Some(quota.reset_credits.clone());
    }
    let raw_data = quota.raw_data.as_ref()?;
    let container = raw_data.get("rate_limit_reset_credits").or_else(|| {
        raw_data
            .get("data")
            .and_then(|data| data.get("rate_limit_reset_credits"))
    });
    let credits = parse_reset_credit_records(container);
    (!credits.is_empty()).then_some(credits)
}

fn reset_credits_match_available_count(
    credits: &[CodexResetCredit],
    available_count: Option<i64>,
) -> bool {
    let Some(available_count) = available_count else {
        return true;
    };
    let cached_available = credits
        .iter()
        .filter(|credit| is_available_reset_credit(credit))
        .count() as i64;
    cached_available == available_count
}

fn reset_credits_available_from_cached_quota(quota: Option<&CodexQuota>) -> Option<i64> {
    quota.and_then(|value| value.reset_credits_available)
}

async fn fetch_codex_reset_credits(
    client: &reqwest::Client,
    source: &CodexAccount,
) -> Result<(Option<i64>, Vec<CodexResetCredit>), String> {
    let access_token = source.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err("OAuth access_token 为空，无法查询重置次数".to_string());
    }
    let response = client
        .get("https://chatgpt.com/backend-api/wham/rate-limit-reset-credits")
        .bearer_auth(access_token)
        .header("OpenAI-Beta", "codex-1")
        .header("Referer", "https://chatgpt.com/")
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .map_err(|error| format!("请求重置次数明细失败: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取重置次数明细响应失败: {}", error))?;
    if !status.is_success() {
        return Err(format!(
            "重置次数明细接口返回 {}: {}",
            status,
            compact_http_body(&body)
        ));
    }
    let raw: Value = serde_json::from_str(&body)
        .map_err(|error| format!("解析重置次数明细 JSON 失败: {}", error))?;
    let credits = parse_reset_credit_records(Some(&raw));
    let available_count = parse_reset_credits_available_count(Some(&raw), &credits);
    Ok((available_count, credits))
}

fn parse_codex_quota(raw: Value) -> CodexQuota {
    let rate_limit = raw.get("rate_limit");
    let primary = rate_limit
        .and_then(|value| value.get("primary_window"))
        .filter(|value| value.is_object());
    let secondary = rate_limit
        .and_then(|value| value.get("secondary_window"))
        .filter(|value| value.is_object());
    let reset_credit_container = raw.get("rate_limit_reset_credits").or_else(|| {
        raw.get("data")
            .and_then(|data| data.get("rate_limit_reset_credits"))
    });
    let reset_credits = parse_reset_credit_records(reset_credit_container);
    let reset_credits_next_expires_at = next_reset_credit_expires_at(&reset_credits);
    let now = chrono::Utc::now().timestamp();
    CodexQuota {
        hourly_percentage: quota_remaining_percentage(primary),
        hourly_reset_time: quota_reset_time(primary, now),
        hourly_window_minutes: quota_window_minutes(primary),
        hourly_window_present: Some(primary.is_some()),
        weekly_percentage: quota_remaining_percentage(secondary),
        weekly_reset_time: quota_reset_time(secondary, now),
        weekly_window_minutes: quota_window_minutes(secondary),
        weekly_window_present: Some(secondary.is_some()),
        reset_credits_available: parse_reset_credits_available_count(
            reset_credit_container,
            &reset_credits,
        ),
        reset_credits,
        reset_credits_next_expires_at,
        raw_data: Some(raw),
    }
}

fn parse_reset_credits_available_count(
    container: Option<&Value>,
    credits: &[CodexResetCredit],
) -> Option<i64> {
    container
        .and_then(|value| {
            value
                .get("available_count")
                .or_else(|| value.get("availableCount"))
                .or_else(|| {
                    value.get("data").and_then(|data| {
                        data.get("available_count")
                            .or_else(|| data.get("availableCount"))
                    })
                })
        })
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|raw| i64::try_from(raw).ok()))
        })
        .or_else(|| {
            (!credits.is_empty()).then(|| {
                credits
                    .iter()
                    .filter(|credit| is_available_reset_credit(credit))
                    .count() as i64
            })
        })
}

fn next_reset_credit_expires_at(credits: &[CodexResetCredit]) -> Option<i64> {
    credits
        .iter()
        .filter(|credit| is_available_reset_credit(credit))
        .filter_map(|credit| credit.expires_at)
        .min()
}

fn parse_reset_credit_records(container: Option<&Value>) -> Vec<CodexResetCredit> {
    container
        .and_then(|value| {
            value
                .get("credits")
                .or_else(|| value.get("data").and_then(|data| data.get("credits")))
        })
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_reset_credit_record)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn parse_reset_credit_record(value: &Value) -> Option<CodexResetCredit> {
    let record = value.as_object()?;
    let raw_status = extract_reset_credit_string(record, &["status", "state"]);
    let expires_at =
        extract_reset_credit_timestamp(record, &["expires_at", "expire_at", "expiresAt"]);
    let status = normalize_reset_credit_status(raw_status.as_deref(), expires_at);

    Some(CodexResetCredit {
        id: extract_reset_credit_string(record, &["id", "credit_id", "creditId"]),
        status,
        reset_type: extract_reset_credit_string(record, &["type", "reset_type", "resetType"]),
        granted_at: extract_reset_credit_timestamp(
            record,
            &["granted_at", "created_at", "grantedAt"],
        ),
        expires_at,
        redeemed_at: extract_reset_credit_timestamp(
            record,
            &["redeemed_at", "used_at", "consumed_at", "redeemedAt"],
        ),
        raw_status,
    })
}

fn extract_reset_credit_string(
    record: &serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        let value = record.get(*key)?;
        match value {
            Value::String(text) => {
                let trimmed = text.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(flag) => Some(flag.to_string()),
            _ => None,
        }
    })
}

fn extract_reset_credit_timestamp(
    record: &serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<i64> {
    keys.iter()
        .find_map(|key| parse_reset_credit_timestamp(record.get(*key)))
}

fn parse_reset_credit_timestamp(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => {
            let mut timestamp = number
                .as_i64()
                .or_else(|| number.as_u64().and_then(|raw| i64::try_from(raw).ok()))?;
            if timestamp > 1_000_000_000_000 {
                timestamp /= 1000;
            }
            Some(timestamp)
        }
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Ok(mut timestamp) = trimmed.parse::<i64>() {
                if timestamp > 1_000_000_000_000 {
                    timestamp /= 1000;
                }
                return Some(timestamp);
            }
            chrono::DateTime::parse_from_rfc3339(trimmed)
                .ok()
                .map(|date| date.timestamp())
        }
        _ => None,
    }
}

fn normalize_reset_credit_status(status: Option<&str>, expires_at: Option<i64>) -> Option<String> {
    let normalized = status
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    if normalized.is_some() {
        return normalized;
    }

    expires_at
        .filter(|timestamp| *timestamp <= chrono::Utc::now().timestamp())
        .map(|_| "expired".to_string())
}

fn is_available_reset_credit(credit: &CodexResetCredit) -> bool {
    let status = credit
        .status
        .as_deref()
        .or(credit.raw_status.as_deref())
        .unwrap_or("available")
        .trim()
        .to_ascii_lowercase();
    if matches!(
        status.as_str(),
        "redeemed" | "used" | "consumed" | "expired"
    ) {
        return false;
    }

    credit
        .expires_at
        .map(|timestamp| timestamp > chrono::Utc::now().timestamp())
        .unwrap_or(true)
}

fn quota_remaining_percentage(window: Option<&Value>) -> i64 {
    let used = window
        .and_then(|value| value.get("used_percent"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    (100 - used).clamp(0, 100)
}

fn quota_reset_time(window: Option<&Value>, now: i64) -> Option<i64> {
    window
        .and_then(|value| value.get("reset_at"))
        .and_then(Value::as_i64)
        .or_else(|| {
            window
                .and_then(|value| value.get("reset_after_seconds"))
                .and_then(Value::as_i64)
                .map(|seconds| now + seconds)
        })
}

fn quota_window_minutes(window: Option<&Value>) -> Option<i64> {
    window
        .and_then(|value| value.get("limit_window_seconds"))
        .and_then(Value::as_i64)
        .map(|seconds| (seconds / 60).max(1))
}

fn compact_http_body(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(300).collect()
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::{
        backup_entry_restore_target, codex_config_backup_path, codex_session_trash_dir,
        default_codex_home, format_codex_config_content, parse_codex_api_key_models,
        parse_codex_quota, prepare_backup_archive_files_with, read_codex_config_file_from,
        rollback_file_on_error, switcher_account_dir, switcher_data_dir,
        validate_prepared_switcher_backup, write_codex_config_file_to, CodexApiKeyModel,
        CodexConfigFileKind,
    };
    use serde_json::json;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn quota_monitoring_is_always_enabled() {
        assert!(super::CodexSwitcherSettings::default().monitor_quota);
        assert!(super::CodexSwitcherSettings::default().show_additional_quota_windows);

        let settings = super::CodexSwitcherSettings {
            monitor_quota: false,
            ..Default::default()
        };
        assert!(settings.normalized().monitor_quota);
    }

    #[test]
    fn model_endpoint_preserves_provider_prefix_and_avoids_duplicate_suffix() {
        assert_eq!(
            super::codex_models_endpoint(Some("https://relay.example/sub2api/"))
                .expect("models endpoint"),
            "https://relay.example/sub2api/models"
        );
        assert_eq!(
            super::codex_models_endpoint(Some("https://relay.example/v1/models?cache=1"))
                .expect("existing models endpoint"),
            "https://relay.example/v1/models"
        );
        assert!(super::codex_models_endpoint(Some("file:///tmp/models")).is_err());
    }

    #[test]
    fn model_list_parser_supports_openai_and_compatible_payloads() {
        let standard = parse_codex_api_key_models(
            r#"{"object":"list","data":[{"id":"gpt-5.6-sol","owned_by":"custom"},{"id":"gpt-5.5"},{"id":"gpt-5.5"}]}"#,
        )
        .expect("standard model response");
        assert_eq!(
            standard,
            vec![
                CodexApiKeyModel {
                    id: "gpt-5.5".to_string(),
                    owned_by: None,
                },
                CodexApiKeyModel {
                    id: "gpt-5.6-sol".to_string(),
                    owned_by: Some("custom".to_string()),
                },
            ]
        );

        let compatible = parse_codex_api_key_models(
            r#"{"models":["model-b",{"name":"model-a","ownedBy":"relay"}]}"#,
        )
        .expect("compatible model response");
        assert_eq!(compatible[0].id, "model-a");
        assert_eq!(compatible[0].owned_by.as_deref(), Some("relay"));
        assert_eq!(compatible[1].id, "model-b");
        assert!(parse_codex_api_key_models(r#"{"result":[]}"#).is_err());

        let invalid_ids = json!({
            "data": [
                { "id": "x".repeat(super::MAX_API_MODEL_ID_CHARS + 1) },
                { "id": "invalid\nmodel" }
            ]
        });
        assert!(parse_codex_api_key_models(&invalid_ids.to_string())
            .expect("invalid ids are ignored")
            .is_empty());
    }

    #[test]
    fn relay_balance_endpoints_preserve_prefix_and_remove_v1_once() {
        let endpoints = super::codex_relay_balance_endpoints(Some(
            "https://relay.example/prefix/v1/?ignored=1#fragment",
        ))
        .expect("balance endpoints");
        assert_eq!(
            endpoints.new_api_status.as_str(),
            "https://relay.example/prefix/api/status"
        );
        assert_eq!(
            endpoints.new_api_usage.as_str(),
            "https://relay.example/prefix/api/usage/token/"
        );
        assert_eq!(
            endpoints.new_api_billing_subscription.as_str(),
            "https://relay.example/prefix/dashboard/billing/subscription"
        );
        assert_eq!(
            endpoints.new_api_billing_usage.as_str(),
            "https://relay.example/prefix/dashboard/billing/usage"
        );
        assert_eq!(
            endpoints.sub2api_usage.as_str(),
            "https://relay.example/prefix/v1/usage"
        );
        assert!(endpoints.insecure_http_origin.is_none());
        assert!(super::codex_relay_balance_endpoints(Some("file:///tmp/v1")).is_err());
        assert!(super::codex_relay_balance_endpoints(Some("  ")).is_err());
    }

    #[test]
    fn remote_http_balance_requires_explicit_confirmation_but_loopback_does_not() {
        for base_url in [
            "http://localhost:18787/v1",
            "http://LOCALHOST:18787/v1",
            "http://127.0.0.1:18787/v1",
            "http://127.255.255.254:18787/v1",
            "http://[::1]:18787/v1",
        ] {
            let endpoints = super::codex_relay_balance_endpoints(Some(base_url))
                .expect("loopback balance endpoints");
            assert!(endpoints.insecure_http_origin.is_none(), "{base_url}");
            assert!(super::ensure_balance_transport_allowed(&endpoints, None).is_ok());
        }

        for (base_url, expected_origin) in [
            ("http://RELAY.EXAMPLE:80/v1", "http://relay.example"),
            (
                "http://localhost.evil.example/v1",
                "http://localhost.evil.example",
            ),
            ("http://192.168.1.8:18787/v1", "http://192.168.1.8:18787"),
            ("http://126.255.255.255/v1", "http://126.255.255.255"),
            ("http://128.0.0.1/v1", "http://128.0.0.1"),
            ("http://[2001:db8::1]/v1", "http://[2001:db8::1]"),
        ] {
            let endpoints = super::codex_relay_balance_endpoints(Some(base_url))
                .expect("remote HTTP balance endpoints");
            let required_origin = endpoints
                .insecure_http_origin
                .as_deref()
                .expect("remote HTTP origin");
            assert_eq!(required_origin, expected_origin);
            let error = super::ensure_balance_transport_allowed(&endpoints, None)
                .expect_err("remote HTTP requires confirmation");
            assert!(error.starts_with("INSECURE_HTTP_CONFIRM_REQUIRED:"));
            assert!(super::ensure_balance_transport_allowed(
                &endpoints,
                Some("http://different.example")
            )
            .is_err());
            assert!(
                super::ensure_balance_transport_allowed(&endpoints, Some(required_origin)).is_ok()
            );
        }

        let https = super::codex_relay_balance_endpoints(Some("https://relay.example/v1"))
            .expect("HTTPS balance endpoints");
        assert!(super::ensure_balance_transport_allowed(&https, None).is_ok());
    }

    #[test]
    fn new_api_balance_parser_uses_dynamic_quota_unit_and_handles_unlimited() {
        let status = r#"{"success":true,"data":{"quota_per_unit":"250000","quota_display_type":"USD","usd_exchange_rate":7.3}}"#;
        let settings = super::parse_new_api_quota_settings(status).expect("quota settings");
        assert_eq!(settings.quota_per_unit, 250_000.0);
        assert_eq!(settings.display_type, super::NewApiQuotaDisplayType::Usd);

        let balance = super::parse_new_api_balance(
            r#"{"code":true,"data":{"object":"token_usage","name":"Codex","total_granted":750000,"total_used":250000,"total_available":500000,"unlimited_quota":false}}"#,
            settings,
        )
        .expect("new api balance");
        assert_eq!(balance.provider, "new_api");
        assert_eq!(balance.balance_kind, "key_quota");
        assert_eq!(balance.available_amount, Some(2.0));
        assert_eq!(balance.used_amount, Some(1.0));
        assert_eq!(balance.total_amount, Some(3.0));
        assert_eq!(balance.plan_name.as_deref(), Some("Codex"));

        let unlimited = super::parse_new_api_balance(
            r#"{"data":{"object":"token_usage","unlimited_quota":true}}"#,
            settings,
        )
        .expect("unlimited new api balance");
        assert!(unlimited.unlimited);
        assert_eq!(unlimited.balance_kind, "unlimited");
        assert_eq!(unlimited.available_amount, None);
    }

    #[test]
    fn new_api_balance_parser_honors_cny_and_token_display_modes() {
        let cny_settings = super::parse_new_api_quota_settings(
            r#"{"data":{"quota_per_unit":500000,"quota_display_type":"CNY","usd_exchange_rate":7.3}}"#,
        )
        .expect("CNY settings");
        let cny = super::parse_new_api_balance(
            r#"{"data":{"object":"token_usage","total_granted":1000000,"total_used":0,"total_available":1000000}}"#,
            cny_settings,
        )
        .expect("CNY balance");
        assert_eq!(cny.currency, "CNY");
        assert!((cny.available_amount.expect("CNY amount") - 14.6).abs() < 1e-9);

        let token_settings = super::parse_new_api_quota_settings(
            r#"{"data":{"quota_per_unit":500000,"quota_display_type":"TOKENS"}}"#,
        )
        .expect("token settings");
        let tokens = super::parse_new_api_balance(
            r#"{"data":{"object":"token_usage","total_granted":1000000,"total_used":250000,"total_available":750000}}"#,
            token_settings,
        )
        .expect("token balance");
        assert_eq!(tokens.currency, "TOKENS");
        assert_eq!(tokens.available_amount, Some(750_000.0));
    }

    #[test]
    fn new_api_account_balance_parser_subtracts_historical_usage() {
        let usd_settings = super::parse_new_api_quota_settings(
            r#"{"data":{"quota_per_unit":500000,"quota_display_type":"USD"}}"#,
        )
        .expect("USD settings");
        let balance = super::parse_new_api_account_balance(
            r#"{"object":"billing_subscription","hard_limit_usd":300}"#,
            r#"{"object":"list","total_usage":17441}"#,
            usd_settings,
        )
        .expect("new api account balance");
        assert_eq!(balance.provider, "new_api");
        assert_eq!(balance.balance_kind, "wallet");
        assert!((balance.available_amount.expect("available") - 125.59).abs() < 1e-9);
        assert!((balance.used_amount.expect("used") - 174.41).abs() < 1e-9);
        assert_eq!(balance.total_amount, Some(300.0));
        assert!(!balance.unlimited);

        assert!(super::parse_new_api_account_balance(
            r#"{"hard_limit_usd":100000000}"#,
            r#"{"total_usage":0}"#,
            usd_settings,
        )
        .is_err());

        let token_settings = super::parse_new_api_quota_settings(
            r#"{"data":{"quota_per_unit":500000,"quota_display_type":"TOKENS"}}"#,
        )
        .expect("token settings");
        let tokens = super::parse_new_api_account_balance(
            r#"{"hard_limit_usd":100000000}"#,
            r#"{"total_usage":2500000000}"#,
            token_settings,
        )
        .expect("legitimate token balance");
        assert_eq!(tokens.currency, "TOKENS");
        assert_eq!(tokens.available_amount, Some(75_000_000.0));
    }

    #[test]
    fn sub2api_balance_parser_distinguishes_wallet_quota_and_subscription() {
        let wallet = super::parse_sub2api_balance(
            r#"{"mode":"unrestricted","isValid":true,"planName":"钱包余额","balance":12.5,"remaining":12.5,"unit":"USD"}"#,
        )
        .expect("wallet balance");
        assert_eq!(wallet.balance_kind, "wallet");
        assert_eq!(wallet.available_amount, Some(12.5));

        let quota = super::parse_sub2api_balance(
            r#"{"mode":"quota_limited","isValid":true,"quota":{"limit":"20","used":7.5,"remaining":12.5,"unit":"USD"}}"#,
        )
        .expect("key quota");
        assert_eq!(quota.balance_kind, "key_quota");
        assert_eq!(quota.available_amount, Some(12.5));
        assert_eq!(quota.used_amount, Some(7.5));
        assert_eq!(quota.total_amount, Some(20.0));

        let subscription = super::parse_sub2api_balance(
            r#"{"mode":"unrestricted","isValid":true,"planName":"Pro","remaining":3.25,"unit":"USD","subscription":{}}"#,
        )
        .expect("subscription balance");
        assert_eq!(subscription.balance_kind, "subscription");
        assert_eq!(subscription.available_amount, Some(3.25));

        let unlimited = super::parse_sub2api_balance(
            r#"{"mode":"unrestricted","isValid":true,"planName":"Unlimited","remaining":-1,"unit":"USD"}"#,
        )
        .expect("unlimited subscription");
        assert!(unlimited.unlimited);
        assert_eq!(unlimited.balance_kind, "unlimited");
        assert_eq!(unlimited.available_amount, None);

        assert!(super::parse_sub2api_balance(
            r#"{"mode":"quota_limited","isValid":true,"rate_limits":[]}"#
        )
        .is_err());
    }

    #[test]
    fn config_state_failure_restores_existing_or_missing_file_snapshot() {
        let codex = tempdir().expect("codex tempdir");
        let config_path = codex.path().join("config.toml");
        std::fs::write(&config_path, b"model = \"changed\"\n").expect("write changed config");

        let error = rollback_file_on_error::<()>(
            Err("保存账号状态失败".to_string()),
            &config_path,
            Some(b"model = \"original\"\n"),
        )
        .expect_err("surface state failure");
        assert_eq!(error, "保存账号状态失败");
        assert_eq!(
            std::fs::read(&config_path).expect("read restored config"),
            b"model = \"original\"\n"
        );

        std::fs::write(&config_path, b"model = \"new\"\n").expect("write new config");
        rollback_file_on_error::<()>(Err("保存账号状态失败".to_string()), &config_path, None)
            .expect_err("surface state failure for new file");
        assert!(!config_path.exists());
    }

    #[test]
    fn codex_config_editor_round_trips_and_backs_up_supported_files() {
        let codex = tempdir().expect("codex tempdir");
        let missing = read_codex_config_file_from(codex.path(), CodexConfigFileKind::AuthJson)
            .expect("read missing auth");
        assert!(!missing.exists);
        assert_eq!(missing.content, "{}\n");

        let first_auth = r#"{"OPENAI_API_KEY":null,"tokens":{"access_token":"token-1"}}"#;
        let saved =
            write_codex_config_file_to(codex.path(), CodexConfigFileKind::AuthJson, first_auth)
                .expect("save auth");
        assert!(saved.exists);
        assert_eq!(saved.content, first_auth);

        let second_auth = r#"{"OPENAI_API_KEY":"sk-test","tokens":{}}"#;
        write_codex_config_file_to(codex.path(), CodexConfigFileKind::AuthJson, second_auth)
            .expect("replace auth");
        assert_eq!(
            std::fs::read_to_string(codex_config_backup_path(
                codex.path(),
                CodexConfigFileKind::AuthJson
            ))
            .expect("read auth backup"),
            first_auth
        );

        let config = "model_provider = \"openai\"\n";
        write_codex_config_file_to(codex.path(), CodexConfigFileKind::ConfigToml, config)
            .expect("save config");
        assert_eq!(
            std::fs::read_to_string(codex.path().join("config.toml")).expect("read config"),
            config
        );
    }

    #[test]
    fn codex_config_editor_rejects_invalid_content_and_formats_json() {
        let codex = tempdir().expect("codex tempdir");
        assert!(write_codex_config_file_to(
            codex.path(),
            CodexConfigFileKind::AuthJson,
            "{invalid"
        )
        .is_err());
        assert!(
            write_codex_config_file_to(codex.path(), CodexConfigFileKind::AuthJson, "null")
                .is_err()
        );
        assert!(write_codex_config_file_to(
            codex.path(),
            CodexConfigFileKind::ConfigToml,
            "model_provider = ["
        )
        .is_err());
        assert!(!codex.path().join("auth.json").exists());
        assert!(!codex.path().join("config.toml").exists());

        let formatted = format_codex_config_content(
            CodexConfigFileKind::AuthJson,
            r#"{"tokens":{"access_token":"token-1"}}"#,
        )
        .expect("format auth");
        assert!(formatted.contains("\n  \"tokens\": {"));
        assert!(formatted.ends_with('\n'));
        assert!(CodexConfigFileKind::parse("other.json").is_err());
    }

    #[test]
    fn backup_restore_target_accepts_known_backup_roots() {
        assert_eq!(
            backup_entry_restore_target("codex/sessions/2026/session.jsonl"),
            Some(default_codex_home().join("sessions/2026/session.jsonl"))
        );
        assert_eq!(
            backup_entry_restore_target("codex/session_index.jsonl"),
            Some(default_codex_home().join("session_index.jsonl"))
        );
        assert_eq!(
            backup_entry_restore_target("codex/session-trash/session.jsonl"),
            Some(codex_session_trash_dir(&default_codex_home()).join("session.jsonl"))
        );
        assert_eq!(
            backup_entry_restore_target("data/accounts.json"),
            Some(switcher_data_dir().join("accounts.json"))
        );
        assert_eq!(
            backup_entry_restore_target("data/statistics/usage_logs.json"),
            Some(switcher_data_dir().join("statistics/usage_logs.json"))
        );
        assert_eq!(
            backup_entry_restore_target("data/statistics/usage.sqlite"),
            Some(switcher_data_dir().join("statistics/usage.sqlite"))
        );
    }

    #[test]
    fn backup_restore_target_rejects_unsafe_paths() {
        for name in [
            "/tmp/evil.jsonl",
            "codex/sessions/../../evil.jsonl",
            "codex/session-trash//evil.jsonl",
            "data/../config.toml",
            "backup.json",
        ] {
            assert_eq!(backup_entry_restore_target(name), None);
        }
    }

    #[test]
    fn malformed_backup_is_rejected_before_live_files_are_replaced() {
        let live_account_database = switcher_account_dir().join("accounts.json");
        std::fs::create_dir_all(
            live_account_database
                .parent()
                .expect("live account database parent"),
        )
        .expect("create live account directory");
        let original = br#"{"accounts":[],"current_account_id":null}"#;
        std::fs::write(&live_account_database, original).expect("write live account database");

        let temp = tempdir().expect("backup tempdir");
        let zip_path = temp.path().join("malformed-backup.zip");
        let file = std::fs::File::create(&zip_path).expect("create backup zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o600);
        zip.start_file("data/account/accounts.json", options)
            .expect("start malformed account entry");
        zip.write_all(b"{not-json")
            .expect("write malformed account entry");
        zip.finish().expect("finish malformed backup");

        let file = std::fs::File::open(&zip_path).expect("open malformed backup");
        let mut archive = zip::ZipArchive::new(file).expect("read malformed backup");
        let mut prepared =
            prepare_backup_archive_files_with(&mut archive, backup_entry_restore_target)
                .expect("stage malformed backup");
        let error = validate_prepared_switcher_backup(&mut prepared)
            .expect_err("reject malformed account database");
        assert!(error.contains("备份账号库无效"));
        assert_eq!(
            std::fs::read(&live_account_database).expect("read live account database"),
            original
        );
        let _ = std::fs::remove_dir_all(switcher_data_dir());
    }

    #[test]
    fn legacy_session_trash_backup_skips_shadowed_files_only() {
        let temp = tempdir().expect("backup tempdir");
        let legacy = temp.path().join("legacy-trash");
        let shadow = temp.path().join("new-trash");
        std::fs::create_dir_all(legacy.join("nested")).expect("legacy nested dir");
        std::fs::create_dir_all(shadow.join("nested")).expect("shadow nested dir");
        std::fs::write(legacy.join("nested").join("keep.jsonl"), "{}").expect("legacy keep");
        std::fs::write(legacy.join("nested").join("duplicate.jsonl"), "legacy")
            .expect("legacy duplicate");
        std::fs::write(shadow.join("nested").join("duplicate.jsonl"), "new")
            .expect("shadow duplicate");

        let zip_path = temp.path().join("backup.zip");
        let file = std::fs::File::create(&zip_path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        super::add_directory_to_backup_zip_skipping_shadow(
            &mut zip,
            &legacy,
            &legacy,
            &shadow,
            "codex/session-trash",
            options,
        )
        .expect("write legacy trash");
        zip.finish().expect("finish zip");

        let file = std::fs::File::open(zip_path).expect("open zip");
        let mut archive = zip::ZipArchive::new(file).expect("read zip");
        let mut names = (0..archive.len())
            .map(|index| {
                archive
                    .by_index(index)
                    .expect("zip entry")
                    .name()
                    .to_string()
            })
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec!["codex/session-trash/nested/keep.jsonl"]);
    }

    #[test]
    fn codex_session_backup_summary_counts_legacy_trash_fallback_files() {
        let codex = tempdir().expect("codex tempdir");
        let primary = codex_session_trash_dir(codex.path());
        let legacy = codex.path().join(".codex-switcher").join("session-trash");
        std::fs::create_dir_all(primary.join("nested")).expect("primary nested dir");
        std::fs::create_dir_all(legacy.join("nested")).expect("legacy nested dir");
        std::fs::write(primary.join("primary.jsonl"), "{}").expect("primary file");
        std::fs::write(primary.join("nested").join("duplicate.jsonl"), "new")
            .expect("primary duplicate");
        std::fs::write(legacy.join("moved.jsonl"), "{}").expect("legacy moved file");
        std::fs::write(legacy.join("nested").join("keep.jsonl"), "{}").expect("legacy keep");
        std::fs::write(legacy.join("nested").join("duplicate.jsonl"), "legacy")
            .expect("legacy duplicate");

        let summary = super::codex_session_backup_summary(codex.path());

        assert_eq!(
            summary
                .get("trashedSessionFiles")
                .and_then(serde_json::Value::as_u64),
            Some(4)
        );
        assert!(primary.join("moved.jsonl").is_file());
    }

    #[test]
    fn parse_quota_keeps_reset_credit_details() {
        let quota = parse_codex_quota(json!({
            "rate_limit": {
                "primary_window": {
                    "used_percent": 12,
                    "limit_window_seconds": 18_000,
                    "reset_at": 1_785_000_000
                },
                "secondary_window": {
                    "used_percent": 33,
                    "limit_window_seconds": 604_800,
                    "reset_at": 1_786_000_000
                }
            },
            "rate_limit_reset_credits": {
                "available_count": 2,
                "credits": [
                    {
                        "id": "credit-1",
                        "status": "available",
                        "granted_at": 1_785_100_000,
                        "expires_at": 4_102_444_800_i64
                    },
                    {
                        "creditId": "credit-2",
                        "state": "redeemed",
                        "grantedAt": "2026-07-01T00:00:00Z",
                        "expiresAt": 4_102_531_200_000_i64,
                        "redeemedAt": 1_785_200_000
                    }
                ]
            }
        }));

        assert_eq!(quota.reset_credits_available, Some(2));
        assert_eq!(quota.reset_credits.len(), 2);
        assert_eq!(quota.reset_credits[0].id.as_deref(), Some("credit-1"));
        assert_eq!(quota.reset_credits[0].status.as_deref(), Some("available"));
        assert_eq!(quota.reset_credits[1].id.as_deref(), Some("credit-2"));
        assert_eq!(quota.reset_credits[1].status.as_deref(), Some("redeemed"));
        assert_eq!(quota.reset_credits[1].granted_at, Some(1_782_864_000));
        assert_eq!(quota.reset_credits[1].expires_at, Some(4_102_531_200));
        assert_eq!(quota.reset_credits_next_expires_at, Some(4_102_444_800));
    }

    #[test]
    fn parse_quota_treats_null_windows_as_absent() {
        let quota = parse_codex_quota(json!({
            "rate_limit": {
                "primary_window": {
                    "used_percent": 1,
                    "limit_window_seconds": 604_800,
                    "reset_at": 1_786_000_000
                },
                "secondary_window": null
            }
        }));

        assert_eq!(quota.hourly_window_present, Some(true));
        assert_eq!(quota.hourly_window_minutes, Some(10_080));
        assert_eq!(quota.weekly_window_present, Some(false));
        assert_eq!(quota.weekly_window_minutes, None);
        assert_eq!(quota.weekly_reset_time, None);

        let empty = parse_codex_quota(json!({
            "rate_limit": {
                "primary_window": null,
                "secondary_window": null
            }
        }));
        assert_eq!(empty.hourly_window_present, Some(false));
        assert_eq!(empty.weekly_window_present, Some(false));
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(api_service::ApiServiceProcessState::default())
        .manage(api_service::ApiServiceDownloadState::default())
        .manage(api_service::ApiServiceOperationState::default())
        .manage(app_update::AppUpdateDownloadState::default())
        .manage(push::PushRuntimeState::default())
        .setup(|app| {
            start_startup_maintenance(app.handle().clone());
            push::start_push_scheduler(app.handle().clone());
            token_keeper::start_token_keeper(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api_service::api_service_state,
            api_service::api_service_update_settings,
            api_service::api_service_start,
            api_service::api_service_stop,
            api_service::api_service_reset,
            api_service::api_service_check_update,
            api_service::api_service_download_update,
            api_service::api_service_import_runtime,
            api_service::api_service_activate_runtime,
            api_service::api_service_delete_runtime,
            api_service::api_service_cancel_download,
            api_service::api_service_bind_accounts,
            api_service::api_service_list_bound_accounts,
            api_service::api_service_delete_bound_accounts,
            app_update::app_update_check,
            app_update::app_update_download,
            app_update::app_update_cancel_download,
            app_update::app_update_open_installer,
            push::push_get_settings,
            push::push_update_settings,
            push::push_run_now,
            push::push_run_rule_now,
            push::push_test_channel,
            push::push_list_logs,
            push::push_count_successful_logs_since,
            push::push_clear_logs,
            list_codex_accounts,
            get_current_codex_account,
            detect_current_codex_account,
            import_codex_from_json,
            import_codex_from_local,
            add_codex_account_with_api_key,
            update_codex_api_key_credentials,
            fetch_codex_api_key_models,
            fetch_codex_api_key_balance,
            check_codex_api_key_model_access,
            set_codex_api_key_default_model,
            update_codex_account_profile,
            update_codex_account_from_json,
            update_codex_api_key_bound_oauth_account,
            update_codex_account_phone,
            export_codex_accounts,
            codex_oauth_login_start,
            codex_oauth_submit_callback_url,
            codex_oauth_login_completed,
            codex_oauth_login_cancel,
            open_external_url,
            open_path_in_file_manager,
            delete_codex_account,
            switch_codex_account,
            restart_codex_app,
            get_codex_switcher_settings,
            update_codex_switcher_settings,
            read_codex_config_file,
            format_codex_config_file,
            write_codex_config_file,
            reset_codex_config_toml,
            get_codex_switcher_paths,
            export_codex_switcher_backup,
            start_codex_switcher_backup,
            start_codex_switcher_session_backup,
            list_codex_switcher_backups,
            list_codex_switcher_session_backups,
            restore_codex_switcher_backup,
            restore_codex_switcher_session_backup,
            delete_codex_switcher_backup,
            import_codex_switcher_backup,
            refresh_codex_quota,
            refresh_all_codex_quotas,
            consume_codex_reset_credit,
            codex_get_usage_dashboard,
            codex_get_usage_activity,
            codex_list_model_pricing,
            codex_update_model_pricing,
            codex_delete_model_pricing,
            codex_reset_model_pricing,
            codex_get_pricing_config,
            codex_update_pricing_config,
            codex_list_sessions_across_instances,
            codex_get_session_token_stats_across_instances,
            codex_move_sessions_to_trash_across_instances,
            codex_list_trashed_sessions_across_instances,
            codex_restore_sessions_from_trash_across_instances,
            codex_repair_session_visibility_across_instances,
            codex_list_session_visibility_repair_instances,
            codex_list_session_visibility_repair_providers,
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let process = window.state::<api_service::ApiServiceProcessState>();
                let download = window.state::<api_service::ApiServiceDownloadState>();
                let operation = window.state::<api_service::ApiServiceOperationState>();
                api_service::shutdown_api_service(process, download, operation);
                let app_update_download = window.state::<app_update::AppUpdateDownloadState>();
                app_update::shutdown_app_update(app_update_download);
                let push_runtime = window.state::<push::PushRuntimeState>();
                push::shutdown_push_scheduler(push_runtime);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn start_startup_maintenance(app: AppHandle) {
    thread::spawn(move || {
        // Give the first window paint priority over installer and runtime directory maintenance.
        thread::sleep(Duration::from_secs(2));
        if let Err(error) = app_update::cleanup_pending_update_artifacts_on_startup(&app) {
            eprintln!("App update cleanup failed during startup: {error}");
        }
        if let Err(error) = api_service::prune_api_service_runtimes_on_startup(&app) {
            eprintln!("API service runtime maintenance failed during startup: {error}");
        }
        api_service::start_auto_update_scheduler(app);
    });
}
