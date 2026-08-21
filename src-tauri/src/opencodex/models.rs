use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    pub desktop_version: String,
    pub engine_version: Option<String>,
    pub engine_source: String,
    pub platform: String,
    pub installed: bool,
    pub initialized: bool,
    pub running: bool,
    pub ready: bool,
    pub pid: Option<u32>,
    pub port: u16,
    pub dashboard_url: String,
    pub integration_status: String,
    pub bundled_runtime_available: bool,
    pub background_service: BackgroundServiceState,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundServiceState {
    pub supported: bool,
    pub installed: bool,
    pub enabled: bool,
    pub running: bool,
    pub viable: bool,
    pub stale: bool,
    pub conflict: bool,
    pub backend: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandAction {
    Init,
    Start,
    Stop,
    Restart,
    Status,
    Doctor,
    Sync,
    ServiceInstall,
    ServiceUninstall,
    Restore,
    Uninstall,
}

impl CommandAction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Status => "status",
            Self::Doctor => "doctor",
            Self::Sync => "sync",
            Self::ServiceInstall => "service_install",
            Self::ServiceUninstall => "service_uninstall",
            Self::Restore => "restore",
            Self::Uninstall => "uninstall",
        }
    }

    pub fn argv(&self, port: u16) -> Vec<String> {
        match self {
            Self::Init => vec!["init".into()],
            Self::Start => vec!["start".into(), "--port".into(), port.to_string()],
            Self::Stop => vec!["stop".into()],
            Self::Restart => vec!["restart".into()],
            Self::Status => vec!["status".into()],
            Self::Doctor => vec!["doctor".into()],
            Self::Sync => vec!["sync".into()],
            Self::ServiceInstall => vec!["service".into(), "install".into()],
            Self::ServiceUninstall => vec!["service".into(), "uninstall".into()],
            Self::Restore => vec!["restore".into()],
            Self::Uninstall => vec!["uninstall".into()],
        }
    }

    pub fn interactive(&self) -> bool {
        matches!(self, Self::Init)
    }
}

#[cfg(test)]
mod tests {
    use super::CommandAction;

    #[test]
    fn background_service_actions_use_explicit_install_and_uninstall_commands() {
        assert_eq!(
            CommandAction::ServiceInstall.argv(15800),
            ["service", "install"]
        );
        assert_eq!(
            CommandAction::ServiceUninstall.argv(15800),
            ["service", "uninstall"]
        );
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunActionRequest {
    pub action: CommandAction,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandStarted {
    pub operation_id: String,
    pub interactive: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandLogEvent {
    pub operation_id: String,
    pub stream: String,
    pub line: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandFinishedEvent {
    pub operation_id: String,
    pub action: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitcherAccountSummary {
    pub source_id: String,
    pub target_account_id: String,
    pub email: String,
    pub plan: Option<String>,
    pub current: bool,
    pub eligible: bool,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitcherAccountScan {
    pub source_path: String,
    pub total_count: usize,
    pub eligible_count: usize,
    pub accounts: Vec<SwitcherAccountSummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSwitcherAccountsRequest {
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitcherImportSkipped {
    pub source_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitcherImportResult {
    pub imported_count: usize,
    pub skipped_count: usize,
    pub imported: Vec<SwitcherAccountSummary>,
    pub skipped: Vec<SwitcherImportSkipped>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRelease {
    pub version: String,
    pub tag: String,
    pub name: String,
    pub prerelease: bool,
    pub published_at: String,
    pub url: String,
    #[serde(default)]
    pub newer_than_current: bool,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineUpdateCatalog {
    pub current_version: Option<String>,
    pub current_source: String,
    pub latest_stable: Option<EngineRelease>,
    pub latest_preview: Option<EngineRelease>,
    pub releases: Vec<EngineRelease>,
    pub installed_versions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallEngineVersionRequest {
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInstallResult {
    pub version: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthBody {
    pub service: Option<String>,
    pub version: Option<String>,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub status: Option<String>,
}
