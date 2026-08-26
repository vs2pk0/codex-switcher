use crate::{
    account::{chatgpt_account_id, write_bytes_atomic, AccountStore, CodexAccount},
    fetch_codex_api_key_models_for_account,
};
use flate2::read::GzDecoder;
use rand::{distributions::Alphanumeric, Rng};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering as CmpOrdering,
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, MutexGuard, TryLockError,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tar::Archive;
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const BINARY_BASENAME: &str = "cli-proxy-api";
const DEFAULT_CONFIG_CACHE: &str = "default-config.yaml";
const DEFAULT_PORT: u16 = 17877;
const CLIPROXYAPI_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/router-for-me/CLIProxyAPI/releases/latest";
const GITHUB_USER_AGENT: &str = "Codex-Switcher-CPA-Service";
const DOWNLOAD_PROGRESS_EVENT: &str = "codex-switcher-api-service-download-progress";
const AUTO_UPDATE_EVENT: &str = "codex-switcher-api-service-auto-update";
const AUTO_UPDATE_POLL_SECONDS: u64 = 15;
const MAX_CHECKSUM_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_RUNTIME_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_EXTRACTED_RUNTIME_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const MAX_OLD_RUNTIMES: usize = 2;
const API_KEY_BINDINGS_FILE: &str = "api-key-bindings.json";

#[derive(Default)]
pub struct ApiServiceProcessState(pub Mutex<Option<Child>>);

#[derive(Default)]
pub struct ApiServiceDownloadState(pub Mutex<DownloadControl>);

pub struct ApiServiceOperationState {
    lock: Mutex<()>,
    shutting_down: AtomicBool,
}

impl Default for ApiServiceOperationState {
    fn default() -> Self {
        Self {
            lock: Mutex::new(()),
            shutting_down: AtomicBool::new(false),
        }
    }
}

#[derive(Default)]
pub struct DownloadControl {
    active_id: Option<String>,
    cancel_requested: bool,
}

#[derive(Debug, Clone)]
struct PackageInfo {
    id: String,
    version: String,
    target: String,
}

