use crate::{
    account::{chatgpt_account_id, AccountStore, CodexAccount},
    token_keeper,
};
use serde_json::{Map, Value};
use std::time::Duration;

const ACCOUNTS_CHECK_URL: &str = "https://chatgpt.com/backend-api/accounts/check/v4-2023-04-27";
const SUBSCRIPTIONS_URL: &str = "https://chatgpt.com/backend-api/subscriptions";
const CHATGPT_REFERER: &str = "https://chatgpt.com/";
const CHATGPT_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default, PartialEq)]
struct SubscriptionSnapshot {
    account_id: Option<String>,
    plan_type: Option<String>,
    active_until: Option<String>,
}

#[derive(Debug, Clone)]
struct AccountRecord {
    key: Option<String>,
    node: Value,
}

pub(crate) async fn refresh_account_subscription(
    account_id: &str,
    force: bool,
) -> Result<bool, String> {
    let account =
        token_keeper::ensure_fresh_access_token(account_id, "订阅状态刷新前 Token 已过期").await?;
    if !should_refresh(&account, force) {
        return Ok(false);
    }

    let previous_plan = account.plan_type.clone();
    let previous_active_until = account.subscription_active_until.clone();
    let snapshot = match fetch_subscription_status(&account).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = AccountStore::default().update_account_subscription_status(
                account_id,
                None,
                None,
                Some(error.clone()),
            );
            return Err(error);
        }
    };
    let retry_error = subscription_missing_or_expired(snapshot.active_until.as_deref())
        .then(|| "订阅接口未返回有效订阅时间".to_string());
    let updated = AccountStore::default().update_account_subscription_status(
        account_id,
        snapshot.plan_type,
        snapshot.active_until,
        retry_error,
    )?;
    Ok(previous_plan != updated.plan_type
        || previous_active_until != updated.subscription_active_until)
}

pub(crate) fn subscription_refresh_due(account: &CodexAccount) -> bool {
    account.auth_mode.as_deref() != Some("apikey") && should_refresh(account, false)
}

fn should_refresh(account: &CodexAccount, force: bool) -> bool {
    if force {
        return true;
    }
    if !subscription_missing_or_expired(account.subscription_active_until.as_deref()) {
        return false;
    }
    account
        .subscription_query_next_retry_at
        .map(|next_retry_at| next_retry_at <= now_timestamp())
        .unwrap_or(true)
}

async fn fetch_subscription_status(account: &CodexAccount) -> Result<SubscriptionSnapshot, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| format!("初始化订阅请求失败: {error}"))?;
    let payload = request_json(
        client.get(ACCOUNTS_CHECK_URL).query(&[(
            "timezone_offset_min",
            chrono::Local::now().offset().local_minus_utc() / 60,
        )]),
        account,
        "/backend-api/accounts/check/v4-2023-04-27",
        "订阅账号信息",
    )
    .await?;
    let mut snapshot = parse_account_check_snapshot(&payload, account)?;
    if !subscription_missing_or_expired(snapshot.active_until.as_deref()) {
        return Ok(snapshot);
    }

    let remote_account_id = snapshot
        .account_id
        .clone()
        .or_else(|| chatgpt_account_id(account))
        .ok_or_else(|| "未获取到 ChatGPT account_id，无法查询订阅状态".to_string())?;
    let payload = request_json(
        client
            .get(SUBSCRIPTIONS_URL)
            .query(&[("account_id", remote_account_id.as_str())]),
        account,
        "/backend-api/subscriptions",
        "订阅信息",
    )
    .await?;
    let subscriptions = parse_subscription_snapshot(&payload, &remote_account_id);
    if subscriptions.plan_type.is_some() {
        snapshot.plan_type = subscriptions.plan_type;
    }
    if subscriptions.active_until.is_some() {
        snapshot.active_until = subscriptions.active_until;
    }
    snapshot.account_id = Some(remote_account_id);
    Ok(snapshot)
}

async fn request_json(
    request: reqwest::RequestBuilder,
    account: &CodexAccount,
    target_path: &str,
    label: &str,
) -> Result<Value, String> {
    let request = request
        .bearer_auth(account.tokens.access_token.trim())
        .header("Accept", "application/json")
        .header("Referer", CHATGPT_REFERER)
        .header("User-Agent", CHATGPT_USER_AGENT)
        .header("x-openai-target-path", target_path)
        .header("x-openai-target-route", target_path);
    let response = request
        .send()
        .await
        .map_err(|error| format!("请求{label}失败: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取{label}响应失败: {error}"))?;
    if !status.is_success() {
        let error_code = extract_error_code(&body).unwrap_or_else(|| "unknown".to_string());
        return Err(format!(
            "{label}接口返回 {status}: error_code={error_code}, body_len={}",
            body.len()
        ));
    }
    serde_json::from_str(&body).map_err(|error| format!("解析{label} JSON 失败: {error}"))
}

