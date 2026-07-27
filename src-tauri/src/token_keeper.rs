use crate::{
    account::{
        is_refresh_token_failure_message, jwt_expiration_timestamp, AccountStore, CodexAccount,
        CodexTokens,
    },
    oauth,
};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tauri::{AppHandle, Emitter};

const TOKEN_KEEPER_TICK_SECONDS: u64 = 60;
const TOKEN_REFRESH_LEAD_SECONDS: i64 = 5 * 60;
const TOKEN_PROACTIVE_REFRESH_SECONDS: i64 = 8 * 24 * 60 * 60;
const REFRESH_FAILURE_BACKOFF_SECONDS: i64 = 15 * 60;
const ACCOUNT_STATE_UPDATED_EVENT: &str = "codex-account-state-updated";

static TOKEN_REFRESH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static NEXT_ALLOWED_ATTEMPT_AT: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();

pub fn start_token_keeper(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        loop {
            run_refresh_cycle(&app).await;
            tokio::time::sleep(Duration::from_secs(TOKEN_KEEPER_TICK_SECONDS)).await;
        }
    });
}

pub(crate) async fn ensure_fresh_access_token(
    account_id: &str,
    reason: &str,
) -> Result<CodexAccount, String> {
    let _guard = TOKEN_REFRESH_LOCK.lock().await;
    let store = AccountStore::default();
    let _ = store.sync_oauth_account_from_current_auth(account_id);
    let mut account = load_oauth_account(&store, account_id)?;
    let now = now_timestamp();
    if has_stale_refresh_token_error(&account, now) {
        account = store.clear_account_refresh_token_error(account_id)?;
    }
    if !token_rotation_required(&account, now) {
        return Ok(account);
    }
    let access_token_usable = !access_token_refresh_required(&account, now);
    if access_token_usable && !allow_attempt(&account.id) {
        return Ok(account);
    }
    match refresh_account(&store, account, reason).await {
        Ok(updated) => {
            clear_attempt_backoff(account_id);
            Ok(updated)
        }
        Err(error) if access_token_usable => {
            mark_attempt_failure(account_id);
            eprintln!(
                "Codex Token 提前续期失败，继续使用尚未过期的 access_token: account_id={account_id}, error={error}"
            );
            load_oauth_account(&store, account_id)
        }
        Err(error) => Err(error),
    }
}

async fn refresh_account_if_due(
    account_id: &str,
    reason: &str,
) -> Result<Option<CodexAccount>, String> {
    let _guard = TOKEN_REFRESH_LOCK.lock().await;
    let store = AccountStore::default();
    let _ = store.sync_oauth_account_from_current_auth(account_id);
    let account = load_oauth_account(&store, account_id)?;
    if !token_refresh_due(&account, now_timestamp()) {
        return Ok(None);
    }
    refresh_account(&store, account, reason).await.map(Some)
}

async fn refresh_account(
    store: &AccountStore,
    account: CodexAccount,
    reason: &str,
) -> Result<CodexAccount, String> {
    let access_refresh_required = access_token_refresh_required(&account, now_timestamp());
    let refresh_token = account
        .tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            let message = "Codex 登录授权缺少 refresh_token，无法自动续期，请重新登录".to_string();
            if access_refresh_required {
                let _ = store.update_account_quota_error(&account.id, message.clone());
            }
            message
        })?;

    let refreshed = oauth::refresh_access_token_with_fallback(
        refresh_token,
        Some(account.tokens.id_token.as_str()),
    )
    .await
    .map_err(|error| {
        let message = format_refresh_error(&error);
        if access_refresh_required {
            let _ = store.update_account_quota_error(&account.id, message.clone());
        } else {
            let _ = store.clear_account_refresh_token_error(&account.id);
        }
        message
    })?;
    store
        .update_refreshed_oauth_tokens(
            &account.id,
            CodexTokens {
                id_token: refreshed.id_token,
                access_token: refreshed.access_token,
                refresh_token: refreshed.refresh_token,
            },
        )
        .map_err(|error| format!("{reason}，保存刷新后的 Token 失败: {error}"))
}