#[derive(Debug)]
struct LocalPackageSnapshot {
    path: PathBuf,
    directory: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeMetadata {
    id: String,
    version: String,
    target: String,
    installed_at: u64,
    package_file: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    id: String,
    version: String,
    target: String,
    compatible: bool,
    path: String,
    binary_path: String,
    installed_at: u64,
    package_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiServiceSettings {
    enabled: bool,
    port: u16,
    management_key: String,
    #[serde(default = "default_api_keys")]
    api_keys: Vec<String>,
    auto_update: bool,
    auto_update_interval_hours: u64,
    last_update_check_at: Option<u64>,
}

impl Default for ApiServiceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            management_key: generate_management_key(),
            api_keys: default_api_keys(),
            auto_update: false,
            auto_update_interval_hours: 24,
            last_update_check_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiServiceInfo {
    running: bool,
    pid: Option<u32>,
    port: u16,
    management_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiServiceState {
    base_dir: String,
    runtime_dir: String,
    workspace_dir: String,
    downloads_dir: String,
    auth_dir: String,
    settings: ApiServiceSettings,
    active_version: Option<String>,
    runtimes: Vec<RuntimeInfo>,
    service: ApiServiceInfo,
    config_path: String,
    installed: bool,
    maintenance_old_runtime_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current_version: Option<String>,
    latest_version: String,
    target: String,
    release_url: String,
    download_url: Option<String>,
    asset_name: Option<String>,
    has_update: bool,
    can_apply: bool,
    latest_installed: bool,
    latest_active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutoUpdateEvent {
    status: String,
    update_info: Option<UpdateInfo>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressEvent {
    status: String,
    asset_name: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiServiceAccountSyncSummary {
    count: usize,
    auth_dir: String,
    oauth_count: usize,
    api_key_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiServiceBoundAccount {
    id: String,
    account_id: Option<String>,
    account_id_exact: bool,
    kind: String,
    label: String,
    email: Option<String>,
    base_url: Option<String>,
    path: String,
    modified_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ManagedApiKeyBindings {
    #[serde(default)]
    bindings: Vec<ManagedApiKeyBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedApiKeyBinding {
    account_id: String,
    api_key_hash: String,
    base_url: String,
    label: String,
    #[serde(default = "default_true")]
    owns_config_entry: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
struct CodexApiKeyConfigEntry {
    api_key: String,
    base_url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    models: Vec<CodexApiKeyConfigModel>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CodexApiKeyConfigModel {
    name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    alias: String,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_yaml::Value>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[tauri::command]
pub async fn api_service_state(app: AppHandle) -> Result<ApiServiceState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let process = app.state::<ApiServiceProcessState>();
        let operation = app.state::<ApiServiceOperationState>();
        let _guard = lock_operation(&operation)?;
        build_state(&process)
    })
    .await
    .map_err(|error| format!("读取 API 服务状态任务失败: {error}"))?
}

#[tauri::command]
pub fn api_service_update_settings(
    process: State<'_, ApiServiceProcessState>,
    operation: State<'_, ApiServiceOperationState>,
    port: u16,
    management_key: String,
    api_keys: Vec<String>,
    auto_update: bool,
    auto_update_interval_hours: u64,
) -> Result<ApiServiceState, String> {
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    if service_pid_for_state(&process)?.is_some() {
        return Err("请先停止 API 服务，再修改端口或密钥".to_string());
    }
    if port == 0 {
        return Err("端口必须在 1-65535 之间".to_string());
    }
    let key = management_key.trim();
    if key.is_empty() || key.contains('\r') || key.contains('\n') {
        return Err("管理密钥不能为空且不能包含换行".to_string());
    }
    let api_keys = normalize_api_keys(api_keys)?;
    let dirs = ApiServiceDirs::new()?;
    let mut settings = read_settings(&dirs)?;
    settings.port = port;
    settings.management_key = key.to_string();
    settings.api_keys = api_keys;
    settings.auto_update = auto_update;
    settings.auto_update_interval_hours = auto_update_interval_hours.clamp(1, 24 * 30);
    write_settings(&dirs, &settings)?;
    if let Ok(runtime) = active_runtime(&dirs) {
        write_runtime_config(&dirs, &runtime, &settings)?;
    }
    build_state(&process)
}

#[tauri::command]
pub fn api_service_start(
    app: AppHandle,
    process: State<'_, ApiServiceProcessState>,
    download: State<'_, ApiServiceDownloadState>,
    operation: State<'_, ApiServiceOperationState>,
) -> Result<ApiServiceState, String> {
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    start_service_impl(&app, &process, &download, &operation)
}

#[tauri::command]
pub fn api_service_stop(
    process: State<'_, ApiServiceProcessState>,
    operation: State<'_, ApiServiceOperationState>,
) -> Result<ApiServiceState, String> {
    let _guard = lock_operation(&operation)?;
    stop_service_impl(&process)?;
    build_state(&process)
}

#[tauri::command]
pub fn api_service_reset(
    process: State<'_, ApiServiceProcessState>,
    download: State<'_, ApiServiceDownloadState>,
    operation: State<'_, ApiServiceOperationState>,
) -> Result<ApiServiceState, String> {
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    let active_download = download
        .0
        .lock()
        .map_err(|_| "下载状态锁已损坏".to_string())?
        .active_id
        .is_some();
    if active_download {
        return Err("请先取消当前下载任务，或等待下载完成后再重置".to_string());
    }

    stop_service_impl(&process)?;
    let dirs = ApiServiceDirs::new()?;
    if dirs.base_dir.exists() {
        fs::remove_dir_all(&dirs.base_dir)
            .map_err(|error| format!("删除 API 服务目录失败: {}", error))?;
    }
    build_state(&process)
}

#[tauri::command]
pub fn api_service_check_update(
    operation: State<'_, ApiServiceOperationState>,
) -> Result<UpdateInfo, String> {
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    check_update_impl()
}

fn check_update_impl() -> Result<UpdateInfo, String> {
    let (_, update) = check_update_with_release_impl()?;
    Ok(update)
}

fn check_update_with_release_impl() -> Result<(GitHubRelease, UpdateInfo), String> {
    let dirs = ApiServiceDirs::new()?;
    let release = fetch_latest_release()?;
    let mut settings = read_settings(&dirs)?;
    settings.last_update_check_at = Some(unix_timestamp()?);
    write_settings(&dirs, &settings)?;
    let update = update_info_from_release(&dirs, &release)?;
    Ok((release, update))
}

#[tauri::command]
pub fn api_service_download_update(
    app: AppHandle,
    process: State<'_, ApiServiceProcessState>,
    download: State<'_, ApiServiceDownloadState>,
    operation: State<'_, ApiServiceOperationState>,
) -> Result<ApiServiceState, String> {
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    download_update_impl(&app, &process, &download, &operation)
}

#[tauri::command]
pub fn api_service_import_runtime(
    app: AppHandle,
    process: State<'_, ApiServiceProcessState>,
    download: State<'_, ApiServiceDownloadState>,
    operation: State<'_, ApiServiceOperationState>,
    package_path: String,
) -> Result<ApiServiceState, String> {
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    let dirs = ApiServiceDirs::new()?;
    let snapshot = snapshot_local_runtime_package(&dirs, Path::new(&package_path))?;
    let install_result = install_runtime_package(&dirs, &snapshot.path, false);
    if let Err(error) = fs::remove_dir_all(&snapshot.directory) {
        eprintln!(
            "failed to clean imported API service package snapshot {}: {error}",
            display_path(&snapshot.directory)
        );
    }
    install_result?;
    let latest = latest_compatible_runtime(&dirs)?
        .ok_or_else(|| "导入后未找到与当前平台兼容的 API 服务版本".to_string())?;
    switch_active_runtime(&app, &process, &download, &operation, &dirs, &latest.id)
}

#[tauri::command]
pub fn api_service_activate_runtime(
    app: AppHandle,
    process: State<'_, ApiServiceProcessState>,
    download: State<'_, ApiServiceDownloadState>,
    operation: State<'_, ApiServiceOperationState>,
    runtime_id: String,
) -> Result<ApiServiceState, String> {
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    let dirs = ApiServiceDirs::new()?;
    switch_active_runtime(
        &app,
        &process,
        &download,
        &operation,
        &dirs,
        runtime_id.trim(),
    )
}

#[tauri::command]
pub fn api_service_delete_runtime(
    process: State<'_, ApiServiceProcessState>,
    operation: State<'_, ApiServiceOperationState>,
    runtime_id: String,
) -> Result<ApiServiceState, String> {
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    let dirs = ApiServiceDirs::new()?;
    let runtime = installed_runtime_by_id(&dirs, runtime_id.trim())?;
    ensure_runtime_can_be_deleted(&dirs, &runtime)?;
    remove_runtime_directory(&dirs, &runtime)?;
    build_state(&process)
}

fn download_update_impl(
    app: &AppHandle,
    process: &State<'_, ApiServiceProcessState>,
    download: &State<'_, ApiServiceDownloadState>,
    operation: &ApiServiceOperationState,
) -> Result<ApiServiceState, String> {
    ensure_not_shutting_down(operation)?;
    let dirs = ApiServiceDirs::new()?;
    let release = fetch_latest_release()?;
    let available = update_info_from_release(&dirs, &release)?;
    download_update_from_release_impl(app, process, download, operation, &release, &available)
}

fn download_update_from_release_impl(
    app: &AppHandle,
    process: &State<'_, ApiServiceProcessState>,
    download: &State<'_, ApiServiceDownloadState>,
    operation: &ApiServiceOperationState,
    release: &GitHubRelease,
    available: &UpdateInfo,
) -> Result<ApiServiceState, String> {
    let dirs = ApiServiceDirs::new()?;
    ensure_not_shutting_down(operation)?;
    if !available.can_apply {
        return Err(format!(
            "已拒绝安装 {}：当前版本 {} 不低于该版本",
            available.latest_version,
            available.current_version.as_deref().unwrap_or("--")
        ));
    }
    let was_running = service_pid_for_state(process)?.is_some();
    if !available.latest_installed {
        download_runtime_from_release(app, process, download, operation, release, false)?;
    }
    ensure_not_shutting_down(operation)?;
    let latest = latest_compatible_runtime(&dirs)?
        .ok_or_else(|| "更新后未找到与当前平台兼容的 API 服务版本".to_string())?;
    let next = match switch_active_runtime(app, process, download, operation, &dirs, &latest.id) {
        Ok(next) => next,
        Err(message) => {
            let _ = emit_download_progress(app, "failed", "", 0, None, Some(&message));
            return Err(message);
        }
    };
    if was_running {
        emit_download_progress(
            app,
            "done",
            "",
            1,
            Some(1),
            Some("API 服务已更新并重新启动"),
        )?;
    } else {
        emit_download_progress(app, "done", "", 1, Some(1), Some("API 服务更新已安装"))?;
    }
    Ok(next)
}

#[tauri::command]
pub fn api_service_cancel_download(
    download: State<'_, ApiServiceDownloadState>,
) -> Result<(), String> {
    let mut guard = download
        .0
        .lock()
        .map_err(|_| "下载状态锁已损坏".to_string())?;
    if guard.active_id.is_some() {
        guard.cancel_requested = true;
    }
    Ok(())
}

pub fn start_auto_update_scheduler(app: AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(5));
        loop {
            if app
                .state::<ApiServiceOperationState>()
                .shutting_down
                .load(Ordering::SeqCst)
            {
                break;
            }
            if let Err(error) = run_scheduled_auto_update(&app) {
                eprintln!("API service auto update failed: {error}");
            }
            thread::sleep(Duration::from_secs(AUTO_UPDATE_POLL_SECONDS));
        }
    });
}

fn run_scheduled_auto_update(app: &AppHandle) -> Result<(), String> {
    let operation = app.state::<ApiServiceOperationState>();
    let Some(_guard) = try_lock_operation(&operation)? else {
        return Ok(());
    };
    ensure_not_shutting_down(&operation)?;
    let dirs = ApiServiceDirs::new()?;
    let settings = read_settings(&dirs)?;
    let has_runtimes = !list_runtimes(&dirs)?.is_empty();
    if !auto_update_due(&settings, has_runtimes, unix_timestamp()?) {
        return Ok(());
    }

    // Persist the attempt before the network request so an outage does not cause a tight retry loop.
    mark_update_check_attempted()?;
    let (release, update) = match check_update_with_release_impl() {
        Ok(result) => result,
        Err(error) => {
            emit_auto_update_event(app, "failed", None, Some(error.clone()));
            return Err(error);
        }
    };
    ensure_not_shutting_down(&operation)?;
    emit_auto_update_event(app, "checked", Some(update.clone()), None);

    if update.latest_active {
        return Ok(());
    }

    let should_apply = update.can_apply;
    if !should_apply {
        return Ok(());
    }

    let process = app.state::<ApiServiceProcessState>();
    let download = app.state::<ApiServiceDownloadState>();
    let result =
        download_update_from_release_impl(app, &process, &download, &operation, &release, &update);

    match result {
        Ok(_) => {
            let mut applied = update;
            applied.current_version = Some(applied.latest_version.clone());
            applied.has_update = false;
            applied.can_apply = false;
            applied.latest_installed = true;
            applied.latest_active = true;
            emit_auto_update_event(app, "updated", Some(applied), None);
            Ok(())
        }
        Err(error) => {
            emit_auto_update_event(app, "failed", Some(update), Some(error.clone()));
            Err(error)
        }
    }
}

fn emit_auto_update_event(
    app: &AppHandle,
    status: &str,
    update_info: Option<UpdateInfo>,
    message: Option<String>,
) {
    let _ = app.emit(
        AUTO_UPDATE_EVENT,
        AutoUpdateEvent {
            status: status.to_string(),
            update_info,
            message,
        },
    );
}

fn lock_operation(operation: &ApiServiceOperationState) -> Result<MutexGuard<'_, ()>, String> {
    operation
        .lock
        .lock()
        .map_err(|_| "API 服务操作锁已损坏".to_string())
}

fn try_lock_operation(
    operation: &ApiServiceOperationState,
) -> Result<Option<MutexGuard<'_, ()>>, String> {
    match operation.lock.try_lock() {
        Ok(guard) => Ok(Some(guard)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Poisoned(_)) => Err("API 服务操作锁已损坏".to_string()),
    }
}

fn ensure_not_shutting_down(operation: &ApiServiceOperationState) -> Result<(), String> {
    if operation.shutting_down.load(Ordering::SeqCst) {
        return Err("应用正在退出，已取消 API 服务操作".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn api_service_bind_accounts(
    process: State<'_, ApiServiceProcessState>,
    operation: State<'_, ApiServiceOperationState>,
    account_ids: Vec<String>,
) -> Result<ApiServiceAccountSyncSummary, String> {
    let mut seen = HashSet::new();
    let ids = account_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty() && seen.insert(id.clone()))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Err("请选择要绑定到 API 服务的账号".to_string());
    }
    if service_pid_for_state(&process)?.is_none() {
        return Err("请先启动 API 服务，再绑定账号".to_string());
    }
    let dirs = ApiServiceDirs::new()?;
    let initial_settings = read_effective_settings(&dirs)?;

    let store = AccountStore::default();
    let accounts = store.list_accounts()?;
    let mut selected = Vec::with_capacity(ids.len());
    for id in ids {
        let account = accounts
            .iter()
            .find(|account| account.id == id)
            .cloned()
            .ok_or_else(|| format!("账号不存在: {id}"))?;
        if account.is_hidden {
            return Err(format!(
                "账号“{}”已启用隐身模式，不能绑定到 API 服务",
                api_service_account_label(&account)
            ));
        }
        selected.push(account);
    }

    let mut api_key_bindings = Vec::new();
    for account in selected
        .iter()
        .filter(|account| is_api_key_account(account))
    {
        if is_current_api_service_account(account, &initial_settings)? {
            return Err(format!(
                "账号“{}”指向当前 API 服务端口，不能绑定服务自身",
                api_service_account_label(account)
            ));
        }
        let models = match fetch_codex_api_key_models_for_account(account).await {
            Ok(models) if !models.is_empty() => models
                .into_iter()
                .map(|model| CodexApiKeyConfigModel {
                    name: model.id,
                    alias: String::new(),
                    extra: BTreeMap::new(),
                })
                .collect(),
            _ => account
                .default_model
                .as_deref()
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(|model| {
                    vec![CodexApiKeyConfigModel {
                        name: model.to_string(),
                        alias: String::new(),
                        extra: BTreeMap::new(),
                    }]
                })
                .unwrap_or_default(),
        };
        api_key_bindings.push((account.clone(), models));
    }

    let mut oauth_bindings = Vec::new();
    for account in selected
        .iter()
        .filter(|account| !is_api_key_account(account))
    {
        let content = store.export_accounts(std::slice::from_ref(&account.id), Some("cpa"))?;
        let mut value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|error| format!("解析 CPA 导出数据失败: {}", error))?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| "CPA 单账号导出格式无效".to_string())?;
        object.insert(
            "codex_switcher_account_id".to_string(),
            serde_json::Value::String(account.id.clone()),
        );
        let content = serde_json::to_string(&value)
            .map_err(|error| format!("生成 CPA 认证文件失败: {error}"))?;
        let file_name = format!(
            "codex-switcher-{}.json",
            &api_service_hash(&account.id)[..32]
        );
        oauth_bindings.push((file_name, content));
    }

    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    if service_pid_for_state(&process)?.is_none() {
        return Err("API 服务已停止，请重新启动后再绑定账号".to_string());
    }
    ensure_auth_dir_config(&dirs)?;
    let settings = read_effective_settings(&dirs)?;
    for (account, _) in &api_key_bindings {
        if is_current_api_service_account(account, &settings)? {
            return Err(format!(
                "账号“{}”指向当前 API 服务端口，不能绑定服务自身",
                api_service_account_label(account)
            ));
        }
    }

    let auth_dir = api_auth_dir(&dirs);
    fs::create_dir_all(&dirs.base_dir)
        .map_err(|error| format!("创建 API 服务目录失败: {}", error))?;
    let staging = tempfile::Builder::new()
        .prefix(".auth-staging-")
        .tempdir_in(&dirs.base_dir)
        .map_err(|error| format!("创建认证临时目录失败: {error}"))?;
    for (file_name, content) in &oauth_bindings {
        write_bytes_atomic(&staging.path().join(file_name), content.as_bytes())
            .map_err(|error| format!("准备 API 服务认证文件失败: {error}"))?;
    }

    let config_path = dirs.workspace_dir.join("config.yaml");
    let manifest_path = managed_api_key_bindings_path(&dirs);
    let config_snapshot = read_optional_file(&config_path)?;
    let manifest_snapshot = read_optional_file(&manifest_path)?;
    let update_result = (|| {
        clear_managed_api_key_bindings(&dirs)?;
        if !api_key_bindings.is_empty() {
            bind_managed_api_key_accounts(&dirs, &api_key_bindings)?;
        }
        replace_auth_directory(&auth_dir, staging.path())
    })();
    if let Err(error) = update_result {
        let config_rollback = restore_optional_file(&config_path, config_snapshot.as_deref());
        let manifest_rollback = restore_optional_file(&manifest_path, manifest_snapshot.as_deref());
        let rollback_errors = [config_rollback.err(), manifest_rollback.err()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if !rollback_errors.is_empty() {
            return Err(format!(
                "{error}；回滚 API Key 配置失败: {}",
                rollback_errors.join("；")
            ));
        }
        return Err(error);
    }

    let oauth_count = oauth_bindings.len();
    let api_key_count = api_key_bindings.len();
    Ok(ApiServiceAccountSyncSummary {
        count: oauth_count + api_key_count,
        auth_dir: display_path(&auth_dir),
        oauth_count,
        api_key_count,
    })
}

#[tauri::command]
pub fn api_service_list_bound_accounts(
    operation: State<'_, ApiServiceOperationState>,
) -> Result<Vec<ApiServiceBoundAccount>, String> {
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    let dirs = ApiServiceDirs::new()?;
    ensure_auth_dir_config(&dirs)?;
    list_all_bound_accounts(&dirs)
}

#[tauri::command]
pub fn api_service_delete_bound_accounts(
    operation: State<'_, ApiServiceOperationState>,
    bound_ids: Vec<String>,
) -> Result<ApiServiceAccountSyncSummary, String> {
    let targets = bound_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<HashSet<_>>();
    if targets.is_empty() {
        return Err("请选择要删除的 API 服务账号".to_string());
    }
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    let dirs = ApiServiceDirs::new()?;
    ensure_auth_dir_config(&dirs)?;
    let auth_dir = api_auth_dir(&dirs);
    let accounts = list_bound_auth_accounts(&auth_dir)?;
    let mut oauth_count = 0;
    for account in accounts {
        if targets.contains(&account.id) {
            fs::remove_file(&account.path)
                .map_err(|error| format!("删除认证文件失败: {}", error))?;
            oauth_count += 1;
        }
    }
    let api_key_count = delete_managed_api_key_bindings(&dirs, &targets)?;
    Ok(ApiServiceAccountSyncSummary {
        count: oauth_count + api_key_count,
        auth_dir: display_path(&auth_dir),
        oauth_count,
        api_key_count,
    })
}

#[tauri::command]
pub fn api_service_delete_account_binding(
    operation: State<'_, ApiServiceOperationState>,
    account_id: String,
) -> Result<ApiServiceAccountSyncSummary, String> {
    let account_id = account_id.trim();
    if account_id.is_empty() || account_id.len() > 128 {
        return Err("账号 ID 无效".to_string());
    }
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    let dirs = ApiServiceDirs::new()?;
    ensure_auth_dir_config(&dirs)?;
    let store = AccountStore::default();
    let local_accounts = store.list_accounts()?;
    let source = local_accounts
        .iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| "账号不存在".to_string())?;
    let duplicate_email_count = local_accounts
        .iter()
        .filter(|account| account.email.eq_ignore_ascii_case(&source.email))
        .count();
    let source_chatgpt_account_id = chatgpt_account_id(source);
    let auth_dir = api_auth_dir(&dirs);
    let mut matched_paths = Vec::new();
    let mut ambiguous_legacy_binding = false;
    for account in list_bound_auth_accounts(&auth_dir)? {
        let content = match fs::read_to_string(&account.path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let value: serde_json::Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if api_auth_matches_source_account(
            &value,
            account_id,
            &source.email,
            source_chatgpt_account_id.as_deref(),
            duplicate_email_count,
        ) {
            matched_paths.push(PathBuf::from(&account.path));
        } else if duplicate_email_count > 1
            && cpa_switcher_account_id_from_value(&value).is_none()
            && cpa_chatgpt_account_id_from_value(&value).is_none()
            && account
                .email
                .as_deref()
                .is_some_and(|email| email.eq_ignore_ascii_case(&source.email))
        {
            ambiguous_legacy_binding = true;
        }
    }
    if ambiguous_legacy_binding {
        return Err(
            "检测到同邮箱的旧版 API 服务认证文件，但缺少账号身份字段，已阻止删除".to_string(),
        );
    }
    for path in &matched_paths {
        fs::remove_file(path).map_err(|error| format!("删除认证文件失败: {error}"))?;
    }
    let oauth_count = matched_paths.len();
    let targets = HashSet::from([format!("apikey:{account_id}")]);
    let api_key_count = delete_managed_api_key_bindings(&dirs, &targets)?;
    Ok(ApiServiceAccountSyncSummary {
        count: oauth_count + api_key_count,
        auth_dir: display_path(&auth_dir),
        oauth_count,
        api_key_count,
    })
}

pub fn shutdown_api_service(
    process: State<'_, ApiServiceProcessState>,
    download: State<'_, ApiServiceDownloadState>,
    operation: State<'_, ApiServiceOperationState>,
) {
    operation.shutting_down.store(true, Ordering::SeqCst);
    if let Ok(mut control) = download.0.lock() {
        control.cancel_requested = true;
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        match try_lock_operation(&operation) {
            Ok(Some(_guard)) => {
                let _ = stop_service_impl(&process);
                return;
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => break,
        }
    }
    // Network reads may still be unwinding. Avoid touching persisted state without the
    // operation lock; the shutdown flag prevents the in-flight update from restarting.
    let _ = stop_service_process_only(&process);
}

pub fn prune_api_service_runtimes_on_startup(app: &AppHandle) -> Result<(), String> {
    let operation = app.state::<ApiServiceOperationState>();
    let _guard = lock_operation(&operation)?;
    ensure_not_shutting_down(&operation)?;
    let dirs = ApiServiceDirs::new()?;
    if active_runtime(&dirs).is_err() {
        if let Some(runtime) = latest_compatible_runtime(&dirs)? {
            write_active_version(&dirs, Some(runtime.id))?;
        }
    }
    prune_old_runtimes(&dirs)
}

fn start_service_impl(
    app: &AppHandle,
    process: &State<'_, ApiServiceProcessState>,
    download: &State<'_, ApiServiceDownloadState>,
    operation: &ApiServiceOperationState,
) -> Result<ApiServiceState, String> {
    ensure_not_shutting_down(operation)?;
    if process_pid(process)?.is_some() {
        return build_state(process);
    }

    let dirs = ApiServiceDirs::new()?;
    let mut settings = read_settings(&dirs)?;
    if active_runtime(&dirs).is_err() {
        let previous_active_version = read_active_version(&dirs)?;
        let runtime = match latest_compatible_runtime(&dirs)? {
            Some(runtime) => runtime,
            None => download_latest_runtime(app, process, download, operation, false)?,
        };
        ensure_not_shutting_down(operation)?;
        write_active_version(&dirs, Some(runtime.id))?;
        if operation.shutting_down.load(Ordering::SeqCst) {
            write_active_version(&dirs, previous_active_version)?;
            return Err("应用退出期间已取消 API 服务版本切换".to_string());
        }
    }

    let runtime = active_runtime(&dirs)?;
    ensure_not_shutting_down(operation)?;
    settings.enabled = true;
    let config_path = write_runtime_config(&dirs, &runtime, &settings)?;
    reject_unmanaged_port_listener(&dirs, settings.port)?;

    let mut command = Command::new(&runtime.binary_path);
    command
        .arg("--config")
        .arg(&config_path)
        .current_dir(&dirs.workspace_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("MANAGEMENT_PASSWORD", &settings.management_key);
    hide_command_window(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| format!("启动 API 服务失败: {}", error))?;
    let pid = child.id();
    let mut guard = match process.0.lock() {
        Ok(guard) => guard,
        Err(_) => {
            let cleanup = match child.kill() {
                Ok(()) => child
                    .wait()
                    .map(|_| "已终止新进程".to_string())
                    .unwrap_or_else(|error| format!("等待新进程退出失败: {error}")),
                Err(error) => format!("终止新进程失败: {error}"),
            };
            return Err(format!("服务状态锁已损坏；{cleanup}"));
        }
    };
    *guard = Some(child);
    drop(guard);
    let startup_result = (|| {
        write_managed_pid(&dirs, Some(pid))?;
        wait_for_service_ready(process, operation, settings.port, Duration::from_secs(12))?;
        write_settings(&dirs, &settings)?;
        build_state(process)
    })();
    match startup_result {
        Ok(state) => Ok(state),
        Err(error) => {
            let cleanup_error = stop_service_impl(process).err();
            Err(match cleanup_error {
                Some(cleanup_error) => format!("{error}; 启动失败清理异常: {cleanup_error}"),
                None => error,
            })
        }
    }
}

fn wait_for_service_ready(
    process: &State<'_, ApiServiceProcessState>,
    operation: &ApiServiceOperationState,
    port: u16,
    timeout: Duration,
) -> Result<(), String> {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let deadline = Instant::now() + timeout;
    loop {
        ensure_not_shutting_down(operation)?;
        if process_pid(process)?.is_none() {
            return Err("API 服务启动后立即退出".to_string());
        }
        if TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("API 服务启动超时，端口 {port} 未就绪"));
        }
        thread::sleep(Duration::from_millis(120));
    }
}

fn stop_service_impl(process: &State<'_, ApiServiceProcessState>) -> Result<(), String> {
    let mut failures = Vec::new();
    if let Err(error) = stop_service_process_only(process) {
        failures.push(error);
    }
    let dirs = match ApiServiceDirs::new() {
        Ok(dirs) => dirs,
        Err(error) => {
            failures.push(error);
            return Err(failures.join("; "));
        }
    };
    if let Err(error) = write_managed_pid(&dirs, None) {
        failures.push(error);
    }
    match read_settings(&dirs) {
        Ok(mut settings) => {
            settings.enabled = false;
            if let Err(error) = write_settings(&dirs, &settings) {
                failures.push(error);
            }
        }
        Err(error) => failures.push(error),
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

fn stop_service_process_only(process: &State<'_, ApiServiceProcessState>) -> Result<(), String> {
    let mut failures = Vec::new();
    let mut guard = process
        .0
        .lock()
        .map_err(|_| "服务状态锁已损坏".to_string())?;
    if let Some(mut child) = guard.take() {
        match child.kill() {
            Ok(()) => {
                if let Err(error) = child.wait() {
                    failures.push(format!("等待 API 服务退出失败: {error}"));
                }
            }
            Err(error) => failures.push(format!("停止 API 服务失败: {error}")),
        }
    }
    drop(guard);

    match ApiServiceDirs::new() {
        Ok(dirs) => match read_managed_pid(&dirs) {
            Ok(Some(pid)) => {
                if let Err(error) = terminate_managed_pid(&dirs, pid) {
                    failures.push(error);
                }
            }
            Ok(None) => {}
            Err(error) => failures.push(error),
        },
        Err(error) => failures.push(error),
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

fn auto_update_due(settings: &ApiServiceSettings, has_runtimes: bool, now: u64) -> bool {
    if !settings.auto_update || !has_runtimes {
        return false;
    }
    let Some(last) = settings.last_update_check_at else {
        return true;
    };
    let interval = settings.auto_update_interval_hours.clamp(1, 24 * 30) * 3600;
    now.saturating_sub(last) >= interval
}

fn should_apply_latest_version(current: Option<&str>, latest: &str) -> Result<bool, String> {
    let latest = parse_semantic_version(latest)?;
    match current {
        Some(current) => Ok(latest > parse_semantic_version(current)?),
        None => Ok(true),
    }
}

fn switch_active_runtime(
    app: &AppHandle,
    process: &State<'_, ApiServiceProcessState>,
    download: &State<'_, ApiServiceDownloadState>,
    operation: &ApiServiceOperationState,
    dirs: &ApiServiceDirs,
    runtime_id: &str,
) -> Result<ApiServiceState, String> {
    ensure_not_shutting_down(operation)?;
    let runtime = compatible_runtime_by_id(dirs, runtime_id)?;
    let previous_active_version = read_active_version(dirs)?;
    if previous_active_version.as_deref() == Some(runtime.id.as_str()) {
        if let Err(error) = prune_old_runtimes(dirs) {
            eprintln!("failed to prune old API service runtimes: {error}");
        }
        return build_state(process);
    }
    let was_running = service_pid_for_state(process)?.is_some();
    if was_running {
        if let Err(error) = stop_service_impl(process) {
            return Err(rollback_runtime_switch(
                app,
                process,
                download,
                operation,
                dirs,
                &previous_active_version,
                was_running,
                format!("停止当前 API 服务失败: {error}"),
            ));
        }
    }
    if let Err(error) = ensure_not_shutting_down(operation) {
        return Err(rollback_runtime_switch(
            app,
            process,
            download,
            operation,
            dirs,
            &previous_active_version,
            was_running,
            error,
        ));
    }
    if let Err(error) = write_active_version(dirs, Some(runtime.id.clone())) {
        return Err(rollback_runtime_switch(
            app,
            process,
            download,
            operation,
            dirs,
            &previous_active_version,
            was_running,
            error,
        ));
    }
    if operation.shutting_down.load(Ordering::SeqCst) {
        return Err(rollback_runtime_switch(
            app,
            process,
            download,
            operation,
            dirs,
            &previous_active_version,
            was_running,
            "应用退出期间已取消 API 服务版本切换".to_string(),
        ));
    }
    if !was_running {
        if let Err(error) = prune_old_runtimes(dirs) {
            eprintln!("failed to prune old API service runtimes: {error}");
        }
        return build_state(process);
    }
    if let Err(error) = ensure_not_shutting_down(operation) {
        return Err(rollback_runtime_switch(
            app,
            process,
            download,
            operation,
            dirs,
            &previous_active_version,
            was_running,
            error,
        ));
    }
    match start_service_impl(app, process, download, operation) {
        Ok(_) => {
            if let Err(error) = prune_old_runtimes(dirs) {
                eprintln!("failed to prune old API service runtimes: {error}");
            }
            build_state(process)
        }
        Err(start_error) => Err(rollback_runtime_switch(
            app,
            process,
            download,
            operation,
            dirs,
            &previous_active_version,
            was_running,
            format!("新版 API 服务启动失败: {start_error}"),
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn rollback_runtime_switch(
    app: &AppHandle,
    process: &State<'_, ApiServiceProcessState>,
    download: &State<'_, ApiServiceDownloadState>,
    operation: &ApiServiceOperationState,
    dirs: &ApiServiceDirs,
    previous_active_version: &Option<String>,
    was_running: bool,
    reason: String,
) -> String {
    let mut failures = Vec::new();
    if let Err(error) = stop_service_impl(process) {
        failures.push(format!("停止失败版本进程异常: {error}"));
    }
    if let Err(error) = write_active_version(dirs, previous_active_version.clone()) {
        failures.push(format!("恢复旧 active 失败: {error}"));
    } else if was_running && !operation.shutting_down.load(Ordering::SeqCst) {
        if let Err(error) = start_service_impl(app, process, download, operation) {
            failures.push(format!("重启旧版本失败: {error}"));
        }
    }
    if failures.is_empty() {
        if was_running && !operation.shutting_down.load(Ordering::SeqCst) {
            format!("{reason}，已恢复旧版本")
        } else {
            reason
        }
    } else {
        format!("{reason}; {}", failures.join("; "))
    }
}

fn mark_update_check_attempted() -> Result<(), String> {
    let dirs = ApiServiceDirs::new()?;
    let mut settings = read_settings(&dirs)?;
    settings.last_update_check_at = Some(unix_timestamp()?);
    write_settings(&dirs, &settings)
}

fn download_latest_runtime(
    app: &AppHandle,
    process: &State<'_, ApiServiceProcessState>,
    download: &State<'_, ApiServiceDownloadState>,
    operation: &ApiServiceOperationState,
    activate: bool,
) -> Result<RuntimeInfo, String> {
    let release = fetch_latest_release()?;
    ensure_not_shutting_down(operation)?;
    download_runtime_from_release(app, process, download, operation, &release, activate)
}

fn download_runtime_from_release(
    app: &AppHandle,
    process: &State<'_, ApiServiceProcessState>,
    download: &State<'_, ApiServiceDownloadState>,
    operation: &ApiServiceOperationState,
    release: &GitHubRelease,
    activate: bool,
) -> Result<RuntimeInfo, String> {
    let _ = process;
    ensure_not_shutting_down(operation)?;
    let download_id = begin_download(download, operation)?;
    let result = (|| {
        let dirs = ApiServiceDirs::new()?;
        let update = update_info_from_release(&dirs, release)?;
        let asset_name = update
            .asset_name
            .ok_or_else(|| format!("最新版本没有匹配 {} 的安装包", update.target))?;
        let download_url = update
            .download_url
            .ok_or_else(|| format!("最新版本没有匹配 {} 的下载地址", update.target))?;
        let package_path = download_release_asset(
            app,
            download,
            &download_id,
            &dirs,
            &asset_name,
            &download_url,
        )?;
        ensure_not_shutting_down(operation)?;
        if let Err(error) = verify_release_asset_checksum(release, &asset_name, &package_path) {
            let _ = fs::remove_file(&package_path);
            return Err(error);
        }
        if let Err(error) = ensure_package_matches_release(&package_path, release) {
            let _ = fs::remove_file(&package_path);
            return Err(error);
        }
        ensure_not_shutting_down(operation)?;
        emit_download_progress(
            app,
            "installing",
            &asset_name,
            1,
            Some(1),
            Some("正在安装 API 服务"),
        )?;
        let install_result = install_runtime_package(&dirs, &package_path, activate);
        let cleanup_result = cleanup_downloads_dir(&dirs);
        match (install_result, cleanup_result) {
            (Ok(runtime), Ok(())) => Ok(runtime),
            (Ok(_), Err(error)) => Err(format!("API 服务已安装，但清理下载文件失败: {}", error)),
            (Err(error), _) => Err(error),
        }
    })();
    if let Err(message) = &result {
        if !message.contains("下载已取消") {
            let _ = emit_download_progress(app, "failed", "", 0, None, Some(message));
        }
    }
    clear_download(download, &download_id);
    result
}

fn build_state(process: &State<'_, ApiServiceProcessState>) -> Result<ApiServiceState, String> {
    let dirs = ApiServiceDirs::new()?;
    let settings = read_effective_settings(&dirs)?;
    let runtimes = list_runtimes(&dirs)?;
    let active_version = read_active_version(&dirs)?;
    let old_runtime_count = runtimes
        .iter()
        .filter(|runtime| Some(runtime.id.as_str()) != active_version.as_deref())
        .count();
    let pid = service_pid_for_state(process)?;
    Ok(ApiServiceState {
        base_dir: display_path(&dirs.base_dir),
        runtime_dir: display_path(&dirs.runtime_dir),
        workspace_dir: display_path(&dirs.workspace_dir),
        downloads_dir: display_path(&dirs.downloads_dir),
        auth_dir: display_path(&api_auth_dir(&dirs)),
        config_path: display_path(&dirs.workspace_dir.join("config.yaml")),
        installed: !runtimes.is_empty(),
        maintenance_old_runtime_count: (old_runtime_count > MAX_OLD_RUNTIMES)
            .then_some(old_runtime_count),
        service: ApiServiceInfo {
            running: pid.is_some(),
            pid,
            port: settings.port,
            management_url: format!("http://127.0.0.1:{}/management.html", settings.port),
        },
        settings,
        active_version,
        runtimes,
    })
}

fn snapshot_local_runtime_package(
    dirs: &ApiServiceDirs,
    source_path: &Path,
) -> Result<LocalPackageSnapshot, String> {
    if source_path.as_os_str().is_empty() {
        return Err("请选择要导入的 API 服务版本包".to_string());
    }
    let source_metadata = fs::symlink_metadata(source_path)
        .map_err(|error| format!("读取导入版本包失败: {error}"))?;
    if source_metadata.file_type().is_symlink() {
        return Err("导入版本包不能是符号链接".to_string());
    }
    if !source_metadata.is_file() {
        return Err("导入版本包必须是普通文件".to_string());
    }
    if source_metadata.len() > MAX_RUNTIME_ARCHIVE_BYTES {
        return Err(format!(
            "CLIProxyAPI 安装包超过安全大小限制（最大 {} MiB）",
            MAX_RUNTIME_ARCHIVE_BYTES / 1024 / 1024
        ));
    }

    let canonical_source = source_path
        .canonicalize()
        .map_err(|error| format!("解析导入版本包路径失败: {error}"))?;
    let package = parse_package_info(&canonical_source)?;
    ensure_package_target_matches_current(&package)?;
    let file_name = canonical_source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "版本包文件名无效".to_string())?;
    let file_name = safe_download_file_name(file_name)?;

    fs::create_dir_all(&dirs.staging_dir)
        .map_err(|error| format!("创建导入临时目录失败: {error}"))?;
    let snapshot_directory = dirs.staging_dir.join(format!(
        "import-{}-{}",
        unix_timestamp()?,
        random_suffix(10)
    ));
    fs::create_dir(&snapshot_directory)
        .map_err(|error| format!("创建导入快照目录失败: {error}"))?;
    let snapshot_path = snapshot_directory.join(file_name);

    let copy_result = (|| {
        let mut source = File::open(&canonical_source)
            .map_err(|error| format!("打开导入版本包失败: {error}"))?;
        let opened_metadata = source
            .metadata()
            .map_err(|error| format!("读取导入版本包元数据失败: {error}"))?;
        if !opened_metadata.is_file() {
            return Err("导入版本包必须是普通文件".to_string());
        }
        let mut target = File::create(&snapshot_path)
            .map_err(|error| format!("创建导入版本包快照失败: {error}"))?;
        let copied = io::copy(
            &mut Read::by_ref(&mut source).take(MAX_RUNTIME_ARCHIVE_BYTES + 1),
            &mut target,
        )
        .map_err(|error| format!("复制导入版本包失败: {error}"))?;
        if copied > MAX_RUNTIME_ARCHIVE_BYTES {
            return Err(format!(
                "CLIProxyAPI 安装包超过安全大小限制（最大 {} MiB）",
                MAX_RUNTIME_ARCHIVE_BYTES / 1024 / 1024
            ));
        }
        target
            .sync_all()
            .map_err(|error| format!("刷新导入版本包快照失败: {error}"))?;
        Ok(())
    })();
    if let Err(error) = copy_result {
        let _ = fs::remove_dir_all(&snapshot_directory);
        return Err(error);
    }

    Ok(LocalPackageSnapshot {
        path: snapshot_path,
        directory: snapshot_directory,
    })
}

fn install_runtime_package(
    dirs: &ApiServiceDirs,
    package_path: &Path,
    activate: bool,
) -> Result<RuntimeInfo, String> {
    if !package_path.exists() {
        return Err(format!("版本包不存在: {}", display_path(package_path)));
    }
    let package = parse_package_info(package_path)?;
    ensure_package_target_matches_current(&package)?;

    fs::create_dir_all(&dirs.runtime_dir)
        .map_err(|error| format!("创建运行时目录失败: {}", error))?;
    fs::create_dir_all(&dirs.staging_dir)
        .map_err(|error| format!("创建临时目录失败: {}", error))?;
    let install_dir = dirs.runtime_dir.join(&package.id);
    let binary_path = runtime_binary_path(&install_dir);
    if binary_path.exists() {
        ensure_default_config_cache(&install_dir)?;
        let runtime = runtime_from_metadata(&install_dir)?;
        if activate {
            write_active_version(dirs, Some(runtime.id.clone()))?;
        }
        return Ok(runtime);
    }
    if install_dir.exists() {
        return Err(format!(
            "运行时目录已存在但不完整: {}",
            display_path(&install_dir)
        ));
    }

    let staging_dir = dirs.staging_dir.join(format!(
        "{}-{}-{}",
        package.id,
        unix_timestamp()?,
        random_suffix(8)
    ));
    fs::create_dir_all(&staging_dir).map_err(|error| format!("创建解包目录失败: {}", error))?;
    let prepare_result = (|| {
        unpack_archive(package_path, &staging_dir)?;
        let staging_binary = runtime_binary_path(&staging_dir);
        ensure_runtime_binary_is_regular_file(&staging_binary)?;
        set_executable(&staging_binary)?;
        ensure_default_config_cache(&staging_dir)?;
        let package_file = package_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("CLIProxyAPI.tar.gz")
            .to_string();
        let metadata = RuntimeMetadata {
            id: package.id.clone(),
            version: package.version.clone(),
            target: package.target.clone(),
            installed_at: unix_timestamp()?,
            package_file,
        };
        write_json(&staging_dir.join("metadata.json"), &metadata)
    })();
    if let Err(error) = prepare_result {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(error);
    }
    if let Err(error) = fs::rename(&staging_dir, &install_dir) {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(format!("安装运行时失败: {}", error));
    }
    let runtime = match runtime_from_metadata(&install_dir) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = fs::remove_dir_all(&install_dir);
            return Err(error);
        }
    };
    if activate {
        write_active_version(dirs, Some(runtime.id.clone()))?;
    }
    Ok(runtime)
}

fn fetch_latest_release() -> Result<GitHubRelease, String> {
    let response = reqwest::blocking::Client::builder()
        .user_agent(GITHUB_USER_AGENT)
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| format!("初始化下载客户端失败: {}", error))?
        .get(CLIPROXYAPI_LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|error| format!("检测 CLIProxyAPI 更新失败: {}", error))?;
    let status = response.status();
    if !status.is_success() {
        let message = response.text().unwrap_or_default();
        return Err(format!(
            "检测 CLIProxyAPI 更新失败: HTTP {} {}",
            status, message
        ));
    }
    response
        .json()
        .map_err(|error| format!("解析 CLIProxyAPI 更新信息失败: {}", error))
}

fn update_info_from_release(
    dirs: &ApiServiceDirs,
    release: &GitHubRelease,
) -> Result<UpdateInfo, String> {
    let latest_version = parse_semantic_version(&release.tag_name)?.to_string();
    let targets = current_package_target_aliases();
    let runtimes = list_runtimes(dirs)?;
    let active_version = read_active_version(dirs)?;
    let active_runtime = active_version
        .as_deref()
        .and_then(|id| runtimes.iter().find(|runtime| runtime.id == id));
    let current_version = active_runtime.map(|runtime| runtime.version.clone());
    let latest_installed = runtimes.iter().any(|runtime| {
        versions_equal(&runtime.version, &latest_version)
            && targets.iter().any(|target| target == &runtime.target)
    });
    let latest_active = active_runtime.is_some_and(|runtime| {
        versions_equal(&runtime.version, &latest_version)
            && targets.iter().any(|target| target == &runtime.target)
    });
    let asset = release_asset_for_target(release, &latest_version, &targets);
    let can_apply = asset.is_some()
        && !latest_active
        && should_apply_latest_version(current_version.as_deref(), &latest_version)?;
    let has_update = can_apply && !latest_installed;
    Ok(UpdateInfo {
        current_version,
        latest_version,
        target: targets.join(" / "),
        release_url: release.html_url.clone(),
        download_url: asset.map(|asset| asset.browser_download_url.clone()),
        asset_name: asset.map(|asset| asset.name.clone()),
        has_update,
        can_apply,
        latest_installed,
        latest_active,
    })
}

fn release_asset_for_target<'a>(
    release: &'a GitHubRelease,
    version: &str,
    targets: &[String],
) -> Option<&'a GitHubAsset> {
    let exact_names = targets
        .iter()
        .flat_map(|target| {
            [
                format!("CLIProxyAPI_{version}_{target}.tar.gz"),
                format!("CLIProxyAPI_{version}_{target}.tgz"),
                format!("CLIProxyAPI_{version}_{target}.zip"),
            ]
        })
        .collect::<Vec<_>>();
    release
        .assets
        .iter()
        .find(|asset| exact_names.iter().any(|name| name == &asset.name))
}

fn download_release_asset(
    app: &AppHandle,
    download: &State<'_, ApiServiceDownloadState>,
    download_id: &str,
    dirs: &ApiServiceDirs,
    asset_name: &str,
    download_url: &str,
) -> Result<PathBuf, String> {
    let file_name = safe_download_file_name(asset_name)?;
    fs::create_dir_all(&dirs.downloads_dir)
        .map_err(|error| format!("创建下载目录失败: {}", error))?;
    let package_path = dirs.downloads_dir.join(&file_name);
    let temp_path = dirs.downloads_dir.join(format!("{file_name}.download"));
    let result = (|| {
        emit_download_progress(
            app,
            "starting",
            asset_name,
            0,
            None,
            Some("准备下载 API 服务"),
        )?;
        let mut response = reqwest::blocking::Client::builder()
            .user_agent(GITHUB_USER_AGENT)
            .timeout(Duration::from_secs(60 * 10))
            .build()
            .map_err(|error| format!("初始化下载客户端失败: {}", error))?
            .get(download_url)
            .header("Accept", "application/octet-stream")
            .send()
            .map_err(|error| format!("下载 CLIProxyAPI 安装包失败: {}", error))?;
        let status = response.status();
        if !status.is_success() {
            let message = response.text().unwrap_or_default();
            return Err(format!(
                "下载 CLIProxyAPI 安装包失败: HTTP {} {}",
                status, message
            ));
        }
        let total_bytes = response.content_length();
        if total_bytes.is_some_and(|size| size > MAX_RUNTIME_ARCHIVE_BYTES) {
            return Err(format!(
                "CLIProxyAPI 安装包超过安全大小限制（最大 {} MiB）",
                MAX_RUNTIME_ARCHIVE_BYTES / 1024 / 1024
            ));
        }
        emit_download_progress(app, "downloading", asset_name, 0, total_bytes, None)?;
        let mut file =
            File::create(&temp_path).map_err(|error| format!("创建下载临时文件失败: {}", error))?;
        let mut downloaded_bytes = 0_u64;
        let mut last_emit_bytes = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            if is_download_cancelled(download, download_id)? {
                emit_download_progress(
                    app,
                    "cancelled",
                    asset_name,
                    downloaded_bytes,
                    total_bytes,
                    Some("下载已取消"),
                )?;
                return Err("下载已取消".to_string());
            }
            let read = response
                .read(&mut buffer)
                .map_err(|error| format!("读取安装包失败: {}", error))?;
            if read == 0 {
                break;
            }
            let next_downloaded = downloaded_bytes.saturating_add(read as u64);
            if next_downloaded > MAX_RUNTIME_ARCHIVE_BYTES {
                return Err(format!(
                    "CLIProxyAPI 安装包超过安全大小限制（最大 {} MiB）",
                    MAX_RUNTIME_ARCHIVE_BYTES / 1024 / 1024
                ));
            }
            file.write_all(&buffer[..read])
                .map_err(|error| format!("写入安装包失败: {}", error))?;
            downloaded_bytes = next_downloaded;
            if downloaded_bytes == total_bytes.unwrap_or(0)
                || downloaded_bytes.saturating_sub(last_emit_bytes) >= 512 * 1024
            {
                emit_download_progress(
                    app,
                    "downloading",
                    asset_name,
                    downloaded_bytes,
                    total_bytes,
                    None,
                )?;
                last_emit_bytes = downloaded_bytes;
            }
        }
        file.flush()
            .map_err(|error| format!("刷新安装包文件失败: {}", error))?;
        fs::rename(&temp_path, &package_path)
            .map_err(|error| format!("保存安装包失败: {}", error))?;
        Ok(package_path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn verify_release_asset_checksum(
    release: &GitHubRelease,
    asset_name: &str,
    package_path: &Path,
) -> Result<(), String> {
    let manifest_asset = release
        .assets
        .iter()
        .find(|asset| asset.name.eq_ignore_ascii_case("checksums.txt"))
        .ok_or_else(|| "最新版本未提供 checksums.txt，已拒绝安装".to_string())?;
    let response = reqwest::blocking::Client::builder()
        .user_agent(GITHUB_USER_AGENT)
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| format!("初始化校验文件客户端失败: {error}"))?
        .get(&manifest_asset.browser_download_url)
        .header("Accept", "text/plain")
        .send()
        .map_err(|error| format!("下载 checksums.txt 失败: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("下载 checksums.txt 失败: HTTP {status}"));
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_CHECKSUM_MANIFEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取 checksums.txt 失败: {error}"))?;
    if bytes.len() > MAX_CHECKSUM_MANIFEST_BYTES {
        return Err("checksums.txt 超过安全大小限制".to_string());
    }
    let manifest = std::str::from_utf8(&bytes)
        .map_err(|error| format!("checksums.txt 不是有效 UTF-8: {error}"))?;
    let expected = checksum_for_asset(manifest, asset_name)
        .ok_or_else(|| format!("checksums.txt 中缺少 {asset_name} 的 SHA-256"))?;

    let mut file = File::open(package_path).map_err(|error| format!("读取安装包失败: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("计算安装包 SHA-256 失败: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err(format!(
            "安装包 SHA-256 校验失败: 期望 {expected}，实际 {actual}"
        ));
    }
    Ok(())
}

fn checksum_for_asset(manifest: &str, asset_name: &str) -> Option<String> {
    manifest.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let checksum = fields.next()?;
        let file_name = fields.next()?.trim_start_matches('*');
        if file_name == asset_name
            && checksum.len() == 64
            && checksum
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            Some(checksum.to_ascii_lowercase())
        } else {
            None
        }
    })
}

fn cleanup_downloads_dir(dirs: &ApiServiceDirs) -> Result<(), String> {
    if !dirs.downloads_dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(&dirs.downloads_dir).map_err(|error| format!("读取下载目录失败: {}", error))?
    {
        let path = entry
            .map_err(|error| format!("读取下载文件失败: {}", error))?
            .path();
        if path.is_file() {
            fs::remove_file(&path).map_err(|error| format!("删除下载文件失败: {}", error))?;
        } else if path.is_dir() {
            fs::remove_dir_all(&path)
                .map_err(|error| format!("删除下载临时目录失败: {}", error))?;
        }
    }
    Ok(())
}

fn api_auth_dir(dirs: &ApiServiceDirs) -> PathBuf {
    dirs.base_dir.join("auth")
}

fn ensure_auth_dir_config(dirs: &ApiServiceDirs) -> Result<(), String> {
    if let Ok(runtime) = active_runtime(dirs) {
        let settings = read_effective_settings(dirs)?;
        write_runtime_config(dirs, &runtime, &settings)?;
    }
    Ok(())
}

fn api_service_account_label(account: &CodexAccount) -> String {
    account
        .account_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(account.email.as_str())
        .to_string()
}

fn is_api_key_account(account: &CodexAccount) -> bool {
    account.auth_mode.as_deref() == Some("apikey")
        || account
            .openai_api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
}

fn normalized_api_base_url(account: &CodexAccount) -> Result<String, String> {
    let raw = account
        .api_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("https://api.openai.com/v1");
    let mut url = reqwest::Url::parse(raw).map_err(|error| format!("Base URL 无效: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err("Base URL 必须是有效的 http:// 或 https:// 地址".to_string());
    }
    url.set_query(None);
    url.set_fragment(None);
    let normalized = url.to_string();
    Ok(normalized.trim_end_matches('/').to_string())
}

fn is_current_api_service_account(
    account: &CodexAccount,
    settings: &ApiServiceSettings,
) -> Result<bool, String> {
    let base_url = normalized_api_base_url(account)?;
    Ok(is_current_api_service_endpoint(
        &base_url,
        account.openai_api_key.as_deref(),
        settings,
    ))
}

fn is_current_api_service_endpoint(
    base_url: &str,
    api_key: Option<&str>,
    settings: &ApiServiceSettings,
) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return false;
    };
    if url.port_or_known_default() != Some(settings.port) {
        return false;
    }
    let local_host = match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback() || address.is_unspecified(),
        Some(url::Host::Ipv6(address)) => address.is_loopback() || address.is_unspecified(),
        None => false,
    };
    let key_is_local = api_key
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .is_some_and(|key| {
            settings
                .api_keys
                .iter()
                .any(|candidate| candidate.trim() == key)
        });
    local_host || key_is_local
}

fn api_service_short_hash(value: &str) -> String {
    api_service_hash(value)[..16].to_string()
}

fn api_service_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn managed_api_key_bindings_path(dirs: &ApiServiceDirs) -> PathBuf {
    dirs.base_dir.join(API_KEY_BINDINGS_FILE)
}

fn read_managed_api_key_bindings(dirs: &ApiServiceDirs) -> Result<ManagedApiKeyBindings, String> {
    let path = managed_api_key_bindings_path(dirs);
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ManagedApiKeyBindings::default())
        }
        Err(error) => return Err(format!("读取 API Key 绑定清单失败: {error}")),
    };
    serde_json::from_str(&content).map_err(|error| format!("解析 API Key 绑定清单失败: {error}"))
}

fn read_codex_api_key_entries(config_path: &Path) -> Result<Vec<CodexApiKeyConfigEntry>, String> {
    let content = fs::read_to_string(config_path)
        .map_err(|error| format!("读取 API 服务配置失败: {error}"))?;
    let root: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|error| format!("解析 API 服务 YAML 配置失败: {error}"))?;
    let Some(mapping) = root.as_mapping() else {
        return Err("API 服务 YAML 配置根节点必须是对象".to_string());
    };
    let key = serde_yaml::Value::String("codex-api-key".to_string());
    let Some(value) = mapping.get(&key) else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    serde_yaml::from_value(value.clone())
        .map_err(|error| format!("解析 codex-api-key 配置失败: {error}"))
}

fn entry_matches_binding(entry: &CodexApiKeyConfigEntry, binding: &ManagedApiKeyBinding) -> bool {
    api_service_hash(entry.api_key.trim()) == binding.api_key_hash
        && entry.base_url.trim().trim_end_matches('/') == binding.base_url.trim_end_matches('/')
}

fn codex_api_key_yaml_lines(entries: &[CodexApiKeyConfigEntry]) -> Result<Vec<String>, String> {
    if entries.is_empty() {
        return Ok(vec!["codex-api-key: []".to_string()]);
    }
    let serialized = serde_yaml::to_string(entries)
        .map_err(|error| format!("序列化 codex-api-key 配置失败: {error}"))?;
    let mut lines = vec!["codex-api-key:".to_string()];
    lines.extend(
        serialized
            .trim_end()
            .lines()
            .map(|line| format!("  {line}")),
    );
    Ok(lines)
}

fn upsert_top_level_block(content: &str, key: &str, replacement: Vec<String>) -> String {
    let trailing_newline = content.ends_with('\n');
    let mut lines = content
        .replace("\r\n", "\n")
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if trailing_newline && lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }

    for index in 0..lines.len() {
        let Some((indent, line_key)) = yaml_key_line(&lines[index]) else {
            continue;
        };
        if indent != 0 || line_key != key {
            continue;
        }
        let mut end = index + 1;
        while end < lines.len() {
            let line = &lines[end];
            if line.starts_with('#') {
                break;
            }
            if line.trim().is_empty() {
                end += 1;
                continue;
            }
            if line.len() == line.trim_start().len() {
                break;
            }
            end += 1;
        }
        lines.splice(index..end, replacement);
        return finish_lines(lines, trailing_newline);
    }

    if !lines.is_empty() && !lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.push(String::new());
    }
    lines.extend(replacement);
    finish_lines(lines, trailing_newline)
}

