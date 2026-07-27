use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use url::Url;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTH_ENDPOINT: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_ENDPOINT: &str = "https://auth.openai.com/oauth/token";
const SCOPES: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
// OpenAI Codex OAuth app currently only accepts the original localhost callback.
// Changing this to another port (for example 16666) causes the authorize page to fail.
pub const CALLBACK_PORT: u16 = 1455;
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const ORIGINATOR: &str = "codex_vscode";
const TOKEN_REFRESH_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexOAuthLoginStartResponse {
    pub login_id: String,
    pub auth_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    #[serde(rename = "id_token", alias = "idToken")]
    pub id_token: String,
    #[serde(rename = "access_token", alias = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refresh_token", alias = "refreshToken")]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone)]
struct OAuthState {
    state: String,
    code_verifier: String,
    code: Option<String>,
}

static OAUTH_STATE: OnceLock<Mutex<HashMap<String, OAuthState>>> = OnceLock::new();

pub fn start_oauth_login() -> Result<CodexOAuthLoginStartResponse, String> {
    let login_id = generate_base64url_token(18);
    let state = generate_base64url_token(18);
    let code_verifier = generate_base64url_token(32);
    let code_challenge = generate_code_challenge(&code_verifier);
    let auth_url = build_auth_url(&state, &code_challenge)?;
    let state_data = OAuthState {
        state,
        code_verifier,
        code: None,
    };
    oauth_state()
        .lock()
        .map_err(|_| "OAuth 状态锁定失败".to_string())?
        .insert(login_id.clone(), state_data);
    Ok(CodexOAuthLoginStartResponse { login_id, auth_url })
}

pub fn submit_callback_url(login_id: &str, callback_url: &str) -> Result<(), String> {
    let url = parse_callback_url(callback_url)?;
    let query = parse_query_params(url.query().unwrap_or_default());
    let code = query
        .get("code")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "回调地址中没有 code".to_string())?;
    let state = query
        .get("state")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "回调地址中没有 state".to_string())?;
    let mut guard = oauth_state()
        .lock()
        .map_err(|_| "OAuth 状态锁定失败".to_string())?;
    let current = guard
        .get_mut(login_id)
        .ok_or_else(|| "OAuth 授权流程不存在或已过期".to_string())?;
    if current.state != state {
        return Err("OAuth state 校验失败".to_string());
    }
    current.code = Some(code);
    Ok(())
}

pub async fn complete_oauth_login(login_id: &str) -> Result<OAuthTokenResponse, String> {
    let (code, code_verifier) = {
        let guard = oauth_state()
            .lock()
            .map_err(|_| "OAuth 状态锁定失败".to_string())?;
        let current = guard
            .get(login_id)
            .ok_or_else(|| "OAuth 授权流程不存在或已过期".to_string())?;
        let code = current
            .code
            .clone()
            .ok_or_else(|| "尚未收到 OAuth 回调 code".to_string())?;
        (code, current.code_verifier.clone())
    };
    let tokens = exchange_code_for_tokens(&code, &code_verifier).await?;
    oauth_state()
        .lock()
        .map_err(|_| "OAuth 状态锁定失败".to_string())?
        .remove(login_id);
    Ok(tokens)
}

pub async fn refresh_access_token_with_fallback(
    refresh_token: &str,
    current_id_token: Option<&str>,
) -> Result<OAuthTokenResponse, String> {
    let refresh_token = refresh_token.trim();
    if refresh_token.is_empty() {
        return Err("缺少 refresh_token，无法自动续期".to_string());
    }
    let client = reqwest::Client::builder()
        .connect_timeout(TOKEN_REFRESH_TIMEOUT)
        .timeout(TOKEN_REFRESH_TIMEOUT)
        .build()
        .map_err(|error| format!("创建 Token 刷新客户端失败: {error}"))?;
    let response = client
        .post(TOKEN_ENDPOINT)
        .json(&serde_json::json!({
            "client_id": CLIENT_ID,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }))
        .send()
        .await
        .map_err(|error| format!("Token 刷新请求失败: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取 Token 刷新响应失败: {error}"))?;
    if !status.is_success() {
        let error_code = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|error| {
                        error
                            .as_str()
                            .map(str::to_string)
                            .or_else(|| error.get("code")?.as_str().map(str::to_string))
                            .or_else(|| error.get("type")?.as_str().map(str::to_string))
                    })
                    .or_else(|| value.get("code")?.as_str().map(str::to_string))
            });
        return Err(format!(
            "Token 刷新失败: status={}, error_code={}, body_len={}",
            status,
            error_code.unwrap_or_else(|| "unknown".to_string()),
            body.len()
        ));
    }
    parse_refresh_token_response(&body, refresh_token, current_id_token)
}

pub fn cancel_oauth_login(login_id: Option<&str>) -> Result<(), String> {
    let mut guard = oauth_state()
        .lock()
        .map_err(|_| "OAuth 状态锁定失败".to_string())?;
    if let Some(expected) = login_id {
        guard.remove(expected);
    } else {
        guard.clear();
    }
    Ok(())
}

pub fn is_login_active(login_id: &str) -> bool {
    oauth_state()
        .lock()
        .ok()
        .map(|guard| guard.contains_key(login_id))
        .unwrap_or(false)
}

