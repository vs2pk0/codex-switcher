mod backend;
mod models;

use backend::Backend;
use models::{
    CommandStarted, EngineInstallResult, EngineUpdateCatalog, ImportSwitcherAccountsRequest,
    InstallEngineVersionRequest, RunActionRequest, SwitcherAccountScan, SwitcherImportResult,
    SystemSnapshot,
};
use std::sync::Arc;
use tauri::State;

pub type OpenCodexBackend = Backend;

#[tauri::command]
pub fn opencodex_get_system_snapshot(backend: State<'_, Arc<OpenCodexBackend>>) -> SystemSnapshot {
    backend.snapshot()
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
pub fn opencodex_read_manager_logs(
    backend: State<'_, Arc<OpenCodexBackend>>,
    limit: usize,
) -> Vec<String> {
    backend.read_logs(limit)
}

#[tauri::command]
pub fn opencodex_scan_switcher_accounts(
    backend: State<'_, Arc<OpenCodexBackend>>,
) -> Result<SwitcherAccountScan, String> {
    backend.scan_codex_switcher_accounts()
}

#[tauri::command]
pub fn opencodex_import_switcher_accounts(
    backend: State<'_, Arc<OpenCodexBackend>>,
    request: ImportSwitcherAccountsRequest,
) -> Result<SwitcherImportResult, String> {
    backend.import_codex_switcher_accounts(request)
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