fn persist_managed_api_key_config(
    dirs: &ApiServiceDirs,
    entries: &[CodexApiKeyConfigEntry],
    bindings: &ManagedApiKeyBindings,
) -> Result<(), String> {
    let config_path = dirs.workspace_dir.join("config.yaml");
    let original = fs::read_to_string(&config_path)
        .map_err(|error| format!("读取 API 服务配置失败: {error}"))?;
    let updated = upsert_top_level_block(
        &original,
        "codex-api-key",
        codex_api_key_yaml_lines(entries)?,
    );
    write_bytes_atomic(&config_path, updated.as_bytes())
        .map_err(|error| format!("写入 API Key 上游配置失败: {error}"))?;
    if let Err(error) = write_json(&managed_api_key_bindings_path(dirs), bindings) {
        let _ = write_bytes_atomic(&config_path, original.as_bytes());
        return Err(format!("写入 API Key 绑定清单失败: {error}"));
    }
    Ok(())
}

fn bind_managed_api_key_accounts(
    dirs: &ApiServiceDirs,
    selected: &[(CodexAccount, Vec<CodexApiKeyConfigModel>)],
) -> Result<(), String> {
    let config_path = dirs.workspace_dir.join("config.yaml");
    let mut entries = read_codex_api_key_entries(&config_path)?;
    let mut managed = read_managed_api_key_bindings(dirs)?;

    for (account, models) in selected {
        let api_key = account
            .openai_api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .ok_or_else(|| format!("账号“{}”缺少 API Key", api_service_account_label(account)))?;
        let base_url = normalized_api_base_url(account)?;
        if let Some(previous) = managed
            .bindings
            .iter()
            .find(|binding| binding.account_id == account.id)
            .cloned()
        {
            if previous.owns_config_entry {
                entries.retain(|entry| !entry_matches_binding(entry, &previous));
            }
        }
        let api_key_hash = api_service_hash(api_key);
        let existing_entry = entries.iter().any(|entry| {
            api_service_hash(entry.api_key.trim()) == api_key_hash
                && entry.base_url.trim().trim_end_matches('/') == base_url.trim_end_matches('/')
        });
        if !existing_entry {
            entries.push(CodexApiKeyConfigEntry {
                api_key: api_key.to_string(),
                base_url: base_url.clone(),
                models: models.clone(),
                extra: BTreeMap::new(),
            });
        }
        managed
            .bindings
            .retain(|binding| binding.account_id != account.id);
        managed.bindings.push(ManagedApiKeyBinding {
            account_id: account.id.clone(),
            api_key_hash,
            base_url,
            label: api_service_account_label(account),
            owns_config_entry: !existing_entry,
        });
    }

    persist_managed_api_key_config(dirs, &entries, &managed)
}

