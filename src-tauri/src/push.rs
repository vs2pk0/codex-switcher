use crate::{
    account::{write_bytes_atomic, AccountStore, CodexAccount, CodexQuota},
    fetch_codex_quota_for_account, switcher_config_data_dir,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::{DateTime, Local, Utc};
use futures_util::{stream, StreamExt};
use hmac::{Hmac, Mac};
use reqwest::{Client, Url};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, MutexGuard,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};

const PUSH_SETTINGS_FILE: &str = "push-settings.json";
const PUSH_DATABASE_FILE: &str = "push.sqlite";
const PUSH_POLL_SECONDS: u64 = 15;
const PUSH_EVENT_CHECK_SECONDS: u64 = 60;
const PUSH_SCHEDULE_RETRY_SECONDS: u64 = 60;
const PUSH_REFRESH_PHASE_TIMEOUT_SECONDS: u64 = 45;
const MAX_PUSH_INTERVAL_MINUTES: u64 = 43_200;
const DEFAULT_RULE_COOLDOWN_MINUTES: u64 = 60;
const MAX_RULE_COOLDOWN_MINUTES: u64 = 10_080;
const MAX_LOG_LIMIT: u32 = 500;
const MAX_CONCURRENT_PUSHES: usize = 8;
const MAX_CONCURRENT_QUOTA_REFRESHES: usize = 4;
const MAX_PUSH_TITLE_BYTES: usize = 160;
const MAX_PUSH_CONTENT_BYTES: usize = 1_800;
const PUSH_USER_AGENT: &str = "Codex-Switcher-Push";
const ACCOUNT_STATE_UPDATED_EVENT: &str = "codex-account-state-updated";
static PUSH_DATABASE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PushChannelType {
    ServerChan,
    PushPlus,
    EnterpriseWechat,
    WxPusher,
    Bark,
    Chanify,
    PushDeer,
    DingTalk,
}

impl PushChannelType {
    fn label(self) -> &'static str {
        match self {
            Self::ServerChan => "Server酱",
            Self::PushPlus => "PushPlus",
            Self::EnterpriseWechat => "企业微信",
            Self::WxPusher => "WxPusher",
            Self::Bark => "Bark",
            Self::Chanify => "Chanify",
            Self::PushDeer => "PushDeer",
            Self::DingTalk => "钉钉",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PushChannelConfig {
    #[serde(default)]
    pub id: String,
    pub channel_type: PushChannelType,
    #[serde(default)]
    pub nickname: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub server_chan_send_key: String,
    #[serde(default)]
    pub push_plus_token: String,
    #[serde(default)]
    pub push_plus_topic: String,
    #[serde(default)]
    pub enterprise_wechat_corp_id: String,
    #[serde(default)]
    pub enterprise_wechat_corp_secret: String,
    #[serde(default)]
    pub enterprise_wechat_agent_id: String,
    #[serde(default = "default_enterprise_wechat_to_user")]
    pub enterprise_wechat_to_user: String,
    #[serde(default)]
    pub wx_pusher_app_token: String,
    #[serde(default)]
    pub wx_pusher_uid: String,
    #[serde(default = "default_bark_api")]
    pub bark_api: String,
    #[serde(default)]
    pub bark_token: String,
    #[serde(default)]
    pub bark_sound: String,
    #[serde(default)]
    pub chanify_token: String,
    #[serde(default)]
    pub push_deer_key: String,
    #[serde(default)]
    pub ding_talk_access_token: String,
    #[serde(default)]
    pub ding_talk_secret: String,
}

impl PushChannelConfig {
    fn normalized(mut self) -> Self {
        self.id = self.id.trim().to_string();
        if self.id.is_empty() {
            self.id = new_id("channel");
        }
        self.nickname = self.nickname.trim().to_string();
        self.server_chan_send_key = self.server_chan_send_key.trim().to_string();
        self.push_plus_token = self.push_plus_token.trim().to_string();
        self.push_plus_topic = self.push_plus_topic.trim().to_string();
        self.enterprise_wechat_corp_id = self.enterprise_wechat_corp_id.trim().to_string();
        self.enterprise_wechat_corp_secret = self.enterprise_wechat_corp_secret.trim().to_string();
        self.enterprise_wechat_agent_id = self.enterprise_wechat_agent_id.trim().to_string();
        self.enterprise_wechat_to_user = self.enterprise_wechat_to_user.trim().to_string();
        if self.enterprise_wechat_to_user.is_empty() {
            self.enterprise_wechat_to_user = default_enterprise_wechat_to_user();
        }
        self.wx_pusher_app_token = self.wx_pusher_app_token.trim().to_string();
        self.wx_pusher_uid = self.wx_pusher_uid.trim().to_string();
        self.bark_api = self.bark_api.trim().trim_end_matches('/').to_string();
        if self.bark_api.is_empty() {
            self.bark_api = default_bark_api();
        }
        self.bark_token = self.bark_token.trim().to_string();
        self.bark_sound = self.bark_sound.trim().to_string();
        self.chanify_token = self.chanify_token.trim().to_string();
        self.push_deer_key = self.push_deer_key.trim().to_string();
        self.ding_talk_access_token = self.ding_talk_access_token.trim().to_string();
        self.ding_talk_secret = self.ding_talk_secret.trim().to_string();
        self
    }

    fn display_name(&self) -> String {
        if self.nickname.is_empty() {
            self.channel_type.label().to_string()
        } else {
            self.nickname.clone()
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum PushRuleSortBy {
    #[default]
    AccountOrder,
    QuotaAsc,
    SubscriptionExpiryAsc,
    TokenExpiryAsc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct PushRuleTriggers {
    pub schedule_enabled: bool,
    pub schedule_interval_minutes: u64,
    pub quota_below_enabled: bool,
    pub quota_below_percent: u8,
    pub subscription_expiry_enabled: bool,
    pub subscription_expiry_hours: u64,
    pub token_expiry_enabled: bool,
    pub token_expiry_hours: u64,
    pub token_expired_enabled: bool,
    pub anomaly_enabled: bool,
}

impl Default for PushRuleTriggers {
    fn default() -> Self {
        Self {
            schedule_enabled: true,
            schedule_interval_minutes: 1440,
            quota_below_enabled: false,
            quota_below_percent: 20,
            subscription_expiry_enabled: false,
            subscription_expiry_hours: 168,
            token_expiry_enabled: false,
            token_expiry_hours: 72,
            token_expired_enabled: true,
            anomaly_enabled: true,
        }
    }
}

impl PushRuleTriggers {
    fn normalized(mut self) -> Self {
        self.schedule_interval_minutes = self
            .schedule_interval_minutes
            .clamp(1, MAX_PUSH_INTERVAL_MINUTES);
        self.quota_below_percent = self.quota_below_percent.min(100);
        self.subscription_expiry_hours = self.subscription_expiry_hours.clamp(1, 24 * 365);
        self.token_expiry_hours = self.token_expiry_hours.clamp(1, 24 * 365);
        self
    }

    fn has_event_trigger(&self) -> bool {
        self.quota_below_enabled
            || self.subscription_expiry_enabled
            || self.token_expiry_enabled
            || self.token_expired_enabled
            || self.anomaly_enabled
    }

    fn has_trigger(&self) -> bool {
        self.schedule_enabled || self.has_event_trigger()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PushRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub account_ids: Vec<String>,
    pub channel_ids: Vec<String>,
    pub triggers: PushRuleTriggers,
    pub sort_by: PushRuleSortBy,
    pub active_refresh: bool,
    pub cooldown_minutes: u64,
    pub next_run_at: u64,
    pub next_evaluation_at: u64,
    pub last_sent_at: u64,
    pub event_last_sent_at: HashMap<String, u64>,
    pub scheduled_retry_delivery_keys: Vec<String>,
}

impl Default for PushRule {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "账号状态提醒".to_string(),
            enabled: true,
            account_ids: Vec::new(),
            channel_ids: Vec::new(),
            triggers: PushRuleTriggers::default(),
            sort_by: PushRuleSortBy::default(),
            active_refresh: false,
            cooldown_minutes: DEFAULT_RULE_COOLDOWN_MINUTES,
            next_run_at: 0,
            next_evaluation_at: 0,
            last_sent_at: 0,
            event_last_sent_at: HashMap::new(),
            scheduled_retry_delivery_keys: Vec::new(),
        }
    }
}

impl PushRule {
    fn normalized(mut self) -> Self {
        self.id = self.id.trim().to_string();
        if self.id.is_empty() {
            self.id = new_id("rule");
        }
        self.name = self.name.trim().to_string();
        if self.name.is_empty() {
            self.name = "账号状态提醒".to_string();
        }
        dedupe_strings(&mut self.account_ids);
        dedupe_strings(&mut self.channel_ids);
        self.triggers = self.triggers.normalized();
        self.cooldown_minutes = self.cooldown_minutes.clamp(1, MAX_RULE_COOLDOWN_MINUTES);
        if !self.enabled || !self.triggers.schedule_enabled {
            self.next_run_at = 0;
            self.scheduled_retry_delivery_keys.clear();
        }
        if !self.enabled || !self.triggers.has_event_trigger() {
            self.next_evaluation_at = 0;
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PushSettings {
    #[serde(default = "default_true")]
    pub automation_enabled: bool,
    pub rules: Vec<PushRule>,
    pub channels: Vec<PushChannelConfig>,
}

impl Default for PushSettings {
    fn default() -> Self {
        Self {
            automation_enabled: true,
            rules: Vec::new(),
            channels: Vec::new(),
        }
    }
}

impl PushSettings {
    fn normalized(mut self) -> Self {
        let mut rule_ids = HashSet::new();
        self.rules = self
            .rules
            .into_iter()
            .map(PushRule::normalized)
            .filter(|rule| rule_ids.insert(rule.id.clone()))
            .collect();
        let mut channel_ids = HashSet::new();
        self.channels = self
            .channels
            .into_iter()
            .map(PushChannelConfig::normalized)
            .filter(|channel| channel_ids.insert(channel.id.clone()))
            .collect();
        self
    }

    fn enabled_rules(&self) -> Vec<&PushRule> {
        self.rules
            .iter()
            .filter(|rule| {
                rule.enabled
                    && rule.triggers.has_trigger()
                    && !rule.account_ids.is_empty()
                    && !rule.channel_ids.is_empty()
            })
            .collect()
    }

    fn enabled_channels(&self) -> Vec<&PushChannelConfig> {
        self.channels
            .iter()
            .filter(|channel| channel.enabled)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushLogEntry {
    id: i64,
    created_at: u64,
    trigger: String,
    rule_id: Option<String>,
    rule_name: Option<String>,
    account_id: Option<String>,
    account_label: Option<String>,
    event_types: String,
    channel_id: String,
    channel_name: String,
    success: bool,
    title: String,
    content: String,
    response: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushRunSummary {
    trigger: String,
    attempted_rules: usize,
    matched_accounts: usize,
    attempted_accounts: usize,
    skipped_accounts: usize,
    refreshed_accounts: usize,
    successful_deliveries: usize,
    failed_deliveries: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushChannelTestResult {
    channel_id: String,
    channel_name: String,
    success: bool,
    message: String,
}

pub struct PushRuntimeState {
    running: AtomicBool,
    shutting_down: AtomicBool,
    settings_lock: Mutex<()>,
}

impl Default for PushRuntimeState {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            settings_lock: Mutex::new(()),
        }
    }
}

pub(super) struct PushRunGuard<'a>(&'a AtomicBool);

impl Drop for PushRunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone)]
struct PushMessage {
    rule_id: String,
    rule_name: String,
    account_ids: Vec<String>,
    account_label: String,
    event_types: String,
    event_delivery_keys: Vec<String>,
    scheduled_delivery_keys: Vec<String>,
    title: String,
    content: String,
}

struct PushExecutionResult {
    summary: PushRunSummary,
    successful_event_deliveries: HashMap<String, HashSet<String>>,
    failed_scheduled_deliveries: HashMap<String, HashSet<String>>,
}

#[derive(Debug, Clone)]
struct ChannelSendResult {
    success: bool,
    message: String,
}

#[derive(Debug)]
struct NewPushLog<'a> {
    created_at: u64,
    trigger: &'a str,
    rule_id: Option<&'a str>,
    rule_name: Option<&'a str>,
    account_id: Option<&'a str>,
    account_label: Option<&'a str>,
    event_types: &'a str,
    channel_id: &'a str,
    channel_name: &'a str,
    success: bool,
    title: &'a str,
    content: &'a str,
    response: &'a str,
}

#[tauri::command]
pub fn push_get_settings(runtime: State<'_, PushRuntimeState>) -> Result<PushSettings, String> {
    let _guard = runtime
        .settings_lock
        .lock()
        .map_err(|_| "推送设置锁已损坏".to_string())?;
    let mut settings = read_settings()?;
    let mut changed = reconcile_rule_accounts(&mut settings)?;
    changed |= initialize_rule_runtime(&mut settings, None)?;
    if changed {
        write_settings(&settings)?;
    }
    Ok(settings)
}

#[tauri::command]
pub fn push_update_settings(
    runtime: State<'_, PushRuntimeState>,
    settings: PushSettings,
) -> Result<PushSettings, String> {
    let _guard = runtime
        .settings_lock
        .lock()
        .map_err(|_| "推送设置锁已损坏".to_string())?;
    let previous = read_settings()?;
    let mut next = settings.normalized();
    reconcile_rule_accounts(&mut next)?;
    initialize_rule_runtime(&mut next, Some(&previous))?;
    write_settings(&next)?;
    Ok(next)
}

#[tauri::command]
pub async fn push_run_now(app: AppHandle) -> Result<PushRunSummary, String> {
    let runtime = app.state::<PushRuntimeState>();
    let _guard = begin_run(&runtime)?;
    let settings = {
        let _settings_guard = runtime
            .settings_lock
            .lock()
            .map_err(|_| "推送设置锁已损坏".to_string())?;
        let mut settings = read_settings()?;
        if reconcile_rule_accounts(&mut settings)? {
            write_settings(&settings)?;
        }
        settings
    };
    let selected_rule_ids = settings
        .enabled_rules()
        .into_iter()
        .map(|rule| rule.id.clone())
        .collect::<HashSet<_>>();
    let scheduled_rule_ids = settings
        .enabled_rules()
        .into_iter()
        .filter(|rule| rule.triggers.schedule_enabled)
        .map(|rule| rule.id.clone())
        .collect::<HashSet<_>>();
    let execution =
        execute_push(&settings, "manual", &selected_rule_ids, &scheduled_rule_ids).await?;
    Ok(execution.summary)
}

#[tauri::command]
pub async fn push_run_rule_now(app: AppHandle, rule_id: String) -> Result<PushRunSummary, String> {
    let runtime = app.state::<PushRuntimeState>();
    let _guard = begin_run(&runtime)?;
    let settings = {
        let _settings_guard = runtime
            .settings_lock
            .lock()
            .map_err(|_| "推送设置锁已损坏".to_string())?;
        let mut settings = read_settings()?;
        if reconcile_rule_accounts(&mut settings)? {
            write_settings(&settings)?;
        }
        settings
    };
    let rule_id = rule_id.trim().to_string();
    let rule = settings
        .enabled_rules()
        .into_iter()
        .find(|rule| rule.id == rule_id)
        .ok_or_else(|| "推送规则不存在或未启用".to_string())?;
    let selected_rule_ids = HashSet::from([rule.id.clone()]);
    let scheduled_rule_ids = if rule.triggers.schedule_enabled {
        HashSet::from([rule.id.clone()])
    } else {
        HashSet::new()
    };
    let execution =
        execute_push(&settings, "manual", &selected_rule_ids, &scheduled_rule_ids).await?;
    Ok(execution.summary)
}

#[tauri::command]
pub async fn push_test_channel(
    app: AppHandle,
    channel: PushChannelConfig,
) -> Result<PushChannelTestResult, String> {
    let runtime = app.state::<PushRuntimeState>();
    let _guard = begin_run(&runtime)?;
    let channel = channel.normalized();
    let title = "Codex Switcher 推送测试";
    let content = "> 渠道连接正常\n\n- **状态**：配置可用\n- **来源**：Codex Switcher";
    let client = push_client()?;
    let result = send_channel(&client, &channel, title, content).await;
    if let Err(error) = insert_log(&NewPushLog {
        created_at: now_millis()?,
        trigger: "test",
        rule_id: None,
        rule_name: None,
        account_id: None,
        account_label: None,
        event_types: "test",
        channel_id: &channel.id,
        channel_name: &channel.display_name(),
        success: result.success,
        title,
        content,
        response: &result.message,
    }) {
        eprintln!("Write push test log failed: {error}");
    }
    let channel_id = channel.id.clone();
    let channel_name = channel.display_name();
    Ok(PushChannelTestResult {
        channel_id,
        channel_name,
        success: result.success,
        message: result.message,
    })
}

#[tauri::command]
pub fn push_list_logs(limit: Option<u32>) -> Result<Vec<PushLogEntry>, String> {
    list_logs(limit.unwrap_or(200).clamp(1, MAX_LOG_LIMIT))
}

#[tauri::command]
pub fn push_count_successful_logs_since(start_at: u64) -> Result<u64, String> {
    count_successful_logs_since(start_at)
}

#[tauri::command]
pub fn push_clear_logs() -> Result<usize, String> {
    let _database_guard = lock_push_database()?;
    let connection = open_log_database()?;
    connection
        .execute("DELETE FROM push_logs", [])
        .map_err(|error| format!("清空推送日志失败: {error}"))
}

pub fn start_push_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        loop {
            if app
                .state::<PushRuntimeState>()
                .shutting_down
                .load(Ordering::SeqCst)
            {
                break;
            }
            if let Err(error) = run_scheduled_push(&app).await {
                eprintln!("Scheduled account push failed: {error}");
            }
            tokio::time::sleep(Duration::from_secs(PUSH_POLL_SECONDS)).await;
        }
    });
}

pub fn shutdown_push_scheduler(runtime: State<'_, PushRuntimeState>) {
    runtime.shutting_down.store(true, Ordering::SeqCst);
}

async fn run_scheduled_push(app: &AppHandle) -> Result<(), String> {
    let runtime = app.state::<PushRuntimeState>();
    let Ok(_guard) = begin_run(&runtime) else {
        return Ok(());
    };
    let (settings, due_rule_ids, scheduled_rule_ids) = {
        let _settings_guard = runtime
            .settings_lock
            .lock()
            .map_err(|_| "推送设置锁已损坏".to_string())?;
        let mut settings = read_settings()?;
        let now = now_millis()?;
        if !settings.automation_enabled {
            return Ok(());
        }
        let mut due_rule_ids = HashSet::new();
        let mut scheduled_rule_ids = HashSet::new();
        let mut runtime_changed = reconcile_rule_accounts(&mut settings)?;
        for rule in settings.rules.iter_mut().filter(|rule| {
            rule.enabled
                && rule.triggers.has_trigger()
                && !rule.account_ids.is_empty()
                && !rule.channel_ids.is_empty()
        }) {
            let schedule_due =
                rule.triggers.schedule_enabled && rule.next_run_at > 0 && rule.next_run_at <= now;
            let event_due = rule.triggers.has_event_trigger()
                && rule.next_evaluation_at > 0
                && rule.next_evaluation_at <= now;
            if schedule_due {
                due_rule_ids.insert(rule.id.clone());
                scheduled_rule_ids.insert(rule.id.clone());
            }
            if event_due {
                rule.next_evaluation_at = next_event_evaluation(now);
                runtime_changed = true;
                due_rule_ids.insert(rule.id.clone());
            }
        }
        if due_rule_ids.is_empty() {
            if runtime_changed {
                write_settings(&settings)?;
            }
            return Ok(());
        }
        write_settings(&settings)?;
        (settings, due_rule_ids, scheduled_rule_ids)
    };
    let account_refresh_attempted = settings
        .rules
        .iter()
        .any(|rule| due_rule_ids.contains(&rule.id) && rule.active_refresh);
    let execution = execute_push(&settings, "scheduled", &due_rule_ids, &scheduled_rule_ids).await;
    if account_refresh_attempted {
        let _ = app.emit(ACCOUNT_STATE_UPDATED_EVENT, ());
    }
    let execution = match execution {
        Ok(execution) => execution,
        Err(error) => {
            complete_scheduled_runs(&runtime, &scheduled_rule_ids, None, &settings)?;
            return Err(error);
        }
    };
    complete_scheduled_runs(
        &runtime,
        &scheduled_rule_ids,
        Some(&execution.failed_scheduled_deliveries),
        &settings,
    )?;
    mark_event_deliveries(&runtime, &execution.successful_event_deliveries, &settings)?;
    Ok(())
}

pub(super) fn begin_run(runtime: &PushRuntimeState) -> Result<PushRunGuard<'_>, String> {
    runtime
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map_err(|_| "已有推送任务正在执行".to_string())?;
    Ok(PushRunGuard(&runtime.running))
}

pub(super) fn lock_settings(runtime: &PushRuntimeState) -> Result<MutexGuard<'_, ()>, String> {
    runtime
        .settings_lock
        .lock()
        .map_err(|_| "推送设置锁已损坏".to_string())
}

pub(super) fn lock_push_database() -> Result<MutexGuard<'static, ()>, String> {
    PUSH_DATABASE_LOCK
        .lock()
        .map_err(|_| "推送日志锁已损坏".to_string())
}