pub fn redirect_uri() -> &'static str {
    REDIRECT_URI
}

fn oauth_state() -> &'static Mutex<HashMap<String, OAuthState>> {
    OAUTH_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn build_auth_url(state: &str, code_challenge: &str) -> Result<String, String> {
    let mut url =
        Url::parse(AUTH_ENDPOINT).map_err(|error| format!("OAuth 授权地址无效: {}", error))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPES)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("prompt", "login")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("state", state)
        .append_pair("originator", ORIGINATOR);
    Ok(url.to_string())
}

fn parse_callback_url(callback_url: &str) -> Result<Url, String> {
    let trimmed = callback_url.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Url::parse(trimmed).map_err(|error| format!("回调地址格式无效: {}", error));
    }
    Url::parse(&format!(
        "{}?{}",
        REDIRECT_URI,
        trimmed.trim_start_matches('?')
    ))
    .map_err(|error| format!("回调地址格式无效: {}", error))
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.trim();
            if key.is_empty() {
                return None;
            }
            let raw_value = parts.next().unwrap_or("");
            Some((
                key.to_string(),
                urlencoding::decode(raw_value)
                    .map(|value| value.into_owned())
                    .unwrap_or_else(|_| raw_value.to_string()),
            ))
        })
        .collect()
}

async fn exchange_code_for_tokens(
    code: &str,
    code_verifier: &str,
) -> Result<OAuthTokenResponse, String> {
    let client = reqwest::Client::new();
    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CLIENT_ID),
            ("code", code),
            ("code_verifier", code_verifier),
            ("redirect_uri", REDIRECT_URI),
        ])
        .send()
        .await
        .map_err(|error| format!("OAuth token 请求失败: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取 OAuth token 响应失败: {}", error))?;
    if !status.is_success() {
        return Err(format!(
            "OAuth token 交换失败: status={}, body={}",
            status, body
        ));
    }
    serde_json::from_str(&body).map_err(|error| format!("解析 OAuth token 响应失败: {}", error))
}

fn generate_base64url_token(byte_len: usize) -> String {
    let mut bytes = vec![0_u8; byte_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_code_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn parse_refresh_token_response(
    body: &str,
    current_refresh_token: &str,
    current_id_token: Option<&str>,
) -> Result<OAuthTokenResponse, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|error| format!("解析 Token 刷新响应失败: {error}"))?;
    let access_token = value
        .get("access_token")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Token 刷新响应缺少 access_token".to_string())?
        .to_string();
    let id_token = value
        .get("id_token")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            current_id_token
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .ok_or_else(|| "Token 刷新响应缺少 id_token，且本地没有可复用值".to_string())?;
    let refresh_token = value
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Some(current_refresh_token.to_string()));
    Ok(OAuthTokenResponse {
        id_token,
        access_token,
        refresh_token,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        parse_refresh_token_response, start_oauth_login, submit_callback_url, OAuthTokenResponse,
    };

    #[test]
    fn oauth_start_builds_openai_pkce_authorize_url() {
        let response = start_oauth_login().expect("start oauth");

        assert!(!response.login_id.is_empty());
        assert!(response
            .auth_url
            .starts_with("https://auth.openai.com/oauth/authorize?"));
        assert!(response
            .auth_url
            .contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
        assert!(response
            .auth_url
            .contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"));
        assert!(response.auth_url.contains("code_challenge_method=S256"));
        assert!(response.auth_url.contains("prompt=login"));
        assert!(response.auth_url.contains("state="));
    }

    #[test]
    fn callback_url_requires_matching_state() {
        let response = start_oauth_login().expect("start oauth");

        let error = submit_callback_url(&response.login_id, "code=abc&state=wrong")
            .expect_err("state should fail");

        assert!(error.contains("state"));
    }

    #[test]
    fn token_response_accepts_openai_snake_case_fields() {
        let tokens: OAuthTokenResponse = serde_json::from_str(
            r#"{
              "id_token": "id-token",
              "access_token": "access-token",
              "refresh_token": "refresh-token",
              "token_type": "Bearer",
              "expires_in": 3600
            }"#,
        )
        .expect("parse token response");

        assert_eq!(tokens.id_token, "id-token");
        assert_eq!(tokens.access_token, "access-token");
        assert_eq!(tokens.refresh_token.as_deref(), Some("refresh-token"));
    }

    #[test]
    fn refresh_response_keeps_rotating_credentials_and_falls_back_to_existing_id_token() {
        let tokens = parse_refresh_token_response(
            r#"{"access_token":"new-access","refresh_token":"new-refresh"}"#,
            "old-refresh",
            Some("old-id"),
        )
        .expect("parse refresh response");

        assert_eq!(tokens.id_token, "old-id");
        assert_eq!(tokens.access_token, "new-access");
        assert_eq!(tokens.refresh_token.as_deref(), Some("new-refresh"));
    }

    #[test]
    fn refresh_response_keeps_old_refresh_token_when_server_omits_rotation() {
        let tokens = parse_refresh_token_response(
            r#"{"id_token":"new-id","access_token":"new-access"}"#,
            "old-refresh",
            None,
        )
        .expect("parse refresh response");

        assert_eq!(tokens.refresh_token.as_deref(), Some("old-refresh"));
    }
}