async fn run_refresh_cycle(app: &AppHandle) {
    let accounts = match AccountStore::default().list_accounts() {
        Ok(accounts) => accounts,
        Err(error) => {
            eprintln!("TokenKeeper 读取 Codex 账号失败: {error}");
            return;
        }
    };
    let mut state_changed = false;
    for account in accounts
        .into_iter()
        .filter(|account| account.auth_mode.as_deref() != Some("apikey"))
    {
        if has_stale_refresh_token_error(&account, now_timestamp()) {
            match AccountStore::default().clear_account_refresh_token_error(&account.id) {
                Ok(_) => {
                    state_changed = true;
                }
                Err(error) => {
                    eprintln!(
                        "TokenKeeper 清理旧版 Token 续期错误失败: account_id={}, error={error}",
                        account.id
                    );
                }
            }
        }
        if !token_refresh_due(&account, now_timestamp()) || !allow_attempt(&account.id) {
            continue;
        }
        match refresh_account_if_due(&account.id, "TokenKeeper 授权保活").await {
            Ok(Some(updated)) => {
                clear_attempt_backoff(&account.id);
                state_changed = true;
                eprintln!(
                    "TokenKeeper Codex Token 保活成功: account_id={}, email={}",
                    updated.id, updated.email
                );
                if let Err(error) =
                    crate::subscription::refresh_account_subscription(&updated.id, true).await
                {
                    eprintln!(
                        "TokenKeeper 刷新 Codex 订阅状态失败: account_id={}, error={error}",
                        updated.id
                    );
                }
            }
            Ok(None) => {
                clear_attempt_backoff(&account.id);
            }
            Err(error) => {
                mark_attempt_failure(&account.id);
                state_changed = true;
                eprintln!(
                    "TokenKeeper Codex Token 保活失败，15 分钟后重试: account_id={}, error={error}",
                    account.id
                );
            }
        }
    }

    let subscription_accounts = match AccountStore::default().list_accounts() {
        Ok(accounts) => accounts,
        Err(error) => {
            eprintln!("TokenKeeper 读取订阅状态账号失败: {error}");
            Vec::new()
        }
    };
    for account in subscription_accounts
        .into_iter()
        .filter(|account| account.auth_mode.as_deref() != Some("apikey"))
        .filter(crate::subscription::subscription_refresh_due)
    {
        if !allow_attempt(&account.id) {
            continue;
        }
        match crate::subscription::refresh_account_subscription(&account.id, false).await {
            Ok(changed) => {
                state_changed |= changed;
            }
            Err(error) => {
                eprintln!(
                    "TokenKeeper 刷新 Codex 订阅状态失败: account_id={}, error={error}",
                    account.id
                );
            }
        }
    }

    if state_changed {
        let _ = app.emit(ACCOUNT_STATE_UPDATED_EVENT, ());
    }
}

fn load_oauth_account(store: &AccountStore, account_id: &str) -> Result<CodexAccount, String> {
    let account = store
        .list_accounts()?
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| "账号不存在".to_string())?;
    if account.auth_mode.as_deref() == Some("apikey") {
        return Err("API Key 账号不支持刷新 OAuth Token".to_string());
    }
    Ok(account)
}

pub(crate) fn token_refresh_due(account: &CodexAccount, now: i64) -> bool {
    if account.auth_mode.as_deref() == Some("apikey")
        || account
            .tokens
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .is_none()
    {
        return false;
    }
    token_rotation_required(account, now)
}

fn token_rotation_required(account: &CodexAccount, now: i64) -> bool {
    access_token_refresh_required(account, now)
        || account
            .token_updated_at
            .map(|updated_at| updated_at <= now - TOKEN_PROACTIVE_REFRESH_SECONDS)
            .unwrap_or(false)
}

fn access_token_refresh_required(account: &CodexAccount, now: i64) -> bool {
    jwt_expiration_timestamp(&account.tokens.access_token)
        .or_else(|| {
            account
                .access_token_expires_at
                .as_deref()
                .and_then(parse_timestamp_seconds)
        })
        .map(|expires_at| expires_at <= now + TOKEN_REFRESH_LEAD_SECONDS)
        .unwrap_or(true)
}

fn has_stale_refresh_token_error(account: &CodexAccount, now: i64) -> bool {
    !access_token_refresh_required(account, now)
        && account
            .quota_error
            .as_ref()
            .is_some_and(|error| is_refresh_token_failure_message(&error.message))
}

fn parse_timestamp_seconds(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if let Ok(mut timestamp) = trimmed.parse::<i64>() {
        if timestamp > 1_000_000_000_000 {
            timestamp /= 1000;
        }
        return Some(timestamp);
    }
    chrono::DateTime::parse_from_rfc3339(trimmed)
        .ok()
        .map(|value| value.timestamp())
}

fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