async fn execute_push(
    settings: &PushSettings,
    trigger: &str,
    selected_rule_ids: &HashSet<String>,
    scheduled_rule_ids: &HashSet<String>,
) -> Result<PushExecutionResult, String> {
    let rules = settings
        .enabled_rules()
        .into_iter()
        .filter(|rule| selected_rule_ids.contains(&rule.id))
        .collect::<Vec<_>>();
    let channels = settings.enabled_channels();
    if rules.is_empty() {
        return Err("未启用任何账号推送规则".to_string());
    }
    if channels.is_empty() {
        return Err("未启用任何推送渠道".to_string());
    }

    let accounts_before_refresh = AccountStore::default().list_accounts()?;
    let refresh_account_ids = rules
        .iter()
        .filter(|rule| rule.active_refresh)
        .flat_map(|rule| rule.account_ids.iter())
        .filter_map(|account_id| {
            accounts_before_refresh
                .iter()
                .find(|account| account.id == *account_id)
        })
        .filter(|account| {
            !is_api_key_account(account)
                || account
                    .bound_oauth_account_id
                    .as_deref()
                    .is_some_and(|bound_id| {
                        accounts_before_refresh
                            .iter()
                            .any(|candidate| candidate.id == bound_id)
                    })
        })
        .map(|account| {
            status_source_account(&accounts_before_refresh, account)
                .id
                .clone()
        })
        .collect::<HashSet<_>>();
    let refreshed_accounts = tokio::time::timeout(
        Duration::from_secs(PUSH_REFRESH_PHASE_TIMEOUT_SECONDS),
        async {
            stream::iter(refresh_account_ids)
                .map(|account_id| async move { refresh_account_quota(&account_id).await })
                .buffer_unordered(MAX_CONCURRENT_QUOTA_REFRESHES)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .filter(|success| *success)
                .count()
        },
    )
    .await
    .unwrap_or_default();

    let accounts = AccountStore::default().list_accounts()?;
    let mut skipped_accounts = 0;
    let mut dispatches = Vec::new();
    for rule in rules {
        let mut selected_accounts = rule
            .account_ids
            .iter()
            .filter_map(|account_id| accounts.iter().find(|account| account.id == *account_id))
            .filter(|account| {
                !is_api_key_account(account) || account.bound_oauth_account_id.is_some()
            })
            .collect::<Vec<_>>();
        skipped_accounts += rule
            .account_ids
            .len()
            .saturating_sub(selected_accounts.len());
        sort_rule_accounts(&accounts, &mut selected_accounts, rule.sort_by);
        let selected_channels = channels
            .iter()
            .filter(|channel| rule.channel_ids.contains(&channel.id))
            .collect::<Vec<_>>();
        if selected_channels.is_empty() {
            skipped_accounts += selected_accounts.len();
            continue;
        }
        let scheduled = scheduled_rule_ids.contains(&rule.id);
        let enforce_event_cooldown = trigger == "scheduled" && !scheduled;
        let mut matched_account_ids = HashSet::new();
        for channel in selected_channels {
            let channel_accounts = if scheduled && !rule.scheduled_retry_delivery_keys.is_empty() {
                selected_accounts
                    .iter()
                    .copied()
                    .filter(|account| {
                        rule.scheduled_retry_delivery_keys
                            .contains(&scheduled_delivery_key(&account.id, &channel.id))
                    })
                    .collect::<Vec<_>>()
            } else {
                selected_accounts.clone()
            };
            let messages = build_rule_push_messages(
                &accounts,
                &channel_accounts,
                rule,
                scheduled,
                &channel.id,
                enforce_event_cooldown,
            );
            for message in messages {
                matched_account_ids.extend(message.account_ids.iter().cloned());
                dispatches.push((message, (*channel).clone()));
            }
        }
        skipped_accounts += selected_accounts
            .len()
            .saturating_sub(matched_account_ids.len());
    }

    if dispatches.is_empty() {
        return Ok(PushExecutionResult {
            summary: PushRunSummary {
                trigger: trigger.to_string(),
                attempted_rules: selected_rule_ids.len(),
                matched_accounts: 0,
                attempted_accounts: 0,
                skipped_accounts,
                refreshed_accounts,
                successful_deliveries: 0,
                failed_deliveries: 0,
            },
            successful_event_deliveries: HashMap::new(),
            failed_scheduled_deliveries: HashMap::new(),
        });
    }

    let attempted_accounts = dispatches
        .iter()
        .flat_map(|(message, _)| message.account_ids.iter().map(String::as_str))
        .collect::<HashSet<_>>()
        .len();
    let attempted_rules = dispatches
        .iter()
        .map(|(message, _)| message.rule_id.as_str())
        .collect::<HashSet<_>>()
        .len();
    let client = push_client()?;
    let results = stream::iter(dispatches)
        .map(|(message, channel)| {
            let client = client.clone();
            async move {
                let result =
                    send_channel(&client, &channel, &message.title, &message.content).await;
                (message, channel, result)
            }
        })
        .buffer_unordered(MAX_CONCURRENT_PUSHES)
        .collect::<Vec<_>>()
        .await;

    let mut successful_deliveries = 0;
    let mut failed_deliveries = 0;
    let mut successful_event_deliveries = HashMap::<String, HashSet<String>>::new();
    let mut failed_scheduled_deliveries = HashMap::<String, HashSet<String>>::new();
    for (message, channel, result) in results {
        if result.success {
            successful_deliveries += 1;
            successful_event_deliveries
                .entry(message.rule_id.clone())
                .or_default()
                .extend(message.event_delivery_keys.iter().cloned());
        } else {
            failed_deliveries += 1;
            failed_scheduled_deliveries
                .entry(message.rule_id.clone())
                .or_default()
                .extend(message.scheduled_delivery_keys.iter().cloned());
        }
        let created_at = now_millis().unwrap_or_default();
        if let Err(error) = insert_log(&NewPushLog {
            created_at,
            trigger,
            rule_id: Some(&message.rule_id),
            rule_name: Some(&message.rule_name),
            account_id: None,
            account_label: Some(&message.account_label),
            event_types: &message.event_types,
            channel_id: &channel.id,
            channel_name: &channel.display_name(),
            success: result.success,
            title: &message.title,
            content: &message.content,
            response: &result.message,
        }) {
            eprintln!("Write push log failed: {error}");
        }
    }
    Ok(PushExecutionResult {
        summary: PushRunSummary {
            trigger: trigger.to_string(),
            attempted_rules,
            matched_accounts: attempted_accounts,
            attempted_accounts,
            skipped_accounts,
            refreshed_accounts,
            successful_deliveries,
            failed_deliveries,
        },
        successful_event_deliveries,
        failed_scheduled_deliveries,
    })
}