fn clear_managed_api_key_bindings(dirs: &ApiServiceDirs) -> Result<(), String> {
    let config_path = dirs.workspace_dir.join("config.yaml");
    let mut entries = read_codex_api_key_entries(&config_path)?;
    let managed = read_managed_api_key_bindings(dirs)?;
    if managed.bindings.is_empty() {
        return Ok(());
    }
    entries.retain(|entry| {
        !managed
            .bindings
            .iter()
            .any(|binding| binding.owns_config_entry && entry_matches_binding(entry, binding))
    });
    persist_managed_api_key_config(dirs, &entries, &ManagedApiKeyBindings::default())
}

fn read_optional_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("读取 {} 失败: {error}", display_path(path))),
    }
}

fn restore_optional_file(path: &Path, content: Option<&[u8]>) -> Result<(), String> {
    match content {
        Some(content) => write_bytes_atomic(path, content),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("删除 {} 失败: {error}", display_path(path))),
        },
    }
}

fn replace_auth_directory(auth_dir: &Path, staging_dir: &Path) -> Result<(), String> {
    let parent = auth_dir
        .parent()
        .ok_or_else(|| "API 服务认证目录无父目录".to_string())?;
    let backup = parent.join(format!(
        ".auth-backup-{}-{}",
        std::process::id(),
        unix_timestamp()?
    ));
    let had_existing = auth_dir.exists();
    if had_existing {
        fs::rename(auth_dir, &backup)
            .map_err(|error| format!("备份 API 服务认证目录失败: {error}"))?;
    }
    if let Err(error) = fs::rename(staging_dir, auth_dir) {
        if had_existing {
            if let Err(restore_error) = fs::rename(&backup, auth_dir) {
                return Err(format!(
                    "替换 API 服务认证目录失败: {error}；恢复旧认证目录失败: {restore_error}"
                ));
            }
        }
        return Err(format!("替换 API 服务认证目录失败: {error}"));
    }
    if had_existing {
        // 新目录已经完整就位；残留备份不影响服务，下次同步可继续使用。
        let _ = fs::remove_dir_all(&backup);
    }
    Ok(())
}