fn parse_account_check_snapshot(
    payload: &Value,
    account: &CodexAccount,
) -> Result<SubscriptionSnapshot, String> {
    let records = collect_account_records(payload);
    if records.is_empty() {
        return Err("accounts/check 返回里没有可用账号".to_string());
    }
    let preferred_account_id = chatgpt_account_id(account);
    let ordering_first_key = payload
        .get("account_ordering")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .and_then(normalize_string);
    let selected = records
        .iter()
        .find(|item| {
            item.node.as_object().is_some_and(|record| {
                let account_record = nested_account_record(record);
                extract_record_field(
                    account_record,
                    &["account_id", "id", "chatgpt_account_id", "workspace_id"],
                ) == preferred_account_id
            })
        })
        .or_else(|| {
            records
                .iter()
                .find(|item| item.key.as_deref().and_then(normalize_string) == ordering_first_key)
        })
        .unwrap_or(&records[0]);
    let record = selected
        .node
        .as_object()
        .ok_or_else(|| "accounts/check 账号记录格式不正确".to_string())?;
    let account_record = nested_account_record(record);
    let entitlement = record.get("entitlement").and_then(Value::as_object);
    Ok(SubscriptionSnapshot {
        account_id: extract_record_field(
            account_record,
            &["account_id", "id", "chatgpt_account_id", "workspace_id"],
        ),
        plan_type: entitlement
            .and_then(|value| extract_record_field(value, &["subscription_plan"]))
            .or_else(|| extract_record_field(account_record, &["plan_type", "planType"])),
        active_until: entitlement
            .and_then(|value| extract_record_field(value, &["expires_at"]))
            .or_else(|| extract_record_field(account_record, &["expires_at"])),
    })
}

fn collect_account_records(payload: &Value) -> Vec<AccountRecord> {
    let mut records = Vec::new();
    match payload.get("accounts") {
        Some(Value::Array(items)) => {
            records.extend(items.iter().filter(|item| item.is_object()).map(|item| {
                AccountRecord {
                    key: None,
                    node: item.clone(),
                }
            }));
        }
        Some(Value::Object(items)) => {
            records.extend(
                items
                    .iter()
                    .filter(|(_, item)| item.is_object())
                    .map(|(key, item)| AccountRecord {
                        key: Some(key.clone()),
                        node: item.clone(),
                    }),
            );
        }
        _ => {}
    }
    if records.is_empty() {
        if let Value::Array(items) = payload {
            records.extend(items.iter().filter(|item| item.is_object()).map(|item| {
                AccountRecord {
                    key: None,
                    node: item.clone(),
                }
            }));
        }
    }
    records
}

fn nested_account_record(record: &Map<String, Value>) -> &Map<String, Value> {
    record
        .get("account")
        .and_then(Value::as_object)
        .unwrap_or(record)
}

fn extract_record_field(record: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| record.get(*key).and_then(normalize_scalar))
}

fn parse_subscription_snapshot(payload: &Value, fallback_account_id: &str) -> SubscriptionSnapshot {
    SubscriptionSnapshot {
        account_id: normalize_string(fallback_account_id),
        plan_type: payload
            .get("subscription_plan")
            .or_else(|| payload.get("plan_type"))
            .and_then(normalize_scalar),
        active_until: payload
            .get("active_until")
            .or_else(|| payload.get("expires_at"))
            .and_then(normalize_scalar),
    }
}

fn normalize_scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => normalize_string(value),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn normalize_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn subscription_missing_or_expired(value: Option<&str>) -> bool {
    value
        .and_then(parse_timestamp_seconds)
        .map(|expires_at| expires_at <= now_timestamp())
        .unwrap_or(true)
}

fn parse_timestamp_seconds(value: &str) -> Option<i64> {
    let value = value.trim();
    if let Ok(mut timestamp) = value.parse::<i64>() {
        if timestamp > 1_000_000_000_000 {
            timestamp /= 1000;
        }
        return Some(timestamp);
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.timestamp())
}

fn extract_error_code(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    value
        .pointer("/detail/code")
        .or_else(|| value.pointer("/error/code"))
        .or_else(|| value.get("code"))
        .and_then(normalize_scalar)
}

fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::{parse_account_check_snapshot, parse_subscription_snapshot, SubscriptionSnapshot};
    use crate::account::{CodexAccount, CodexTokens};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use serde_json::json;

    fn account() -> CodexAccount {
        let payload = URL_SAFE_NO_PAD
            .encode(r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"account-b"}}"#);
        CodexAccount {
            id: "local-account".to_string(),
            email: "owner@example.com".to_string(),
            account_name: None,
            is_hidden: false,
            tags: Vec::new(),
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
            access_token_expires_at: None,
            token_updated_at: None,
            subscription_query_last_attempt_at: None,
            subscription_query_next_retry_at: None,
            subscription_query_last_error: None,
            quota: None,
            quota_error: None,
            usage_updated_at: None,
            tokens: CodexTokens {
                id_token: String::new(),
                access_token: format!("header.{payload}.signature"),
                refresh_token: Some("refresh-token".to_string()),
            },
            created_at: 1,
            last_used: 1,
        }
    }

    #[test]
    fn selects_matching_account_and_reads_entitlement() {
        let payload = json!({
            "account_ordering": ["account-a", "account-b"],
            "accounts": {
                "account-a": {
                    "account": {"account_id": "account-a", "plan_type": "free"},
                    "entitlement": {"expires_at": "2026-08-01T00:00:00Z"}
                },
                "account-b": {
                    "account": {"account_id": "account-b"},
                    "entitlement": {
                        "subscription_plan": "pro",
                        "expires_at": "2026-09-01T00:00:00Z"
                    }
                }
            }
        });

        assert_eq!(
            parse_account_check_snapshot(&payload, &account()).unwrap(),
            SubscriptionSnapshot {
                account_id: Some("account-b".to_string()),
                plan_type: Some("pro".to_string()),
                active_until: Some("2026-09-01T00:00:00Z".to_string()),
            }
        );
    }

    #[test]
    fn subscriptions_endpoint_supports_alternate_field_names() {
        let payload = json!({
            "plan_type": "team",
            "expires_at": 1_800_000_000
        });
        assert_eq!(
            parse_subscription_snapshot(&payload, "account-a"),
            SubscriptionSnapshot {
                account_id: Some("account-a".to_string()),
                plan_type: Some("team".to_string()),
                active_until: Some("1800000000".to_string()),
            }
        );
    }
}