fn mark_event_deliveries(
    runtime: &PushRuntimeState,
    successful_event_deliveries: &HashMap<String, HashSet<String>>,
    evaluated_settings: &PushSettings,
) -> Result<(), String> {
    if successful_event_deliveries.is_empty() {
        return Ok(());
    }
    let _settings_guard = runtime
        .settings_lock
        .lock()
        .map_err(|_| "推送设置锁已损坏".to_string())?;
    let mut settings = read_settings()?;
    let now = now_millis()?;
    let mut changed = false;
    for (rule_id, event_keys) in successful_event_deliveries {
        if !dispatch_configuration_matches(&settings, evaluated_settings, rule_id) {
            continue;
        }
        if let Some(rule) = settings.rules.iter_mut().find(|rule| rule.id == *rule_id) {
            for event_key in event_keys {
                changed |= rule.event_last_sent_at.insert(event_key.clone(), now) != Some(now);
            }
            if !event_keys.is_empty() && rule.last_sent_at != now {
                rule.last_sent_at = now;
                changed = true;
            }
        }
    }
    if changed {
        write_settings(&settings)?;
    }
    Ok(())
}

fn complete_scheduled_runs(
    runtime: &PushRuntimeState,
    scheduled_rule_ids: &HashSet<String>,
    failed_scheduled_deliveries: Option<&HashMap<String, HashSet<String>>>,
    evaluated_settings: &PushSettings,
) -> Result<(), String> {
    if scheduled_rule_ids.is_empty() {
        return Ok(());
    }
    let _settings_guard = runtime
        .settings_lock
        .lock()
        .map_err(|_| "推送设置锁已损坏".to_string())?;
    let mut settings = read_settings()?;
    let now = now_millis()?;
    let mut changed = false;
    for rule_id in scheduled_rule_ids {
        if !dispatch_configuration_matches(&settings, evaluated_settings, rule_id) {
            continue;
        }
        let Some(rule) = settings.rules.iter_mut().find(|rule| rule.id == *rule_id) else {
            continue;
        };
        let outcome_known = failed_scheduled_deliveries.is_some();
        let failed_for_rule =
            failed_scheduled_deliveries.and_then(|deliveries| deliveries.get(rule_id));
        let (next_run_at, failed_delivery_keys) = scheduled_completion_state(
            now,
            rule.triggers.schedule_interval_minutes,
            &rule.scheduled_retry_delivery_keys,
            outcome_known,
            failed_for_rule,
        );
        if rule.scheduled_retry_delivery_keys != failed_delivery_keys {
            rule.scheduled_retry_delivery_keys = failed_delivery_keys;
            changed = true;
        }
        if rule.next_run_at != next_run_at {
            rule.next_run_at = next_run_at;
            changed = true;
        }
    }
    if changed {
        write_settings(&settings)?;
    }
    Ok(())
}

fn scheduled_completion_timestamp(now: u64, interval_minutes: u64, successful: bool) -> u64 {
    if successful {
        next_timestamp(now, interval_minutes)
    } else {
        now.saturating_add(PUSH_SCHEDULE_RETRY_SECONDS.saturating_mul(1_000))
    }
}

fn scheduled_completion_state(
    now: u64,
    interval_minutes: u64,
    previous_failed_delivery_keys: &[String],
    outcome_known: bool,
    failed_delivery_keys: Option<&HashSet<String>>,
) -> (u64, Vec<String>) {
    let mut failed_delivery_keys = match (outcome_known, failed_delivery_keys) {
        (true, Some(keys)) => keys.iter().cloned().collect::<Vec<_>>(),
        (true, None) => Vec::new(),
        (false, _) => previous_failed_delivery_keys.to_vec(),
    };
    failed_delivery_keys.sort();
    failed_delivery_keys.dedup();
    let run_completed = outcome_known && failed_delivery_keys.is_empty();
    (
        scheduled_completion_timestamp(now, interval_minutes, run_completed),
        failed_delivery_keys,
    )
}

fn rule_delivery_configuration_matches(current: &PushRule, evaluated: &PushRule) -> bool {
    current.enabled
        && evaluated.enabled
        && current.account_ids == evaluated.account_ids
        && current.channel_ids == evaluated.channel_ids
        && current.triggers == evaluated.triggers
        && current.active_refresh == evaluated.active_refresh
        && current.cooldown_minutes == evaluated.cooldown_minutes
}

fn dispatch_configuration_matches(
    current: &PushSettings,
    evaluated: &PushSettings,
    rule_id: &str,
) -> bool {
    let Some(current_rule) = current.rules.iter().find(|rule| rule.id == rule_id) else {
        return false;
    };
    let Some(evaluated_rule) = evaluated.rules.iter().find(|rule| rule.id == rule_id) else {
        return false;
    };
    if !rule_delivery_configuration_matches(current_rule, evaluated_rule) {
        return false;
    }

    evaluated_rule.channel_ids.iter().all(|channel_id| {
        let current_channel = current
            .channels
            .iter()
            .find(|channel| channel.id == *channel_id);
        let evaluated_channel = evaluated
            .channels
            .iter()
            .find(|channel| channel.id == *channel_id);
        current_channel == evaluated_channel
    })
}

async fn refresh_account_quota(account_id: &str) -> bool {
    let store = AccountStore::default();
    match fetch_codex_quota_for_account(account_id).await {
        Ok(quota) => store.update_account_quota(account_id, quota).is_ok(),
        Err(error) => {
            let _ = store.update_account_quota_error(account_id, error);
            false
        }
    }
}

#[derive(Debug, Clone)]
struct AccountPushSection {
    account_id: String,
    account_label: String,
    event_types: Vec<&'static str>,
    event_delivery_keys: Vec<String>,
    scheduled_delivery_keys: Vec<String>,
    lines: Vec<String>,
}

fn build_rule_push_messages(
    accounts: &[CodexAccount],
    selected_accounts: &[&CodexAccount],
    rule: &PushRule,
    scheduled: bool,
    channel_id: &str,
    enforce_event_cooldown: bool,
) -> Vec<PushMessage> {
    let sections = selected_accounts
        .iter()
        .filter_map(|account| {
            build_account_push_section(
                accounts,
                account,
                rule,
                scheduled,
                channel_id,
                enforce_event_cooldown,
            )
        })
        .collect::<Vec<_>>();
    if sections.is_empty() {
        return Vec::new();
    }

    let mut section_groups = Vec::<Vec<AccountPushSection>>::new();
    let mut current_group = Vec::new();
    for section in sections {
        let mut candidate = current_group.clone();
        candidate.push(section.clone());
        if !current_group.is_empty()
            && render_push_sections(&candidate).len() > MAX_PUSH_CONTENT_BYTES
        {
            section_groups.push(current_group);
            current_group = vec![section];
        } else {
            current_group = candidate;
        }
    }
    if !current_group.is_empty() {
        section_groups.push(current_group);
    }

    let total_batches = section_groups.len();
    section_groups
        .into_iter()
        .enumerate()
        .map(|(batch_index, sections)| {
            build_push_message(rule, &sections, batch_index + 1, total_batches)
        })
        .collect()
}

fn build_push_message(
    rule: &PushRule,
    sections: &[AccountPushSection],
    batch_number: usize,
    total_batches: usize,
) -> PushMessage {
    let mut event_types = Vec::new();
    let mut event_set = HashSet::new();
    let mut event_delivery_keys = Vec::new();
    let mut scheduled_delivery_keys = Vec::new();
    for section in sections {
        for event_type in &section.event_types {
            if event_set.insert(*event_type) {
                event_types.push(*event_type);
            }
        }
        event_delivery_keys.extend(section.event_delivery_keys.iter().cloned());
        scheduled_delivery_keys.extend(section.scheduled_delivery_keys.iter().cloned());
    }

    let labels = sections
        .iter()
        .take(3)
        .map(|section| section.account_label.clone())
        .collect::<Vec<_>>();
    let account_label = if sections.len() > labels.len() {
        format!("{} 等 {} 个账号", labels.join("、"), sections.len())
    } else {
        labels.join("、")
    };
    let batch_suffix = if total_batches > 1 {
        format!(" [{batch_number}/{total_batches}]")
    } else {
        String::new()
    };
    PushMessage {
        rule_id: rule.id.clone(),
        rule_name: rule.name.clone(),
        account_ids: sections
            .iter()
            .map(|section| section.account_id.clone())
            .collect(),
        account_label,
        event_types: event_types.join(","),
        event_delivery_keys,
        scheduled_delivery_keys,
        title: truncate_utf8(
            &format!(
                "Codex Switcher · {}（{}）{}",
                rule.name,
                sections.len(),
                batch_suffix
            ),
            MAX_PUSH_TITLE_BYTES,
        ),
        content: truncate_utf8(&render_push_sections(sections), MAX_PUSH_CONTENT_BYTES),
    }
}

fn render_push_sections(sections: &[AccountPushSection]) -> String {
    let mut content = vec![format!("> 匹配账号：**{}** 个", sections.len())];
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            content.push(String::new());
        }
        content.push(format!("## {}", section.account_label));
        content.push(format!(
            "> 触发：{}",
            section
                .event_types
                .iter()
                .map(|event_type| push_event_label(event_type))
                .collect::<Vec<_>>()
                .join("、")
        ));
        content.extend(section.lines.iter().cloned());
    }
    content.join("\n")
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    const SUFFIX: &str = "\n…内容已截断";
    let mut end = max_bytes.saturating_sub(SUFFIX.len()).min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &value[..end], SUFFIX)
}