fn list_all_bound_accounts(dirs: &ApiServiceDirs) -> Result<Vec<ApiServiceBoundAccount>, String> {
    let auth_dir = api_auth_dir(dirs);
    let local_accounts = AccountStore::default().list_accounts()?;
    let mut bound = list_bound_auth_accounts(&auth_dir)?;
    for item in &mut bound {
        if item.account_id.is_none() {
            item.account_id = item.email.as_deref().and_then(|email| {
                local_accounts
                    .iter()
                    .find(|account| account.email.eq_ignore_ascii_case(email))
                    .map(|account| account.id.clone())
            });
        }
    }

    let config_path = dirs.workspace_dir.join("config.yaml");
    let entries = read_codex_api_key_entries(&config_path)?;
    let managed = read_managed_api_key_bindings(dirs)?;
    let modified_at = fs::metadata(&config_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    for binding in managed.bindings {
        if !entries
            .iter()
            .any(|entry| entry_matches_binding(entry, &binding))
        {
            continue;
        }
        let source = local_accounts
            .iter()
            .find(|account| account.id == binding.account_id);
        bound.push(ApiServiceBoundAccount {
            id: format!("apikey:{}", binding.account_id),
            account_id: Some(binding.account_id.clone()),
            account_id_exact: true,
            kind: "apikey".to_string(),
            label: source
                .map(api_service_account_label)
                .unwrap_or(binding.label),
            email: source.map(|account| account.email.clone()),
            base_url: Some(binding.base_url),
            path: display_path(&config_path),
            modified_at,
        });
    }
    bound.sort_by(|left, right| {
        left.kind.cmp(&right.kind).then_with(|| {
            left.label
                .to_ascii_lowercase()
                .cmp(&right.label.to_ascii_lowercase())
        })
    });
    Ok(bound)
}

fn delete_managed_api_key_bindings(
    dirs: &ApiServiceDirs,
    targets: &HashSet<String>,
) -> Result<usize, String> {
    let config_path = dirs.workspace_dir.join("config.yaml");
    let mut entries = read_codex_api_key_entries(&config_path)?;
    let mut managed = read_managed_api_key_bindings(dirs)?;
    let removed = managed
        .bindings
        .iter()
        .filter(|binding| targets.contains(&format!("apikey:{}", binding.account_id)))
        .cloned()
        .collect::<Vec<_>>();
    if removed.is_empty() {
        return Ok(0);
    }
    entries.retain(|entry| {
        !removed
            .iter()
            .any(|binding| binding.owns_config_entry && entry_matches_binding(entry, binding))
    });
    managed
        .bindings
        .retain(|binding| !targets.contains(&format!("apikey:{}", binding.account_id)));
    persist_managed_api_key_config(dirs, &entries, &managed)?;
    Ok(removed.len())
}

fn list_bound_auth_accounts(auth_dir: &Path) -> Result<Vec<ApiServiceBoundAccount>, String> {
    if !auth_dir.exists() {
        return Ok(Vec::new());
    }
    let mut accounts = Vec::new();
    for entry in
        fs::read_dir(auth_dir).map_err(|error| format!("读取 API 服务认证目录失败: {}", error))?
    {
        let path = entry
            .map_err(|error| format!("读取认证文件失败: {}", error))?
            .path();
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let value: serde_json::Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(email) = cpa_email_from_value(&value) else {
            continue;
        };
        let modified_at = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        let account_id = cpa_switcher_account_id_from_value(&value);
        accounts.push(ApiServiceBoundAccount {
            id: format!("oauth:{}", api_service_short_hash(&display_path(&path))),
            account_id_exact: account_id.is_some(),
            account_id,
            kind: "oauth".to_string(),
            label: email.clone(),
            email: Some(email),
            base_url: None,
            path: display_path(&path),
            modified_at,
        });
    }
    accounts.sort_by(|left, right| {
        left.label
            .to_ascii_lowercase()
            .cmp(&right.label.to_ascii_lowercase())
    });
    Ok(accounts)
}

fn cpa_email_from_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Array(items) => items.iter().find_map(cpa_email_from_value),
        serde_json::Value::Object(map) => map
            .get("email")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .map(ToString::to_string),
        _ => None,
    }
}

fn cpa_switcher_account_id_from_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Array(items) => {
            items.iter().find_map(cpa_switcher_account_id_from_value)
        }
        serde_json::Value::Object(map) => map
            .get("codex_switcher_account_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|account_id| !account_id.is_empty() && account_id.len() <= 128)
            .map(ToString::to_string),
        _ => None,
    }
}

fn cpa_chatgpt_account_id_from_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Array(items) => items.iter().find_map(cpa_chatgpt_account_id_from_value),
        serde_json::Value::Object(map) => map
            .get("account_id")
            .or_else(|| map.get("chatgpt_account_id"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|account_id| !account_id.is_empty() && account_id.len() <= 256)
            .map(ToString::to_string),
        _ => None,
    }
}

fn api_auth_matches_source_account(
    value: &serde_json::Value,
    source_id: &str,
    source_email: &str,
    source_chatgpt_account_id: Option<&str>,
    duplicate_email_count: usize,
) -> bool {
    let switcher_account_id = cpa_switcher_account_id_from_value(value);
    let chatgpt_account_id = cpa_chatgpt_account_id_from_value(value);
    if switcher_account_id.as_deref() == Some(source_id) {
        return true;
    }
    if source_chatgpt_account_id.is_some()
        && chatgpt_account_id.as_deref() == source_chatgpt_account_id
    {
        return true;
    }
    switcher_account_id.is_none()
        && chatgpt_account_id.is_none()
        && duplicate_email_count == 1
        && cpa_email_from_value(value)
            .as_deref()
            .is_some_and(|email| email.eq_ignore_ascii_case(source_email))
}

fn emit_download_progress(
    app: &AppHandle,
    status: &str,
    asset_name: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    message: Option<&str>,
) -> Result<(), String> {
    let _ = app.emit(
        DOWNLOAD_PROGRESS_EVENT,
        DownloadProgressEvent {
            status: status.to_string(),
            asset_name: asset_name.to_string(),
            downloaded_bytes,
            total_bytes,
            message: message.map(ToString::to_string),
        },
    );
    Ok(())
}

fn begin_download(
    download: &State<'_, ApiServiceDownloadState>,
    operation: &ApiServiceOperationState,
) -> Result<String, String> {
    let id = format!("{}-{}", std::process::id(), unix_timestamp()?);
    let mut guard = download
        .0
        .lock()
        .map_err(|_| "下载状态锁已损坏".to_string())?;
    ensure_not_shutting_down(operation)?;
    if guard.active_id.is_some() {
        return Err("已有下载任务正在进行".to_string());
    }
    guard.active_id = Some(id.clone());
    guard.cancel_requested = false;
    Ok(id)
}

fn clear_download(download: &State<'_, ApiServiceDownloadState>, download_id: &str) {
    if let Ok(mut guard) = download.0.lock() {
        if guard.active_id.as_deref() == Some(download_id) {
            guard.active_id = None;
            guard.cancel_requested = false;
        }
    }
}

fn is_download_cancelled(
    download: &State<'_, ApiServiceDownloadState>,
    download_id: &str,
) -> Result<bool, String> {
    let guard = download
        .0
        .lock()
        .map_err(|_| "下载状态锁已损坏".to_string())?;
    Ok(guard.active_id.as_deref() == Some(download_id) && guard.cancel_requested)
}

fn write_runtime_config(
    dirs: &ApiServiceDirs,
    runtime: &RuntimeInfoInternal,
    settings: &ApiServiceSettings,
) -> Result<PathBuf, String> {
    fs::create_dir_all(&dirs.workspace_dir)
        .map_err(|error| format!("创建 API 服务工作区失败: {}", error))?;
    let config_path = dirs.workspace_dir.join("config.yaml");
    let mut content = if config_path.exists() {
        fs::read_to_string(&config_path)
            .map_err(|error| format!("读取 API 服务配置失败: {}", error))?
    } else {
        fs::read_to_string(default_config_path(runtime)?)
            .map_err(|error| format!("读取 API 服务默认配置失败: {}", error))?
    };
    content = upsert_top_level_scalar(&content, "port", &settings.port.to_string());
    content = upsert_top_level_scalar(
        &content,
        "auth-dir",
        &format!(
            "\"{}\"",
            escape_yaml_double_quoted(&display_path(&api_auth_dir(dirs)))
        ),
    );
    content = upsert_top_level_sequence(&content, "api-keys", &settings.api_keys);
    content = upsert_nested_yaml_scalar(&content, "remote-management", "secret-key", "\"\"");
    fs::write(&config_path, content)
        .map_err(|error| format!("写入 API 服务配置失败: {}", error))?;
    Ok(config_path)
}

fn unpack_archive(package_path: &Path, target_dir: &Path) -> Result<(), String> {
    let file_name = package_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "版本包文件名无效".to_string())?;
    if file_name.ends_with(".zip") {
        return unpack_zip_archive(package_path, target_dir);
    }
    unpack_tar_gz_archive(package_path, target_dir)
}

fn unpack_tar_gz_archive(package_path: &Path, target_dir: &Path) -> Result<(), String> {
    let file = File::open(package_path).map_err(|error| format!("打开版本包失败: {}", error))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let mut entry_count = 0_usize;
    let mut extracted_bytes = 0_u64;
    for entry in archive
        .entries()
        .map_err(|error| format!("读取版本包失败: {}", error))?
    {
        entry_count += 1;
        if entry_count > MAX_ARCHIVE_ENTRIES {
            return Err(format!(
                "版本包条目数量超过安全限制（最大 {MAX_ARCHIVE_ENTRIES}）"
            ));
        }
        let mut entry = entry.map_err(|error| format!("读取版本包条目失败: {}", error))?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err("版本包不能包含符号链接或硬链接".to_string());
        }
        if !entry_type.is_file() && !entry_type.is_dir() {
            continue;
        }
        let entry_path = entry
            .path()
            .map_err(|error| format!("读取版本包路径失败: {}", error))?;
        let output_path = target_dir.join(safe_relative_path(&entry_path)?);
        if entry_type.is_dir() {
            fs::create_dir_all(&output_path).map_err(|error| format!("创建目录失败: {}", error))?;
        } else {
            extracted_bytes = checked_extracted_total(extracted_bytes, entry.size())?;
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|error| format!("创建目录失败: {}", error))?;
            }
            entry
                .unpack(&output_path)
                .map_err(|error| format!("解包文件失败: {}", error))?;
        }
    }
    Ok(())
}

fn unpack_zip_archive(package_path: &Path, target_dir: &Path) -> Result<(), String> {
    let file = File::open(package_path).map_err(|error| format!("打开版本包失败: {}", error))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("读取 zip 版本包失败: {}", error))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!(
            "版本包条目数量超过安全限制（最大 {MAX_ARCHIVE_ENTRIES}）"
        ));
    }
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("读取 zip 版本包条目失败: {}", error))?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("版本包不能包含符号链接".to_string());
        }
        let output_path = target_dir.join(safe_relative_path(Path::new(entry.name()))?);
        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(|error| format!("创建目录失败: {}", error))?;
            continue;
        }
        if !entry.is_file() {
            continue;
        }
        extracted_bytes = checked_extracted_total(extracted_bytes, entry.size())?;
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建目录失败: {}", error))?;
        }
        let mut output =
            File::create(&output_path).map_err(|error| format!("创建解包文件失败: {}", error))?;
        io::copy(&mut entry, &mut output)
            .map_err(|error| format!("解包 zip 文件失败: {}", error))?;
    }
    Ok(())
}

fn checked_extracted_total(current: u64, next: u64) -> Result<u64, String> {
    let total = current
        .checked_add(next)
        .ok_or_else(|| "版本包解压大小溢出".to_string())?;
    if total > MAX_EXTRACTED_RUNTIME_BYTES {
        return Err(format!(
            "版本包解压后超过安全大小限制（最大 {} MiB）",
            MAX_EXTRACTED_RUNTIME_BYTES / 1024 / 1024
        ));
    }
    Ok(total)
}

fn safe_relative_path(path: &Path) -> Result<PathBuf, String> {
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            _ => return Err(format!("版本包包含不安全路径: {}", display_path(path))),
        }
    }
    if safe.as_os_str().is_empty() {
        return Err("版本包包含空路径".to_string());
    }
    Ok(safe)
}

fn active_runtime(dirs: &ApiServiceDirs) -> Result<RuntimeInfoInternal, String> {
    let active_id =
        read_active_version(dirs)?.ok_or_else(|| "还没有可用的 API 服务版本".to_string())?;
    runtime_by_id(dirs, &active_id)
}

fn runtime_by_id(dirs: &ApiServiceDirs, id: &str) -> Result<RuntimeInfoInternal, String> {
    let runtime = compatible_runtime_by_id(dirs, id)?;
    let path = dirs.runtime_dir.join(&runtime.id);
    let binary_path = runtime_binary_path(&path);
    Ok(RuntimeInfoInternal { path, binary_path })
}

fn compatible_runtime_by_id(dirs: &ApiServiceDirs, id: &str) -> Result<RuntimeInfo, String> {
    let runtime = installed_runtime_by_id(dirs, id)?;
    if !runtime_target_is_compatible(&runtime.target) {
        return Err(format!(
            "API 服务版本平台不匹配: 当前平台需要 {}, 但版本是 {}",
            current_package_target_aliases().join(" 或 "),
            runtime.target
        ));
    }
    Ok(runtime)
}

fn installed_runtime_by_id(dirs: &ApiServiceDirs, id: &str) -> Result<RuntimeInfo, String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("API 服务版本 ID 不能为空".to_string());
    }
    let runtime = list_runtimes(dirs)?
        .into_iter()
        .find(|runtime| runtime.id == id)
        .ok_or_else(|| format!("API 服务版本不存在: {id}"))?;
    Ok(runtime)
}

fn latest_compatible_runtime(dirs: &ApiServiceDirs) -> Result<Option<RuntimeInfo>, String> {
    Ok(list_runtimes(dirs)?
        .into_iter()
        .find(|runtime| runtime_target_is_compatible(&runtime.target)))
}

fn list_runtimes(dirs: &ApiServiceDirs) -> Result<Vec<RuntimeInfo>, String> {
    if !dirs.runtime_dir.exists() {
        return Ok(Vec::new());
    }
    let mut runtimes = Vec::new();
    for entry in
        fs::read_dir(&dirs.runtime_dir).map_err(|error| format!("读取运行时目录失败: {}", error))?
    {
        let entry = entry.map_err(|error| format!("读取运行时条目失败: {}", error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("读取运行时条目类型失败: {error}"))?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if let Ok(runtime) = runtime_from_metadata(&path) {
            runtimes.push(runtime);
        }
    }
    runtimes.sort_by(compare_runtimes_newest_first);
    Ok(runtimes)
}

fn compare_runtimes_newest_first(left: &RuntimeInfo, right: &RuntimeInfo) -> CmpOrdering {
    match (
        parse_semantic_version(&left.version),
        parse_semantic_version(&right.version),
    ) {
        (Ok(left_version), Ok(right_version)) => right_version
            .cmp(&left_version)
            .then_with(|| right.installed_at.cmp(&left.installed_at))
            .then_with(|| left.id.cmp(&right.id)),
        (Ok(_), Err(_)) => CmpOrdering::Less,
        (Err(_), Ok(_)) => CmpOrdering::Greater,
        (Err(_), Err(_)) => right
            .installed_at
            .cmp(&left.installed_at)
            .then_with(|| left.id.cmp(&right.id)),
    }
}

fn retained_runtime_ids(
    runtimes: &[RuntimeInfo],
    active_id: &str,
    max_old_runtimes: usize,
) -> HashSet<String> {
    let mut sorted = runtimes.to_vec();
    sorted.sort_by(compare_runtimes_newest_first);
    let mut retained = HashSet::new();
    if sorted.iter().any(|runtime| runtime.id == active_id) {
        retained.insert(active_id.to_string());
    }
    for runtime in sorted
        .into_iter()
        .filter(|runtime| runtime.id != active_id)
        .take(max_old_runtimes)
    {
        retained.insert(runtime.id);
    }
    retained
}

fn prune_old_runtimes(dirs: &ApiServiceDirs) -> Result<(), String> {
    let runtimes = list_runtimes(dirs)?;
    if runtimes.is_empty() {
        return Ok(());
    }
    let active_id = read_active_version(dirs)?
        .ok_or_else(|| "当前 API 服务版本为空，已拒绝清理旧版本".to_string())?;
    compatible_runtime_by_id(dirs, &active_id)?;
    let compatible = runtimes
        .iter()
        .filter(|runtime| runtime_target_is_compatible(&runtime.target))
        .cloned()
        .collect::<Vec<_>>();
    let retained = retained_runtime_ids(&compatible, &active_id, MAX_OLD_RUNTIMES);
    for runtime in runtimes {
        if !retained.contains(&runtime.id) {
            remove_runtime_directory(dirs, &runtime)?;
        }
    }
    Ok(())
}

fn remove_runtime_directory(dirs: &ApiServiceDirs, runtime: &RuntimeInfo) -> Result<(), String> {
    let runtime_path = dirs.runtime_dir.join(&runtime.id);
    let metadata = fs::symlink_metadata(&runtime_path)
        .map_err(|error| format!("读取 API 服务版本目录失败: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("API 服务版本目录不能是符号链接".to_string());
    }
    if !metadata.is_dir() {
        return Err("API 服务版本路径不是目录".to_string());
    }
    let runtime_root = dirs
        .runtime_dir
        .canonicalize()
        .map_err(|error| format!("解析 API 服务运行时根目录失败: {error}"))?;
    let canonical_runtime = runtime_path
        .canonicalize()
        .map_err(|error| format!("解析 API 服务版本目录失败: {error}"))?;
    if canonical_runtime.parent() != Some(runtime_root.as_path()) {
        return Err("API 服务版本目录不在运行时根目录内，已拒绝删除".to_string());
    }
    let verified = runtime_from_metadata(&canonical_runtime)?;
    if verified.id != runtime.id {
        return Err("运行时目录校验失败，已拒绝删除".to_string());
    }
    fs::remove_dir_all(&canonical_runtime)
        .map_err(|error| format!("删除 API 服务版本 {} 失败: {error}", runtime.version))
}

fn ensure_runtime_can_be_deleted(
    dirs: &ApiServiceDirs,
    runtime: &RuntimeInfo,
) -> Result<(), String> {
    if read_active_version(dirs)?.as_deref() == Some(runtime.id.as_str()) {
        Err("当前使用中的 API 服务版本不能删除".to_string())
    } else {
        Ok(())
    }
}

fn runtime_from_metadata(path: &Path) -> Result<RuntimeInfo, String> {
    let metadata = runtime_metadata(path)?;
    let directory_id = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "运行时目录名称无效".to_string())?;
    if metadata.id != directory_id {
        return Err(format!(
            "运行时元数据 ID 与目录不一致: {} != {}",
            metadata.id, directory_id
        ));
    }
    parse_semantic_version(&metadata.version)?;
    if metadata.id != format!("{}_{}", metadata.version, metadata.target) {
        return Err("运行时元数据 ID、版本和平台不一致".to_string());
    }
    let binary_path = runtime_binary_path(path);
    ensure_runtime_binary_is_regular_file(&binary_path)?;
    Ok(RuntimeInfo {
        id: metadata.id,
        version: metadata.version,
        compatible: runtime_target_is_compatible(&metadata.target),
        target: metadata.target,
        path: display_path(path),
        binary_path: display_path(&binary_path),
        installed_at: metadata.installed_at,
        package_file: metadata.package_file,
    })
}

fn ensure_runtime_binary_is_regular_file(binary_path: &Path) -> Result<(), String> {
    ensure_regular_file(binary_path, "运行时可执行文件")
}

fn ensure_regular_file(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{label}不存在或无法读取 {}: {error}", display_path(path)))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{label}必须是普通文件: {}", display_path(path)));
    }
    Ok(())
}

