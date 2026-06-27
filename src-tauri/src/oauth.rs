use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use url::Url;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTH_ENDPOINT: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_ENDPOINT: &str = "https://auth.openai.com/oauth/token";
const SCOPES: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const ORIGINATOR: &str = "codex_vscode";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexOAuthLoginStartResponse {
    pub login_id: String,
    pub auth_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthTokenResponse {
    pub id_token: String,
    pub access_token: String,
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

#[cfg(test)]
mod tests {
    use super::{start_oauth_login, submit_callback_url};

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
        assert!(response.auth_url.contains("code_challenge_method=S256"));
        assert!(response.auth_url.contains("state="));
    }

    #[test]
    fn callback_url_requires_matching_state() {
        let response = start_oauth_login().expect("start oauth");

        let error = submit_callback_url(&response.login_id, "code=abc&state=wrong")
            .expect_err("state should fail");

        assert!(error.contains("state"));
    }
}