fn build_account_push_section(
    accounts: &[CodexAccount],
    account: &CodexAccount,
    rule: &PushRule,
    scheduled: bool,
    channel_id: &str,
    enforce_event_cooldown: bool,
) -> Option<AccountPushSection> {
    let source = status_source_account(accounts, account);
    let quota = source.quota.as_ref();
    let quota_error = source.quota_error.as_ref();
    let subscription_expiry = parse_account_expiry(source.subscription_active_until.as_deref());
    let token_expiry = parse_account_expiry(source.access_token_expires_at.as_deref());
    let now = Utc::now();
    let token_expired_error = quota_error
        .and_then(|error| error.code.as_deref())
        .is_some_and(|code| code == "token_expired");
    let token_expired = token_expired_error || token_expiry.is_some_and(|value| value <= now);
    let anomaly = quota_error.filter(|error| error.code.as_deref() != Some("token_expired"));
    let quota_minimum = minimum_quota_percentage(quota);
    let quota_match = rule.triggers.quota_below_enabled
        && quota_minimum.is_some_and(|value| value < f64::from(rule.triggers.quota_below_percent));
    let subscription_match = rule.triggers.subscription_expiry_enabled
        && expires_within(
            subscription_expiry,
            now,
            rule.triggers.subscription_expiry_hours,
            true,
        );
    let token_expiry_match = rule.triggers.token_expiry_enabled
        && expires_within(token_expiry, now, rule.triggers.token_expiry_hours, false);
    let token_expired_match = rule.triggers.token_expired_enabled && token_expired;
    let anomaly_match = rule.triggers.anomaly_enabled && anomaly.is_some();
    let now_millis = now_millis().unwrap_or_default();
    let event_is_active = |event_type: &'static str, matched: bool| {
        if !matched {
            return false;
        }
        let event_key = event_delivery_key(&account.id, event_type, channel_id);
        !enforce_event_cooldown || !event_cooldown_active(rule, &event_key, now_millis)
    };
    let quota_event = event_is_active("quotaBelow", quota_match);
    let subscription_event = event_is_active("subscriptionExpiry", subscription_match);
    let token_expiry_event = event_is_active("tokenExpirySoon", token_expiry_match);
    let token_expired_event = event_is_active("tokenExpired", token_expired_match);
    let anomaly_event = event_is_active("anomaly", anomaly_match);
    if !scheduled
        && !quota_event
        && !subscription_event
        && !token_expiry_event
        && !token_expired_event
        && !anomaly_event
    {
        return None;
    }

    let mut event_types = Vec::new();
    let mut event_delivery_keys = Vec::new();
    let mut scheduled_delivery_keys = Vec::new();
    if scheduled {
        event_types.push("schedule");
        scheduled_delivery_keys.push(scheduled_delivery_key(&account.id, channel_id));
    }
    if quota_event {
        event_types.push("quotaBelow");
        event_delivery_keys.push(event_delivery_key(&account.id, "quotaBelow", channel_id));
    }
    if subscription_event {
        event_types.push("subscriptionExpiry");
        event_delivery_keys.push(event_delivery_key(
            &account.id,
            "subscriptionExpiry",
            channel_id,
        ));
    }
    if token_expiry_event {
        event_types.push("tokenExpirySoon");
        event_delivery_keys.push(event_delivery_key(
            &account.id,
            "tokenExpirySoon",
            channel_id,
        ));
    }
    if token_expired_event {
        event_types.push("tokenExpired");
        event_delivery_keys.push(event_delivery_key(&account.id, "tokenExpired", channel_id));
    }
    if anomaly_event {
        event_types.push("anomaly");
        event_delivery_keys.push(event_delivery_key(&account.id, "anomaly", channel_id));
    }

    let mut lines = Vec::new();
    if scheduled || quota_event {
        lines.extend(format_quota_lines(quota));
        if let Some(updated_at) = source.usage_updated_at {
            lines.push(format!("- **额度更新**：{}", format_timestamp(updated_at)));
        }
    }
    if scheduled || subscription_event {
        lines.push(format_expiry_line("订阅", subscription_expiry, now));
    }
    if scheduled || token_expiry_event || token_expired_event {
        lines.push(if token_expired && token_expiry.is_none() {
            "- **Token**：已过期".to_string()
        } else {
            format_expiry_line("Token", token_expiry, now)
        });
    }
    if let Some(error) = anomaly.filter(|_| scheduled || anomaly_event) {
        lines.push(format!("- **异常**：{}", compact_text(&error.message, 500)));
    }
    if lines.is_empty() {
        lines.push("- **状态**：暂无可显示数据".to_string());
    }

    Some(AccountPushSection {
        account_id: account.id.clone(),
        account_label: account_display_name(account),
        event_types,
        event_delivery_keys,
        scheduled_delivery_keys,
        lines,
    })
}

fn push_event_label(event_type: &str) -> &'static str {
    match event_type {
        "schedule" => "定时状态",
        "quotaBelow" => "额度不足",
        "subscriptionExpiry" => "订阅临期",
        "tokenExpirySoon" => "Token 临期",
        "tokenExpired" => "Token 已过期",
        "anomaly" => "账号异常",
        _ => "状态变化",
    }
}

fn sort_rule_accounts(
    accounts: &[CodexAccount],
    selected_accounts: &mut Vec<&CodexAccount>,
    sort_by: PushRuleSortBy,
) {
    selected_accounts.sort_by(|left, right| match sort_by {
        PushRuleSortBy::AccountOrder => std::cmp::Ordering::Equal,
        PushRuleSortBy::QuotaAsc => account_minimum_quota(accounts, left)
            .partial_cmp(&account_minimum_quota(accounts, right))
            .unwrap_or(std::cmp::Ordering::Equal),
        PushRuleSortBy::SubscriptionExpiryAsc => account_expiry_millis(accounts, left, true)
            .cmp(&account_expiry_millis(accounts, right, true)),
        PushRuleSortBy::TokenExpiryAsc => account_expiry_millis(accounts, left, false)
            .cmp(&account_expiry_millis(accounts, right, false)),
    });
}

fn account_minimum_quota(accounts: &[CodexAccount], account: &CodexAccount) -> f64 {
    let source = status_source_account(accounts, account);
    minimum_quota_percentage(source.quota.as_ref()).unwrap_or(f64::MAX)
}

fn account_expiry_millis(
    accounts: &[CodexAccount],
    account: &CodexAccount,
    subscription: bool,
) -> i64 {
    let source = status_source_account(accounts, account);
    let value = if subscription {
        source.subscription_active_until.as_deref()
    } else {
        source.access_token_expires_at.as_deref()
    };
    parse_account_expiry(value)
        .map(|expiry| expiry.timestamp_millis())
        .unwrap_or(i64::MAX)
}

fn expires_within(
    expiry: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    hours: u64,
    include_expired: bool,
) -> bool {
    let Some(expiry) = expiry else {
        return false;
    };
    let remaining = expiry.timestamp_millis() - now.timestamp_millis();
    (include_expired || remaining > 0)
        && remaining <= (hours.saturating_mul(3_600_000)).min(i64::MAX as u64) as i64
}

fn format_expiry_line(label: &str, expiry: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(expiry) = expiry else {
        return format!("- **{label}**：未知");
    };
    let time = expiry
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let remaining_seconds = (expiry.timestamp() - now.timestamp()).max(0) as u64;
    if expiry <= now {
        format!("- **{label}**：已过期（{time}）")
    } else {
        format!(
            "- **{label}**：{time}（剩余 {}）",
            format_duration(remaining_seconds)
        )
    }
}

fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    if days > 0 {
        format!("{days} 天 {hours} 小时")
    } else {
        format!("{} 小时", hours.max(1))
    }
}

fn status_source_account<'a>(
    accounts: &'a [CodexAccount],
    account: &'a CodexAccount,
) -> &'a CodexAccount {
    if is_api_key_account(account) {
        if let Some(bound_id) = account.bound_oauth_account_id.as_deref() {
            if let Some(bound) = accounts.iter().find(|candidate| candidate.id == bound_id) {
                return bound;
            }
        }
    }
    account
}

fn reconcile_rule_accounts(settings: &mut PushSettings) -> Result<bool, String> {
    if settings.rules.is_empty() {
        return Ok(false);
    }
    let accounts = AccountStore::default().list_accounts()?;
    let now = now_millis()?;
    let available_account_ids = accounts
        .iter()
        .filter(|account| {
            !is_api_key_account(account)
                || account
                    .bound_oauth_account_id
                    .as_deref()
                    .is_some_and(|bound_id| {
                        accounts.iter().any(|candidate| candidate.id == bound_id)
                    })
        })
        .map(|account| account.id.as_str())
        .collect::<HashSet<_>>();
    let mut changed = false;
    for rule in &mut settings.rules {
        let previous_len = rule.account_ids.len();
        let had_scheduled_retry = !rule.scheduled_retry_delivery_keys.is_empty();
        rule.account_ids
            .retain(|account_id| available_account_ids.contains(account_id.as_str()));
        if rule.account_ids.len() == previous_len {
            continue;
        }
        changed = true;
        let selected_account_ids = rule
            .account_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        rule.event_last_sent_at.retain(|key, _| {
            delivery_key_account_id(key)
                .is_some_and(|account_id| selected_account_ids.contains(account_id))
        });
        rule.scheduled_retry_delivery_keys.retain(|key| {
            delivery_key_account_id(key)
                .is_some_and(|account_id| selected_account_ids.contains(account_id))
        });
        if rule.account_ids.is_empty() {
            rule.next_run_at = 0;
            rule.next_evaluation_at = 0;
            rule.scheduled_retry_delivery_keys.clear();
        } else if had_scheduled_retry && rule.scheduled_retry_delivery_keys.is_empty() {
            rule.next_run_at = next_timestamp(now, rule.triggers.schedule_interval_minutes);
        }
    }
    Ok(changed)
}

fn is_api_key_account(account: &CodexAccount) -> bool {
    account.auth_mode.as_deref() == Some("apikey")
        || account
            .openai_api_key
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

fn account_display_name(account: &CodexAccount) -> String {
    account
        .account_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&account.email)
        .to_string()
}

fn format_quota_lines(quota: Option<&CodexQuota>) -> Vec<String> {
    let Some(quota) = quota else {
        return vec!["- 额度：暂无缓存".to_string()];
    };
    let mut lines = Vec::new();
    if quota.hourly_window_present != Some(false) {
        lines.push(format!(
            "- **{}**：{}%{}",
            quota_window_label(quota.hourly_window_minutes, "短周期"),
            quota.hourly_percentage,
            format_reset_time(quota.hourly_reset_time)
        ));
    }
    if quota.weekly_window_present != Some(false) {
        lines.push(format!(
            "- **{}**：{}%{}",
            quota_window_label(quota.weekly_window_minutes, "长周期"),
            quota.weekly_percentage,
            format_reset_time(quota.weekly_reset_time)
        ));
    }
    lines.extend(format_additional_quota_lines(quota));
    if lines.is_empty() {
        lines.push("- 额度：暂无可显示窗口".to_string());
    }
    lines
}

fn format_additional_quota_lines(quota: &CodexQuota) -> Vec<String> {
    let Some(raw) = quota.raw_data.as_ref() else {
        return Vec::new();
    };
    let additional_limits =
        aliased_json_value(raw, "additional_rate_limits", "additionalRateLimits")
            .or_else(|| {
                raw.get("data").and_then(|data| {
                    aliased_json_value(data, "additional_rate_limits", "additionalRateLimits")
                })
            })
            .and_then(Value::as_array);
    let Some(additional_limits) = additional_limits else {
        return Vec::new();
    };

    let mut lines = Vec::new();
    for limit in additional_limits {
        let label = aliased_json_value(limit, "limit_name", "limitName")
            .and_then(Value::as_str)
            .map(|value| value.replace(['-', '_'], " "))
            .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|value| !value.is_empty());
        let rate_limit = aliased_json_value(limit, "rate_limit", "rateLimit");
        let (Some(label), Some(rate_limit)) = (label, rate_limit) else {
            continue;
        };

        for (slot, fallback) in [("primary", "短周期"), ("secondary", "长周期")] {
            let snake_case = format!("{slot}_window");
            let camel_case = format!("{slot}Window");
            let Some(window) = aliased_json_value(rate_limit, &snake_case, &camel_case)
                .filter(|value| value.as_object().is_some_and(|object| !object.is_empty()))
            else {
                continue;
            };
            let percentage = json_number(aliased_json_value(
                window,
                "remaining_percent",
                "remainingPercent",
            ))
            .or_else(|| {
                json_number(aliased_json_value(window, "used_percent", "usedPercent"))
                    .map(|used| 100.0 - used)
            });
            let Some(percentage) = percentage else {
                continue;
            };
            let window_minutes = json_number(aliased_json_value(
                window,
                "limit_window_seconds",
                "limitWindowSeconds",
            ))
            .filter(|seconds| *seconds > 0.0)
            .map(|seconds| (seconds / 60.0).round().max(1.0) as i64);
            let reset_time = json_number(aliased_json_value(window, "reset_at", "resetAt"))
                .map(|value| value.round() as i64);
            lines.push(format!(
                "- **{} {}**：{}%{}",
                label,
                quota_window_label(window_minutes, fallback),
                format_percentage(percentage),
                format_reset_time(reset_time)
            ));
        }
    }
    lines
}