fn runtime_metadata(path: &Path) -> Result<RuntimeMetadata, String> {
    let metadata_path = path.join("metadata.json");
    let content = fs::read_to_string(&metadata_path)
        .map_err(|error| format!("读取版本元数据失败: {}", error))?;
    serde_json::from_str(&content).map_err(|error| format!("解析版本元数据失败: {}", error))
}

fn ensure_default_config_cache(runtime_path: &Path) -> Result<PathBuf, String> {
    let cache_path = runtime_path.join(DEFAULT_CONFIG_CACHE);
    match fs::symlink_metadata(&cache_path) {
        Ok(_) => {
            ensure_regular_file(&cache_path, "默认配置缓存")?;
            return Ok(cache_path);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("读取默认配置缓存失败: {error}")),
    }
    let source = packaged_default_config_path(runtime_path)?;
    fs::copy(&source, &cache_path).map_err(|error| format!("缓存默认配置失败: {}", error))?;
    ensure_regular_file(&cache_path, "默认配置缓存")?;
    Ok(cache_path)
}

fn default_config_path(runtime: &RuntimeInfoInternal) -> Result<PathBuf, String> {
    ensure_default_config_cache(&runtime.path)
}

fn packaged_default_config_path(runtime_path: &Path) -> Result<PathBuf, String> {
    for name in ["config.yaml", "config.example.yaml"] {
        let path = runtime_path.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                ensure_regular_file(&path, "打包的默认配置文件")?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("读取打包的默认配置文件失败: {error}")),
        }
    }
    Err("当前版本缺少默认配置文件".to_string())
}

fn read_settings(dirs: &ApiServiceDirs) -> Result<ApiServiceSettings, String> {
    let content = match fs::read_to_string(&dirs.settings_path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ApiServiceSettings::default())
        }
        Err(error) => return Err(format!("读取 API 服务设置失败: {}", error)),
    };
    let mut settings: ApiServiceSettings = serde_json::from_str(&content)
        .map_err(|error| format!("解析 API 服务设置失败: {}", error))?;
    if settings.management_key.trim().is_empty() {
        settings.management_key = generate_management_key();
    }
    if !has_usable_api_keys(&settings.api_keys) {
        settings.api_keys = default_api_keys();
    }
    if settings.port == 0 {
        settings.port = DEFAULT_PORT;
    }
    if settings.auto_update_interval_hours == 0 {
        settings.auto_update_interval_hours = 24;
    }
    Ok(settings)
}

fn read_effective_settings(dirs: &ApiServiceDirs) -> Result<ApiServiceSettings, String> {
    let mut settings = read_settings(dirs)?;
    if let Some(api_keys) = read_config_api_keys(dirs)? {
        settings.api_keys = api_keys;
    }
    Ok(settings)
}

fn write_settings(dirs: &ApiServiceDirs, settings: &ApiServiceSettings) -> Result<(), String> {
    fs::create_dir_all(&dirs.base_dir)
        .map_err(|error| format!("创建 API 服务目录失败: {}", error))?;
    write_json(&dirs.settings_path, settings)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct InternalState {
    active_version: Option<String>,
    managed_pid: Option<u32>,
}

fn read_internal_state(dirs: &ApiServiceDirs) -> Result<InternalState, String> {
    let content = match fs::read_to_string(&dirs.state_path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(InternalState::default())
        }
        Err(error) => return Err(format!("读取 API 服务状态失败: {}", error)),
    };
    serde_json::from_str(&content).map_err(|error| format!("解析 API 服务状态失败: {}", error))
}

fn write_internal_state(dirs: &ApiServiceDirs, state: &InternalState) -> Result<(), String> {
    fs::create_dir_all(&dirs.base_dir)
        .map_err(|error| format!("创建 API 服务目录失败: {}", error))?;
    write_json(&dirs.state_path, state)
}

fn read_active_version(dirs: &ApiServiceDirs) -> Result<Option<String>, String> {
    Ok(read_internal_state(dirs)?.active_version)
}

fn write_active_version(
    dirs: &ApiServiceDirs,
    active_version: Option<String>,
) -> Result<(), String> {
    let mut state = read_internal_state(dirs)?;
    state.active_version = active_version;
    write_internal_state(dirs, &state)
}

fn read_managed_pid(dirs: &ApiServiceDirs) -> Result<Option<u32>, String> {
    Ok(read_internal_state(dirs)?.managed_pid)
}

fn write_managed_pid(dirs: &ApiServiceDirs, managed_pid: Option<u32>) -> Result<(), String> {
    let mut state = read_internal_state(dirs)?;
    state.managed_pid = managed_pid;
    write_internal_state(dirs, &state)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value)
        .map_err(|error| format!("序列化 JSON 失败: {}", error))?;
    write_bytes_atomic(path, content.as_bytes())
}

fn reject_unmanaged_port_listener(dirs: &ApiServiceDirs, port: u16) -> Result<(), String> {
    let Some(pid) = listener_pid_on_port(port)? else {
        return Ok(());
    };
    if pid_matches_managed_runtime(dirs, pid) {
        return Ok(());
    }
    Err(format!("端口 {} 已被其他进程 PID {} 占用", port, pid))
}

fn service_pid_for_state(
    process: &State<'_, ApiServiceProcessState>,
) -> Result<Option<u32>, String> {
    if let Some(pid) = process_pid(process)? {
        return Ok(Some(pid));
    }
    let dirs = ApiServiceDirs::new()?;
    let Some(pid) = read_managed_pid(&dirs)? else {
        return Ok(None);
    };
    if pid_is_running(pid) && pid_matches_managed_runtime(&dirs, pid) {
        return Ok(Some(pid));
    }
    Ok(None)
}

fn terminate_managed_pid(dirs: &ApiServiceDirs, pid: u32) -> Result<(), String> {
    if pid == std::process::id() || !pid_is_running(pid) {
        return Ok(());
    }
    if !pid_matches_managed_runtime(dirs, pid) {
        return Err(format!("拒绝停止非本应用托管的进程 PID {}", pid));
    }
    terminate_pid(pid)
}

fn process_pid(process: &State<'_, ApiServiceProcessState>) -> Result<Option<u32>, String> {
    let mut guard = process
        .0
        .lock()
        .map_err(|_| "服务状态锁已损坏".to_string())?;
    let Some(child) = guard.as_mut() else {
        return Ok(None);
    };
    match child
        .try_wait()
        .map_err(|error| format!("读取服务状态失败: {}", error))?
    {
        Some(_) => {
            *guard = None;
            Ok(None)
        }
        None => Ok(Some(child.id())),
    }
}

#[cfg(unix)]
pub(crate) fn listener_pid_on_port(port: u16) -> Result<Option<u32>, String> {
    let output = Command::new("lsof")
        .arg("-nP")
        .arg(format!("-iTCP:{port}"))
        .arg("-sTCP:LISTEN")
        .arg("-Fp")
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("检查端口监听失败: {}", error)),
    };
    if !output.status.success() {
        return Ok(None);
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(pid) = line.strip_prefix('p').and_then(|value| value.parse().ok()) {
            return Ok(Some(pid));
        }
    }
    Ok(None)
}

#[cfg(windows)]
pub(crate) fn listener_pid_on_port(port: u16) -> Result<Option<u32>, String> {
    let script = format!(
        "$c = Get-NetTCPConnection -LocalPort {} -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1; if ($null -ne $c) {{ [Console]::Out.Write($c.OwningProcess) }}",
        port
    );
    if let Ok(output) = windows_powershell_output(&script, "检查端口监听失败") {
        if output.status.success() {
            if let Ok(pid) = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u32>()
            {
                return Ok(Some(pid));
            }
        }
    }
    listener_pid_on_port_with_netstat(port)
}

#[cfg(all(not(unix), not(windows)))]
pub(crate) fn listener_pid_on_port(_port: u16) -> Result<Option<u32>, String> {
    Ok(None)
}

#[cfg(unix)]
fn pid_command(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("command=")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(windows)]
fn pid_command(pid: u32) -> Option<String> {
    let script = format!(
        "$p = Get-CimInstance Win32_Process -Filter \"ProcessId = {}\" -ErrorAction SilentlyContinue; if ($null -ne $p) {{ [Console]::Out.Write($p.CommandLine) }}",
        pid
    );
    let output = windows_powershell_output(&script, "读取进程命令失败").ok()?;
    if !output.status.success() {
        return None;
    }
    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!command.is_empty()).then_some(command)
}

#[cfg(all(not(unix), not(windows)))]
fn pid_command(_pid: u32) -> Option<String> {
    None
}

fn pid_matches_managed_runtime(dirs: &ApiServiceDirs, pid: u32) -> bool {
    let Some(command) = pid_command(pid) else {
        return false;
    };
    command.contains(BINARY_BASENAME)
        && (command.contains(&display_path(&dirs.runtime_dir))
            || command.contains(&display_path(&dirs.workspace_dir.join("config.yaml"))))
}

#[cfg(unix)]
fn pid_is_running(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn pid_is_running(pid: u32) -> bool {
    let script = format!(
        "if (Get-Process -Id {} -ErrorAction SilentlyContinue) {{ [Console]::Out.Write('1') }}",
        pid
    );
    windows_powershell_output(&script, "检查进程状态失败")
        .ok()
        .is_some_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "1"
        })
}

#[cfg(all(not(unix), not(windows)))]
fn pid_is_running(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
pub(crate) fn terminate_pid(pid: u32) -> Result<(), String> {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .map_err(|error| format!("停止端口占用进程失败: {}", error))?;
    if !status.success() {
        return Err(format!("停止端口占用进程失败: kill -TERM {}", pid));
    }
    for _ in 0..20 {
        if !pid_is_running(pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let status = Command::new("kill")
        .arg("-KILL")
        .arg(pid.to_string())
        .status()
        .map_err(|error| format!("强制停止端口占用进程失败: {}", error))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("强制停止端口占用进程失败: kill -KILL {}", pid))
}

#[cfg(windows)]
pub(crate) fn terminate_pid(pid: u32) -> Result<(), String> {
    let pid_text = pid.to_string();
    let mut command = Command::new("taskkill");
    command.args(["/PID", &pid_text, "/T", "/F"]);
    hide_command_window(&mut command);
    let status = command
        .status()
        .map_err(|error| format!("停止端口占用进程失败: {}", error))?;
    if !status.success() && pid_is_running(pid) {
        return Err(format!("停止端口占用进程失败: taskkill /PID {}", pid));
    }
    for _ in 0..20 {
        if !pid_is_running(pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!("停止端口占用进程超时: PID {}", pid))
}

#[cfg(all(not(unix), not(windows)))]
pub(crate) fn terminate_pid(_pid: u32) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn windows_powershell_output(
    script: &str,
    error_context: &str,
) -> Result<std::process::Output, String> {
    let mut command = Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
        ])
        .arg(script);
    hide_command_window(&mut command);
    command
        .output()
        .map_err(|error| format!("{}: {}", error_context, error))
}

#[cfg(windows)]
fn listener_pid_on_port_with_netstat(port: u16) -> Result<Option<u32>, String> {
    let mut command = Command::new("netstat");
    command.args(["-ano", "-p", "TCP"]);
    hide_command_window(&mut command);
    let output = command
        .output()
        .map_err(|error| format!("检查端口监听失败: {}", error))?;
    if !output.status.success() {
        return Ok(None);
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 5 || !parts[0].eq_ignore_ascii_case("TCP") {
            continue;
        }
        if !parts[3].eq_ignore_ascii_case("LISTENING") || !address_matches_port(parts[1], port) {
            continue;
        }
        if let Ok(pid) = parts[4].parse::<u32>() {
            return Ok(Some(pid));
        }
    }
    Ok(None)
}

#[cfg(windows)]
fn address_matches_port(address: &str, port: u16) -> bool {
    let expected = format!(":{}", port);
    address.ends_with(&expected) || address.ends_with(&format!("]:{}", port))
}

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_command_window(_command: &mut Command) {}

fn parse_package_info(path: &Path) -> Result<PackageInfo, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "版本包文件名无效".to_string())?;
    let base_name = file_name
        .strip_suffix(".tar.gz")
        .or_else(|| file_name.strip_suffix(".tgz"))
        .or_else(|| file_name.strip_suffix(".zip"))
        .ok_or_else(|| "版本包必须是 .tar.gz、.tgz 或 .zip 文件".to_string())?;
    let descriptor = base_name
        .strip_prefix("CLIProxyAPI_")
        .ok_or_else(|| "版本包命名需匹配 CLIProxyAPI_<version>_<os>_<arch>".to_string())?;
    let parts = descriptor.split('_').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err("版本包命名需匹配 CLIProxyAPI_<version>_<os>_<arch>".to_string());
    }
    let version = parse_semantic_version(parts[0])?.to_string();
    Ok(PackageInfo {
        id: format!("{}_{}_{}", version, parts[1], parts[2]),
        version,
        target: format!("{}_{}", parts[1], parts[2]),
    })
}

fn ensure_package_target_matches_current(package: &PackageInfo) -> Result<(), String> {
    if runtime_target_is_compatible(&package.target) {
        return Ok(());
    }
    Err(format!(
        "版本包平台不匹配: 当前平台需要 {}, 但包是 {}",
        current_package_target_aliases().join(" 或 "),
        package.target
    ))
}

fn ensure_package_matches_release(path: &Path, release: &GitHubRelease) -> Result<(), String> {
    let package = parse_package_info(path)?;
    let expected = parse_semantic_version(&release.tag_name)?;
    let actual = parse_semantic_version(&package.version)?;
    if actual != expected {
        return Err(format!(
            "版本包版本与发布标签不匹配: 发布版本 {expected}，安装包版本 {actual}"
        ));
    }
    Ok(())
}

fn runtime_binary_path(runtime_path: &Path) -> PathBuf {
    runtime_path.join(binary_name())
}

fn binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "cli-proxy-api.exe"
    } else {
        BINARY_BASENAME
    }
}

fn safe_download_file_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains('\\') {
        return Err("下载文件名不安全".to_string());
    }
    let path = Path::new(trimmed);
    let components = path.components().collect::<Vec<_>>();
    if components.len() != 1 || !matches!(components[0], Component::Normal(_)) {
        return Err("下载文件名不安全".to_string());
    }
    Ok(trimmed.to_string())
}

