mod backend;
mod models;

use backend::Backend;
use models::{
    CommandStarted, DeleteEngineVersionRequest, DeleteSwitcherAccountRequest, EngineDeleteResult,
    EngineInstallResult, EngineUpdateCatalog, ImportSwitcherAccountsRequest,
    InstallEngineVersionRequest, RunActionRequest, SwitcherAccountScan, SwitcherDeleteResult,
    SwitcherImportResult, SystemSnapshot, UpdateVisionModelsRequest, VisionModelCatalog,
    VisionModelsUpdateResult,
};
use std::sync::Arc;
use tauri::State;

pub type OpenCodexBackend = Backend;

#[tauri::command]
pub async fn opencodex_get_system_snapshot(
    backend: State<'_, Arc<OpenCodexBackend>>,
) -> Result<SystemSnapshot, String> {
    let backend = Arc::clone(backend.inner());
    tauri::async_runtime::spawn_blocking(move || backend.snapshot())
        .await
        .map_err(|error| format!("读取 OpenCodex 状态任务失败：{error}"))
}

#[tauri::command]
pub fn opencodex_run_action(
    backend: State<'_, Arc<OpenCodexBackend>>,
    request: RunActionRequest,
) -> Result<CommandStarted, String> {
    backend.inner().run_action(request)
}

#[tauri::command]
pub fn opencodex_write_command_input(
    backend: State<'_, Arc<OpenCodexBackend>>,
    operation_id: String,
    value: String,
) -> Result<(), String> {
    backend.write_input(&operation_id, &value)
}

#[tauri::command]
pub fn opencodex_open_dashboard_window(
    backend: State<'_, Arc<OpenCodexBackend>>,
    port: u16,
) -> Result<(), String> {
    backend.open_dashboard_window(port)
}

#[tauri::command]
pub fn opencodex_open_dashboard_browser(
    backend: State<'_, Arc<OpenCodexBackend>>,
    port: u16,
) -> Result<(), String> {
    backend.open_dashboard_browser(port)
}

#[tauri::command]
pub async fn opencodex_read_manager_logs(
    backend: State<'_, Arc<OpenCodexBackend>>,
    limit: usize,
) -> Result<Vec<String>, String> {
    let backend = Arc::clone(backend.inner());
    tauri::async_runtime::spawn_blocking(move || backend.read_logs(limit))
        .await
        .map_err(|error| format!("读取 OpenCodex 日志任务失败：{error}"))
}

#[tauri::command]
pub async fn opencodex_scan_switcher_accounts(
    backend: State<'_, Arc<OpenCodexBackend>>,
) -> Result<SwitcherAccountScan, String> {
    let backend = Arc::clone(backend.inner());
    tauri::async_runtime::spawn_blocking(move || backend.scan_codex_switcher_accounts())
        .await
        .map_err(|error| format!("扫描 OpenCodex 账号任务失败：{error}"))?
}

#[tauri::command]
pub async fn opencodex_import_switcher_accounts(
    backend: State<'_, Arc<OpenCodexBackend>>,
    request: ImportSwitcherAccountsRequest,
) -> Result<SwitcherImportResult, String> {
    let backend = Arc::clone(backend.inner());
    tauri::async_runtime::spawn_blocking(move || backend.import_codex_switcher_accounts(request))
        .await
        .map_err(|error| format!("导入 OpenCodex 账号任务失败：{error}"))?
}

#[tauri::command]
pub async fn opencodex_bind_switcher_accounts(
    backend: State<'_, Arc<OpenCodexBackend>>,
    request: ImportSwitcherAccountsRequest,
) -> Result<SwitcherImportResult, String> {
    let backend = Arc::clone(backend.inner());
    tauri::async_runtime::spawn_blocking(move || backend.bind_codex_switcher_accounts(request))
        .await
        .map_err(|error| format!("绑定 OpenCodex 账号任务失败：{error}"))?
}

#[tauri::command]
pub fn opencodex_delete_switcher_account(
    backend: State<'_, Arc<OpenCodexBackend>>,
    request: DeleteSwitcherAccountRequest,
) -> Result<SwitcherDeleteResult, String> {
    backend.delete_codex_switcher_account(request)
}

#[tauri::command]
pub fn opencodex_get_engine_update_catalog(
    backend: State<'_, Arc<OpenCodexBackend>>,
) -> Result<EngineUpdateCatalog, String> {
    backend.get_engine_update_catalog()
}

#[tauri::command]
pub fn opencodex_install_engine_version(
    backend: State<'_, Arc<OpenCodexBackend>>,
    request: InstallEngineVersionRequest,
) -> Result<EngineInstallResult, String> {
    backend.install_engine_version(request)
}

#[tauri::command]
pub fn opencodex_activate_bundled_engine(
    backend: State<'_, Arc<OpenCodexBackend>>,
) -> Result<EngineInstallResult, String> {
    backend.activate_bundled_engine()
}

#[tauri::command]
pub fn opencodex_delete_engine_version(
    backend: State<'_, Arc<OpenCodexBackend>>,
    request: DeleteEngineVersionRequest,
) -> Result<EngineDeleteResult, String> {
    backend.delete_engine_version(request)
}

#[tauri::command]
pub fn opencodex_get_vision_models(
    backend: State<'_, Arc<OpenCodexBackend>>,
) -> Result<VisionModelCatalog, String> {
    backend.get_vision_models()
}

#[tauri::command]
pub fn opencodex_update_vision_models(
    backend: State<'_, Arc<OpenCodexBackend>>,
    request: UpdateVisionModelsRequest,
) -> Result<VisionModelsUpdateResult, String> {
    backend.update_vision_models(request)
}
