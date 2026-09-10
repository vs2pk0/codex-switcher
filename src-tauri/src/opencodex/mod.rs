mod backend;
pub(crate) mod config_repair;
mod engine_switch;
mod isolation;
mod models;

use backend::Backend;
use models::{
    CommandStarted, DeleteEngineVersionRequest, DeleteSwitcherAccountRequest, EngineDeleteResult,
    EngineInstallResult, EngineUpdateCatalog, ImageGenerationSettings, ImageGenerationUpdate,
    ImageGenerationUpdateResult, ImportSwitcherAccountsRequest, InstallEngineVersionRequest,
    RunActionRequest, SwitcherAccountScan, SwitcherDeleteResult, SwitcherImportResult,
    SystemSnapshot, UpdateVisionModelsRequest, VisionModelCatalog, VisionModelsUpdateResult,
    VisionSidecarUpdate,
};
use std::sync::Arc;
use tauri::State;

pub type OpenCodexBackend = Backend;

#[tauri::command]
pub async fn opencodex_transfer_data(
    backend: State<'_, Arc<OpenCodexBackend>>,
    source_id: String,
    target_id: String,
    mode: String,
    history: bool,
    execute: bool,
    fingerprint: Option<String>,
) -> Result<serde_json::Value, String> {
    let source = backend.inner().for_instance(Some(&source_id))?;
    let target = backend.inner().for_instance(Some(&target_id))?;
    tauri::async_runtime::spawn_blocking(move || {
        source.transfer_data(&target, &mode, history, execute, fingerprint.as_deref())
    })
    .await
    .map_err(|e| format!("数据传输任务失败：{e}"))?
}

#[tauri::command]
pub async fn opencodex_get_system_snapshot(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
) -> Result<SystemSnapshot, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.snapshot())
        .await
        .map_err(|error| format!("读取 OpenCodex 状态任务失败：{error}"))
}

#[tauri::command]
pub fn opencodex_run_action(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    request: RunActionRequest,
) -> Result<CommandStarted, String> {
    backend
        .inner()
        .for_instance(request.instance_id.as_deref().or(instance_id.as_deref()))?
        .run_action(request)
}

#[tauri::command]
pub fn opencodex_write_command_input(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    operation_id: String,
    value: String,
) -> Result<(), String> {
    backend
        .inner()
        .for_instance(instance_id.as_deref())?
        .write_input(&operation_id, &value)
}

#[tauri::command]
pub fn opencodex_open_dashboard_window(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    port: u16,
) -> Result<(), String> {
    backend
        .inner()
        .for_instance(instance_id.as_deref())?
        .open_dashboard_window(port)
}

#[tauri::command]
pub fn opencodex_open_dashboard_browser(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    port: u16,
) -> Result<(), String> {
    backend
        .inner()
        .for_instance(instance_id.as_deref())?
        .open_dashboard_browser(port)
}

#[tauri::command]
pub async fn opencodex_read_manager_logs(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    limit: usize,
) -> Result<Vec<String>, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.read_logs(limit))
        .await
        .map_err(|error| format!("读取 OpenCodex 日志任务失败：{error}"))
}

#[tauri::command]
pub async fn opencodex_scan_switcher_accounts(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
) -> Result<SwitcherAccountScan, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.scan_codex_switcher_accounts())
        .await
        .map_err(|error| format!("扫描 OpenCodex 账号任务失败：{error}"))?
}

#[tauri::command]
pub async fn opencodex_import_switcher_accounts(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    request: ImportSwitcherAccountsRequest,
) -> Result<SwitcherImportResult, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.import_codex_switcher_accounts(request))
        .await
        .map_err(|error| format!("导入 OpenCodex 账号任务失败：{error}"))?
}

#[tauri::command]
pub async fn opencodex_bind_switcher_accounts(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    request: ImportSwitcherAccountsRequest,
) -> Result<SwitcherImportResult, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.bind_codex_switcher_accounts(request))
        .await
        .map_err(|error| format!("绑定 OpenCodex 账号任务失败：{error}"))?
}

#[tauri::command]
pub fn opencodex_delete_switcher_account(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    request: DeleteSwitcherAccountRequest,
) -> Result<SwitcherDeleteResult, String> {
    backend
        .inner()
        .for_instance(instance_id.as_deref())?
        .delete_codex_switcher_account(request)
}

#[tauri::command]
pub async fn opencodex_get_engine_update_catalog(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
) -> Result<EngineUpdateCatalog, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.get_engine_update_catalog())
        .await
        .map_err(|error| format!("检测 OpenCodex Engine 更新任务失败：{error}"))?
}

#[tauri::command]
pub async fn opencodex_install_engine_version(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    request: InstallEngineVersionRequest,
) -> Result<EngineInstallResult, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.install_engine_version(request))
        .await
        .map_err(|error| format!("安装 OpenCodex Engine 任务失败：{error}"))?
}

#[tauri::command]
pub async fn opencodex_delete_engine_version(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    request: DeleteEngineVersionRequest,
) -> Result<EngineDeleteResult, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.delete_engine_version(request))
        .await
        .map_err(|error| format!("删除 Engine 任务失败：{error}"))?
}

#[tauri::command]
pub fn opencodex_get_vision_models(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
) -> Result<VisionModelCatalog, String> {
    backend
        .inner()
        .for_instance(instance_id.as_deref())?
        .get_vision_models()
}

#[tauri::command]
pub fn opencodex_update_vision_models(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    request: UpdateVisionModelsRequest,
) -> Result<VisionModelsUpdateResult, String> {
    backend
        .inner()
        .for_instance(request.instance_id.as_deref().or(instance_id.as_deref()))?
        .update_vision_models(request)
}

#[tauri::command]
pub async fn opencodex_get_vision_sidecar_settings(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.vision_sidecar_settings(None))
        .await
        .map_err(|error| format!("读取图片描述设置任务失败：{error}"))?
}

#[tauri::command]
pub async fn opencodex_get_image_generation_settings(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
) -> Result<ImageGenerationSettings, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.image_generation_settings())
        .await
        .map_err(|error| format!("读取图片生成设置任务失败：{error}"))?
}

#[tauri::command]
pub async fn opencodex_update_image_generation_settings(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    request: ImageGenerationUpdate,
) -> Result<ImageGenerationUpdateResult, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.update_image_generation_settings(request))
        .await
        .map_err(|error| format!("保存图片生成设置任务失败：{error}"))?
}

#[tauri::command]
pub async fn opencodex_update_vision_sidecar_settings(
    backend: State<'_, Arc<OpenCodexBackend>>,
    instance_id: Option<String>,
    request: VisionSidecarUpdate,
) -> Result<serde_json::Value, String> {
    let backend = backend.inner().for_instance(instance_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || backend.vision_sidecar_settings(Some(request)))
        .await
        .map_err(|error| format!("保存图片描述设置任务失败：{error}"))?
}