fn minimum_quota_percentage(quota: Option<&CodexQuota>) -> Option<f64> {
    let quota = quota?;
    let mut percentages = Vec::new();
    if quota.hourly_window_present != Some(false) {
        percentages.push(quota.hourly_percentage as f64);
    }
    if quota.weekly_window_present != Some(false) {
        percentages.push(quota.weekly_percentage as f64);
    }
    if let Some(raw) = quota.raw_data.as_ref() {
        let additional_limits =
            aliased_json_value(raw, "additional_rate_limits", "additionalRateLimits")
                .or_else(|| {
                    raw.get("data").and_then(|data| {
                        aliased_json_value(data, "additional_rate_limits", "additionalRateLimits")
                    })
                })
                .and_then(Value::as_array);
        if let Some(additional_limits) = additional_limits {
            for limit in additional_limits {
                let Some(rate_limit) = aliased_json_value(limit, "rate_limit", "rateLimit") else {
                    continue;
                };
                for (snake_case, camel_case) in [
                    ("primary_window", "primaryWindow"),
                    ("secondary_window", "secondaryWindow"),
                ] {
                    let Some(window) = aliased_json_value(rate_limit, snake_case, camel_case)
                    else {
                        continue;
                    };
                    if let Some(remaining) = json_number(aliased_json_value(
                        window,
                        "remaining_percent",
                        "remainingPercent",
                    ))
                    .or_else(|| {
                        json_number(aliased_json_value(window, "used_percent", "usedPercent"))
                            .map(|used| 100.0 - used)
                    }) {
                        percentages.push(remaining.clamp(0.0, 100.0));
                    }
                }
            }
        }
    }
    percentages.into_iter().reduce(f64::min)
}

fn aliased_json_value<'a>(
    value: &'a Value,
    snake_case: &str,
    camel_case: &str,
) -> Option<&'a Value> {
    value.get(snake_case).or_else(|| value.get(camel_case))
}

fn json_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .filter(|value| value.is_finite())
}

fn format_percentage(value: f64) -> String {
    let rounded = (value.clamp(0.0, 100.0) * 10.0).round() / 10.0;
    if rounded.fract().abs() < f64::EPSILON {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

fn quota_window_label(minutes: Option<i64>, fallback: &str) -> String {
    let Some(minutes) = minutes.filter(|value| *value > 0) else {
        return fallback.to_string();
    };
    if minutes % 1440 == 0 {
        format!("{} 天额度", minutes / 1440)
    } else if minutes % 60 == 0 {
        format!("{} 小时额度", minutes / 60)
    } else {
        format!("{minutes} 分钟额度")
    }
}

fn format_reset_time(timestamp: Option<i64>) -> String {
    timestamp
        .and_then(normalize_unix_seconds)
        .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0))
        .map(|value| {
            format!(
                " · {} 重置",
                value.with_timezone(&Local).format("%m-%d %H:%M")
            )
        })
        .unwrap_or_default()
}

fn parse_account_expiry(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(number) = value.parse::<i64>() {
        let seconds = normalize_unix_seconds(number)?;
        return DateTime::<Utc>::from_timestamp(seconds, 0);
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

fn normalize_unix_seconds(value: i64) -> Option<i64> {
    if value <= 0 {
        None
    } else if value > 10_000_000_000 {
        Some(value / 1000)
    } else {
        Some(value)
    }
}

fn format_timestamp(value: i64) -> String {
    normalize_unix_seconds(value)
        .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0))
        .map(|date| {
            date.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| value.to_string())
}

fn push_client() -> Result<Client, String> {
    Client::builder()
        .user_agent(PUSH_USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("初始化推送客户端失败: {error}"))
}

async fn send_channel(
    client: &Client,
    channel: &PushChannelConfig,
    title: &str,
    content: &str,
) -> ChannelSendResult {
    let result = match channel.channel_type {
        PushChannelType::ServerChan => send_server_chan(client, channel, title, content).await,
        PushChannelType::PushPlus => send_push_plus(client, channel, title, content).await,
        PushChannelType::EnterpriseWechat => {
            send_enterprise_wechat(client, channel, title, content).await
        }
        PushChannelType::WxPusher => send_wx_pusher(client, channel, title, content).await,
        PushChannelType::Bark => send_bark(client, channel, title, content).await,
        PushChannelType::Chanify => send_chanify(client, channel, title, content).await,
        PushChannelType::PushDeer => send_push_deer(client, channel, title, content).await,
        PushChannelType::DingTalk => send_ding_talk(client, channel, title, content).await,
    };
    match result {
        Ok(result) => result,
        Err(message) => ChannelSendResult {
            success: false,
            message,
        },
    }
}

async fn send_server_chan(
    client: &Client,
    channel: &PushChannelConfig,
    title: &str,
    content: &str,
) -> Result<ChannelSendResult, String> {
    require_field(&channel.server_chan_send_key, "SendKey")?;
    let mut url = Url::parse("https://sctapi.ftqq.com/").map_err(url_error)?;
    url.path_segments_mut()
        .map_err(|_| "Server酱地址无效".to_string())?
        .push(&format!("{}.send", channel.server_chan_send_key));
    let response = client
        .post(url)
        .json(&json!({ "title": title, "desp": as_markdown(content) }))
        .send()
        .await
        .map_err(request_error)?;
    parse_code_response(response, &[0, 200]).await
}

async fn send_push_plus(
    client: &Client,
    channel: &PushChannelConfig,
    title: &str,
    content: &str,
) -> Result<ChannelSendResult, String> {
    require_field(&channel.push_plus_token, "Token")?;
    let mut payload = json!({
        "token": channel.push_plus_token,
        "title": title,
        "content": as_markdown(content),
        "template": "markdown",
        "channel": "wechat"
    });
    if !channel.push_plus_topic.is_empty() {
        payload["topic"] = Value::String(channel.push_plus_topic.clone());
    }
    let response = client
        .post("https://www.pushplus.plus/send")
        .json(&payload)
        .send()
        .await
        .map_err(request_error)?;
    let status = response.status();
    let body = response.text().await.map_err(request_error)?;
    let json = parse_json(&body);
    let code = json.get("code").and_then(Value::as_i64);
    Ok(ChannelSendResult {
        success: status.is_success() && code == Some(200),
        message: response_message(status.as_u16(), &json, &body),
    })
}

async fn send_enterprise_wechat(
    client: &Client,
    channel: &PushChannelConfig,
    title: &str,
    content: &str,
) -> Result<ChannelSendResult, String> {
    require_field(&channel.enterprise_wechat_corp_id, "企业 ID")?;
    require_field(&channel.enterprise_wechat_corp_secret, "应用 Secret")?;
    require_field(&channel.enterprise_wechat_agent_id, "AgentId")?;
    let token_response = client
        .get("https://qyapi.weixin.qq.com/cgi-bin/gettoken")
        .query(&[
            ("corpid", channel.enterprise_wechat_corp_id.as_str()),
            ("corpsecret", channel.enterprise_wechat_corp_secret.as_str()),
        ])
        .send()
        .await
        .map_err(request_error)?;
    let token_status = token_response.status();
    let token_body = token_response.text().await.map_err(request_error)?;
    let token_json = parse_json(&token_body);
    if !token_status.is_success() || token_json.get("errcode").and_then(Value::as_i64) != Some(0) {
        return Ok(ChannelSendResult {
            success: false,
            message: response_message(token_status.as_u16(), &token_json, &token_body),
        });
    }
    let access_token = token_json
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "企业微信未返回 access_token".to_string())?;
    let agent_id = channel
        .enterprise_wechat_agent_id
        .parse::<i64>()
        .map(Value::from)
        .unwrap_or_else(|_| Value::String(channel.enterprise_wechat_agent_id.clone()));
    let payload = enterprise_wechat_payload(channel, title, content, agent_id);
    let response = client
        .post("https://qyapi.weixin.qq.com/cgi-bin/message/send")
        .query(&[("access_token", access_token)])
        .json(&payload)
        .send()
        .await
        .map_err(request_error)?;
    parse_error_code_response(response).await
}

fn enterprise_wechat_payload(
    channel: &PushChannelConfig,
    title: &str,
    content: &str,
    agent_id: Value,
) -> Value {
    json!({
        "touser": channel.enterprise_wechat_to_user,
        "agentid": agent_id,
        "msgtype": "text",
        "text": {
            "content": enterprise_wechat_text(title, content)
        }
    })
}

async fn send_wx_pusher(
    client: &Client,
    channel: &PushChannelConfig,
    title: &str,
    content: &str,
) -> Result<ChannelSendResult, String> {
    require_field(&channel.wx_pusher_app_token, "AppToken")?;
    require_field(&channel.wx_pusher_uid, "UID")?;
    let response = client
        .post("https://wxpusher.zjiecode.com/api/send/message")
        .json(&json!({
            "appToken": channel.wx_pusher_app_token,
            "content": as_markdown(content),
            "summary": title,
            "contentType": 3,
            "uids": [channel.wx_pusher_uid]
        }))
        .send()
        .await
        .map_err(request_error)?;
    parse_code_response(response, &[0, 1000]).await
}

async fn send_bark(
    client: &Client,
    channel: &PushChannelConfig,
    title: &str,
    content: &str,
) -> Result<ChannelSendResult, String> {
    require_field(&channel.bark_token, "Device Key")?;
    let mut url = https_url(&channel.bark_api)?;
    url.path_segments_mut()
        .map_err(|_| "Bark API 地址无效".to_string())?
        .push(&channel.bark_token);
    let mut payload = json!({
        "title": title,
        "body": as_plain_text(content),
        "group": "Codex Switcher"
    });
    if !channel.bark_sound.is_empty() {
        payload["sound"] = Value::String(channel.bark_sound.clone());
    }
    let response = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(request_error)?;
    parse_code_response(response, &[0, 200]).await
}

async fn send_chanify(
    client: &Client,
    channel: &PushChannelConfig,
    title: &str,
    content: &str,
) -> Result<ChannelSendResult, String> {
    require_field(&channel.chanify_token, "Sender Token")?;
    let mut url = Url::parse("https://api.chanify.net/v1/sender/").map_err(url_error)?;
    url.path_segments_mut()
        .map_err(|_| "Chanify 地址无效".to_string())?
        .push(&channel.chanify_token);
    let response = client
        .post(url)
        .json(&json!({
            "sound": 1,
            "priority": 10,
            "title": title,
            "text": as_plain_text(content)
        }))
        .send()
        .await
        .map_err(request_error)?;
    parse_http_response(response).await
}

async fn send_push_deer(
    client: &Client,
    channel: &PushChannelConfig,
    title: &str,
    content: &str,
) -> Result<ChannelSendResult, String> {
    require_field(&channel.push_deer_key, "PushKey")?;
    let response = client
        .post("https://api2.pushdeer.com/message/push")
        .form(&[
            ("pushkey", channel.push_deer_key.as_str()),
            ("type", "markdown"),
            ("text", title),
            ("desp", as_markdown(content).as_str()),
        ])
        .send()
        .await
        .map_err(request_error)?;
    parse_code_response(response, &[0, 200]).await
}

async fn send_ding_talk(
    client: &Client,
    channel: &PushChannelConfig,
    title: &str,
    content: &str,
) -> Result<ChannelSendResult, String> {
    require_field(&channel.ding_talk_access_token, "AccessToken")?;
    let mut url = Url::parse("https://oapi.dingtalk.com/robot/send").map_err(url_error)?;
    url.query_pairs_mut()
        .append_pair("access_token", &channel.ding_talk_access_token);
    if !channel.ding_talk_secret.is_empty() {
        let timestamp = now_millis()?;
        url.query_pairs_mut()
            .append_pair("timestamp", &timestamp.to_string())
            .append_pair(
                "sign",
                &ding_talk_signature(timestamp, &channel.ding_talk_secret)?,
            );
    }
    let response = client
        .post(url)
        .json(&json!({
            "msgtype": "markdown",
            "markdown": { "title": title, "text": as_markdown(content) },
            "at": { "atMobiles": [], "isAtAll": false }
        }))
        .send()
        .await
        .map_err(request_error)?;
    parse_error_code_response(response).await
}