fn current_package_target() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => other,
    };
    format!("{os}_{arch}")
}

fn current_package_target_aliases() -> Vec<String> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let mut targets = vec![current_package_target()];
    if std::env::consts::ARCH == "aarch64" {
        let target = format!("{os}_aarch64");
        if !targets.iter().any(|item| item == &target) {
            targets.push(target);
        }
    }
    targets
}

fn runtime_target_is_compatible(target: &str) -> bool {
    current_package_target_aliases()
        .iter()
        .any(|expected| expected == target)
}

fn normalize_release_version(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).to_string()
}

fn parse_semantic_version(value: &str) -> Result<Version, String> {
    let normalized = normalize_release_version(value);
    Version::parse(&normalized).map_err(|error| format!("无效的语义化版本 {value:?}: {error}"))
}

fn versions_equal(left: &str, right: &str) -> bool {
    match (parse_semantic_version(left), parse_semantic_version(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn upsert_top_level_scalar(content: &str, key: &str, value: &str) -> String {
    let trailing_newline = content.ends_with('\n');
    let mut lines = content
        .replace("\r\n", "\n")
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if trailing_newline && lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let replacement = format!("{key}: {value}");
    for line in &mut lines {
        if line.trim_start().starts_with('#') {
            continue;
        }
        if let Some((indent, line_key)) = yaml_key_line(line) {
            if indent == 0 && line_key == key {
                *line = replacement;
                return finish_lines(lines, trailing_newline);
            }
        }
    }
    lines.insert(0, replacement);
    finish_lines(lines, trailing_newline)
}

fn upsert_nested_yaml_scalar(content: &str, section: &str, key: &str, value: &str) -> String {
    let trailing_newline = content.ends_with('\n');
    let mut lines = content
        .replace("\r\n", "\n")
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if trailing_newline && lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    for index in 0..lines.len() {
        if lines[index].trim_start().starts_with('#') {
            continue;
        }
        let Some((section_indent, section_key)) = yaml_key_line(&lines[index]) else {
            continue;
        };
        if section_key != section {
            continue;
        }
        let section_prefix = &lines[index][..section_indent];
        let child_prefix = format!("{section_prefix}  ");
        let replacement = format!("{child_prefix}{key}: {value}");
        for child_index in (index + 1)..lines.len() {
            let line = &lines[child_index];
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            let Some((child_indent, child_key)) = yaml_key_line(line) else {
                continue;
            };
            if child_indent <= section_indent {
                lines.insert(child_index, replacement);
                return finish_lines(lines, trailing_newline);
            }
            if child_key == key {
                lines[child_index] = replacement;
                return finish_lines(lines, trailing_newline);
            }
        }
        lines.push(replacement);
        return finish_lines(lines, trailing_newline);
    }
    lines.push(format!("{section}:"));
    lines.push(format!("  {key}: {value}"));
    finish_lines(lines, trailing_newline)
}

fn upsert_top_level_sequence(content: &str, key: &str, values: &[String]) -> String {
    let trailing_newline = content.ends_with('\n');
    let mut lines = content
        .replace("\r\n", "\n")
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if trailing_newline && lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }

    let replacement = yaml_sequence_lines(key, values);
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim_start().starts_with('#') {
            index += 1;
            continue;
        }
        let Some((indent, line_key)) = yaml_key_line(&lines[index]) else {
            index += 1;
            continue;
        };
        if indent != 0 || line_key != key {
            index += 1;
            continue;
        }

        let mut end = index + 1;
        while end < lines.len() {
            if lines[end].trim().is_empty() || lines[end].trim_start().starts_with('#') {
                end += 1;
                continue;
            }
            let Some((next_indent, _)) = yaml_key_line(&lines[end]) else {
                end += 1;
                continue;
            };
            if next_indent <= indent {
                break;
            }
            end += 1;
        }
        lines.splice(index..end, replacement);
        return finish_lines(lines, trailing_newline);
    }

    if !lines.is_empty() && !lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.push(String::new());
    }
    lines.extend(yaml_sequence_lines(key, values));
    finish_lines(lines, trailing_newline)
}

fn yaml_sequence_lines(key: &str, values: &[String]) -> Vec<String> {
    let mut lines = vec![format!("{key}:")];
    for value in values {
        lines.push(format!("  - \"{}\"", escape_yaml_double_quoted(value)));
    }
    lines
}

fn read_config_api_keys(dirs: &ApiServiceDirs) -> Result<Option<Vec<String>>, String> {
    let config_path = dirs.workspace_dir.join("config.yaml");
    let content = match fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("读取 API 服务配置失败: {}", error)),
    };
    let keys = read_top_level_sequence_values(&content, "api-keys");
    if has_usable_api_keys(&keys) {
        return Ok(Some(
            keys.into_iter()
                .filter(|key| !is_placeholder_api_key(key))
                .collect(),
        ));
    }
    Ok(None)
}

fn read_top_level_sequence_values(content: &str, key: &str) -> Vec<String> {
    let lines = content
        .replace("\r\n", "\n")
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let Some((indent, line_key)) = yaml_key_line(line) else {
            continue;
        };
        if indent != 0 || line_key != key {
            continue;
        }
        let mut values = Vec::new();
        for child in lines.iter().skip(index + 1) {
            let trimmed = child.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let child_indent = child.len() - child.trim_start().len();
            if child_indent <= indent {
                break;
            }
            if let Some(raw_value) = trimmed.strip_prefix('-') {
                let value = unquote_yaml_scalar(raw_value.trim());
                if !value.is_empty() {
                    values.push(value);
                }
            }
        }
        return values;
    }
    Vec::new()
}

fn normalize_api_keys(api_keys: Vec<String>) -> Result<Vec<String>, String> {
    let keys = api_keys
        .into_iter()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .collect::<Vec<_>>();
    if keys.is_empty() {
        return Err("至少需要保留一个 API 密钥".to_string());
    }
    for key in &keys {
        if key.contains('\r') || key.contains('\n') {
            return Err("API 密钥不能包含换行".to_string());
        }
        if is_placeholder_api_key(key) {
            return Err("默认示例 API 密钥不可用，请使用随机生成或自定义密钥".to_string());
        }
    }
    Ok(keys)
}

fn has_usable_api_keys(api_keys: &[String]) -> bool {
    api_keys
        .iter()
        .any(|key| !key.trim().is_empty() && !is_placeholder_api_key(key))
}

fn is_placeholder_api_key(key: &str) -> bool {
    matches!(
        key.trim().trim_matches('"').trim_matches('\''),
        "your-api-key-1" | "your-api-key-2" | "your-api-key-3"
    )
}

fn unquote_yaml_scalar(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        return trimmed[1..trimmed.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }
    if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        return trimmed[1..trimmed.len() - 1].replace("''", "'");
    }
    trimmed
        .split_once(" #")
        .map(|(head, _)| head)
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

fn escape_yaml_double_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn yaml_key_line(line: &str) -> Option<(usize, &str)> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim_start();
    let (key, _) = trimmed.split_once(':')?;
    Some((indent, key.trim()))
}

fn finish_lines(lines: Vec<String>, trailing_newline: bool) -> String {
    let mut content = lines.join("\n");
    if trailing_newline {
        content.push('\n');
    }
    content
}

fn generate_management_key() -> String {
    random_suffix(32)
}

fn random_suffix(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

fn default_api_keys() -> Vec<String> {
    (0..3).map(|_| generate_api_key()).collect()
}

fn generate_api_key() -> String {
    let suffix = random_suffix(40);
    format!("sk-cpa-{suffix}")
}

fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("读取系统时间失败: {}", error))
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("读取可执行文件权限失败: {}", error))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| format!("设置可执行权限失败: {}", error))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[derive(Debug, Clone)]
struct ApiServiceDirs {
    base_dir: PathBuf,
    runtime_dir: PathBuf,
    staging_dir: PathBuf,
    workspace_dir: PathBuf,
    downloads_dir: PathBuf,
    settings_path: PathBuf,
    state_path: PathBuf,
}

impl ApiServiceDirs {
    fn new() -> Result<Self, String> {
        let home = dirs::home_dir()
            .or_else(|| std::env::current_dir().ok())
            .ok_or_else(|| "无法定位用户主目录".to_string())?;
        Ok(Self::from_base_dir(
            home.join(".codex_switcher").join("api-service"),
        ))
    }

    fn from_base_dir(base_dir: PathBuf) -> Self {
        Self {
            runtime_dir: base_dir.join("runtimes"),
            staging_dir: base_dir.join("staging"),
            workspace_dir: base_dir.join("workspace"),
            downloads_dir: base_dir.join("downloads"),
            settings_path: base_dir.join("settings.json"),
            state_path: base_dir.join("state.json"),
            base_dir,
        }
    }
}

struct RuntimeInfoInternal {
    path: PathBuf,
    binary_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpa_switcher_account_id_metadata_is_strictly_validated() {
        let valid = serde_json::json!({
            "email": "member@example.com",
            "codex_switcher_account_id": "oauth-member"
        });
        assert_eq!(
            cpa_switcher_account_id_from_value(&valid).as_deref(),
            Some("oauth-member")
        );
        assert!(cpa_switcher_account_id_from_value(
            &serde_json::json!({ "codex_switcher_account_id": " " })
        )
        .is_none());
        assert!(cpa_switcher_account_id_from_value(
            &serde_json::json!({ "codex_switcher_account_id": "x".repeat(129) })
        )
        .is_none());
    }

    #[test]
    fn hidden_account_cleanup_matches_exact_or_legacy_identity_without_email_collisions() {
        let exact = serde_json::json!({
            "email": "shared@example.com",
            "account_id": "different-chatgpt",
            "codex_switcher_account_id": "source-one"
        });
        assert!(api_auth_matches_source_account(
            &exact,
            "source-one",
            "shared@example.com",
            Some("source-chatgpt"),
            2,
        ));

        let legacy = serde_json::json!({
            "email": "shared@example.com",
            "account_id": "source-chatgpt"
        });
        assert!(api_auth_matches_source_account(
            &legacy,
            "source-one",
            "shared@example.com",
            Some("source-chatgpt"),
            2,
        ));

        let other_identity = serde_json::json!({
            "email": "shared@example.com",
            "account_id": "other-chatgpt"
        });
        assert!(!api_auth_matches_source_account(
            &other_identity,
            "source-one",
            "shared@example.com",
            Some("source-chatgpt"),
            2,
        ));

        let identity_free = serde_json::json!({ "email": "shared@example.com" });
        assert!(!api_auth_matches_source_account(
            &identity_free,
            "source-one",
            "shared@example.com",
            None,
            2,
        ));
        assert!(api_auth_matches_source_account(
            &identity_free,
            "source-one",
            "shared@example.com",
            None,
            1,
        ));
    }

    fn test_runtime(version: &str, target: &str, installed_at: u64) -> RuntimeInfo {
        let id = format!("{version}_{target}");
        RuntimeInfo {
            id: id.clone(),
            version: version.to_string(),
            target: target.to_string(),
            compatible: runtime_target_is_compatible(target),
            path: format!("/tmp/{id}"),
            binary_path: format!("/tmp/{id}/{}", binary_name()),
            installed_at,
            package_file: format!("CLIProxyAPI_{version}_{target}.zip"),
        }
    }

    fn write_test_runtime(
        dirs: &ApiServiceDirs,
        version: &str,
        target: &str,
        installed_at: u64,
    ) -> RuntimeInfo {
        let runtime = test_runtime(version, target, installed_at);
        let runtime_path = dirs.runtime_dir.join(&runtime.id);
        fs::create_dir_all(&runtime_path).unwrap();
        fs::write(runtime_binary_path(&runtime_path), b"test binary").unwrap();
        fs::write(runtime_path.join("config.example.yaml"), b"port: 8317\n").unwrap();
        write_json(
            &runtime_path.join("metadata.json"),
            &RuntimeMetadata {
                id: runtime.id.clone(),
                version: runtime.version.clone(),
                target: runtime.target.clone(),
                installed_at,
                package_file: runtime.package_file.clone(),
            },
        )
        .unwrap();
        runtime_from_metadata(&runtime_path).unwrap()
    }

    fn test_dirs() -> (tempfile::TempDir, ApiServiceDirs) {
        let temporary = tempfile::tempdir().unwrap();
        let dirs = ApiServiceDirs::from_base_dir(temporary.path().join("api-service"));
        (temporary, dirs)
    }