fn allow_attempt(account_id: &str) -> bool {
    next_allowed_attempts()
        .lock()
        .ok()
        .and_then(|state| state.get(account_id).copied())
        .map(|next_attempt| next_attempt <= now_timestamp())
        .unwrap_or(true)
}

fn clear_attempt_backoff(account_id: &str) {
    if let Ok(mut state) = next_allowed_attempts().lock() {
        state.remove(account_id);
    }
}

fn mark_attempt_failure(account_id: &str) {
    if let Ok(mut state) = next_allowed_attempts().lock() {
        state.insert(
            account_id.to_string(),
            now_timestamp() + REFRESH_FAILURE_BACKOFF_SECONDS,
        );
    }
}

fn next_allowed_attempts() -> &'static Mutex<HashMap<String, i64>> {
    NEXT_ALLOWED_ATTEMPT_AT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn format_refresh_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("refresh_token_reused") {
        return format!(
            "Codex 授权已失效：refresh_token 已被其它客户端或实例使用，请重新登录。原始错误: {error}"
        );
    }
    if lower.contains("refresh_token_expired") {
        return format!("Codex 登录授权已过期，无法自动刷新，请重新登录。原始错误: {error}");
    }
    if lower.contains("refresh_token_invalidated") || lower.contains("invalid_grant") {
        return format!("Codex 登录授权已失效，无法自动刷新，请重新登录。原始错误: {error}");
    }
    format!("Codex Token 自动刷新失败: {error}")
}

#[cfg(test)]
mod tests {
    use super::{
        access_token_refresh_required, has_stale_refresh_token_error, token_refresh_due,
        token_rotation_required, TOKEN_PROACTIVE_REFRESH_SECONDS,
    };
    use crate::account::{CodexAccount, CodexQuotaErrorInfo, CodexTokens};

    fn account() -> CodexAccount {
        CodexAccount {
            id: "account-1".to_string(),
            email: "owner@example.com".to_string(),
            account_name: None,
            auth_mode: None,
            openai_api_key: None,
            api_base_url: None,
            api_provider_name: None,
            api_official_url: None,
            default_model: None,
            plan_type: None,
            auth_file_plan_type: None,
            bound_oauth_account_id: None,
            bound_oauth_use_local_gateway: false,
            bound_phone: None,
            subscription_active_until: None,
            access_token_expires_at: Some("4102444800".to_string()),
            token_updated_at: Some(1_000_000),
            subscription_query_last_attempt_at: None,
            subscription_query_next_retry_at: None,
            subscription_query_last_error: None,
            quota: None,
            quota_error: None,
            usage_updated_at: None,
            tokens: CodexTokens {
                id_token: String::new(),
                access_token: "not-a-jwt".to_string(),
                refresh_token: Some("refresh-token".to_string()),
            },
            created_at: 1,
            last_used: 1,
        }
    }

    #[test]
    fn refresh_is_due_after_eight_days_even_when_access_token_is_valid() {
        let mut account = account();
        let now = account.token_updated_at.unwrap() + TOKEN_PROACTIVE_REFRESH_SECONDS + 1;
        assert!(token_refresh_due(&account, now));

        account.token_updated_at = Some(now);
        assert!(!token_refresh_due(&account, now));
    }

    #[test]
    fn account_without_refresh_token_is_not_automatically_rotated() {
        let mut account = account();
        account.tokens.refresh_token = None;
        assert!(!token_refresh_due(&account, 2_000_000));
    }

    #[test]
    fn unknown_refresh_time_does_not_force_a_valid_token_rotation() {
        let mut account = account();
        account.token_updated_at = None;
        assert!(!token_rotation_required(&account, 2_000_000));
    }

    #[test]
    fn access_token_expiring_tomorrow_remains_usable_after_proactive_refresh_failure() {
        let mut account = account();
        let now = 2_000_000;
        account.access_token_expires_at = Some((now + 24 * 60 * 60).to_string());
        account.token_updated_at = Some(now - TOKEN_PROACTIVE_REFRESH_SECONDS - 1);

        assert!(token_rotation_required(&account, now));
        assert!(!access_token_refresh_required(&account, now));
    }

    #[test]
    fn stale_refresh_token_error_is_clearable_while_access_token_is_valid() {
        let mut account = account();
        account.quota_error = Some(CodexQuotaErrorInfo {
            code: Some("token_expired".to_string()),
            message: "refresh_token 已被其它客户端使用".to_string(),
            timestamp: 1,
        });

        assert!(has_stale_refresh_token_error(&account, 2_000_000));
        assert!(!has_stale_refresh_token_error(&account, 4_102_444_600));
    }
}