async fn parse_code_response(
    response: reqwest::Response,
    success_codes: &[i64],
) -> Result<ChannelSendResult, String> {
    let status = response.status();
    let body = response.text().await.map_err(request_error)?;
    let json = parse_json(&body);
    let code = json.get("code").and_then(Value::as_i64);
    Ok(ChannelSendResult {
        success: code_response_succeeded(status, code, success_codes),
        message: response_message(status.as_u16(), &json, &body),
    })
}

fn code_response_succeeded(
    status: reqwest::StatusCode,
    code: Option<i64>,
    success_codes: &[i64],
) -> bool {
    status.is_success() && code.is_some_and(|value| success_codes.contains(&value))
}

async fn parse_error_code_response(
    response: reqwest::Response,
) -> Result<ChannelSendResult, String> {
    let status = response.status();
    let body = response.text().await.map_err(request_error)?;
    let json = parse_json(&body);
    let code = json.get("errcode").and_then(Value::as_i64);
    Ok(ChannelSendResult {
        success: status.is_success() && code == Some(0),
        message: response_message(status.as_u16(), &json, &body),
    })
}

async fn parse_http_response(response: reqwest::Response) -> Result<ChannelSendResult, String> {
    let status = response.status();
    let body = response.text().await.map_err(request_error)?;
    Ok(ChannelSendResult {
        success: status.is_success(),
        message: if body.trim().is_empty() {
            format!("HTTP {}", status.as_u16())
        } else {
            format!("HTTP {}：{}", status.as_u16(), compact_text(&body, 300))
        },
    })
}

fn parse_json(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or(Value::Null)
}

fn response_message(status: u16, json: &Value, body: &str) -> String {
    let message = ["msg", "errmsg", "message"]
        .iter()
        .find_map(|key| json.get(*key).and_then(Value::as_str))
        .map(|value| compact_text(value, 300))
        .or_else(|| (!body.trim().is_empty()).then(|| compact_text(body.trim(), 300)))
        .unwrap_or_else(|| format!("HTTP {status}"));
    let code = json
        .get("code")
        .or_else(|| json.get("errcode"))
        .map(|value| value.to_string())
        .unwrap_or_else(|| status.to_string());
    format!("code {code}：{message}")
}

fn ding_talk_signature(timestamp: u64, secret: &str) -> Result<String, String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|_| "钉钉 Secret 无效".to_string())?;
    mac.update(format!("{timestamp}\n{secret}").as_bytes());
    Ok(BASE64_STANDARD.encode(mac.finalize().into_bytes()))
}

fn require_field(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("缺少配置：{label}"))
    } else {
        Ok(())
    }
}

fn https_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(url_error)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("推送 API 必须是无内嵌凭据的 HTTPS 地址".to_string());
    }
    Ok(url)
}

fn url_error(error: url::ParseError) -> String {
    format!("推送地址无效: {error}")
}

fn request_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "推送请求超时".to_string()
    } else if error.is_connect() {
        "无法连接推送服务".to_string()
    } else if let Some(status) = error.status() {
        format!("推送服务返回 HTTP {}", status.as_u16())
    } else {
        "推送请求失败".to_string()
    }
}

fn as_markdown(content: &str) -> String {
    content
        .lines()
        .map(str::trim_end)
        .map(|line| {
            if line.is_empty() || line.trim_start().starts_with('#') || line.ends_with("  ") {
                line.to_string()
            } else {
                format!("{line}  ")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn as_plain_text(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            let without_quote = trimmed.strip_prefix("> ").unwrap_or(trimmed);
            let without_heading = if without_quote.starts_with('#') {
                without_quote.trim_start_matches('#').trim_start()
            } else {
                without_quote
            };
            without_heading.replace("**", "")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn enterprise_wechat_text(title: &str, content: &str) -> String {
    let body = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return String::new();
            }
            let without_quote = trimmed.strip_prefix("> ").unwrap_or(trimmed);
            if without_quote.starts_with('#') {
                let heading = without_quote
                    .trim_start_matches('#')
                    .trim_start()
                    .replace("**", "");
                return format!("【{heading}】");
            }
            if let Some(item) = without_quote.strip_prefix("- ") {
                return format!("• {}", item.replace("**", ""));
            }
            without_quote.replace("**", "")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if body.is_empty() {
        title.trim().to_string()
    } else {
        format!("{}\n\n{body}", title.trim())
    }
}

fn compact_text(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        compact
    } else {
        format!("{}…", compact.chars().take(max_chars).collect::<String>())
    }
}

pub(super) fn settings_path() -> PathBuf {
    switcher_config_data_dir().join(PUSH_SETTINGS_FILE)
}

pub(super) fn database_path() -> PathBuf {
    switcher_config_data_dir().join(PUSH_DATABASE_FILE)
}

fn read_settings() -> Result<PushSettings, String> {
    read_settings_from(&settings_path())
}

fn read_settings_from(path: &Path) -> Result<PushSettings, String> {
    if !path.is_file() {
        return Ok(PushSettings::default());
    }
    let content = fs::read_to_string(path).map_err(|error| format!("读取推送设置失败: {error}"))?;
    serde_json::from_str::<PushSettings>(&content)
        .map(PushSettings::normalized)
        .map_err(|error| format!("解析推送设置失败: {error}"))
}

pub(super) fn validate_settings_backup(path: &Path) -> Result<(), String> {
    read_settings_from(path).map(|_| ())
}

fn write_settings(settings: &PushSettings) -> Result<(), String> {
    write_settings_to(&settings_path(), settings)
}

fn write_settings_to(path: &Path, settings: &PushSettings) -> Result<(), String> {
    let content = serde_json::to_vec_pretty(&settings.clone().normalized())
        .map_err(|error| format!("序列化推送设置失败: {error}"))?;
    write_bytes_atomic(path, &content).map_err(|error| format!("写入推送设置失败: {error}"))
}

fn open_log_database() -> Result<Connection, String> {
    open_log_database_at(&database_path())
}

pub(super) fn checkpoint_push_database_for_backup() -> Result<(), String> {
    let path = database_path();
    if !path.is_file() {
        return Ok(());
    }
    let connection = open_log_database_at(&path)?;
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| format!("整理推送日志数据库失败: {error}"))
}

fn open_log_database_at(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建推送日志目录失败: {error}"))?;
    }
    let connection =
        Connection::open(path).map_err(|error| format!("打开推送日志失败: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("设置推送日志权限失败: {error}"))?;
    }
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| format!("配置推送日志失败: {error}"))?;
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS push_logs (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               created_at INTEGER NOT NULL,
               trigger_kind TEXT NOT NULL,
               rule_id TEXT,
               rule_name TEXT,
               account_id TEXT,
               account_label TEXT,
               event_types TEXT NOT NULL,
               channel_id TEXT NOT NULL,
               channel_name TEXT NOT NULL,
               success INTEGER NOT NULL,
               title TEXT NOT NULL,
               content TEXT NOT NULL,
               response TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_push_logs_created_at
               ON push_logs(created_at DESC);",
        )
        .map_err(|error| format!("初始化推送日志失败: {error}"))?;
    ensure_log_column(&connection, "rule_id", "TEXT")?;
    ensure_log_column(&connection, "rule_name", "TEXT")?;
    Ok(connection)
}

fn ensure_log_column(connection: &Connection, name: &str, data_type: &str) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA table_info(push_logs)")
        .map_err(|error| format!("读取推送日志结构失败: {error}"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("读取推送日志结构失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析推送日志结构失败: {error}"))?;
    drop(statement);
    if !columns.iter().any(|column| column == name) {
        connection
            .execute(
                &format!("ALTER TABLE push_logs ADD COLUMN {name} {data_type}"),
                [],
            )
            .map_err(|error| format!("升级推送日志结构失败: {error}"))?;
    }
    Ok(())
}

fn insert_log(log: &NewPushLog<'_>) -> Result<(), String> {
    let _database_guard = lock_push_database()?;
    insert_log_at(&database_path(), log)
}

fn insert_log_at(path: &Path, log: &NewPushLog<'_>) -> Result<(), String> {
    let connection = open_log_database_at(path)?;
    connection
        .execute(
            "INSERT INTO push_logs (
               created_at, trigger_kind, rule_id, rule_name, account_id, account_label,
               event_types, channel_id, channel_name, success, title, content, response
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                log.created_at as i64,
                log.trigger,
                log.rule_id,
                log.rule_name,
                log.account_id,
                log.account_label,
                log.event_types,
                log.channel_id,
                log.channel_name,
                i64::from(log.success),
                compact_text(log.title, 500),
                compact_text(log.content, 10_000),
                compact_text(log.response, 2_000),
            ],
        )
        .map(|_| ())
        .map_err(|error| format!("写入推送日志失败: {error}"))
}

fn list_logs(limit: u32) -> Result<Vec<PushLogEntry>, String> {
    let _database_guard = lock_push_database()?;
    list_logs_at(&database_path(), limit)
}

fn list_logs_at(path: &Path, limit: u32) -> Result<Vec<PushLogEntry>, String> {
    let connection = open_log_database_at(path)?;
    let mut statement = connection
        .prepare(
            "SELECT id, created_at, trigger_kind, account_id, account_label, event_types,
                    channel_id, channel_name, success, title, content, response,
                    rule_id, rule_name
             FROM push_logs
             ORDER BY id DESC
             LIMIT ?1",
        )
        .map_err(|error| format!("读取推送日志失败: {error}"))?;
    let rows = statement
        .query_map(params![limit.clamp(1, MAX_LOG_LIMIT)], |row| {
            Ok(PushLogEntry {
                id: row.get(0)?,
                created_at: row.get::<_, i64>(1)? as u64,
                trigger: row.get(2)?,
                rule_id: row.get(12)?,
                rule_name: row.get(13)?,
                account_id: row.get(3)?,
                account_label: row.get(4)?,
                event_types: row.get(5)?,
                channel_id: row.get(6)?,
                channel_name: row.get(7)?,
                success: row.get::<_, i64>(8)? != 0,
                title: row.get(9)?,
                content: row.get(10)?,
                response: row.get(11)?,
            })
        })
        .map_err(|error| format!("查询推送日志失败: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析推送日志失败: {error}"))
}

fn count_successful_logs_since(start_at: u64) -> Result<u64, String> {
    let _database_guard = lock_push_database()?;
    count_successful_logs_since_at(&database_path(), start_at)
}

fn count_successful_logs_since_at(path: &Path, start_at: u64) -> Result<u64, String> {
    let connection = open_log_database_at(path)?;
    let count = connection
        .query_row(
            "SELECT COUNT(*) FROM push_logs WHERE success = 1 AND created_at >= ?1",
            params![start_at.min(i64::MAX as u64) as i64],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("统计推送日志失败: {error}"))?;
    u64::try_from(count).map_err(|error| format!("解析推送日志统计失败: {error}"))
}

fn default_true() -> bool {
    true
}

fn default_enterprise_wechat_to_user() -> String {
    "@all".to_string()
}

fn default_bark_api() -> String {
    "https://api.day.app".to_string()
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    *values = values
        .drain(..)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect();
}