    fn write_test_zip_package(path: &Path) {
        let file = File::create(path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default();
        archive.start_file(binary_name(), options).unwrap();
        archive.write_all(b"test binary").unwrap();
        archive.start_file("config.example.yaml", options).unwrap();
        archive.write_all(b"port: 8317\n").unwrap();
        archive.finish().unwrap();
    }

    fn write_test_zip_with_binary_directory(path: &Path) {
        let file = File::create(path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default();
        archive
            .add_directory(format!("{}/", binary_name()), options)
            .unwrap();
        archive.start_file("config.example.yaml", options).unwrap();
        archive.write_all(b"port: 8317\n").unwrap();
        archive.finish().unwrap();
    }

    fn write_test_zip_without_config(path: &Path) {
        let file = File::create(path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default();
        archive.start_file(binary_name(), options).unwrap();
        archive.write_all(b"test binary").unwrap();
        archive.finish().unwrap();
    }

    fn write_test_zip_with_config_cache_directory(path: &Path) {
        let file = File::create(path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default();
        archive.start_file(binary_name(), options).unwrap();
        archive.write_all(b"test binary").unwrap();
        archive
            .add_directory(format!("{DEFAULT_CONFIG_CACHE}/"), options)
            .unwrap();
        archive.finish().unwrap();
    }

    #[test]
    fn runtimes_are_sorted_by_semver_not_lexicographically() {
        let target = current_package_target();
        let mut runtimes = [
            test_runtime("7.2.9", &target, 30),
            test_runtime("7.2.10", &target, 10),
            test_runtime("7.2.10-rc.1", &target, 40),
        ];
        runtimes.sort_by(compare_runtimes_newest_first);
        assert_eq!(
            runtimes
                .iter()
                .map(|runtime| runtime.version.as_str())
                .collect::<Vec<_>>(),
            vec!["7.2.10", "7.2.10-rc.1", "7.2.9"]
        );
    }

    #[test]
    fn retention_keeps_active_and_two_newest_other_versions() {
        let (_temporary, dirs) = test_dirs();
        let target = current_package_target();
        let active = write_test_runtime(&dirs, "7.2.60", &target, 1);
        write_test_runtime(&dirs, "7.2.71", &target, 2);
        let previous = write_test_runtime(&dirs, "7.2.72", &target, 3);
        let latest = write_test_runtime(&dirs, "7.2.73", &target, 4);
        write_active_version(&dirs, Some(active.id.clone())).unwrap();

        prune_old_runtimes(&dirs).unwrap();

        let retained = list_runtimes(&dirs)
            .unwrap()
            .into_iter()
            .map(|runtime| runtime.id)
            .collect::<HashSet<_>>();
        assert_eq!(retained.len(), 3);
        assert!(retained.contains(&active.id));
        assert!(retained.contains(&previous.id));
        assert!(retained.contains(&latest.id));
    }

    #[test]
    fn latest_compatible_runtime_uses_semver_and_ignores_other_platforms() {
        let (_temporary, dirs) = test_dirs();
        let target = current_package_target();
        write_test_runtime(&dirs, "7.2.9", &target, 20);
        let latest = write_test_runtime(&dirs, "7.2.10", &target, 10);
        write_test_runtime(&dirs, "99.0.0", "unsupported_arch", 30);

        assert_eq!(
            latest_compatible_runtime(&dirs).unwrap().unwrap().id,
            latest.id
        );
    }

    #[test]
    fn local_package_is_snapshotted_before_installing() {
        let (temporary, dirs) = test_dirs();
        let target = current_package_target();
        let source = temporary
            .path()
            .join(format!("CLIProxyAPI_8.0.0_{target}.zip"));
        write_test_zip_package(&source);

        let snapshot = snapshot_local_runtime_package(&dirs, &source).unwrap();
        assert!(snapshot.path.starts_with(&dirs.staging_dir));
        assert_ne!(snapshot.path, source);
        let installed = install_runtime_package(&dirs, &snapshot.path, false).unwrap();
        assert_eq!(installed.version, "8.0.0");
        assert!(Path::new(&installed.binary_path).exists());
    }

    #[test]
    fn runtime_package_rejects_a_directory_in_place_of_the_binary() {
        let (temporary, dirs) = test_dirs();
        let target = current_package_target();
        let source = temporary
            .path()
            .join(format!("CLIProxyAPI_8.0.0_{target}.zip"));
        write_test_zip_with_binary_directory(&source);

        let error = install_runtime_package(&dirs, &source, false).unwrap_err();
        assert!(error.contains("必须是普通文件"));
        assert!(list_runtimes(&dirs).unwrap().is_empty());
    }

    #[test]
    fn failed_runtime_install_removes_staging_directory() {
        let (temporary, dirs) = test_dirs();
        let target = current_package_target();
        let source = temporary
            .path()
            .join(format!("CLIProxyAPI_8.0.0_{target}.zip"));
        write_test_zip_without_config(&source);

        let error = install_runtime_package(&dirs, &source, false).unwrap_err();
        assert!(error.contains("缺少默认配置文件"));
        assert_eq!(fs::read_dir(&dirs.staging_dir).unwrap().count(), 0);
        assert!(list_runtimes(&dirs).unwrap().is_empty());
    }

    #[test]
    fn runtime_package_rejects_a_directory_in_place_of_default_config() {
        let (temporary, dirs) = test_dirs();
        let target = current_package_target();
        let source = temporary
            .path()
            .join(format!("CLIProxyAPI_8.0.0_{target}.zip"));
        write_test_zip_with_config_cache_directory(&source);

        let error = install_runtime_package(&dirs, &source, false).unwrap_err();
        assert!(error.contains("默认配置缓存必须是普通文件"));
        assert_eq!(fs::read_dir(&dirs.staging_dir).unwrap().count(), 0);
        assert!(list_runtimes(&dirs).unwrap().is_empty());
    }

    #[test]
    fn local_package_snapshot_rejects_oversized_files() {
        let (temporary, dirs) = test_dirs();
        let target = current_package_target();
        let source = temporary
            .path()
            .join(format!("CLIProxyAPI_8.0.0_{target}.zip"));
        File::create(&source)
            .unwrap()
            .set_len(MAX_RUNTIME_ARCHIVE_BYTES + 1)
            .unwrap();

        let error = snapshot_local_runtime_package(&dirs, &source).unwrap_err();
        assert!(error.contains("超过安全大小限制"));
    }

    #[cfg(unix)]
    #[test]
    fn local_package_snapshot_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let (temporary, dirs) = test_dirs();
        let target = current_package_target();
        let actual = temporary.path().join("actual.zip");
        fs::write(&actual, b"not important").unwrap();
        let link = temporary
            .path()
            .join(format!("CLIProxyAPI_8.0.0_{target}.zip"));
        symlink(&actual, &link).unwrap();

        let error = snapshot_local_runtime_package(&dirs, &link).unwrap_err();
        assert!(error.contains("不能是符号链接"));
    }

    #[test]
    fn runtime_delete_rejects_paths_outside_runtime_root() {
        let (_temporary, dirs) = test_dirs();
        fs::create_dir_all(&dirs.runtime_dir).unwrap();
        let outside = dirs.base_dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        let malicious = RuntimeInfo {
            id: "../outside".to_string(),
            version: "8.0.0".to_string(),
            target: current_package_target(),
            compatible: true,
            path: display_path(&outside),
            binary_path: display_path(&runtime_binary_path(&outside)),
            installed_at: 1,
            package_file: "CLIProxyAPI_8.0.0_test.zip".to_string(),
        };

        let error = remove_runtime_directory(&dirs, &malicious).unwrap_err();
        assert!(error.contains("不在运行时根目录内"));
        assert!(outside.exists());
    }

    #[test]
    fn active_runtime_cannot_be_deleted() {
        let (_temporary, dirs) = test_dirs();
        let runtime = write_test_runtime(&dirs, "8.0.0", &current_package_target(), 1);
        write_active_version(&dirs, Some(runtime.id.clone())).unwrap();

        let error = ensure_runtime_can_be_deleted(&dirs, &runtime).unwrap_err();
        assert!(error.contains("不能删除"));
    }

    #[test]
    fn incompatible_runtime_can_still_be_selected_for_deletion() {
        let (_temporary, dirs) = test_dirs();
        let runtime = write_test_runtime(&dirs, "8.0.0", "unsupported_arch", 1);

        assert!(compatible_runtime_by_id(&dirs, &runtime.id).is_err());
        let selected = installed_runtime_by_id(&dirs, &runtime.id).unwrap();
        ensure_runtime_can_be_deleted(&dirs, &selected).unwrap();
        remove_runtime_directory(&dirs, &selected).unwrap();
        assert!(!dirs.runtime_dir.join(runtime.id).exists());
    }

    #[test]
    fn auto_update_due_requires_enabled_installed_runtime_and_elapsed_interval() {
        let mut settings = ApiServiceSettings::default();
        let now = 10_000;
        assert!(!auto_update_due(&settings, true, now));

        settings.auto_update = true;
        assert!(!auto_update_due(&settings, false, now));
        assert!(auto_update_due(&settings, true, now));

        settings.auto_update_interval_hours = 2;
        settings.last_update_check_at = Some(now - 7_199);
        assert!(!auto_update_due(&settings, true, now));
        settings.last_update_check_at = Some(now - 7_200);
        assert!(auto_update_due(&settings, true, now));
    }

    #[test]
    fn auto_update_due_clamps_interval_to_supported_range() {
        let mut settings = ApiServiceSettings {
            auto_update: true,
            auto_update_interval_hours: 0,
            last_update_check_at: Some(1_000),
            ..ApiServiceSettings::default()
        };
        assert!(!auto_update_due(&settings, true, 4_599));
        assert!(auto_update_due(&settings, true, 4_600));

        settings.auto_update_interval_hours = 10_000;
        let max_interval = 24 * 30 * 3600;
        assert!(!auto_update_due(&settings, true, 1_000 + max_interval - 1));
        assert!(auto_update_due(&settings, true, 1_000 + max_interval));
    }

    #[test]
    fn latest_version_is_only_applied_when_it_is_newer_or_current_is_missing() {
        assert!(should_apply_latest_version(None, "7.2.71").unwrap());
        assert!(should_apply_latest_version(Some("7.2.70"), "7.2.71").unwrap());
        assert!(!should_apply_latest_version(Some("7.2.71"), "7.2.71").unwrap());
        assert!(!should_apply_latest_version(Some("7.3.0"), "7.2.71").unwrap());
        assert!(should_apply_latest_version(Some("7.2.71-rc.1"), "7.2.71").unwrap());
        assert!(should_apply_latest_version(Some("7.2.71"), "7.2.72-rc.1").unwrap());
        assert!(!should_apply_latest_version(Some("7.2.72"), "7.2.72-rc.1").unwrap());
        assert!(should_apply_latest_version(Some("not-a-version"), "7.2.71").is_err());
        assert!(should_apply_latest_version(Some("7.2.71"), "release-latest").is_err());
    }

    #[test]
    fn release_asset_matching_requires_the_exact_version_and_target() {
        let mismatched = GitHubRelease {
            tag_name: "v7.2.71".to_string(),
            html_url: String::new(),
            assets: vec![GitHubAsset {
                name: "CLIProxyAPI_17.2.710_darwin_aarch64.tar.gz".to_string(),
                browser_download_url: "https://example.invalid/mismatch".to_string(),
            }],
        };
        assert!(
            release_asset_for_target(&mismatched, "7.2.71", &["darwin_aarch64".to_string()])
                .is_none()
        );

        let matching = GitHubRelease {
            tag_name: "v7.2.71".to_string(),
            html_url: String::new(),
            assets: vec![GitHubAsset {
                name: "CLIProxyAPI_7.2.71_darwin_aarch64.tar.gz".to_string(),
                browser_download_url: "https://example.invalid/match".to_string(),
            }],
        };
        assert!(
            release_asset_for_target(&matching, "7.2.71", &["darwin_aarch64".to_string()])
                .is_some()
        );
        assert!(ensure_package_matches_release(
            Path::new("CLIProxyAPI_17.2.710_darwin_aarch64.tar.gz"),
            &matching
        )
        .is_err());
    }

    #[test]
    fn extracted_runtime_size_is_bounded() {
        assert_eq!(
            checked_extracted_total(MAX_EXTRACTED_RUNTIME_BYTES - 1, 1).unwrap(),
            MAX_EXTRACTED_RUNTIME_BYTES
        );
        assert!(checked_extracted_total(MAX_EXTRACTED_RUNTIME_BYTES, 1).is_err());
        assert!(checked_extracted_total(u64::MAX, 1).is_err());
    }

    #[test]
    fn checksum_manifest_matches_only_the_exact_asset_name() {
        let manifest = concat!(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  CLIProxyAPI_7.2.71_darwin_aarch64.tar.gz\n",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  CLIProxyAPI_7.2.71_windows_amd64.zip\n",
        );
        assert_eq!(
            checksum_for_asset(manifest, "CLIProxyAPI_7.2.71_windows_amd64.zip").as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
        assert_eq!(checksum_for_asset(manifest, "CLIProxyAPI_7.2.71.zip"), None);
    }

    #[test]
    fn current_api_service_endpoint_detects_loopback_and_local_keys_on_the_active_port() {
        let settings = ApiServiceSettings {
            port: 17877,
            api_keys: vec!["local-client-key".to_string()],
            ..ApiServiceSettings::default()
        };

        assert!(is_current_api_service_endpoint(
            "http://localhost:17877/v1",
            Some("other-key"),
            &settings
        ));
        assert!(is_current_api_service_endpoint(
            "http://127.12.34.56:17877/v1",
            None,
            &settings
        ));
        assert!(is_current_api_service_endpoint(
            "http://[::1]:17877/v1",
            None,
            &settings
        ));
        assert!(is_current_api_service_endpoint(
            "http://192.0.2.10:17877/v1",
            Some("local-client-key"),
            &settings
        ));
        assert!(!is_current_api_service_endpoint(
            "http://192.0.2.10:17877/v1",
            Some("other-key"),
            &settings
        ));
        assert!(!is_current_api_service_endpoint(
            "http://localhost:8317/v1",
            Some("local-client-key"),
            &settings
        ));
    }

    #[test]
    fn managed_api_key_config_preserves_manual_entries_and_surrounding_yaml() {
        let (_temporary, dirs) = test_dirs();
        fs::create_dir_all(&dirs.workspace_dir).unwrap();
        let config_path = dirs.workspace_dir.join("config.yaml");
        fs::write(
            &config_path,
            concat!(
                "port: 17877\n",
                "codex-api-key:\n",
                "  - api-key: manual-key\n",
                "    base-url: https://manual.example/v1\n",
                "    prefix: manual\n",
                "    models:\n",
                "      - name: manual-model\n",
                "        display-name: Manual Model\n",
                "        force-mapping: true\n",
                "\n",
                "# Keep this provider section comment.\n",
                "openai-compatibility: []\n",
            ),
        )
        .unwrap();
        let mut entries = read_codex_api_key_entries(&config_path).unwrap();
        entries.push(CodexApiKeyConfigEntry {
            api_key: "managed-secret-key".to_string(),
            base_url: "https://managed.example/v1".to_string(),
            models: vec![CodexApiKeyConfigModel {
                name: "gpt-5.6-sol".to_string(),
                alias: String::new(),
                extra: BTreeMap::new(),
            }],
            extra: BTreeMap::new(),
        });
        let bindings = ManagedApiKeyBindings {
            bindings: vec![ManagedApiKeyBinding {
                account_id: "api_managed".to_string(),
                api_key_hash: api_service_hash("managed-secret-key"),
                base_url: "https://managed.example/v1".to_string(),
                label: "Managed Provider".to_string(),
                owns_config_entry: true,
            }],
        };

        persist_managed_api_key_config(&dirs, &entries, &bindings).unwrap();

        let updated = fs::read_to_string(&config_path).unwrap();
        assert!(updated.contains("# Keep this provider section comment."));
        assert!(updated.contains("openai-compatibility: []"));
        let parsed = read_codex_api_key_entries(&config_path).unwrap();
        assert_eq!(parsed.len(), 2);
        assert!(parsed.iter().any(|entry| {
            entry.api_key == "manual-key"
                && entry
                    .extra
                    .get("prefix")
                    .and_then(serde_yaml::Value::as_str)
                    == Some("manual")
        }));
        let manual_model = &parsed
            .iter()
            .find(|entry| entry.api_key == "manual-key")
            .unwrap()
            .models[0];
        assert_eq!(
            manual_model
                .extra
                .get("display-name")
                .and_then(serde_yaml::Value::as_str),
            Some("Manual Model")
        );
        assert_eq!(
            manual_model
                .extra
                .get("force-mapping")
                .and_then(serde_yaml::Value::as_bool),
            Some(true)
        );
        let manifest = fs::read_to_string(managed_api_key_bindings_path(&dirs)).unwrap();
        assert!(!manifest.contains("managed-secret-key"));
    }

    #[test]
    fn deleting_managed_api_key_binding_does_not_remove_manual_entries() {
        let (_temporary, dirs) = test_dirs();
        fs::create_dir_all(&dirs.workspace_dir).unwrap();
        let config_path = dirs.workspace_dir.join("config.yaml");
        fs::write(
            &config_path,
            concat!(
                "port: 17877\n",
                "codex-api-key:\n",
                "  - api-key: manual-key\n",
                "    base-url: https://manual.example/v1\n",
                "  - api-key: managed-key\n",
                "    base-url: https://managed.example/v1\n",
            ),
        )
        .unwrap();
        write_json(
            &managed_api_key_bindings_path(&dirs),
            &ManagedApiKeyBindings {
                bindings: vec![
                    ManagedApiKeyBinding {
                        account_id: "api_managed".to_string(),
                        api_key_hash: api_service_hash("managed-key"),
                        base_url: "https://managed.example/v1".to_string(),
                        label: "Managed Provider".to_string(),
                        owns_config_entry: true,
                    },
                    ManagedApiKeyBinding {
                        account_id: "api_manual".to_string(),
                        api_key_hash: api_service_hash("manual-key"),
                        base_url: "https://manual.example/v1".to_string(),
                        label: "Manual Provider".to_string(),
                        owns_config_entry: false,
                    },
                ],
            },
        )
        .unwrap();

        let removed = delete_managed_api_key_bindings(
            &dirs,
            &HashSet::from([
                "apikey:api_managed".to_string(),
                "apikey:api_manual".to_string(),
            ]),
        )
        .unwrap();

        assert_eq!(removed, 2);
        let entries = read_codex_api_key_entries(&config_path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].api_key, "manual-key");
        assert!(read_managed_api_key_bindings(&dirs)
            .unwrap()
            .bindings
            .is_empty());
    }

    #[test]
    fn replacing_auth_directory_swaps_complete_contents() {
        let (_temporary, dirs) = test_dirs();
        let auth_dir = api_auth_dir(&dirs);
        fs::create_dir_all(&auth_dir).unwrap();
        fs::write(auth_dir.join("old.json"), b"old").unwrap();
        let staging = dirs.base_dir.join("staging-auth");
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("new.json"), b"new").unwrap();

        replace_auth_directory(&auth_dir, &staging).unwrap();

        assert!(!auth_dir.join("old.json").exists());
        assert_eq!(fs::read(auth_dir.join("new.json")).unwrap(), b"new");
    }

    #[test]
    fn replacing_auth_directory_restores_existing_directory_on_failure() {
        let (_temporary, dirs) = test_dirs();
        let auth_dir = api_auth_dir(&dirs);
        fs::create_dir_all(&auth_dir).unwrap();
        fs::write(auth_dir.join("old.json"), b"old").unwrap();

        let error = replace_auth_directory(&auth_dir, &dirs.base_dir.join("missing-staging"))
            .expect_err("missing staging must fail");

        assert!(error.contains("替换 API 服务认证目录失败"));
        assert_eq!(fs::read(auth_dir.join("old.json")).unwrap(), b"old");
    }

    #[test]
    fn oauth_binding_file_hashes_are_distinct_and_bounded() {
        let first = format!("codex-switcher-{}.json", &api_service_hash("a/b")[..32]);
        let second = format!("codex-switcher-{}.json", &api_service_hash("a?b")[..32]);
        let long = format!(
            "codex-switcher-{}.json",
            &api_service_hash(&"account".repeat(10_000))[..32]
        );

        assert_ne!(first, second);
        assert_eq!(first.len(), "codex-switcher-".len() + 32 + ".json".len());
        assert_eq!(long.len(), first.len());
    }
}
