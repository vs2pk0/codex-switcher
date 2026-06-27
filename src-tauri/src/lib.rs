mod account;
mod oauth;
mod session;

use account::{AccountStore, CodexAccount, CodexQuota};
use oauth::CodexOAuthLoginStartResponse;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use session::{
    CodexSessionRecord, CodexSessionTokenStats, CodexSessionTrashSummary,
    CodexSessionVisibilityRepairInstanceList, CodexSessionVisibilityRepairProviderList,
    CodexSessionVisibilityRepairSummary, CodexTrashedSessionRecord, SessionStore,
};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexOAuthCallbackEvent {
    login_id: String,
    ok: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexSwitcherSettings {
    monitor_quota: bool,
    quota_refresh_minutes: u64,
    current_account_refresh_minutes: u64,
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
    #[serde(default = "default_show_quota_countdowns")]
    show_quota_countdowns: bool,
    #[serde(default = "default_badge_style")]
    badge_style: String,
    #[serde(default = "default_badge_styles")]
    badge_styles: HashMap<String, String>,
    #[serde(default = "default_max_columns")]
    max_columns: u64,
}

impl Default for CodexSwitcherSettings {
    fn default() -> Self {
        Self {
            monitor_quota: false,
            quota_refresh_minutes: 10,
            current_account_refresh_minutes: 10,
            sort_mode: "created_at".to_string(),
            sort_direction: default_sort_direction(),
            custom_order: Vec::new(),
            pinned_account_ids: Vec::new(),
            account_type_filter: default_account_type_filter(),
            page_size: default_page_size(),
            show_quota_countdowns: default_show_quota_countdowns(),
            badge_style: default_badge_style(),
            badge_styles: default_badge_styles(),
            max_columns: default_max_columns(),
        }
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

fn default_show_quota_countdowns() -> bool {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexSwitcherPaths {
    app_dir: String,
    accounts_json: String,
    settings_json: String,
    backup_dir: String,
    account_dir: String,
    session_dir: String,
    data_dir: String,
    codex_home: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexSwitcherBackupFile {
    name: String,
    path: String,
    created_at: String,
    size_bytes: u64,
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
    AccountStore::default().add_api_key_account_with_binding(
        api_key,
        api_base_url,
        api_provider_name,
        api_official_url,
        account_name,
        bound_oauth_account_id,
        bound_oauth_use_local_gateway.unwrap_or(false),
    )
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
    let status = open_path_with_system(&target)?;
    if status.success() {
        Ok(())
    } else {
        Err("打开文件夹失败".to_string())
    }
}

fn start_oauth_callback_listener(app_handle: AppHandle, login_id: String) -> Result<(), String> {
    let listener = TcpListener::bind("127.0.0.1:1455")
        .map_err(|error| format!("OAuth 回调端口 1455 启动失败: {}", error))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("OAuth 回调监听配置失败: {}", error))?;

    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10 * 60);
        while Instant::now() < deadline && oauth::is_login_active(&login_id) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0_u8; 4096];
                    let size = stream.read(&mut buffer).unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..size]);
                    let callback_result = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .ok_or_else(|| "OAuth 回调请求格式无效".to_string())
                        .and_then(|path| {
                            oauth::submit_callback_url(
                                &login_id,
                                &format!(
                                    "{}{}",
                                    oauth::redirect_uri().trim_end_matches("/auth/callback"),
                                    path
                                ),
                            )
                        });
                    let (ok, message, html) = match callback_result {
                        Ok(()) => (
                            true,
                            "已收到 OAuth 回调，正在保存账号".to_string(),
                            "<html><body><h2>Codex OAuth 授权完成</h2><p>可以回到 Codex Switcher 了。</p></body></html>",
                        ),
                        Err(error) => (
                            false,
                            error,
                            "<html><body><h2>Codex OAuth 授权失败</h2><p>请回到应用重试或手动粘贴回调地址。</p></body></html>",
                        ),
                    };
                    let body = html.as_bytes();
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(body);
                    let _ = app_handle.emit(
                        "codex-oauth-callback-received",
                        CodexOAuthCallbackEvent {
                            login_id: login_id.clone(),
                            ok,
                            message,
                        },
                    );
                    break;
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

fn open_path_with_system(path: &PathBuf) -> Result<std::process::ExitStatus, String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .status()
            .map_err(|error| format!("打开文件夹失败: {}", error))
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .status()
            .map_err(|error| format!("打开文件夹失败: {}", error))
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .status()
            .map_err(|error| format!("打开文件夹失败: {}", error))
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
        Command::new("cmd")
            .args(["/C", "start", "", url])
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

#[tauri::command]
fn delete_codex_account(account_id: String) -> Result<(), String> {
    AccountStore::default().delete_account(&account_id)
}

#[tauri::command]
fn switch_codex_account(account_id: String) -> Result<CodexAccount, String> {
    AccountStore::default().switch_account(&account_id)
}

#[tauri::command]
fn restart_codex_app() -> Result<String, String> {
    AccountStore::default().restart_codex_app()
}

#[tauri::command]
fn get_codex_switcher_settings() -> Result<CodexSwitcherSettings, String> {
    read_switcher_settings()
}

#[tauri::command]
fn update_codex_switcher_settings(
    settings: CodexSwitcherSettings,
) -> Result<CodexSwitcherSettings, String> {
    write_switcher_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
fn reset_codex_config_toml() -> Result<bool, String> {
    let path = default_codex_home().join("config.toml");
    if !path.exists() {
        return Ok(false);
    }
    std::fs::remove_file(&path).map_err(|error| {
        format!(
            "删除 Codex config.toml 失败 ({}): {}",
            path.display(),
            error
        )
    })?;
    Ok(true)
}

#[tauri::command]
fn get_codex_switcher_paths() -> Result<CodexSwitcherPaths, String> {
    ensure_switcher_data_dirs()?;
    let app_dir = switcher_data_dir();
    let account_dir = switcher_account_dir();
    let session_dir = switcher_session_dir();
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
        data_dir: data_dir.to_string_lossy().to_string(),
        codex_home: default_codex_home().to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn export_codex_switcher_backup() -> Result<CodexSwitcherBackupFile, String> {
    let backup = serde_json::json!({
        "app": "Codex Switcher",
        "version": 1,
        "exportedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "accounts": list_codex_accounts()?,
        "currentAccount": get_current_codex_account()?,
        "settings": read_switcher_settings()?,
    });
    let content = serde_json::to_string_pretty(&backup)
        .map_err(|error| format!("序列化备份失败: {}", error))?;
    let backup_dir = switcher_backup_dir();
    std::fs::create_dir_all(&backup_dir).map_err(|error| format!("创建备份目录失败: {}", error))?;
    let filename = format!(
        "codex-switcher-backup-{}.zip",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let backup_path = backup_dir.join(filename);
    write_switcher_backup_zip(&backup_path, &content)?;
    backup_file_info(&backup_path)
}

#[tauri::command]
fn list_codex_switcher_backups() -> Result<Vec<CodexSwitcherBackupFile>, String> {
    let backup_dir = switcher_backup_dir();
    std::fs::create_dir_all(&backup_dir).map_err(|error| format!("创建备份目录失败: {}", error))?;
    let mut backups = Vec::new();
    for entry in
        std::fs::read_dir(&backup_dir).map_err(|error| format!("读取备份目录失败: {}", error))?
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
fn restore_codex_switcher_backup(backup_path: String) -> Result<Vec<CodexAccount>, String> {
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
    import_codex_switcher_backup(backup_json)
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
        if fetch_codex_quota_for_account(&account.id)
            .await
            .and_then(|quota| {
                AccountStore::default()
                    .update_account_quota(&account.id, quota)
                    .map(|_| ())
            })
            .is_ok()
        {
            count += 1;
        }
    }
    Ok(count)
}

#[tauri::command]
async fn consume_codex_reset_credit(account_id: String) -> Result<CodexAccount, String> {
    let source = quota_source_account(&account_id)?;
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
    dirs::home_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join(".codex_switcher")
}

fn switcher_account_dir() -> PathBuf {
    switcher_data_dir().join("account")
}

fn switcher_session_dir() -> PathBuf {
    switcher_data_dir().join("session")
}

fn switcher_config_data_dir() -> PathBuf {
    switcher_data_dir().join("data")
}

fn switcher_backup_dir() -> PathBuf {
    switcher_data_dir().join("backup")
}

fn ensure_switcher_data_dirs() -> Result<(), String> {
    for dir in [
        switcher_data_dir(),
        switcher_account_dir(),
        switcher_session_dir(),
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

fn write_switcher_backup_zip(backup_path: &Path, backup_json: &str) -> Result<(), String> {
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
    let root = switcher_data_dir();
    add_directory_to_backup_zip(&mut zip, &root, &root, &switcher_backup_dir(), options)?;
    zip.finish()
        .map_err(|error| format!("完成 ZIP 备份失败: {}", error))?;
    Ok(())
}

fn add_directory_to_backup_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    current: &Path,
    excluded_dir: &Path,
    options: zip::write::FileOptions,
) -> Result<(), String> {
    if !current.exists() || current.starts_with(excluded_dir) {
        return Ok(());
    }
    for entry in std::fs::read_dir(current)
        .map_err(|error| format!("读取备份数据目录失败 ({}): {}", current.display(), error))?
    {
        let path = entry
            .map_err(|error| format!("读取备份数据文件失败: {}", error))?
            .path();
        if path.starts_with(excluded_dir) {
            continue;
        }
        if path.is_dir() {
            add_directory_to_backup_zip(zip, root, &path, excluded_dir, options)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| format!("计算备份相对路径失败: {}", error))?;
        let name = format!("data/{}", relative.to_string_lossy().replace('\\', "/"));
        zip.start_file(name, options)
            .map_err(|error| format!("写入备份文件失败: {}", error))?;
        let mut file =
            std::fs::File::open(&path).map_err(|error| format!("读取备份文件失败: {}", error))?;
        std::io::copy(&mut file, zip).map_err(|error| format!("写入备份文件失败: {}", error))?;
    }
    Ok(())
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
    let backup_dir = switcher_backup_dir()
        .canonicalize()
        .map_err(|error| format!("读取备份目录失败: {}", error))?;
    let backup_path = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| format!("读取备份文件失败: {}", error))?;
    if !backup_path.starts_with(&backup_dir)
        || backup_path.extension().and_then(|value| value.to_str()) != Some("zip")
    {
        return Err("只能操作备份目录内的 ZIP 文件".to_string());
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
    serde_json::from_str(&content).map_err(|error| format!("解析设置失败: {}", error))
}

fn write_switcher_settings(settings: &CodexSwitcherSettings) -> Result<(), String> {
    std::fs::create_dir_all(switcher_config_data_dir())
        .map_err(|error| format!("创建设置目录失败: {}", error))?;
    let content = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("序列化设置失败: {}", error))?;
    std::fs::write(switcher_settings_path(), content)
        .map_err(|error| format!("写入设置失败: {}", error))
}

async fn fetch_codex_quota_for_account(account_id: &str) -> Result<CodexQuota, String> {
    let source = quota_source_account(account_id)?;
    let access_token = source.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err("OAuth access_token 为空，无法查询额度".to_string());
    }
    let client = reqwest::Client::new();
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
    Ok(parse_codex_quota(raw))
}

fn quota_source_account(account_id: &str) -> Result<CodexAccount, String> {
    let accounts = AccountStore::default().list_accounts()?;
    let account = accounts
        .iter()
        .find(|item| item.id == account_id)
        .cloned()
        .ok_or_else(|| "账号不存在".to_string())?;
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
        account
    };
    Ok(source)
}

fn parse_codex_quota(raw: Value) -> CodexQuota {
    let rate_limit = raw.get("rate_limit");
    let primary = rate_limit.and_then(|value| value.get("primary_window"));
    let secondary = rate_limit.and_then(|value| value.get("secondary_window"));
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
        reset_credits_available: raw
            .get("rate_limit_reset_credits")
            .and_then(|value| value.get("available_count"))
            .and_then(Value::as_i64),
        raw_data: Some(raw),
    }
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

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_codex_accounts,
            get_current_codex_account,
            import_codex_from_json,
            import_codex_from_local,
            add_codex_account_with_api_key,
            update_codex_api_key_credentials,
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
            reset_codex_config_toml,
            get_codex_switcher_paths,
            export_codex_switcher_backup,
            list_codex_switcher_backups,
            restore_codex_switcher_backup,
            delete_codex_switcher_backup,
            import_codex_switcher_backup,
            refresh_codex_quota,
            refresh_all_codex_quotas,
            consume_codex_reset_credit,
            codex_list_sessions_across_instances,
            codex_get_session_token_stats_across_instances,
            codex_move_sessions_to_trash_across_instances,
            codex_list_trashed_sessions_across_instances,
            codex_restore_sessions_from_trash_across_instances,
            codex_repair_session_visibility_across_instances,
            codex_list_session_visibility_repair_instances,
            codex_list_session_visibility_repair_providers,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