fn initialize_rule_runtime(
    settings: &mut PushSettings,
    previous: Option<&PushSettings>,
) -> Result<bool, String> {
    let now = now_millis()?;
    let previous_rules = previous
        .map(|settings| {
            settings
                .rules
                .iter()
                .map(|rule| (rule.id.as_str(), rule))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let unchanged_delivery_rule_ids = previous
        .map(|previous| {
            settings
                .rules
                .iter()
                .filter(|rule| dispatch_configuration_matches(settings, previous, &rule.id))
                .map(|rule| rule.id.clone())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    let enabled_channel_ids = settings
        .channels
        .iter()
        .filter(|channel| channel.enabled)
        .map(|channel| channel.id.as_str())
        .collect::<HashSet<_>>();
    let mut changed = false;
    for rule in &mut settings.rules {
        let has_delivery_targets = !rule.account_ids.is_empty()
            && rule
                .channel_ids
                .iter()
                .any(|channel_id| enabled_channel_ids.contains(channel_id.as_str()));
        let previous_rule = previous_rules.get(rule.id.as_str()).copied();
        let previous_schedule_matches = previous_rule.is_some_and(|previous_rule| {
            previous_rule.enabled
                && previous_rule.triggers.schedule_enabled
                && previous_rule.triggers.schedule_interval_minutes
                    == rule.triggers.schedule_interval_minutes
                && unchanged_delivery_rule_ids.contains(&rule.id)
        });
        let next_run_at = if rule.enabled && has_delivery_targets && rule.triggers.schedule_enabled
        {
            previous_rule
                .filter(|_| previous_schedule_matches)
                .map(|previous_rule| previous_rule.next_run_at)
                .or_else(|| previous.is_none().then_some(rule.next_run_at))
                .filter(|value| *value > 0)
                .unwrap_or_else(|| next_timestamp(now, rule.triggers.schedule_interval_minutes))
        } else {
            0
        };
        let previous_events_match = unchanged_delivery_rule_ids.contains(&rule.id);
        let next_evaluation_at =
            if rule.enabled && has_delivery_targets && rule.triggers.has_event_trigger() {
                previous_rule
                    .filter(|_| previous_events_match)
                    .map(|previous_rule| previous_rule.next_evaluation_at)
                    .or_else(|| {
                        previous
                            .is_none()
                            .then(|| rule.next_evaluation_at.min(next_event_evaluation(now)))
                    })
                    .filter(|value| *value > 0)
                    .unwrap_or(now)
            } else {
                0
            };
        let last_sent_at = match previous_rule {
            Some(previous_rule) if previous_events_match => previous_rule.last_sent_at,
            Some(_) => 0,
            None => rule.last_sent_at,
        };
        let event_last_sent_at = match previous_rule {
            Some(previous_rule) if previous_events_match => {
                previous_rule.event_last_sent_at.clone()
            }
            Some(_) => HashMap::new(),
            None => rule.event_last_sent_at.clone(),
        };
        let scheduled_retry_delivery_keys =
            if rule.enabled && has_delivery_targets && rule.triggers.schedule_enabled {
                match previous_rule {
                    Some(previous_rule)
                        if previous_schedule_matches
                            && unchanged_delivery_rule_ids.contains(&rule.id) =>
                    {
                        previous_rule.scheduled_retry_delivery_keys.clone()
                    }
                    Some(_) => Vec::new(),
                    None => rule.scheduled_retry_delivery_keys.clone(),
                }
            } else {
                Vec::new()
            };
        changed |= rule.next_run_at != next_run_at
            || rule.next_evaluation_at != next_evaluation_at
            || rule.last_sent_at != last_sent_at
            || rule.event_last_sent_at != event_last_sent_at
            || rule.scheduled_retry_delivery_keys != scheduled_retry_delivery_keys;
        rule.next_run_at = next_run_at;
        rule.next_evaluation_at = next_evaluation_at;
        rule.last_sent_at = last_sent_at;
        rule.event_last_sent_at = event_last_sent_at;
        rule.scheduled_retry_delivery_keys = scheduled_retry_delivery_keys;
    }
    Ok(changed)
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}-{:016x}", rand::random::<u64>())
}

fn now_millis() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|error| format!("读取系统时间失败: {error}"))
}

fn next_timestamp(now: u64, interval_minutes: u64) -> u64 {
    now.saturating_add(
        interval_minutes
            .clamp(1, MAX_PUSH_INTERVAL_MINUTES)
            .saturating_mul(60_000),
    )
}

fn next_event_evaluation(now: u64) -> u64 {
    now.saturating_add(PUSH_EVENT_CHECK_SECONDS.saturating_mul(1_000))
}

fn event_delivery_key(account_id: &str, event_type: &str, channel_id: &str) -> String {
    format!("{account_id}\u{1f}{event_type}\u{1f}{channel_id}")
}

fn scheduled_delivery_key(account_id: &str, channel_id: &str) -> String {
    format!("{account_id}\u{1f}{channel_id}")
}

fn delivery_key_account_id(value: &str) -> Option<&str> {
    value.split_once('\u{1f}').map(|(account_id, _)| account_id)
}

fn event_cooldown_active(rule: &PushRule, event_key: &str, now: u64) -> bool {
    rule.event_last_sent_at
        .get(event_key)
        .is_some_and(|last_sent_at| {
            *last_sent_at > 0
                && now < last_sent_at.saturating_add(rule.cooldown_minutes.saturating_mul(60_000))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_account() -> CodexAccount {
        CodexAccount {
            id: "account-1".to_string(),
            email: "user@example.com".to_string(),
            account_name: Some("测试账号".to_string()),
            is_hidden: false,
            tags: Vec::new(),
            auth_mode: None,
            openai_api_key: None,
            api_base_url: None,
            api_provider_name: None,
            api_official_url: None,
            default_model: None,
            plan_type: Some("pro".to_string()),
            auth_file_plan_type: None,
            bound_oauth_account_id: None,
            bound_oauth_use_local_gateway: false,
            bound_phone: None,
            subscription_active_until: None,
            access_token_expires_at: Some("2020-01-01T00:00:00Z".to_string()),
            token_updated_at: None,
            subscription_query_last_attempt_at: None,
            subscription_query_next_retry_at: None,
            subscription_query_last_error: None,
            quota: Some(CodexQuota {
                hourly_percentage: 80,
                hourly_reset_time: Some(1_900_000_000),
                hourly_window_minutes: Some(300),
                hourly_window_present: Some(true),
                weekly_percentage: 60,
                weekly_reset_time: Some(1_900_000_000),
                weekly_window_minutes: Some(10_080),
                weekly_window_present: Some(true),
                reset_credits_available: None,
                reset_credits: Vec::new(),
                reset_credits_next_expires_at: None,
                raw_data: None,
            }),
            quota_error: None,
            usage_updated_at: Some(1_900_000_000_000),
            tokens: crate::account::CodexTokens {
                id_token: String::new(),
                access_token: String::new(),
                refresh_token: None,
            },
            created_at: 1,
            last_used: 1,
        }
    }

    fn test_push_channel() -> PushChannelConfig {
        serde_json::from_value(json!({
            "id": "channel-1",
            "channelType": "pushPlus",
            "pushPlusToken": "token"
        }))
        .unwrap()
    }

    fn configured_rule(mut rule: PushRule) -> PushRule {
        rule.account_ids = vec!["account-1".to_string()];
        rule.channel_ids = vec!["channel-1".to_string()];
        rule
    }

    #[test]
    fn normalizes_rules_channels_and_interval() {
        let settings = PushSettings {
            rules: vec![
                PushRule {
                    id: "same".to_string(),
                    account_ids: vec![" account-1 ".to_string(), "account-1".to_string()],
                    channel_ids: vec!["one".to_string(), "one".to_string()],
                    triggers: PushRuleTriggers {
                        schedule_interval_minutes: 0,
                        ..PushRuleTriggers::default()
                    },
                    ..PushRule::default()
                },
                PushRule {
                    id: "same".to_string(),
                    ..PushRule::default()
                },
            ],
            channels: vec![],
            ..PushSettings::default()
        }
        .normalized();
        assert_eq!(settings.rules.len(), 1);
        assert_eq!(settings.rules[0].triggers.schedule_interval_minutes, 1);
        assert_eq!(settings.rules[0].account_ids, vec!["account-1"]);
        assert_eq!(settings.rules[0].channel_ids, vec!["one"]);
    }

    #[test]
    fn loading_settings_clamps_future_event_evaluation() {
        let now = now_millis().unwrap();
        let future_run = now + 120_000;
        let future_evaluation = now + 60_000;
        let mut settings = PushSettings {
            rules: vec![configured_rule(PushRule {
                next_run_at: future_run,
                next_evaluation_at: future_evaluation,
                ..PushRule::default()
            })],
            channels: vec![test_push_channel()],
            ..PushSettings::default()
        };
        assert!(!initialize_rule_runtime(&mut settings, None).unwrap());
        assert_eq!(settings.rules[0].next_run_at, future_run);
        assert!(settings.rules[0].next_evaluation_at <= next_event_evaluation(now));
        assert!(settings.rules[0].next_evaluation_at >= now);
    }

    #[test]
    fn loading_settings_preserves_due_rule_runtime() {
        let now = now_millis().unwrap();
        let due_run = now.saturating_sub(60_000).max(1);
        let due_evaluation = now.saturating_sub(30_000).max(1);
        let mut settings = PushSettings {
            rules: vec![configured_rule(PushRule {
                next_run_at: due_run,
                next_evaluation_at: due_evaluation,
                ..PushRule::default()
            })],
            channels: vec![test_push_channel()],
            ..PushSettings::default()
        };

        assert!(!initialize_rule_runtime(&mut settings, None).unwrap());
        assert_eq!(settings.rules[0].next_run_at, due_run);
        assert_eq!(settings.rules[0].next_evaluation_at, due_evaluation);
    }

    #[test]
    fn changing_event_trigger_rechecks_and_resets_cooldown() {
        let now = now_millis().unwrap();
        let future_evaluation = now + 3_600_000;
        let previous = PushSettings {
            rules: vec![configured_rule(PushRule {
                id: "rule-1".to_string(),
                triggers: PushRuleTriggers {
                    quota_below_enabled: true,
                    quota_below_percent: 70,
                    ..PushRuleTriggers::default()
                },
                next_evaluation_at: future_evaluation,
                last_sent_at: now.saturating_sub(120_000),
                event_last_sent_at: HashMap::from([(
                    event_delivery_key("account-1", "quotaBelow", "channel-1"),
                    now.saturating_sub(120_000),
                )]),
                ..PushRule::default()
            })],
            channels: vec![test_push_channel()],
            ..PushSettings::default()
        };
        let mut next = previous.clone();
        next.rules[0].triggers.quota_below_percent = 68;

        let before = now_millis().unwrap();
        assert!(initialize_rule_runtime(&mut next, Some(&previous)).unwrap());
        let after = now_millis().unwrap();

        assert!((before..=after).contains(&next.rules[0].next_evaluation_at));
        assert_eq!(next.rules[0].last_sent_at, 0);
        assert!(next.rules[0].event_last_sent_at.is_empty());
    }

    #[test]
    fn rules_without_delivery_targets_do_not_get_runtime_deadlines() {
        let mut settings = PushSettings {
            rules: vec![PushRule {
                next_run_at: 123,
                next_evaluation_at: 456,
                ..PushRule::default()
            }],
            channels: vec![test_push_channel()],
            ..PushSettings::default()
        };

        assert!(initialize_rule_runtime(&mut settings, None).unwrap());
        assert_eq!(settings.rules[0].next_run_at, 0);
        assert_eq!(settings.rules[0].next_evaluation_at, 0);
    }

    #[test]
    fn completed_dispatch_does_not_cool_down_changed_settings() {
        let channel = serde_json::from_value::<PushChannelConfig>(json!({
            "id": "channel-1",
            "channelType": "pushPlus",
            "pushPlusToken": "old-token"
        }))
        .unwrap();
        let evaluated = PushSettings {
            rules: vec![PushRule {
                id: "rule-1".to_string(),
                account_ids: vec!["account-1".to_string()],
                channel_ids: vec!["channel-1".to_string()],
                triggers: PushRuleTriggers {
                    quota_below_enabled: true,
                    quota_below_percent: 70,
                    ..PushRuleTriggers::default()
                },
                ..PushRule::default()
            }],
            channels: vec![channel],
            ..PushSettings::default()
        };

        assert!(dispatch_configuration_matches(
            &evaluated, &evaluated, "rule-1"
        ));

        let mut changed_rule = evaluated.clone();
        changed_rule.rules[0].triggers.quota_below_percent = 68;
        assert!(!dispatch_configuration_matches(
            &changed_rule,
            &evaluated,
            "rule-1"
        ));

        let mut changed_channel = evaluated.clone();
        changed_channel.channels[0].push_plus_token = "new-token".to_string();
        assert!(!dispatch_configuration_matches(
            &changed_channel,
            &evaluated,
            "rule-1"
        ));
    }

    #[test]
    fn event_cooldown_is_scoped_to_account_event_and_channel() {
        let now = 10_000_000;
        let key = event_delivery_key("account-1", "quotaBelow", "channel-1");
        let mut rule = PushRule {
            cooldown_minutes: 60,
            event_last_sent_at: HashMap::from([(key.clone(), now - 30 * 60_000)]),
            ..PushRule::default()
        };
        assert!(event_cooldown_active(&rule, &key, now));
        assert!(!event_cooldown_active(
            &rule,
            &event_delivery_key("account-2", "quotaBelow", "channel-1"),
            now,
        ));
        assert!(!event_cooldown_active(
            &rule,
            &event_delivery_key("account-1", "quotaBelow", "channel-2"),
            now,
        ));
        rule.event_last_sent_at
            .insert(key.clone(), now - 61 * 60_000);
        assert!(!event_cooldown_active(&rule, &key, now));
    }

    #[test]
    fn cooled_account_does_not_suppress_another_account_or_channel() {
        let mut first = test_account();
        first.id = "account-1".to_string();
        let mut second = test_account();
        second.id = "account-2".to_string();
        second.email = "second@example.com".to_string();
        let now = now_millis().unwrap();
        let rule = PushRule {
            triggers: PushRuleTriggers {
                schedule_enabled: false,
                quota_below_enabled: true,
                quota_below_percent: 70,
                token_expired_enabled: false,
                anomaly_enabled: false,
                ..PushRuleTriggers::default()
            },
            event_last_sent_at: HashMap::from([(
                event_delivery_key("account-1", "quotaBelow", "channel-1"),
                now,
            )]),
            ..PushRule::default()
        };
        let accounts = vec![first, second];
        let selected_accounts = accounts.iter().collect::<Vec<_>>();

        let first_channel = build_rule_push_messages(
            &accounts,
            &selected_accounts,
            &rule,
            false,
            "channel-1",
            true,
        );
        assert_eq!(first_channel.len(), 1);
        assert_eq!(first_channel[0].account_ids, vec!["account-2"]);

        let second_channel = build_rule_push_messages(
            &accounts,
            &selected_accounts,
            &rule,
            false,
            "channel-2",
            true,
        );
        assert_eq!(second_channel[0].account_ids.len(), 2);

        let manual = build_rule_push_messages(
            &accounts,
            &selected_accounts,
            &rule,
            false,
            "channel-1",
            false,
        );
        assert_eq!(manual[0].account_ids.len(), 2);
    }

    #[test]
    fn scheduled_completion_retries_failures_without_waiting_full_interval() {
        let now = 1_000_000;
        assert_eq!(
            scheduled_completion_timestamp(now, 1440, false),
            now + PUSH_SCHEDULE_RETRY_SECONDS * 1_000,
        );
        assert_eq!(
            scheduled_completion_timestamp(now, 1440, true),
            now + 1440 * 60_000,
        );
        let previous = vec!["account-1\u{1f}channel-1".to_string()];
        let failed = HashSet::from(["account-2\u{1f}channel-2".to_string()]);
        assert_eq!(
            scheduled_completion_state(now, 1440, &previous, true, Some(&failed)),
            (
                now + PUSH_SCHEDULE_RETRY_SECONDS * 1_000,
                vec!["account-2\u{1f}channel-2".to_string()],
            ),
        );
        assert_eq!(
            scheduled_completion_state(now, 1440, &previous, true, None),
            (now + 1440 * 60_000, Vec::new()),
        );
        assert_eq!(
            scheduled_completion_state(now, 1440, &previous, false, None),
            (now + PUSH_SCHEDULE_RETRY_SECONDS * 1_000, previous,),
        );
    }

    #[test]
    fn quota_threshold_is_strictly_below() {
        let account = test_account();
        let mut rule = PushRule {
            triggers: PushRuleTriggers {
                schedule_enabled: false,
                quota_below_enabled: true,
                quota_below_percent: 60,
                token_expired_enabled: false,
                anomaly_enabled: false,
                ..PushRuleTriggers::default()
            },
            ..PushRule::default()
        };
        let accounts = vec![account];

        assert!(build_rule_push_messages(
            &accounts,
            &[&accounts[0]],
            &rule,
            false,
            "channel-1",
            false,
        )
        .is_empty());
        rule.triggers.quota_below_percent = 61;
        assert_eq!(
            build_rule_push_messages(&accounts, &[&accounts[0]], &rule, false, "channel-1", false,)
                .len(),
            1,
        );
    }

    #[test]
    fn cached_quota_and_expired_token_build_a_message() {
        let mut account = test_account();
        account.quota.as_mut().unwrap().raw_data = Some(json!({
            "additional_rate_limits": [{
                "limit_name": "GPT-5.3-Codex-Spark",
                "rate_limit": {
                    "primary_window": {
                        "remaining_percent": 99.5,
                        "limit_window_seconds": 18_000,
                        "reset_at": 1_900_000_000
                    },
                    "secondary_window": {
                        "used_percent": 10,
                        "limit_window_seconds": 604_800,
                        "reset_at": 1_900_000_000
                    }
                }
            }]
        }));
        let rule = PushRule::default();
        let accounts = vec![account];
        let messages =
            build_rule_push_messages(&accounts, &[&accounts[0]], &rule, true, "channel-1", false);
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.content.contains("**5 小时额度**：80%"));
        assert!(message.content.contains("**7 天额度**：60%"));
        assert!(message
            .content
            .contains("**GPT 5.3 Codex Spark 5 小时额度**：99.5%"));
        assert!(message
            .content
            .contains("**GPT 5.3 Codex Spark 7 天额度**：90%"));
        assert!(message.content.contains("**Token**：已过期"));
        assert!(message.content.contains("> 触发：定时状态、Token 已过期"));
        assert_eq!(message.event_types, "schedule,tokenExpired");
        assert_eq!(
            message.scheduled_delivery_keys,
            vec![scheduled_delivery_key("account-1", "channel-1")]
        );
    }

    #[test]
    fn bound_api_key_uses_the_oauth_source_status() {
        let mut oauth = test_account();
        oauth.id = "oauth-1".to_string();
        oauth.access_token_expires_at = Some("2099-01-01T00:00:00Z".to_string());
        let mut api_key = test_account();
        api_key.id = "api-1".to_string();
        api_key.email = "api-key".to_string();
        api_key.auth_mode = Some("apikey".to_string());
        api_key.openai_api_key = Some("sk-test".to_string());
        api_key.bound_oauth_account_id = Some(oauth.id.clone());
        api_key.quota.as_mut().unwrap().hourly_percentage = 5;
        api_key.quota.as_mut().unwrap().weekly_percentage = 5;
        api_key.quota_error = Some(crate::account::CodexQuotaErrorInfo {
            code: Some("quota_error".to_string()),
            message: "旧异常".to_string(),
            timestamp: 1,
        });
        let accounts = vec![oauth, api_key];
        let messages = build_rule_push_messages(
            &accounts,
            &[&accounts[1]],
            &PushRule::default(),
            true,
            "channel-1",
            false,
        );

        assert_eq!(messages.len(), 1);
        assert!(messages[0].content.contains("**5 小时额度**：80%"));
        assert!(messages[0].content.contains("**7 天额度**：60%"));
        assert!(!messages[0].content.contains("旧异常"));
        assert_eq!(account_minimum_quota(&accounts, &accounts[1]), 60.0);
    }

    #[test]
    fn enterprise_wechat_uses_wechat_compatible_text_payload() {
        let channel = serde_json::from_value::<PushChannelConfig>(json!({
            "channelType": "enterpriseWechat",
            "enterpriseWechatToUser": "@all"
        }))
        .unwrap();
        let payload = enterprise_wechat_payload(
            &channel,
            "Codex Switcher · 额度提醒",
            "> 匹配账号：**1** 个\n\n## user@example.com\n- **7 天额度**：67%",
            Value::from(1),
        );

        assert_eq!(payload["msgtype"], "text");
        assert!(payload.get("markdown").is_none());
        assert_eq!(
            payload["text"]["content"].as_str().unwrap(),
            "Codex Switcher · 额度提醒\n\n匹配账号：1 个\n\n【user@example.com】\n• 7 天额度：67%"
        );
    }

    #[test]
    fn code_response_requires_an_explicit_success_code() {
        assert!(!code_response_succeeded(
            reqwest::StatusCode::OK,
            None,
            &[0, 200],
        ));
        assert!(!code_response_succeeded(
            reqwest::StatusCode::BAD_GATEWAY,
            Some(0),
            &[0, 200],
        ));
        assert!(code_response_succeeded(
            reqwest::StatusCode::OK,
            Some(200),
            &[0, 200],
        ));
    }

    #[test]
    fn healthy_event_only_rule_does_not_send() {
        let mut account = test_account();
        account.access_token_expires_at = Some("2099-01-01T00:00:00Z".to_string());
        let rule = PushRule {
            triggers: PushRuleTriggers {
                schedule_enabled: false,
                quota_below_enabled: false,
                subscription_expiry_enabled: false,
                token_expiry_enabled: false,
                token_expired_enabled: true,
                anomaly_enabled: true,
                ..PushRuleTriggers::default()
            },
            ..PushRule::default()
        };
        let accounts = vec![account];
        assert!(build_rule_push_messages(
            &accounts,
            &[&accounts[0]],
            &rule,
            false,
            "channel-1",
            true,
        )
        .is_empty());
    }

    #[test]
    fn large_rule_messages_are_split_without_losing_accounts() {
        let accounts = (0..24)
            .map(|index| {
                let mut account = test_account();
                account.id = format!("account-{index}");
                account.email = format!("user-{index}@example.com");
                account.account_name = Some(format!("测试账号 {index}"));
                account
            })
            .collect::<Vec<_>>();
        let selected_accounts = accounts.iter().collect::<Vec<_>>();
        let messages = build_rule_push_messages(
            &accounts,
            &selected_accounts,
            &PushRule::default(),
            true,
            "channel-1",
            false,
        );

        assert!(messages.len() > 1);
        assert!(messages
            .iter()
            .all(|message| message.content.len() <= MAX_PUSH_CONTENT_BYTES));
        assert!(messages.iter().all(|message| message.title.contains('/')));
        let account_ids = messages
            .iter()
            .flat_map(|message| message.account_ids.iter().cloned())
            .collect::<HashSet<_>>();
        assert_eq!(account_ids.len(), accounts.len());
    }

    #[test]
    fn settings_round_trip_and_logs_use_sqlite() {
        let directory = tempdir().unwrap();
        let settings_path = directory.path().join("push-settings.json");
        let database_path = directory.path().join("push.sqlite");
        let settings = PushSettings {
            rules: vec![PushRule {
                id: "rule-1".to_string(),
                triggers: PushRuleTriggers {
                    schedule_interval_minutes: 30,
                    ..PushRuleTriggers::default()
                },
                ..PushRule::default()
            }],
            ..PushSettings::default()
        };
        write_settings_to(&settings_path, &settings).unwrap();
        assert_eq!(
            read_settings_from(&settings_path).unwrap().rules[0]
                .triggers
                .schedule_interval_minutes,
            30
        );

        insert_log_at(
            &database_path,
            &NewPushLog {
                created_at: 123,
                trigger: "test",
                rule_id: Some("rule-1"),
                rule_name: Some("测试规则"),
                account_id: Some("account-1"),
                account_label: Some("测试账号"),
                event_types: "quota",
                channel_id: "channel-1",
                channel_name: "Bark",
                success: true,
                title: "title",
                content: "content",
                response: "ok",
            },
        )
        .unwrap();
        let logs = list_logs_at(&database_path, 20).unwrap();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].success);
        assert_eq!(logs[0].channel_name, "Bark");
        assert_eq!(logs[0].rule_name.as_deref(), Some("测试规则"));
        assert_eq!(
            count_successful_logs_since_at(&database_path, 100).unwrap(),
            1
        );
        assert_eq!(
            count_successful_logs_since_at(&database_path, 124).unwrap(),
            0
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&database_path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn existing_log_database_is_migrated_for_rule_columns() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("push.sqlite");
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE push_logs (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   created_at INTEGER NOT NULL,
                   trigger_kind TEXT NOT NULL,
                   account_id TEXT,
                   account_label TEXT,
                   event_types TEXT NOT NULL,
                   channel_id TEXT NOT NULL,
                   channel_name TEXT NOT NULL,
                   success INTEGER NOT NULL,
                   title TEXT NOT NULL,
                   content TEXT NOT NULL,
                   response TEXT NOT NULL
                 );",
            )
            .unwrap();
        drop(connection);

        insert_log_at(
            &database_path,
            &NewPushLog {
                created_at: 456,
                trigger: "manual",
                rule_id: Some("rule-2"),
                rule_name: Some("迁移规则"),
                account_id: None,
                account_label: Some("两个账号"),
                event_types: "quotaBelow",
                channel_id: "channel-2",
                channel_name: "PushPlus",
                success: true,
                title: "title",
                content: "content",
                response: "ok",
            },
        )
        .unwrap();
        let logs = list_logs_at(&database_path, 20).unwrap();
        assert_eq!(logs[0].rule_id.as_deref(), Some("rule-2"));
        assert_eq!(logs[0].rule_name.as_deref(), Some("迁移规则"));
    }

    #[test]
    fn custom_push_api_requires_https_without_credentials() {
        assert!(https_url("https://api.day.app").is_ok());
        assert!(https_url("http://api.day.app").is_err());
        assert!(https_url("https://user:pass@example.com").is_err());
        assert!(https_url("https://:pass@example.com").is_err());
    }
}
