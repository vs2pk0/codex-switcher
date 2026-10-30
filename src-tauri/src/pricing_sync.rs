use crate::account::write_bytes_atomic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;

static CONFIG_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ModelsDevSyncConfig {
    pub auto_sync_enabled: bool,
    pub include_common_models: bool,
    pub selected_model_keys: Vec<String>,
    pub excluded_common_model_keys: Vec<String>,
    pub last_sync_at: Option<i64>,
    pub last_sync_error: Option<String>,
}

impl Default for ModelsDevSyncConfig {
    fn default() -> Self {
        Self {
            auto_sync_enabled: false,
            include_common_models: true,
            selected_model_keys: vec![],
            excluded_common_model_keys: vec![],
            last_sync_at: None,
            last_sync_error: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelsDevSyncState {
    config: ModelsDevSyncConfig,
    config_path: String,
}

fn path() -> PathBuf {
    crate::switcher_statistics_dir().join("models-dev-sync.json")
}
fn read() -> Result<ModelsDevSyncConfig, String> {
    match std::fs::read(path()) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| format!("解析 models.dev 同步配置失败: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(ModelsDevSyncConfig::default())
        }
        Err(error) => Err(format!("读取 models.dev 同步配置失败: {error}")),
    }
}
fn write(config: &ModelsDevSyncConfig) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(config).map_err(|error| error.to_string())?;
    write_bytes_atomic(&path(), &bytes)
}
fn state(config: ModelsDevSyncConfig) -> ModelsDevSyncState {
    ModelsDevSyncState {
        config,
        config_path: path().to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub(crate) fn codex_get_models_dev_sync_config() -> Result<ModelsDevSyncState, String> {
    let _guard = CONFIG_LOCK.lock().map_err(|error| error.to_string())?;
    let config = read()?;
    // Make the editable local configuration and its folder available on first use.
    if !path().exists() {
        write(&config)?;
    }
    Ok(state(config))
}

#[tauri::command]
pub(crate) fn codex_update_models_dev_sync_config(
    mut config: ModelsDevSyncConfig,
) -> Result<ModelsDevSyncState, String> {
    let _guard = CONFIG_LOCK.lock().map_err(|error| error.to_string())?;
    let previous = read()?;
    config.last_sync_at = previous.last_sync_at;
    config.last_sync_error = previous.last_sync_error;
    for keys in [
        &mut config.selected_model_keys,
        &mut config.excluded_common_model_keys,
    ] {
        keys.retain(|key| !key.trim().is_empty());
        keys.sort();
        keys.dedup();
    }
    write(&config)?;
    Ok(state(config))
}

#[tauri::command]
pub(crate) fn codex_record_models_dev_sync_result(
    synced_at: Option<i64>,
    error: Option<String>,
) -> Result<ModelsDevSyncState, String> {
    let _guard = CONFIG_LOCK.lock().map_err(|error| error.to_string())?;
    let mut config = read()?;
    if synced_at.is_some() {
        config.last_sync_at = synced_at;
    }
    config.last_sync_error = error;
    write(&config)?;
    Ok(state(config))
}

#[tauri::command]
pub(crate) async fn codex_fetch_models_dev() -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("Codex-Switcher/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get("https://models.dev/api.json")
        .send()
        .await
        .map_err(|error| format!("获取 models.dev 失败: {error}"))?
        .error_for_status()
        .map_err(|error| format!("获取 models.dev 失败: {error}"))?;
    let data: Value = response
        .json()
        .await
        .map_err(|error| format!("解析 models.dev 失败: {error}"))?;
    if !data.is_object() {
        return Err("models.dev 返回的数据格式无效".into());
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_partial_config_remain_compatible() {
        let config: ModelsDevSyncConfig =
            serde_json::from_str("{\"selectedModelKeys\":[\"openai/gpt-6-astra\"]}").unwrap();
        assert!(!config.auto_sync_enabled);
        assert!(config.include_common_models);
        assert_eq!(config.selected_model_keys.len(), 1);
        assert_eq!(config.last_sync_at, None);
    }
}
