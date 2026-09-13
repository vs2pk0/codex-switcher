use super::engine_switch::{switch_engine, SwitchStep};
use super::models::{
    BackgroundServiceState, CommandAction, CommandFinishedEvent, CommandLogEvent, CommandStarted,
    DeleteEngineVersionRequest, DeleteSwitcherAccountRequest, EngineDeleteResult,
    EngineInstallResult, EngineProgress, EngineRelease, EngineUpdateCatalog, HealthBody,
    ImageGenerationProviderOption, ImageGenerationRecentRequest, ImageGenerationSettings,
    ImageGenerationUpdate, ImageGenerationUpdateResult, ImportSwitcherAccountsRequest,
    InstallEngineVersionRequest, RunActionRequest, SwitcherAccountScan, SwitcherDeleteResult,
    SwitcherImportResult, SystemSnapshot, UpdateVisionModelsRequest, VisionModel,
    VisionModelCatalog, VisionModelsUpdateResult, VisionSidecarUpdate,
};
use chrono::Utc;
use once_cell::sync::Lazy;
use regex::Regex;
use semver::Version;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager as _, WebviewUrl, WebviewWindowBuilder};

const DEFAULT_PORT: u16 = 15800;
const START_TIMEOUT: Duration = Duration::from_secs(35);
const HTTP_TIMEOUT: Duration = Duration::from_millis(750);
const MANAGED_ENGINE_HISTORY_LIMIT: usize = 3;
const MANAGER_LOG_TAIL_MAX_BYTES: u64 = 2 * 1024 * 1024;
/// 图片生成镜像提供方的名称后缀：源提供方不是 openai-responses 时，用同一 baseUrl/apiKey 创建镜像承接 /v1/images。
const IMAGE_MIRROR_SUFFIX: &str = "-images";
/// Engine 对 images.timeoutMs 的上限（对应 MAX_IMAGE_TIMEOUT_MS）。
const IMAGE_TIMEOUT_MAX_MS: u64 = 300_000;
const IMAGE_TIMEOUT_MIN_MS: u64 = 5_000;
/// 图片生成可复用的提供方适配器：openai-responses 可直连，openai-chat 需镜像。
const IMAGE_DIRECT_ADAPTER: &str = "openai-responses";
const IMAGE_MIRRORABLE_ADAPTERS: &[&str] = &[IMAGE_DIRECT_ADAPTER, "openai-chat"];
/// Engine 内置注册表中的提供方 id，不能作为 images.provider（Engine 会返回 400）。
const ENGINE_BUILTIN_PROVIDER_IDS: &[&str] = &[
    "openai",
    "openai-apikey",
    "cursor",
    "xai",
    "command-code",
    "commandcode",
    "anthropic",
    "anthropic-apikey",
    "kimi",
    "kimi-code",
    "kiro",
    "nous",
    "meta-model",
    "meta-muse",
    "umans",
    "opencode-go",
    "opencode-zen",
    "opencode-free",
    "neuralwatt",
    "openrouter",
    "cline-pass",
    "cline",
    "orcarouter",
    "bizrouter",
    "google",
    "deepseek",
    "chutes",
    "deepinfra",
    "hyperbolic",
    "nscale",
    "vultr",
    "baseten",
    "sambanova",
    "nebius",
    "digitalocean",
    "scaleway",
    "featherless",
    "novita",
    "firepass",
    "moonshot",
    "nvidia",
    "zai",
    "zhipu-bigmodel",
    "zhipu-bigmodel-coding",
    "siliconflow",
    "qwen-cloud",
    "tencent-coding-plan",
    "volcengine",
    "volcengine-coding-plan",
    "volcengine-agent-plan",
    "alibaba-token-plan",
    "alibaba-token-plan-intl",
    "zenmux",
    "litellm",
    "ollama-cloud",
    "minimax",
    "minimax-cn",
    "xiaomi-mimo",
    "mimo-free",
    "mimo",
    "cloudflare-workers-ai",
    "github-copilot",
];

static SECRET_FIELD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(api[_-]?key|access[_-]?token|refresh[_-]?token|authorization|password)(\s*[:=]\s*)([^\s,;]+)")
        .expect("valid secret-field regex")
});
static BEARER_TOKEN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)bearer\s+[A-Za-z0-9._~+\-/]+=*").expect("valid bearer regex"));
static COMMON_TOKEN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:sk|ghp|github_pat)-?[A-Za-z0-9_\-]{16,}\b").expect("valid token regex")
});

#[derive(Clone)]
struct Launcher {
    program: PathBuf,
    prefix_args: Vec<OsString>,
    working_dir: Option<PathBuf>,
    version: Option<String>,
    source: &'static str,
}

#[derive(Debug, serde::Deserialize)]
struct InstanceIntegrationResult {
    success: bool,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccountBindingRestartMode {
    BackgroundService,
    Standalone,
}

struct InteractiveProcess {
    operation_id: String,
    stdin: ChildStdin,
}

#[derive(serde::Deserialize)]
struct RemoteEngineCatalog {
    releases: Vec<EngineRelease>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineHelperInstallResult {
    version: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagementModelRow {
    provider: String,
    id: String,
    namespaced: String,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    native: bool,
    #[serde(default)]
    input_modalities: Vec<String>,
}

pub struct Backend {
    app: AppHandle,
    root: PathBuf,
    instance_id: String,
    home: PathBuf,
    codex_home: PathBuf,
    default_port: u16,
    children: Mutex<HashMap<String, Arc<Backend>>>,
    operation_busy: AtomicBool,
    interactive: Mutex<Option<InteractiveProcess>>,
    sequence: AtomicU64,
}

impl Backend {
    pub fn delete_instance(
        self: &Arc<Self>,
        id: &str,
    ) -> Result<crate::instances::DeleteCodexInstanceResult, String> {
        if id == "default" {
            return Err("系统默认实例不能删除".into());
        }
        let instance = self.for_instance(Some(id))?;
        instance.begin_mutation()?;
        let result = (|| {
            if let Ok(launcher) = instance.active_launcher() {
                let service = instance.query_background_service_state()?;
                if service.conflict {
                    return Err("该实例后台服务状态存在冲突，请先修复再删除".into());
                }
                if let Some(port) = instance.running_open_codex_port() {
                    instance.stop_for_account_binding(&launcher, port)?;
                }
                if service.installed {
                    let output = instance
                        .command(&launcher, &["service".into(), "uninstall".into()])
                        .stdin(Stdio::null())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .output()
                        .map_err(|e| e.to_string())?;
                    if !output.status.success() {
                        return Err(format!(
                            "移除实例后台服务失败：{}",
                            instance.redact(String::from_utf8_lossy(&output.stderr).trim())
                        ));
                    }
                    if instance.query_background_service_state()?.installed {
                        return Err("实例后台服务尚未移除，未删除数据".into());
                    }
                }
            } else if instance.home.join("service-state.json").exists()
                || instance.read_runtime_port().is_some()
            {
                return Err(
                    "该实例仍有运行记录，但 Engine 不可用；请重新激活 Engine 并停止服务后删除"
                        .into(),
                );
            }
            crate::instances::delete_codex_instance(id.to_string())
        })();
        // Keep this backend cached while concurrent requests wind down. Registry
        // resolution checks the instance list first, so deleted IDs cannot revive it.
        instance.finish_mutation();
        result
    }

    fn data_helper(&self, action: &str, input: &Value) -> Result<Value, String> {
        let mut command = Command::new(self.bundled_runtime_path()?);
        command
            .arg(self.bundled_engine_dir()?.join("manager-data-transfer.ts"))
            .arg(action)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        self.configure_environment(&mut command);
        hide_command_window(&mut command);
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let write = child.stdin.take().ok_or("数据传输入口不可用")?.write_all(
            serde_json::to_string(input)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        );
        if let Err(e) = write {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.to_string());
        }
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(self.redact(String::from_utf8_lossy(&output.stderr).trim()));
        }
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
    }

    pub fn transfer_data(
        &self,
        target: &Self,
        mode: &str,
        history: bool,
        execute: bool,
        fingerprint: Option<&str>,
    ) -> Result<Value, String> {
        if self.instance_id == target.instance_id {
            return Err("请选择不同的源和目标实例".into());
        }
        let (first, second) = if self.instance_id < target.instance_id {
            (self, target)
        } else {
            (target, self)
        };
        first.begin_mutation()?;
        if let Err(e) = second.begin_mutation() {
            first.finish_mutation();
            return Err(e);
        }
        let result = (|| {
            let source_version = self
                .active_launcher()?
                .version
                .ok_or("源实例没有可复制的 Engine")?;
            let target_version = target.active_launcher().ok().and_then(|l| l.version);
            let align_engine =
                transfer_engine_needs_alignment(&source_version, target_version.as_deref());
            let input = serde_json::json!({
                "source": self.home, "target": target.home, "manager": target.root,
                "port": target.read_configured_port().unwrap_or(target.default_port),
                "mode": mode, "history": history, "execute": execute,
                "fingerprint": fingerprint,
                "sourceEngine": source_version, "targetEngine": target_version,
            });
            if !execute {
                let mut result = target.data_helper("transfer", &input)?;
                if align_engine {
                    result["rows"]
                        .as_array_mut()
                        .ok_or("传输预览格式无效")?
                        .insert(
                            0,
                            serde_json::json!({
                                "name": if target_version.is_some() {
                                    format!("Engine v{source_version}（复制并切换）")
                                } else {
                                    format!("Engine v{source_version}（完整复制）")
                                },
                                "source": 1,
                                "target": usize::from(target_version.is_some()),
                                "result": 1
                            }),
                        );
                }
                return Ok(result);
            }
            // Validate data before stopping either runtime.
            let mut preview = input.clone();
            preview["execute"] = Value::Bool(false);
            let checked = target.data_helper("transfer", &preview)?;
            if checked.get("fingerprint").and_then(Value::as_str) != fingerprint {
                return Err("源或目标数据/Engine 版本已变化，请重新预览后再执行".into());
            }
            let source_port = self.running_open_codex_port();
            let target_port = target.running_open_codex_port();
            let source_launcher = if source_port.is_some() {
                Some(self.active_launcher()?)
            } else {
                None
            };
            let target_launcher = if target_port.is_some() {
                Some(target.active_launcher()?)
            } else {
                None
            };
            let source_mode = source_launcher
                .as_ref()
                .map(|_| self.query_background_service_state())
                .transpose()?;
            let target_mode = target_launcher
                .as_ref()
                .map(|_| target.query_background_service_state())
                .transpose()?;
            let mut copied_engine_directory = None;
            let operation = (|| {
                if let (Some(l), Some(p)) = (&source_launcher, source_port) {
                    self.stop_for_account_binding(l, p)?;
                }
                if let (Some(l), Some(p)) = (&target_launcher, target_port) {
                    target.stop_for_account_binding(l, p)?;
                }
                if align_engine {
                    let directory = target.managed_engine_root().join(&source_version);
                    if !directory.exists() {
                        fs::create_dir_all(target.managed_engine_root())
                            .map_err(|e| e.to_string())?;
                        let stage = target
                            .managed_engine_root()
                            .join(target.operation_id("copy-stage"));
                        let prepare = (|| {
                            super::isolation::copy_engine_tree(
                                &self.managed_engine_root().join(&source_version),
                                &stage,
                            )?;
                            let package = stage.join("node_modules/@bitkyc08/opencodex");
                            super::isolation::rebind_engine(
                                &package,
                                &target.instance_id,
                                &dirs::home_dir().ok_or("无法定位用户目录")?,
                            )?;
                            validate_managed_package(&package, &source_version)?;
                            fs::rename(&stage, &directory).map_err(|e| e.to_string())
                        })();
                        if let Err(error) = prepare {
                            if stage.exists() {
                                let _ = fs::remove_dir_all(&stage);
                            }
                            return Err(error);
                        }
                        copied_engine_directory = Some(directory);
                    }
                    if let Err(error) = target.activate_engine_safely(&source_version, "transfer") {
                        if let Some(directory) = &copied_engine_directory {
                            let _ = fs::remove_dir_all(directory);
                        }
                        return Err(error);
                    }
                }
                let result = target.data_helper("transfer", &input);
                if result.is_err() && align_engine {
                    target.write_active_engine(target_version.as_deref())?;
                    if let Some(directory) = &copied_engine_directory {
                        fs::remove_dir_all(directory)
                            .map_err(|e| format!("复制失败后的 Engine 清理失败：{e}"))?;
                    }
                }
                result.map(|mut value| {
                    if align_engine {
                        value["message"] = Value::String(if target_version.is_some() {
                            format!("数据已复制，目标 Engine 已安全切换为 v{source_version}；端口与实例身份保持独立")
                        } else {
                            format!("Engine v{source_version} 和数据已复制，目标可直接启动，无需安装；端口与实例身份保持独立")
                        });
                    }
                    value
                })
            })();
            let restart_target_launcher =
                if operation.is_ok() && align_engine && target_port.is_some() {
                    Some(target.active_launcher()?)
                } else {
                    target_launcher.clone()
                };
            let mut restart_errors = Vec::new();
            if let (Some(l), Some(p), Some(mode)) = (&source_launcher, source_port, &source_mode) {
                if !self.open_codex_service_running() {
                    if let Err(e) =
                        self.restart_after_account_binding(l, p, account_binding_restart_mode(mode))
                    {
                        restart_errors.push(e);
                    }
                }
            }
            if let (Some(l), Some(p), Some(mode)) =
                (&restart_target_launcher, target_port, &target_mode)
            {
                if !target.open_codex_service_running() {
                    if let Err(e) = target.restart_after_account_binding(
                        l,
                        p,
                        account_binding_restart_mode(mode),
                    ) {
                        restart_errors.push(e);
                        if let Ok(ref transferred) = operation {
                            if let Some(backup) =
                                transferred.get("backupPath").and_then(Value::as_str)
                            {
                                let recovery = (|| -> Result<(), String> {
                                    if target.open_codex_service_running() {
                                        target.stop_for_account_binding(l, p)?;
                                    }
                                    fs::rename(
                                        &target.home,
                                        target.root.join(target.operation_id("failed-transfer")),
                                    )
                                    .map_err(|e| e.to_string())?;
                                    fs::rename(backup, &target.home).map_err(|e| e.to_string())?;
                                    if align_engine {
                                        target.write_active_engine(target_version.as_deref())?;
                                        if let Some(directory) = &copied_engine_directory {
                                            if directory.exists() {
                                                fs::remove_dir_all(directory).map_err(|e| {
                                                    format!("目标回滚后的 Engine 清理失败：{e}")
                                                })?;
                                            }
                                        }
                                    }
                                    target.restart_after_account_binding(
                                        target_launcher.as_ref().unwrap_or(l),
                                        p,
                                        account_binding_restart_mode(mode),
                                    )?;
                                    Ok(())
                                })();
                                restart_errors.push(match recovery {
                                    Ok(()) => "目标数据已回滚，原服务已恢复".into(),
                                    Err(e) => format!("目标恢复失败：{e}；备份位于 {backup}"),
                                });
                            }
                        }
                    }
                }
            }
            if !restart_errors.is_empty() {
                return Err(format!(
                    "{}；{}",
                    operation
                        .as_ref()
                        .err()
                        .map(String::as_str)
                        .unwrap_or("传输后的服务恢复失败"),
                    restart_errors.join("；")
                ));
            }
            operation
        })();
        second.finish_mutation();
        first.finish_mutation();
        result
    }

    /// Immutable per-instance backends: asynchronous work never follows a UI selection.
    pub fn for_instance(self: &Arc<Self>, id: Option<&str>) -> Result<Arc<Self>, String> {
        let instance = crate::instances::resolve_instance(id.unwrap_or("default"))?;
        if instance.is_default {
            return Ok(Arc::clone(self));
        }
        if !instance
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err("实例 ID 不安全".into());
        }
        let mut children = self.children.lock().map_err(|_| "实例管理锁不可用")?;
        if let Some(child) = children.get(&instance.id) {
            if child.codex_home.as_path() == Path::new(&instance.codex_home) {
                return Ok(Arc::clone(child));
            }
            if child.operation_busy.load(Ordering::Acquire) || child.open_codex_service_running() {
                return Err("请先停止该实例 OpenCodex，再变更实例目录".into());
            }
        }
        let profile = crate::instances::default_profile_root(&instance.id);
        super::isolation::validate_directory(&profile)?;
        let home = profile.join(".opencodex");
        let root = profile.join("opencodex-manager");
        super::isolation::validate_directory(&home)?;
        super::isolation::validate_directory(&root)?;
        fs::create_dir_all(root.join("logs")).map_err(|e| e.to_string())?;
        let port_path = root.join("port.json");
        let default_port = if port_path.exists() {
            let port: u16 =
                serde_json::from_slice(&fs::read(&port_path).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("实例端口记录无效：{e}"))?;
            validate_port(port)?;
            port
        } else {
            let mut reserved = HashSet::from([self.read_configured_port().unwrap_or(DEFAULT_PORT)]);
            reserved.extend(children.values().map(|child| child.default_port));
            for other in crate::instances::list_codex_instances()? {
                let path = crate::instances::default_profile_root(&other.id)
                    .join("opencodex-manager/port.json");
                if let Ok(bytes) = fs::read(path) {
                    if let Ok(port) = serde_json::from_slice::<u16>(&bytes) {
                        reserved.insert(port);
                    }
                }
            }
            let port = (15801..=65535)
                .find(|p| {
                    !reserved.contains(p) && std::net::TcpListener::bind(("127.0.0.1", *p)).is_ok()
                })
                .ok_or("没有可用的实例端口")?;
            fs::write(&port_path, port.to_string()).map_err(|e| e.to_string())?;
            port
        };
        let child = Arc::new(Self {
            app: self.app.clone(),
            root,
            home,
            default_port,
            instance_id: instance.id.clone(),
            codex_home: PathBuf::from(instance.codex_home),
            operation_busy: AtomicBool::new(false),
            interactive: Mutex::new(None),
            sequence: AtomicU64::new(1),
            children: Mutex::new(HashMap::new()),
        });
        children.insert(instance.id, Arc::clone(&child));
        Ok(child)
    }

    fn configure_environment(&self, command: &mut Command) {
        command
            .env("OPENCODEX_HOME", &self.home)
            .env("CODEX_HOME", &self.codex_home)
            .env("CODEX_SQLITE_HOME", &self.codex_home)
            .env(
                "OPENCODEX_SWITCHER_ACCOUNTS_PATH",
                crate::switcher_data_dir().join("account/accounts.json"),
            )
            .env(
                "OPENCODEX_SYSTEM_SERVICE_NAME",
                if self.instance_id == "default" {
                    "opencodex-proxy".to_string()
                } else {
                    format!("opencodex-proxy-{}", self.instance_id)
                },
            );
        // Scope fallback caches, other client integrations and relative storage
        // paths to the child process; never mutate the desktop's environment.
        if self.instance_id != "default" {
            if let Some(profile) = self.home.parent() {
                command
                    .env("HOME", profile)
                    .env("USERPROFILE", profile)
                    .env("XDG_CONFIG_HOME", profile.join(".config"))
                    .env("XDG_DATA_HOME", profile.join(".local/share"))
                    .env("XDG_CACHE_HOME", profile.join(".cache"));
            }
        }
    }

    fn read_configured_port(&self) -> Option<u16> {
        super::isolation::read_port(&self.home.join("config.json"))
    }

    fn read_runtime_port(&self) -> Option<u16> {
        super::isolation::read_port(&self.home.join("runtime-port.json"))
    }

    fn owned_health(&self, port: u16) -> Option<HealthBody> {
        let health = probe_health(port)?;
        let record: Value =
            serde_json::from_slice(&fs::read(self.home.join("runtime-port.json")).ok()?).ok()?;
        (record.get("port")?.as_u64()? == u64::from(port)
            && record.get("pid")?.as_u64()? == u64::from(health.pid?))
        .then_some(health)
    }

    fn running_open_codex_port(&self) -> Option<u16> {
        self.read_runtime_port()
            .filter(|p| self.owned_health(*p).is_some())
    }

    fn open_codex_service_running(&self) -> bool {
        self.running_open_codex_port().is_some()
    }

    /// 服务未运行时后台拉起 OpenCodex（用于启动已接入 OpenCodex 的 Codex 实例前）。
    /// 返回本次是否真正启动了服务。
    pub fn ensure_service_running(&self) -> Result<bool, String> {
        if self.open_codex_service_running() {
            return Ok(false);
        }
        let launcher = self.active_launcher()?;
        let port = self.read_configured_port().unwrap_or(self.default_port);
        self.begin_mutation()?;
        let started = self.start_background(&launcher, port, launcher.version.as_deref(), None);
        self.finish_mutation();
        started.map(|_| true)
    }

    fn validate_instance_port(&self, port: u16) -> Result<(), String> {
        for other in crate::instances::list_codex_instances()? {
            if other.id == self.instance_id {
                continue;
            }
            let home = if other.is_default {
                config_dir().ok_or("无法定位主实例")?
            } else {
                crate::instances::default_profile_root(&other.id).join(".opencodex")
            };
            let allocated = if other.is_default {
                Some(DEFAULT_PORT)
            } else {
                fs::read(
                    crate::instances::default_profile_root(&other.id)
                        .join("opencodex-manager/port.json"),
                )
                .ok()
                .and_then(|b| serde_json::from_slice::<u16>(&b).ok())
            };
            if allocated == Some(port)
                || super::isolation::read_port(&home.join("config.json")) == Some(port)
                || super::isolation::read_port(&home.join("runtime-port.json")) == Some(port)
            {
                return Err(format!(
                    "端口 {port} 属于实例 {}，请选择其他端口",
                    other.name
                ));
            }
        }
        if TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), HTTP_TIMEOUT)
            .is_ok()
            && self.owned_health(port).is_none()
        {
            return Err(format!("端口 {port} 被其他进程占用，未操作该进程"));
        }
        Ok(())
    }

    pub fn new(app: AppHandle) -> Result<Self, String> {
        let root = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("无法解析客户端数据目录：{error}"))?
            .join("opencodex-manager");
        fs::create_dir_all(root.join("logs"))
            .map_err(|error| format!("无法创建日志目录：{error}"))?;
        Ok(Self {
            app,
            root,
            instance_id: "default".into(),
            home: config_dir().ok_or("无法定位 OpenCodex 目录")?,
            codex_home: crate::instances::codex_home_for(None)?,
            default_port: DEFAULT_PORT,
            children: Mutex::new(HashMap::new()),
            operation_busy: AtomicBool::new(false),
            interactive: Mutex::new(None),
            sequence: AtomicU64::new(1),
        })
    }

    fn begin_mutation(&self) -> Result<(), String> {
        self.operation_busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "已有操作正在执行，请等待完成后重试".to_string())?;
        let check = (|| {
            if self.instance_id != "default" {
                super::isolation::validate_directory(&self.home)?;
                super::isolation::validate_directory(&self.root)?;
            }
            if self.root.join("transfer-journal.json").exists() {
                if self.open_codex_service_running() {
                    return Err("存在未完成的数据传输，请停止服务后恢复".into());
                }
                self.data_helper(
                    "recover",
                    &serde_json::json!({"manager": self.root, "target": self.home}),
                )?;
            }
            Ok(())
        })();
        if check.is_err() {
            self.finish_mutation();
        }
        check
    }

    fn finish_mutation(&self) {
        self.operation_busy.store(false, Ordering::Release);
    }

    fn operation_id(&self, action: &str) -> String {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        format!(
            "{}:{action}-{}-{sequence}",
            self.instance_id,
            Utc::now().timestamp_millis()
        )
    }

    fn manager_log_path(&self) -> PathBuf {
        self.root.join("logs").join("manager.log")
    }

    fn service_log_path(&self) -> PathBuf {
        self.root.join("logs").join("engine.log")
    }

    fn redact(&self, line: &str) -> String {
        let line = SECRET_FIELD.replace_all(line, "$1$2[REDACTED]");
        let line = BEARER_TOKEN.replace_all(&line, "Bearer [REDACTED]");
        COMMON_TOKEN.replace_all(&line, "[REDACTED]").into_owned()
    }

    fn persist_log(&self, stream: &str, line: &str) {
        let safe = self.redact(line);
        let record = format!("[{}] [{stream}] {safe}\n", Utc::now().to_rfc3339());
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.manager_log_path())
        {
            let _ = file.write_all(record.as_bytes());
        }
    }

    fn emit_log(&self, operation_id: &str, stream: &str, line: &str) {
        let safe = self.redact(line);
        self.persist_log(stream, &safe);
        let _ = self.app.emit(
            "opencodex-command-log",
            CommandLogEvent {
                operation_id: operation_id.to_string(),
                stream: stream.to_string(),
                line: safe,
                timestamp: Utc::now().to_rfc3339(),
            },
        );
    }

    fn emit_finished(
        &self,
        operation_id: &str,
        action: &str,
        success: bool,
        exit_code: Option<i32>,
        message: String,
    ) {
        let _ = self.app.emit(
            "opencodex-command-finished",
            CommandFinishedEvent {
                operation_id: operation_id.to_string(),
                action: action.to_string(),
                success,
                exit_code,
                message,
                timestamp: Utc::now().to_rfc3339(),
            },
        );
    }

    fn emit_engine_progress(&self, operation_id: &str, version: &str, progress: EngineProgress) {
        let _ = self.app.emit(
            "opencodex-engine-progress",
            serde_json::json!({
                "operationId": operation_id,
                "version": version,
                "stage": progress.stage,
                "downloadedBytes": progress.downloaded_bytes,
                "totalBytes": progress.total_bytes,
            }),
        );
    }

    fn engine_stage(&self, operation_id: &str, version: &str, stage: &str) {
        self.emit_engine_progress(
            operation_id,
            version,
            EngineProgress {
                stage: stage.to_string(),
                ..EngineProgress::default()
            },
        );
    }

    fn active_launcher(&self) -> Result<Launcher, String> {
        let program = self.bundled_runtime_path()?;
        let version = self.active_managed_version().ok_or_else(|| {
            "尚未安装或选择 OpenCodex Engine，请先在版本管理中下载并激活版本".to_string()
        })?;
        let package_root = self.managed_package_root(&version);
        validate_managed_package(&package_root, &version)?;
        if self.instance_id != "default"
            && fs::read_to_string(package_root.join(".switcher-instance"))
                .ok()
                .as_deref()
                != Some(&self.instance_id)
        {
            return Err("Engine 尚未完成实例隔离，请在版本管理中重新激活该版本".into());
        }
        let cli = package_root.join("src").join("cli").join("index.ts");
        if !cli.is_file() {
            return Err(format!(
                "所选 OpenCodex Engine 文件缺失，请重新下载：{}",
                cli.display()
            ));
        }
        let version = package_version(&package_root);
        Ok(Launcher {
            program,
            prefix_args: vec![cli.into_os_string()],
            working_dir: Some(package_root),
            version,
            source: "managed",
        })
    }

    fn bundled_runtime_path(&self) -> Result<PathBuf, String> {
        let runtime = self
            .bundled_engine_dir()?
            .join("node_modules")
            .join("bun")
            .join("bin")
            .join("bun.exe");
        runtime
            .is_file()
            .then_some(runtime)
            .ok_or_else(|| "客户端内置 Bun 运行时缺失，请重新安装完整客户端".to_string())
    }

    fn managed_engine_root(&self) -> PathBuf {
        self.root.join("engines")
    }

    fn managed_package_root(&self, version: &str) -> PathBuf {
        self.managed_engine_root()
            .join(version)
            .join("node_modules")
            .join("@bitkyc08")
            .join("opencodex")
    }

    fn active_engine_marker(&self) -> PathBuf {
        self.root.join("active-engine.json")
    }

    fn active_managed_version(&self) -> Option<String> {
        let marker = self.active_engine_marker();
        let backup = marker.with_extension("json.bak");
        [marker, backup].into_iter().find_map(|path| {
            let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
            let version = value.get("version")?.as_str()?;
            validate_engine_version(version).ok()
        })
    }

    fn write_active_engine(&self, version: Option<&str>) -> Result<(), String> {
        if let Some(value) = version {
            validate_engine_version(value)?;
            validate_managed_package(&self.managed_package_root(value), value)?;
        }
        fs::create_dir_all(&self.root)
            .map_err(|error| format!("无法创建 Engine 状态目录：{error}"))?;
        let marker = self.active_engine_marker();
        let backup = marker.with_extension("json.bak");
        let temporary = self.root.join(format!(
            ".active-engine-{}.tmp",
            Utc::now().timestamp_micros()
        ));
        let bytes = serde_json::to_vec_pretty(&serde_json::json!({ "version": version }))
            .map_err(|error| format!("无法生成 Engine 状态：{error}"))?;
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| format!("无法创建 Engine 临时状态：{error}"))?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|error| format!("无法保存 Engine 临时状态：{error}"))?;
        }
        let _ = fs::remove_file(&backup);
        if marker.exists() {
            fs::rename(&marker, &backup)
                .map_err(|error| format!("无法备份当前 Engine 状态：{error}"))?;
        }
        if let Err(error) = fs::rename(&temporary, &marker) {
            let _ = fs::rename(&backup, &marker);
            let _ = fs::remove_file(&temporary);
            return Err(format!("无法激活 Engine 版本：{error}"));
        }
        let _ = fs::remove_file(backup);
        Ok(())
    }

    fn installed_managed_versions(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.managed_engine_root()) else {
            return Vec::new();
        };
        let mut versions = entries
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|version| {
                validate_engine_version(version).is_ok()
                    && validate_managed_package(&self.managed_package_root(version), version)
                        .is_ok()
            })
            .collect::<Vec<_>>();
        versions.sort_by(|left, right| Version::parse(right).ok().cmp(&Version::parse(left).ok()));
        versions
    }

    fn prune_managed_engine_history(&self, active_version: &str) -> Result<Vec<String>, String> {
        let installed = self.installed_managed_versions();
        let removable =
            managed_versions_to_remove(&installed, active_version, MANAGED_ENGINE_HISTORY_LIMIT);
        let mut removed = Vec::new();
        let service = self.query_background_service_state()?;
        for version in removable {
            let directory = self.managed_engine_root().join(&version);
            if service_references_directory(&service, &directory) {
                continue;
            }
            fs::remove_dir_all(&directory)
                .map_err(|error| format!("清理旧 Engine v{version} 失败：{error}"))?;
            removed.push(version);
        }
        Ok(removed)
    }

    fn bundled_engine_dir(&self) -> Result<PathBuf, String> {
        if let Ok(resource_dir) = self.app.path().resource_dir() {
            let bundled = resource_dir.join("opencodex-engine");
            if bundled.join("node_modules").is_dir() {
                return Ok(bundled);
            }
        }
        // Tauri dev runs from the source tree before resources are copied into an app bundle.
        let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("opencodex-engine");
        if development.join("node_modules").is_dir() {
            return Ok(development);
        }
        Err("客户端管理组件资源不存在，请重新安装完整客户端".to_string())
    }

    fn command(&self, launcher: &Launcher, args: &[String]) -> Command {
        self.command_with_codex_home(launcher, args, None)
    }

    fn command_with_codex_home(
        &self,
        launcher: &Launcher,
        args: &[String],
        _codex_home: Option<&Path>,
    ) -> Command {
        let mut command = Command::new(&launcher.program);
        command.args(&launcher.prefix_args).args(args);
        if let Some(working_dir) = launcher.working_dir.as_ref() {
            command.current_dir(working_dir);
        }
        command.env("NO_COLOR", "1").env("FORCE_COLOR", "0");
        self.configure_environment(&mut command);
        hide_command_window(&mut command);
        command
    }

    pub fn snapshot(&self) -> SystemSnapshot {
        let configured_port = self.read_configured_port().unwrap_or(self.default_port);
        let runtime_port = self.read_runtime_port();
        let health = runtime_port.and_then(|p| self.owned_health(p));
        let live_port = health
            .as_ref()
            .and_then(|body| body.port)
            .or(runtime_port)
            .unwrap_or(configured_port);
        let ready = health
            .as_ref()
            .and_then(|_| probe_ready(live_port))
            .is_some_and(|body| body.status.as_deref() == Some("ready"));
        let launcher = self.active_launcher().ok();
        let engine_version = launcher
            .as_ref()
            .and_then(|item| item.version.clone())
            .or_else(|| health.as_ref().and_then(|item| item.version.clone()));
        let engine_source = launcher
            .as_ref()
            .map_or("missing", |item| item.source)
            .to_string();
        let installed = launcher.is_some();
        let configuration = Some(self.home.clone());
        let initialized = configuration.as_deref().is_some_and(config_is_initialized);
        let codex_integration_enabled = configuration
            .as_deref()
            .is_none_or(codex_integration_is_enabled);
        let running = health.is_some();
        let integration_status = if !initialized {
            "等待初始化"
        } else if !codex_integration_enabled {
            "集成已关闭"
        } else if ready {
            "连接正常"
        } else if running {
            "等待同步"
        } else if installed {
            "等待服务"
        } else {
            "等待初始化"
        };
        let background_service = self.background_service_state();
        SystemSnapshot {
            instance_id: self.instance_id.clone(),
            data_dir: self.home.to_string_lossy().into_owned(),
            desktop_version: self.app.package_info().version.to_string(),
            engine_version,
            engine_source,
            platform: platform_name(),
            installed,
            initialized,
            running,
            ready,
            pid: health.as_ref().and_then(|body| body.pid),
            port: live_port,
            dashboard_url: format!("http://127.0.0.1:{live_port}"),
            integration_status: integration_status.to_string(),
            bundled_runtime_available: self.bundled_runtime_path().is_ok(),
            background_service,
        }
    }

    pub fn vision_sidecar_settings(
        &self,
        update: Option<VisionSidecarUpdate>,
    ) -> Result<Value, String> {
        let mut update = update;
        if let Some(settings) = update.as_mut() {
            settings.model = settings.model.trim().to_string();
            validate_vision_sidecar_update(settings)?;
            self.begin_mutation()?;
        }
        let result = (|| {
            let port = self
                .running_open_codex_port()
                .ok_or("请先启动所选实例的 OpenCodex 服务")?;
            let token = fs::read_to_string(self.home.join("admin-api-token"))
                .map_err(|_| "无法读取所选实例的管理凭证".to_string())?;
            if token.trim().is_empty() {
                return Err("所选实例的管理凭证为空".into());
            }
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|e| e.to_string())?;
            let url = format!("http://127.0.0.1:{port}/api/sidecar-settings");
            let request = match update.as_ref() {
                Some(settings) => client
                    .put(&url)
                    .json(&serde_json::json!({ "vision": settings })),
                None => client.get(&url),
            };
            let response = request
                .bearer_auth(token.trim())
                .send()
                .map_err(|e| format!("图片描述设置请求失败：{e}"))?;
            let status = response.status();
            let body: Value = response
                .json()
                .map_err(|_| "图片描述设置接口返回无效数据".to_string())?;
            if !status.is_success() {
                let detail = body
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("当前 Engine 不支持图片描述设置接口");
                return Err(format!(
                    "图片描述设置失败（{status}）：{}",
                    self.redact(detail)
                ));
            }
            // Only expose vision data; other sidecars may contain unrelated credentials.
            Ok(
                serde_json::json!({ "vision": body.get("vision"), "visionModels": body.get("visionModels") }),
            )
        })();
        if update.is_some() {
            self.finish_mutation();
        }
        result
    }

    pub fn get_vision_models(&self) -> Result<VisionModelCatalog, String> {
        let port = self
            .running_open_codex_port()
            .ok_or_else(|| "请先启动 OpenCodex 服务，再读取当前模型".to_string())?;
        let directory = self.home.clone();
        let token = fs::read_to_string(directory.join("admin-api-token"))
            .map_err(|error| format!("无法读取 OpenCodex 管理凭证：{error}"))?;
        let token = token.trim();
        if token.is_empty() {
            return Err("OpenCodex 管理凭证为空".to_string());
        }
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|error| format!("无法创建 OpenCodex 管理连接：{error}"))?
            .get(format!("http://127.0.0.1:{port}/api/models"))
            .bearer_auth(token)
            .send()
            .map_err(|error| format!("读取 OpenCodex 模型失败：{error}"))?;
        if !response.status().is_success() {
            return Err(format!("OpenCodex 模型接口返回 {}", response.status()));
        }
        let rows = response
            .json::<Vec<ManagementModelRow>>()
            .map_err(|error| format!("OpenCodex 模型接口返回了无效数据：{error}"))?;
        let config_text = fs::read_to_string(directory.join("config.json"))
            .map_err(|error| format!("无法读取 OpenCodex 配置：{error}"))?;
        let config: Value = serde_json::from_str(&config_text)
            .map_err(|error| format!("OpenCodex 配置格式无效：{error}"))?;
        let configured_providers = config
            .get("providers")
            .and_then(Value::as_object)
            .map(|providers| providers.keys().cloned().collect::<HashSet<_>>())
            .unwrap_or_default();
        let selected = configured_sidecar_models(&config);
        let mut models = rows
            .into_iter()
            .filter(|row| !row.native && configured_providers.contains(&row.provider))
            .map(|row| {
                let sidecar_enabled = selected
                    .get(&row.provider)
                    .is_some_and(|ids| ids.contains(&row.id));
                let native_vision =
                    !sidecar_enabled && row.input_modalities.iter().any(|item| item == "image");
                VisionModel {
                    provider: row.provider,
                    id: row.id,
                    namespaced: row.namespaced,
                    disabled: row.disabled,
                    native_vision,
                    sidecar_enabled,
                }
            })
            .collect::<Vec<_>>();
        models.sort_by(|left, right| {
            left.provider
                .cmp(&right.provider)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(VisionModelCatalog {
            models,
            sidecar_model: config
                .pointer("/visionSidecar/model")
                .and_then(Value::as_str)
                .map(str::to_string),
            sidecar_backend: config
                .pointer("/visionSidecar/backend")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    pub fn update_vision_models(
        &self,
        request: UpdateVisionModelsRequest,
    ) -> Result<VisionModelsUpdateResult, String> {
        let codex_home = self.codex_home.clone();
        let live = self.get_vision_models()?;
        let known = live
            .models
            .iter()
            .filter(|model| !model.native_vision && !model.disabled)
            .map(|model| (model.provider.clone(), model.id.clone()))
            .collect::<HashSet<_>>();
        let selected = request
            .models
            .into_iter()
            .map(|model| {
                (
                    model.provider.trim().to_string(),
                    model.id.trim().to_string(),
                )
            })
            .collect::<HashSet<_>>();
        if selected.iter().any(|model| !known.contains(model)) {
            return Err("选择中包含不存在、已禁用或原生支持图片的模型，请刷新后重试".to_string());
        }

        let mut provider_models = BTreeMap::<String, Vec<String>>::new();
        for model in &live.models {
            if !model.native_vision {
                provider_models.entry(model.provider.clone()).or_default();
            }
        }
        for (provider, id) in &selected {
            provider_models
                .entry(provider.clone())
                .or_default()
                .push(id.clone());
        }
        for ids in provider_models.values_mut() {
            ids.sort();
            ids.dedup();
        }

        self.begin_mutation()?;
        let result = (|| {
            let launcher = self.active_launcher()?;
            let mut changed_providers = Vec::new();
            for (provider, ids) in &provider_models {
                let current = live
                    .models
                    .iter()
                    .filter(|model| model.provider == *provider && model.sidecar_enabled)
                    .map(|model| model.id.clone())
                    .collect::<HashSet<_>>();
                if current == ids.iter().cloned().collect::<HashSet<_>>() {
                    continue;
                }
                let path = format!("providers.{provider}.noVisionModels");
                let json = serde_json::to_string(ids)
                    .map_err(|error| format!("无法生成图片模型配置：{error}"))?;
                let args = vec![
                    "config".to_string(),
                    "set".to_string(),
                    path,
                    json,
                    "--json".to_string(),
                ];
                let output = self
                    .command(&launcher, &args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|error| format!("无法保存 {provider} 图片模型配置：{error}"))?;
                if !output.status.success() {
                    let detail = self.redact(String::from_utf8_lossy(&output.stderr).trim());
                    return Err(if detail.is_empty() {
                        format!("保存 {provider} 图片模型配置失败")
                    } else {
                        detail
                    });
                }
                changed_providers.push(provider.clone());
            }
            if !changed_providers.is_empty() {
                let output = self
                    .command(&launcher, &["restart".to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|error| format!("配置已保存，但重启 OpenCodex 失败：{error}"))?;
                if !output.status.success() {
                    let detail = self.redact(String::from_utf8_lossy(&output.stderr).trim());
                    return Err(if detail.is_empty() {
                        "配置已保存，但重启 OpenCodex 失败".to_string()
                    } else {
                        format!("配置已保存，但重启 OpenCodex 失败：{detail}")
                    });
                }
            }
            // Provider capabilities and routing both belong to this instance.
            self.run_instance_integration_helper(
                &CommandAction::Sync,
                self.read_runtime_port()
                    .or_else(|| self.read_configured_port())
                    .unwrap_or(self.default_port),
                &codex_home,
            )?;
            let count = selected.len();
            Ok(VisionModelsUpdateResult {
                selected_count: count,
                changed_providers,
                message: if count == 0 {
                    "已关闭所有图片转文字模型并同步 Codex".to_string()
                } else {
                    format!("已为 {count} 个模型开启图片输入并同步 Codex")
                },
            })
        })();
        self.finish_mutation();
        if let Ok(value) = &result {
            self.persist_log("system", &value.message);
        }
        result
    }

    fn read_open_codex_config(&self) -> Result<Value, String> {
        let config_text = fs::read_to_string(self.home.join("config.json"))
            .map_err(|error| format!("无法读取 OpenCodex 配置：{error}"))?;
        serde_json::from_str(&config_text)
            .map_err(|error| format!("OpenCodex 配置格式无效：{error}"))
    }

    /// 执行 `ocx config set/unset`，失败时返回脱敏后的错误。
    fn run_config_mutation(
        &self,
        launcher: &Launcher,
        args: &[String],
        failure: &str,
    ) -> Result<(), String> {
        let output = self
            .command(launcher, args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| format!("{failure}：{error}"))?;
        if output.status.success() {
            return Ok(());
        }
        let detail = self.redact(String::from_utf8_lossy(&output.stderr).trim());
        Err(if detail.is_empty() {
            failure.to_string()
        } else {
            format!("{failure}：{detail}")
        })
    }

    /// 读取 Codex「Image Gen」在 OpenCodex 中的图片生成上游设置（只读 config.json，不要求服务运行）。
    pub fn image_generation_settings(&self) -> Result<ImageGenerationSettings, String> {
        let config = self.read_open_codex_config()?;
        let mut settings = resolve_image_generation_settings(&config);
        if let Some(provider) = settings.configured_provider.clone() {
            settings.recent_requests =
                read_recent_image_requests(&self.home.join("usage.jsonl"), &provider);
        }
        Ok(settings)
    }

    /// 保存图片生成上游：写入 images.provider / images.timeoutMs，必要时创建或清理镜像提供方，并重启运行中的服务。
    pub fn update_image_generation_settings(
        &self,
        request: ImageGenerationUpdate,
    ) -> Result<ImageGenerationUpdateResult, String> {
        let provider_name = request
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        if let Some(timeout) = request.timeout_ms {
            if !(IMAGE_TIMEOUT_MIN_MS..=IMAGE_TIMEOUT_MAX_MS).contains(&timeout) {
                return Err(format!(
                    "图片生成超时需在 {} 到 {} 秒之间",
                    IMAGE_TIMEOUT_MIN_MS / 1000,
                    IMAGE_TIMEOUT_MAX_MS / 1000
                ));
            }
        }
        let config = self.read_open_codex_config()?;
        let providers = config
            .get("providers")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let options = image_generation_options(&config);

        // 计算目标 images.provider 与需要创建的镜像提供方。
        let mut mirror_to_create: Option<(String, Value)> = None;
        let target_provider = match provider_name.as_deref() {
            None => None,
            Some(name) => {
                let option = options
                    .iter()
                    .find(|option| option.name == name)
                    .ok_or_else(|| {
                        format!("提供方 {name} 不存在或不能用于图片生成，请刷新后重试")
                    })?;
                if option.builtin {
                    return Err(format!(
                        "{name} 是 OpenCodex 内置提供方，不能作为图片生成上游"
                    ));
                }
                if !option.has_api_key {
                    return Err(format!("提供方 {name} 没有可用的 API Key"));
                }
                if option.direct {
                    Some(name.to_string())
                } else {
                    let source = providers
                        .get(name)
                        .ok_or_else(|| format!("提供方 {name} 不存在"))?;
                    let mirror_name = format!("{name}{IMAGE_MIRROR_SUFFIX}");
                    if let Some(existing) = providers.get(&mirror_name) {
                        if !is_image_mirror_provider(&mirror_name, existing, &providers) {
                            return Err(format!(
                                "提供方 {mirror_name} 已存在且不是图片生成镜像，请先在 OpenCodex 中重命名或删除它"
                            ));
                        }
                    }
                    mirror_to_create =
                        Some((mirror_name.clone(), build_image_mirror_provider(source)));
                    Some(mirror_name)
                }
            }
        };

        // 合并 images 段：保留 bridge 等其他字段，只改 provider 与 timeoutMs。
        let mut images = config
            .get("images")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        match target_provider.as_deref() {
            Some(name) => {
                images.insert("provider".into(), Value::String(name.to_string()));
            }
            None => {
                images.remove("provider");
            }
        }
        match request.timeout_ms {
            Some(timeout) => {
                images.insert("timeoutMs".into(), Value::from(timeout));
            }
            None => {
                images.remove("timeoutMs");
            }
        }

        // 不再被引用的旧镜像提供方一并清理。
        let stale_mirrors = providers
            .iter()
            .filter(|(name, provider)| is_image_mirror_provider(name, provider, &providers))
            .map(|(name, _)| name.clone())
            .filter(|name| {
                mirror_to_create
                    .as_ref()
                    .is_none_or(|(created, _)| created != name)
            })
            .collect::<Vec<_>>();

        self.begin_mutation()?;
        let result = (|| {
            let launcher = self.active_launcher()?;
            if let Some((mirror_name, mirror)) = &mirror_to_create {
                let json = serde_json::to_string(mirror)
                    .map_err(|error| format!("无法生成镜像提供方配置：{error}"))?;
                self.run_config_mutation(
                    &launcher,
                    &[
                        "config".into(),
                        "set".into(),
                        format!("providers.{mirror_name}"),
                        json,
                        "--json".into(),
                    ],
                    &format!("无法创建图片生成镜像提供方 {mirror_name}"),
                )?;
            }
            if images.is_empty() {
                if config.get("images").is_some() {
                    self.run_config_mutation(
                        &launcher,
                        &[
                            "config".into(),
                            "unset".into(),
                            "images".into(),
                            "--json".into(),
                        ],
                        "无法清除图片生成设置",
                    )?;
                }
            } else {
                let json = serde_json::to_string(&Value::Object(images.clone()))
                    .map_err(|error| format!("无法生成图片生成配置：{error}"))?;
                self.run_config_mutation(
                    &launcher,
                    &[
                        "config".into(),
                        "set".into(),
                        "images".into(),
                        json,
                        "--json".into(),
                    ],
                    "无法保存图片生成设置",
                )?;
            }
            for stale in &stale_mirrors {
                self.run_config_mutation(
                    &launcher,
                    &[
                        "config".into(),
                        "unset".into(),
                        format!("providers.{stale}"),
                        "--json".into(),
                    ],
                    &format!("无法清理旧的图片生成镜像提供方 {stale}"),
                )?;
            }
            let restarted = if self.open_codex_service_running() {
                self.run_config_mutation(
                    &launcher,
                    &["restart".into()],
                    "配置已保存，但重启 OpenCodex 失败",
                )?;
                true
            } else {
                false
            };
            let settings = self.image_generation_settings()?;
            let message = match (&provider_name, &mirror_to_create) {
                (None, _) => "已恢复 OpenCodex 默认图片生成路径".to_string(),
                (Some(name), Some((mirror, _))) => {
                    format!("图片生成已切换到 {name}（通过镜像提供方 {mirror} 转发）")
                }
                (Some(name), None) => format!("图片生成已切换到 {name}"),
            };
            Ok(ImageGenerationUpdateResult {
                settings,
                restarted,
                message,
            })
        })();
        self.finish_mutation();
        if let Ok(value) = &result {
            self.persist_log("system", &value.message);
        }
        result
    }

    fn background_service_state(&self) -> BackgroundServiceState {
        self.query_background_service_state()
            .unwrap_or_else(|summary| BackgroundServiceState {
                summary,
                ..BackgroundServiceState::default()
            })
    }

    fn query_background_service_state(&self) -> Result<BackgroundServiceState, String> {
        self.run_background_service_helper("status")
    }

    fn run_background_service_helper(
        &self,
        action: &str,
    ) -> Result<BackgroundServiceState, String> {
        let package = self
            .active_launcher()?
            .working_dir
            .ok_or("无法定位所选 Engine")?;
        self.run_background_service_helper_for(action, &package)
    }

    fn run_background_service_helper_for(
        &self,
        action: &str,
        package: &Path,
    ) -> Result<BackgroundServiceState, String> {
        let engine = self.bundled_engine_dir()?;
        let runtime = self.bundled_runtime_path()?;
        let helper = engine.join("manager-service-status.ts");
        if !helper.is_file() {
            return Err("客户端缺少后台服务状态组件".to_string());
        }
        let mut command = Command::new(runtime);
        command
            .arg(helper)
            .arg(action)
            .current_dir(engine)
            .env("OPENCODEX_PACKAGE_ROOT", package)
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        self.configure_environment(&mut command);
        hide_command_window(&mut command);
        let output = command
            .output()
            .map_err(|error| format!("无法检测后台服务状态：{error}"))?;
        if !output.status.success() {
            let detail = self.redact(String::from_utf8_lossy(&output.stderr).trim());
            return Err(if detail.is_empty() {
                "后台服务状态检测失败".to_string()
            } else {
                detail
            });
        }
        serde_json::from_slice::<BackgroundServiceState>(&output.stdout)
            .map_err(|error| format!("后台服务状态组件返回无效结果：{error}"))
    }

    fn set_background_service_port(&self, port: u16) -> Result<(), String> {
        validate_port(port)?;
        let package = self
            .active_launcher()?
            .working_dir
            .ok_or("无法定位所选 Engine")?;
        let engine = self.bundled_engine_dir()?;
        let runtime = self.bundled_runtime_path()?;
        let helper = engine.join("manager-service-status.ts");
        if !helper.is_file() {
            return Err("客户端缺少后台服务管理组件".to_string());
        }
        let mut command = Command::new(runtime);
        command
            .arg(helper)
            .arg("set-port")
            .arg(port.to_string())
            .current_dir(engine)
            .env("OPENCODEX_PACKAGE_ROOT", package)
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        self.configure_environment(&mut command);
        hide_command_window(&mut command);
        let output = command
            .output()
            .map_err(|error| format!("无法同步后台服务端口：{error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let detail = self.redact(String::from_utf8_lossy(&output.stderr).trim());
            Err(if detail.is_empty() {
                "后台服务端口同步失败".to_string()
            } else {
                detail
            })
        }
    }

    pub fn run_action(
        self: &Arc<Self>,
        request: RunActionRequest,
    ) -> Result<CommandStarted, String> {
        validate_port(request.port)?;
        self.validate_instance_port(request.port)?;
        let isolated_instance_integration =
            isolated_instance_integration_action(&request.action, request.instance_id.as_deref());
        let codex_home = Some(self.codex_home.clone());
        self.begin_mutation()?;
        let launcher = match self.active_launcher() {
            Ok(value) => value,
            Err(error) => {
                self.finish_mutation();
                return Err(error);
            }
        };
        let operation_id = self.operation_id(request.action.label());
        let interactive = request.action.interactive();
        let started = CommandStarted {
            operation_id: operation_id.clone(),
            interactive,
        };
        let backend = Arc::clone(self);
        thread::spawn(move || {
            backend.action_worker(
                operation_id,
                request.action,
                request.port,
                launcher,
                codex_home,
                isolated_instance_integration,
            )
        });
        Ok(started)
    }

    fn action_worker(
        self: Arc<Self>,
        operation_id: String,
        action: CommandAction,
        port: u16,
        launcher: Launcher,
        codex_home: Option<PathBuf>,
        isolated_instance_integration: bool,
    ) {
        let action_label = action.label();
        let result = if isolated_instance_integration {
            let instance_home = codex_home
                .as_deref()
                .ok_or_else(|| "无法定位多开实例 Codex Home".to_string());
            instance_home.and_then(|home| {
                (if matches!(action, CommandAction::Sync) {
                    let instance = crate::instances::list_codex_instances()?
                        .into_iter()
                        .find(|instance| Path::new(&instance.codex_home) == home)
                        .ok_or_else(|| "目标实例不存在，请刷新后重试".to_string())?;
                    self.emit_log(
                        &operation_id,
                        "system",
                        "正在预检目标实例，尚未停止实例或重置配置…",
                    );
                    self.run_instance_integration_process("preflight", port, home)?;
                    crate::instances::run_with_instance_opened_on_success(&instance.id, |_| {
                        self.emit_log(
                            &operation_id,
                            "system",
                            "预检通过，已停止实例，仅同步接口和模型配置…",
                        );
                        let message = self.run_instance_integration_helper(&action, port, home)?;
                        Ok(format!(
                            "{message}；已有账号与会话历史未迁移，正在重新打开实例"
                        ))
                    })
                } else {
                    self.run_instance_integration_helper(&action, port, home)
                })
                .map(|message| (Some(0), message))
            })
        } else if matches!(action, CommandAction::Start) {
            if self.owned_health(port).is_some() {
                self.emit_log(&operation_id, "system", "OpenCodex 服务已经在运行。");
                Ok((Some(0), "服务已在运行".to_string()))
            } else {
                self.start_background(
                    &launcher,
                    port,
                    launcher.version.as_deref(),
                    Some(&operation_id),
                )
                .map(|_| (Some(0), format!("服务已在端口 {port} 启动")))
            }
        } else {
            let args = action.argv(port);
            let preparation = if matches!(action, CommandAction::ServiceInstall) {
                self.emit_log(
                    &operation_id,
                    "system",
                    &format!("同步后台服务端口：{port}"),
                );
                self.set_background_service_port(port)
            } else if matches!(
                action,
                CommandAction::Uninstall | CommandAction::ServiceUninstall
            ) {
                self.run_instance_integration_helper(
                    &CommandAction::Restore,
                    port,
                    &self.codex_home,
                )
                .map(|_| ())
            } else {
                Ok(())
            };
            preparation
                .and_then(|_| {
                    self.run_captured(
                        &launcher,
                        &args,
                        action.interactive(),
                        &operation_id,
                        codex_home.as_deref(),
                    )
                })
                .and_then(|code| {
                    if code == Some(0) {
                        Ok((code, format!("{} 操作已完成", display_action(&action))))
                    } else {
                        Err(format!(
                            "{} 操作失败，退出码 {:?}",
                            display_action(&action),
                            code
                        ))
                    }
                })
        };

        match result {
            Ok((code, message)) => {
                self.emit_finished(&operation_id, action_label, true, code, message)
            }
            Err(message) => {
                self.emit_log(&operation_id, "stderr", &message);
                self.emit_finished(&operation_id, action_label, false, None, message);
            }
        }
        if let Ok(mut active) = self.interactive.lock() {
            if active
                .as_ref()
                .is_some_and(|item| item.operation_id == operation_id)
            {
                *active = None;
            }
        }
        self.finish_mutation();
    }

    fn run_captured(
        self: &Arc<Self>,
        launcher: &Launcher,
        args: &[String],
        interactive: bool,
        operation_id: &str,
        codex_home: Option<&Path>,
    ) -> Result<Option<i32>, String> {
        self.emit_log(
            operation_id,
            "system",
            &format!("执行：ocx {}", args.join(" ")),
        );
        let mut command = self.command_with_codex_home(launcher, args, codex_home);
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(if interactive {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        let mut child = command
            .spawn()
            .map_err(|error| format!("无法启动 Engine：{error}"))?;
        if interactive {
            if let Some(stdin) = child.stdin.take() {
                let mut slot = self
                    .interactive
                    .lock()
                    .map_err(|_| "交互输入锁不可用".to_string())?;
                *slot = Some(InteractiveProcess {
                    operation_id: operation_id.to_string(),
                    stdin,
                });
            }
        }

        let mut readers = Vec::new();
        if let Some(stdout) = child.stdout.take() {
            let backend = Arc::clone(self);
            let id = operation_id.to_string();
            readers.push(thread::spawn(move || {
                read_stream(backend, id, "stdout", stdout)
            }));
        }
        if let Some(stderr) = child.stderr.take() {
            let backend = Arc::clone(self);
            let id = operation_id.to_string();
            readers.push(thread::spawn(move || {
                read_stream(backend, id, "stderr", stderr)
            }));
        }
        let status = child
            .wait()
            .map_err(|error| format!("等待 Engine 退出失败：{error}"))?;
        for reader in readers {
            let _ = reader.join();
        }
        Ok(status.code())
    }

    fn start_background(
        &self,
        launcher: &Launcher,
        port: u16,
        expected_version: Option<&str>,
        operation_id: Option<&str>,
    ) -> Result<HealthBody, String> {
        validate_port(port)?;
        self.validate_instance_port(port)?;
        let args = vec!["start".to_string(), "--port".to_string(), port.to_string()];
        if let Some(id) = operation_id {
            self.emit_log(id, "system", &format!("后台启动：ocx start --port {port}"));
        }
        let stdout = append_file(self.service_log_path())?;
        let stderr = stdout
            .try_clone()
            .map_err(|error| format!("无法复制日志句柄：{error}"))?;
        let mut command = self.command(launcher, &args);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        prepare_background_command(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("无法启动后台 Engine：{error}"))?;
        if let Some(id) = operation_id {
            self.emit_log(
                id,
                "system",
                &format!("Engine 进程已创建，PID {}，等待健康检查…", child.id()),
            );
        }
        let deadline = Instant::now() + START_TIMEOUT;
        while Instant::now() < deadline {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| format!("检查 Engine 进程失败：{error}"))?
            {
                return Err(format!(
                    "Engine 在健康检查前退出，退出码 {:?}。请查看运行日志",
                    status.code()
                ));
            }
            if let Some(health) = probe_health(port) {
                let correct_version = expected_version
                    .map(|expected| health.version.as_deref() == Some(expected))
                    .unwrap_or(true);
                if correct_version && health.pid == Some(child.id()) {
                    thread::spawn(move || {
                        let _ = child.wait();
                    });
                    return Ok(health);
                }
            }
            thread::sleep(Duration::from_millis(250));
        }
        let _ = child.kill();
        let _ = child.wait();
        Err(format!(
            "Engine 未能在 {} 秒内通过 /healthz 检查",
            START_TIMEOUT.as_secs()
        ))
    }

    pub fn write_input(&self, operation_id: &str, value: &str) -> Result<(), String> {
        if value.len() > 4096 {
            return Err("单次输入不能超过 4096 字节".to_string());
        }
        let mut slot = self
            .interactive
            .lock()
            .map_err(|_| "交互输入锁不可用".to_string())?;
        let process = slot
            .as_mut()
            .ok_or_else(|| "当前没有等待输入的命令".to_string())?;
        if process.operation_id != operation_id {
            return Err("命令操作 ID 不匹配".to_string());
        }
        process
            .stdin
            .write_all(value.as_bytes())
            .map_err(|error| format!("发送输入失败：{error}"))?;
        process
            .stdin
            .flush()
            .map_err(|error| format!("刷新输入失败：{error}"))
    }

    pub fn read_logs(&self, limit: usize) -> Vec<String> {
        let limit = limit.clamp(1, 2000);
        let Ok(lines) =
            read_last_log_lines(&self.manager_log_path(), limit, MANAGER_LOG_TAIL_MAX_BYTES)
        else {
            return Vec::new();
        };
        lines.into_iter().map(|line| self.redact(&line)).collect()
    }

    pub fn open_dashboard_window(&self, port: u16) -> Result<(), String> {
        validate_port(port)?;
        let url = tauri::Url::parse(&format!("http://127.0.0.1:{port}"))
            .map_err(|error| format!("Dashboard 地址无效：{error}"))?;
        if let Some(window) = self
            .app
            .get_webview_window(&format!("opencodex-dashboard-{}", self.instance_id))
        {
            window
                .navigate(url)
                .map_err(|error| format!("无法刷新 Dashboard 窗口：{error}"))?;
            window
                .show()
                .map_err(|error| format!("无法显示 Dashboard 窗口：{error}"))?;
            window
                .set_focus()
                .map_err(|error| format!("无法聚焦 Dashboard 窗口：{error}"))?;
            return Ok(());
        }
        WebviewWindowBuilder::new(
            &self.app,
            format!("opencodex-dashboard-{}", self.instance_id),
            WebviewUrl::External(url),
        )
        .title("OpenCodex Web 管理")
        .inner_size(1280.0, 820.0)
        .min_inner_size(900.0, 620.0)
        .center()
        .build()
        .map(|_| ())
        .map_err(|error| format!("无法创建 Dashboard 窗口：{error}"))
    }

    pub fn open_dashboard_browser(&self, port: u16) -> Result<(), String> {
        validate_port(port)?;
        open_browser_url(&format!("http://127.0.0.1:{port}"))
    }

    pub fn scan_codex_switcher_accounts(&self) -> Result<SwitcherAccountScan, String> {
        self.run_switcher_helper("scan", None)
    }

    pub fn import_codex_switcher_accounts(
        &self,
        request: ImportSwitcherAccountsRequest,
    ) -> Result<SwitcherImportResult, String> {
        validate_switcher_import_request(&request)?;
        self.begin_mutation()?;
        let result = (|| {
            if self.open_codex_service_running() {
                return Err(
                    "导入账号前请先停止 OpenCodex 服务，避免运行中的 Engine 覆盖账号配置"
                        .to_string(),
                );
            }
            let input = serde_json::to_vec(&request)
                .map_err(|error| format!("无法生成导入请求：{error}"))?;
            self.run_switcher_helper::<SwitcherImportResult>("import", Some(&input))
        })();
        self.finish_mutation();
        if let Ok(imported) = &result {
            self.persist_log(
                "system",
                &format!(
                    "已从 Codex Switcher 导入 {} 个账号，跳过 {} 个账号",
                    imported.imported_count, imported.skipped_count
                ),
            );
        }
        result
    }

    pub fn bind_codex_switcher_accounts(
        &self,
        request: ImportSwitcherAccountsRequest,
    ) -> Result<SwitcherImportResult, String> {
        validate_switcher_import_request(&request)?;
        self.begin_mutation()?;
        let result = (|| {
            let port = self
                .running_open_codex_port()
                .ok_or_else(|| "请先启动 OpenCodex 服务，再绑定账号".to_string())?;
            let launcher = self.active_launcher()?;
            let background_service = self.query_background_service_state()?;
            let restart_mode = account_binding_restart_mode(&background_service);
            let input = serde_json::to_vec(&request)
                .map_err(|error| format!("无法生成绑定请求：{error}"))?;
            if let Err(stop_error) = self.stop_for_account_binding(&launcher, port) {
                if probe_health(port).is_some() {
                    return Err(stop_error);
                }
                return match self.restart_after_account_binding(&launcher, port, restart_mode) {
                    Ok(()) => Err(format!("{stop_error}；OpenCodex 已按原运行方式恢复")),
                    Err(restart_error) => Err(format!(
                        "{stop_error}；OpenCodex 服务恢复也失败：{restart_error}。请手动启动 OpenCodex"
                    )),
                };
            }

            let import_result =
                self.run_switcher_helper::<SwitcherImportResult>("import", Some(&input));
            let restart_result = self.restart_after_account_binding(&launcher, port, restart_mode);

            match (import_result, restart_result) {
                (Ok(imported), Ok(_)) => Ok(imported),
                (Ok(_), Err(restart_error)) => Err(format!(
                    "账号已写入 OpenCodex，但服务恢复失败：{restart_error}。请手动启动 OpenCodex"
                )),
                (Err(import_error), Ok(_)) => Err(import_error),
                (Err(import_error), Err(restart_error)) => Err(format!(
                    "{import_error}；OpenCodex 服务恢复也失败：{restart_error}。请手动启动 OpenCodex"
                )),
            }
        })();
        self.finish_mutation();
        if let Ok(imported) = &result {
            self.persist_log(
                "system",
                &format!(
                    "已绑定 {} 个 Switcher 账号到 OpenCodex，跳过 {} 个账号，并恢复服务",
                    imported.imported_count, imported.skipped_count
                ),
            );
        }
        result
    }

    fn stop_for_account_binding(&self, launcher: &Launcher, port: u16) -> Result<(), String> {
        let args = CommandAction::Stop.argv(port);
        let mut command = self.command(launcher, &args);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = command
            .output()
            .map_err(|error| format!("无法停止 OpenCodex：{error}"))?;
        if !output.status.success() {
            let detail = self.redact(String::from_utf8_lossy(&output.stderr).trim());
            return Err(if detail.is_empty() {
                "停止 OpenCodex 失败".to_string()
            } else {
                format!("停止 OpenCodex 失败：{detail}")
            });
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if probe_health(port).is_none() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(150));
        }
        Err("等待 OpenCodex 停止超时".to_string())
    }

    fn restart_after_account_binding(
        &self,
        launcher: &Launcher,
        port: u16,
        mode: AccountBindingRestartMode,
    ) -> Result<(), String> {
        match mode {
            AccountBindingRestartMode::BackgroundService => {
                self.start_background_service_after_binding(launcher, port)
            }
            AccountBindingRestartMode::Standalone => self
                .start_background(launcher, port, launcher.version.as_deref(), None)
                .map(|_| ()),
        }
    }

    fn start_background_service_after_binding(
        &self,
        launcher: &Launcher,
        port: u16,
    ) -> Result<(), String> {
        let args = vec!["service".to_string(), "start".to_string()];
        let mut command = self.command(launcher, &args);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = command
            .output()
            .map_err(|error| format!("无法恢复 OpenCodex 后台服务：{error}"))?;
        let detail = self.redact(String::from_utf8_lossy(&output.stderr).trim());
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if probe_health(port).is_some() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(200));
        }
        Err(if detail.is_empty() {
            if output.status.success() {
                format!("OpenCodex 后台服务未能在端口 {port} 恢复")
            } else {
                "OpenCodex 后台服务启动失败".to_string()
            }
        } else {
            format!("OpenCodex 后台服务启动失败：{detail}")
        })
    }

    pub fn delete_codex_switcher_account(
        &self,
        request: DeleteSwitcherAccountRequest,
    ) -> Result<SwitcherDeleteResult, String> {
        if request.source_id.is_empty() || request.source_id.len() > 128 {
            return Err("删除请求包含无效账号 ID".to_string());
        }
        self.begin_mutation()?;
        let result = (|| {
            let input = serde_json::to_vec(&request)
                .map_err(|error| format!("无法生成删除请求：{error}"))?;
            self.run_switcher_helper::<SwitcherDeleteResult>("delete", Some(&input))
        })();
        self.finish_mutation();
        if let Ok(deleted) = &result {
            if deleted.deleted {
                self.persist_log(
                    "system",
                    &format!("已从 OpenCodex 删除 Switcher 账号 {}", deleted.source_id),
                );
            }
        }
        result
    }

    pub fn get_engine_update_catalog(&self) -> Result<EngineUpdateCatalog, String> {
        let launcher = self.active_launcher().ok();
        let current_version = launcher.as_ref().and_then(|item| item.version.clone());
        let current_source = launcher
            .as_ref()
            .map_or("missing", |item| item.source)
            .to_string();
        let installed_versions = self.installed_managed_versions();
        let remote = self.run_engine_update_helper::<RemoteEngineCatalog>("catalog", None, None);
        Ok(build_engine_update_catalog(
            current_version,
            current_source,
            installed_versions,
            remote,
        ))
    }

    pub fn install_engine_version(
        &self,
        request: InstallEngineVersionRequest,
    ) -> Result<EngineInstallResult, String> {
        let version = validate_engine_version(&request.version)?;
        if request.operation_id.is_empty() || request.operation_id.len() > 128 {
            return Err("Engine 操作 ID 无效".to_string());
        }
        self.begin_mutation()?;
        let operation_id = &request.operation_id;
        self.engine_stage(operation_id, &version, "checking");
        let result = (|| {
            let package = self.managed_package_root(&version);
            let already_installed = validate_managed_package(&package, &version).is_ok();
            if !already_installed {
                let input = serde_json::to_vec(&serde_json::json!({
                    "version": version,
                    "engineRoot": self.managed_engine_root(),
                    "archivePath": request.archive_path,
                }))
                .map_err(|error| format!("无法生成 Engine 安装请求：{error}"))?;
                let installed: EngineHelperInstallResult = self.run_engine_update_helper(
                    "install",
                    Some(&input),
                    Some((operation_id, &version)),
                )?;
                if installed.version != version {
                    return Err("Engine 更新器返回了错误的版本".to_string());
                }
                validate_managed_package(&package, &version)?;
            }
            super::isolation::scope_engine(
                &package,
                &self.instance_id,
                &dirs::home_dir().ok_or("无法定位用户目录")?,
            )?;
            let restarted = self.activate_engine_safely(&version, operation_id)?;
            Ok(EngineInstallResult {
                version: version.clone(),
                source: "managed".to_string(),
                message: if restarted {
                    format!("已切换到 Engine v{version}，服务已自动重启并通过健康检查")
                } else if already_installed {
                    format!("已切换到 Engine v{version}")
                } else {
                    format!("Engine v{version} 下载、校验并激活完成")
                },
            })
        })();
        self.engine_stage(
            operation_id,
            &request.version,
            if result.is_ok() { "complete" } else { "error" },
        );
        self.finish_mutation();
        if let Ok(value) = &result {
            self.persist_log("system", &value.message);
        }
        result
    }

    fn activate_engine_safely(&self, version: &str, operation_id: &str) -> Result<bool, String> {
        let old_launcher = match self.active_launcher() {
            Ok(launcher) => launcher,
            Err(_) => {
                // A first download has no old Engine to query or restore. Use the
                // newly verified package only for read-only service inspection.
                let package = self.managed_package_root(version);
                validate_managed_package(&package, version)?;
                let service = self.run_background_service_helper_for("status", &package)?;
                if self.running_open_codex_port().is_some()
                    || service.installed
                    || service.running
                    || service.conflict
                {
                    return Err("检测到未受当前版本记录管理的 OpenCodex 服务，请先停止并解除后台服务注册，再激活下载版本".to_string());
                }
                self.write_active_engine(Some(version))?;
                self.active_launcher()?;
                return Ok(false);
            }
        };
        let old_version = old_launcher.version.as_deref();
        let service = self.query_background_service_state()?;
        if service.conflict {
            return Err("后台服务存在冲突，请先修复后台服务后再切换 Engine".to_string());
        }
        let running_port = self.running_open_codex_port();
        let port = running_port
            .unwrap_or_else(|| self.read_configured_port().unwrap_or(self.default_port));
        if running_port
            .and_then(probe_health)
            .is_some_and(|health| health.version != old_launcher.version)
        {
            return Err("运行中的 Engine 与当前版本记录不一致，请先停止服务再切换".to_string());
        }
        let was_running = running_port.is_some() || service.running;
        let needs_stop = was_running;
        let mut next_launcher: Option<Launcher> = None;
        switch_engine(needs_stop, |step| {
            match step {
                SwitchStep::StopOld => {
                    self.engine_stage(operation_id, version, "stopping");
                    self.stop_for_account_binding(&old_launcher, port)
                }
                SwitchStep::ActivateNew => {
                    self.engine_stage(operation_id, version, "activating");
                    self.write_active_engine(Some(version))?;
                    next_launcher = Some(self.active_launcher()?);
                    Ok(())
                }
                SwitchStep::StartNew => {
                    if was_running {
                        self.engine_stage(operation_id, version, "restarting");
                    }
                    self.restore_engine_runtime(
                        next_launcher.as_ref().ok_or("无法定位新 Engine")?,
                        port,
                        was_running,
                        &service,
                    )
                }
                SwitchStep::StopNew => {
                    self.engine_stage(operation_id, version, "recovering");
                    if needs_stop {
                        self.stop_for_account_binding(
                            next_launcher.as_ref().unwrap_or(&old_launcher),
                            port,
                        )
                    } else {
                        Ok(())
                    }
                }
                SwitchStep::ActivateOld => self.write_active_engine(old_version),
                SwitchStep::RecoverOld => {
                    self.engine_stage(operation_id, version, "recovering");
                    // A failed stop can leave the original service untouched.
                    if was_running
                        && probe_health(port)
                            .is_some_and(|health| health.version == old_launcher.version)
                        && (!service.running || {
                            let current = self.query_background_service_state()?;
                            current.running && current.enabled == service.enabled
                        })
                    {
                        return Ok(());
                    }
                    self.restore_engine_runtime(&old_launcher, port, was_running, &service)
                }
            }
        })?;
        // Prune only after successful activation and recovery checks.
        match self.prune_managed_engine_history(version) {
            Ok(removed) if !removed.is_empty() => self.persist_log(
                "system",
                &format!("已清理旧 Engine 版本：{}", removed.join("、")),
            ),
            Err(error) => self.persist_log("stderr", &error),
            _ => {}
        }
        Ok(was_running)
    }

    fn restore_engine_runtime(
        &self,
        launcher: &Launcher,
        port: u16,
        was_running: bool,
        service: &BackgroundServiceState,
    ) -> Result<(), String> {
        // Leave stopped/disabled registrations untouched. Their referenced Engine
        // stays protected from deletion until the registration is repaired.
        if !was_running {
            return Ok(());
        }
        if service.running {
            // `start` alone reuses the old baked CLI path. `repair` preserves the
            // installed backend and rewrites its entrypoint without re-registering.
            self.set_background_service_port(port)?;
            let output = self
                .command(launcher, &["service".into(), "repair".into()])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .map_err(|error| format!("更新后台服务启动路径失败：{error}"))?;
            if service.backend.as_deref() == Some("systemd") && !service.enabled {
                self.run_background_service_helper("restore-disabled-autostart")?;
            }
            if !output.status.success() {
                let detail = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&output.stderr),
                    String::from_utf8_lossy(&output.stdout)
                );
                return Err(format!(
                    "更新后台服务启动路径失败：{}",
                    self.redact(detail.trim())
                ));
            }
        } else {
            self.start_background(launcher, port, launcher.version.as_deref(), None)?;
        }
        self.wait_engine_version(launcher, port)
    }

    fn wait_engine_version(&self, launcher: &Launcher, port: u16) -> Result<(), String> {
        let deadline = Instant::now() + START_TIMEOUT;
        while Instant::now() < deadline {
            if probe_health(port).is_some_and(|health| health.version == launcher.version) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(250));
        }
        Err(format!(
            "Engine v{} 未通过端口 {port} 的版本健康检查",
            launcher.version.as_deref().unwrap_or("?")
        ))
    }

    pub fn delete_engine_version(
        &self,
        request: DeleteEngineVersionRequest,
    ) -> Result<EngineDeleteResult, String> {
        let version = validate_engine_version(&request.version)?;
        self.begin_mutation()?;
        let result = (|| {
            let versions = self.installed_managed_versions();
            if !versions.contains(&version) {
                return Err(format!("本地未安装 Engine v{version}"));
            }
            let active = self.active_managed_version().as_deref() == Some(version.as_str());
            let last = versions.len() == 1;
            let service = self.query_background_service_state()?;
            if service.conflict {
                return Err("后台服务存在冲突，未删除任何数据".into());
            }
            if (active || last) && (self.running_open_codex_port().is_some() || service.running) {
                return Err("当前实例 OpenCodex 仍在运行，请先停止再删除".into());
            }
            if last {
                if !request.remove_data {
                    return Err(
                        "这是最后一个版本，请刷新并确认清理全部实例 OpenCodex 数据后再删除".into(),
                    );
                }
                self.remove_stopped_instance_opencodex(&version, &service)?;
                return Ok(EngineDeleteResult { version: version.clone(), message: "已删除当前实例的最后一个 Engine 及全部 OpenCodex 数据目录、配置和备份；Codex 会话与其他实例不受影响".into() });
            }
            if active {
                let remaining = versions
                    .iter()
                    .find(|v| **v != version)
                    .ok_or("没有可用的剩余版本")?;
                self.activate_engine_safely(remaining, "delete-current")?;
            }
            if self
                .running_open_codex_port()
                .and_then(probe_health)
                .is_some_and(|health| health.version.as_deref() == Some(&version))
            {
                return Err("该 Engine 版本仍在运行，不能删除".to_string());
            }
            // A stopped service can still reference an older managed directory.
            if service_references_directory(
                &self.query_background_service_state()?,
                &self.managed_engine_root().join(&version),
            ) {
                return Err("后台服务仍引用该版本，请先重新注册后台服务以更新启动路径".to_string());
            }
            let directory = self.managed_engine_root().join(&version);
            if !validate_managed_package(&self.managed_package_root(&version), &version).is_ok() {
                return Err(format!("本地未安装 Engine v{version}"));
            }
            fs::remove_dir_all(&directory)
                .map_err(|error| format!("删除 Engine v{version} 失败：{error}"))?;
            Ok(EngineDeleteResult {
                version: version.clone(),
                message: format!("已删除本地 Engine v{version}"),
            })
        })();
        self.finish_mutation();
        if let Ok(value) = &result {
            if self.root.exists() {
                self.persist_log("system", &value.message);
            }
        }
        result
    }

    fn remove_stopped_instance_opencodex(
        &self,
        version: &str,
        service: &BackgroundServiceState,
    ) -> Result<(), String> {
        // These immutable directories are resolved by the backend, never supplied by IPC.
        super::isolation::validate_cleanup(&self.home, &self.root, &self.codex_home)?;
        let launcher = self.active_launcher().or_else(|_| {
            self.write_active_engine(Some(version))?;
            self.active_launcher()
        })?;
        if crate::instances::codex_home_has_opencodex_routing(&self.codex_home) {
            self.run_instance_integration_helper(
                &CommandAction::Restore,
                self.default_port,
                &self.codex_home,
            )?;
        }
        if service.installed {
            let output = self
                .command(&launcher, &["service".into(), "uninstall".into()])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "解除后台服务失败，未删除数据：{}",
                    self.redact(String::from_utf8_lossy(&output.stderr).trim())
                ));
            }
        }
        let after = self.query_background_service_state()?;
        if after.installed
            || after.running
            || after.conflict
            || self.running_open_codex_port().is_some()
        {
            return Err("服务尚未完全停止或解除注册，未删除数据".into());
        }
        super::isolation::remove_instance_data(&self.home, &self.root, &self.codex_home)
    }

    fn run_engine_update_helper<T: DeserializeOwned>(
        &self,
        action: &str,
        input: Option<&[u8]>,
        progress: Option<(&str, &str)>,
    ) -> Result<T, String> {
        let engine = self.bundled_engine_dir()?;
        let runtime = self.bundled_runtime_path()?;
        let helper = engine.join("manager-engine-update.ts");
        if !helper.is_file() {
            return Err("客户端缺少 Engine 更新组件，请重新安装完整客户端".to_string());
        }
        let mut command = Command::new(runtime);
        command
            .arg(helper)
            .arg(action)
            .current_dir(&engine)
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        self.configure_environment(&mut command);
        hide_command_window(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("无法启动 Engine 更新器：{error}"))?;
        if let Some(bytes) = input {
            let sent = child
                .stdin
                .take()
                .ok_or_else(|| "无法打开 Engine 更新器输入".to_string())?
                .write_all(bytes)
                .map_err(|error| format!("无法发送 Engine 更新请求：{error}"));
            if let Err(error) = sent {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        let mut stdout = child.stdout.take().ok_or("无法读取 Engine 更新器输出")?;
        let stderr = child.stderr.take().ok_or("无法读取 Engine 更新器进度")?;
        let (output, errors) = thread::scope(|scope| {
            let reader = scope.spawn(move || {
                let mut output = Vec::new();
                stdout.read_to_end(&mut output).map(|_| output)
            });
            let mut errors = String::new();
            for line in BufReader::new(stderr).lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(error) => {
                        let _ = child.kill();
                        errors.push_str(&format!("读取更新进度失败：{error}"));
                        break;
                    }
                };
                if let Some(event) = parse_engine_progress(&line) {
                    if let Some((operation_id, version)) = progress {
                        self.emit_engine_progress(operation_id, version, event);
                    }
                } else {
                    if errors.len() > 8192 {
                        errors.clear();
                    }
                    errors.push_str(&line);
                    errors.push('\n');
                }
            }
            (
                reader
                    .join()
                    .unwrap_or_else(|_| Err(std::io::Error::other("更新输出线程失败"))),
                errors,
            )
        });
        let status = child
            .wait()
            .map_err(|error| format!("等待 Engine 更新器失败：{error}"))?;
        if !status.success() {
            let detail = self.redact(errors.trim());
            return Err(if detail.is_empty() {
                "Engine 更新失败".to_string()
            } else {
                detail
            });
        }
        let output = output.map_err(|error| format!("读取 Engine 更新结果失败：{error}"))?;
        serde_json::from_slice(&output)
            .map_err(|error| format!("Engine 更新器返回了无效结果：{error}"))
    }

    pub(crate) fn reset_instance_config(
        &self,
        instance_id: &str,
    ) -> Result<crate::CodexConfigFileContent, String> {
        self.begin_mutation()?;
        let result = self.reset_instance_config_inner(instance_id);
        self.finish_mutation();
        result
    }

    fn reset_instance_config_inner(
        &self,
        instance_id: &str,
    ) -> Result<crate::CodexConfigFileContent, String> {
        let home = crate::instances::codex_home_for(Some(instance_id))?;
        let has_integration_settings = Some(self.home.clone())
            .is_some_and(|dir| dir.join("config.json").exists())
            || home.join(".switcher-opencodex/config.json").exists();
        if has_integration_settings {
            self.run_instance_integration_process("disable", DEFAULT_PORT, &home)?;
        }
        crate::reset_codex_config_for_instance(instance_id)
    }

    fn run_instance_integration_helper(
        &self,
        action: &CommandAction,
        port: u16,
        codex_home: &Path,
    ) -> Result<String, String> {
        let result = self.apply_instance_integration(action, port, codex_home);
        if matches!(action, CommandAction::Sync) && result.is_err() {
            // Do not leave desired=ON after a failed catalog sync: the default
            // service's convergence loop would otherwise inject it again.
            self.run_instance_integration_process("disable", port, codex_home)
                .map_err(|error| {
                    format!(
                        "{}；同步失败后的接入关闭也失败：{error}",
                        result.as_ref().unwrap_err()
                    )
                })?;
            super::config_repair::repair_invalid_catalog_references(codex_home)?;
        }
        result
    }

    fn apply_instance_integration(
        &self,
        action: &CommandAction,
        port: u16,
        codex_home: &Path,
    ) -> Result<String, String> {
        let action_name = match action {
            CommandAction::Sync => "sync",
            CommandAction::Restore => "restore",
            _ => return Err("多开实例集成操作只支持同步或恢复".to_string()),
        };
        super::config_repair::repair_invalid_catalog_references(codex_home)?;
        let result = self.run_instance_integration_process(action_name, port, codex_home);
        let invalid_catalog_removed =
            super::config_repair::repair_invalid_catalog_references(codex_home)?;
        // An Engine restore can remove routing and then fail its history phase.
        // Still repair dangling references, but keep the original failure visible.
        if matches!(action, CommandAction::Restore) {
            crate::session::SessionStore::new(codex_home.to_path_buf())
                .repair_missing_opencodex_provider()
                .map_err(|error| format!("恢复后的旧会话修复失败：{error}"))?;
        }
        let message = result?;
        if matches!(action, CommandAction::Sync) && invalid_catalog_removed {
            return Err("同步生成了无效或空的模型目录，已移除该引用，未自动打开实例。请检查服务模型列表后重试。".to_string());
        }
        if matches!(action, CommandAction::Sync)
            && !crate::instances::codex_home_has_opencodex_routing(codex_home)
        {
            return Err(format!(
                "目标实例没有接入 OpenCodex，未将此次操作标记为同步成功：{message}"
            ));
        }
        if matches!(action, CommandAction::Restore)
            && crate::instances::codex_home_has_opencodex_routing(codex_home)
        {
            return Err(
                "目标实例仍有 OpenCodex 路由，恢复未完成；为避免配置丢失，不会继续卸载".to_string(),
            );
        }
        Ok(message)
    }

    fn run_instance_integration_process(
        &self,
        action_name: &str,
        port: u16,
        codex_home: &Path,
    ) -> Result<String, String> {
        if codex_home != self.codex_home {
            return Err("目标实例与 OpenCodex 后端不一致，已拒绝跨实例操作".into());
        }
        let engine = self.bundled_engine_dir()?;
        let runtime = self.bundled_runtime_path()?;
        // Reset must remain available after migration away from the bundled
        // Engine. Disabling intent alone needs only the packaged helper/runtime.
        let active_package_root = if action_name == "disable" {
            self.active_launcher()
                .ok()
                .and_then(|launcher| launcher.working_dir)
        } else {
            Some(
                self.active_launcher()?
                    .working_dir
                    .ok_or_else(|| "无法定位当前激活的 OpenCodex Engine".to_string())?,
            )
        };
        let helper = engine.join("manager-instance-integration.ts");
        if !helper.is_file() {
            return Err("客户端缺少多开实例隔离组件，请重新安装完整客户端".to_string());
        }
        let mut command = Command::new(runtime);
        let source_home = self.home.clone();
        let integration_home = self.home.clone();
        command
            .arg(helper)
            .arg(action_name)
            .arg(port.to_string())
            .current_dir(&engine)
            .env("CODEX_HOME", codex_home)
            // Engine Workers import configuration before receiving their env
            // message. Give the process its final instance home from startup.
            .env("OPENCODEX_HOME", integration_home)
            .env("OPENCODEX_MANAGER_SOURCE_HOME", source_home)
            .env("OPENCODEX_MANAGER_DEFAULT_INSTANCE", "1")
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(package) = active_package_root {
            command.env("OPENCODEX_PACKAGE_ROOT", package);
        } else {
            command.env_remove("OPENCODEX_PACKAGE_ROOT");
        }
        self.configure_environment(&mut command);
        hide_command_window(&mut command);
        let output = command
            .output()
            .map_err(|error| format!("无法启动多开实例隔离组件：{error}"))?;
        let detail = self.redact(String::from_utf8_lossy(&output.stderr).trim());
        if !detail.is_empty() {
            // Discovery warnings explain empty/no-write catalogs. Preserve them
            // even when the helper returns a structured failure on stdout.
            self.persist_log("stderr", &detail);
        }
        if let Ok(result) = serde_json::from_slice::<InstanceIntegrationResult>(&output.stdout) {
            return if result.success {
                Ok(result.message)
            } else {
                Err(result.message)
            };
        }
        Err(if detail.is_empty() {
            format!("OpenCodex 实例隔离操作 {action_name} 失败")
        } else {
            detail
        })
    }

    fn run_switcher_helper<T: DeserializeOwned>(
        &self,
        action: &str,
        input: Option<&[u8]>,
    ) -> Result<T, String> {
        let package = self
            .active_launcher()?
            .working_dir
            .ok_or("无法定位所选 Engine")?;
        let engine = self.bundled_engine_dir()?;
        let runtime = engine
            .join("node_modules")
            .join("bun")
            .join("bin")
            .join("bun.exe");
        let helper = engine.join("manager-switcher-import.ts");
        if !runtime.is_file() {
            return Err("客户端内置 Bun 运行时缺失".to_string());
        }
        if !helper.is_file() {
            return Err("客户端缺少 Codex Switcher 导入组件，请重新安装完整客户端".to_string());
        }
        let mut command = Command::new(runtime);
        command
            .arg(helper)
            .arg(action)
            .current_dir(&engine)
            .env("OPENCODEX_PACKAGE_ROOT", package)
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        self.configure_environment(&mut command);
        hide_command_window(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("无法启动 Switcher 账号转换器：{error}"))?;
        if let Some(bytes) = input {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| "无法打开 Switcher 转换器输入".to_string())?;
            stdin
                .write_all(bytes)
                .map_err(|error| format!("无法发送 Switcher 导入请求：{error}"))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|error| format!("等待 Switcher 转换器失败：{error}"))?;
        if !output.status.success() {
            let detail = self.redact(String::from_utf8_lossy(&output.stderr).trim());
            return Err(if detail.is_empty() {
                "Codex Switcher 账号转换失败".to_string()
            } else {
                detail
            });
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("Switcher 转换器返回了无效结果：{error}"))
    }
}

fn build_engine_update_catalog(
    current_version: Option<String>,
    current_source: String,
    installed_versions: Vec<String>,
    remote: Result<RemoteEngineCatalog, String>,
) -> EngineUpdateCatalog {
    let current_semver = current_version
        .as_deref()
        .and_then(|value| Version::parse(value).ok());
    let (mut releases, remote_error) = match remote {
        Ok(remote) => (remote.releases, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    for release in &mut releases {
        release.newer_than_current = current_semver.as_ref().is_none_or(|current| {
            Version::parse(&release.version).is_ok_and(|next| next > *current)
        });
        release.installed = installed_versions.contains(&release.version);
        release.active = current_version.as_deref() == Some(release.version.as_str());
    }
    let latest_stable = releases.iter().find(|release| !release.prerelease).cloned();
    let latest_preview = releases.iter().find(|release| release.prerelease).cloned();
    EngineUpdateCatalog {
        current_version,
        current_source,
        latest_stable,
        latest_preview,
        releases,
        installed_versions,
        remote_error,
    }
}

fn read_stream<R: Read + Send + 'static>(
    backend: Arc<Backend>,
    operation_id: String,
    stream: &'static str,
    mut reader: R,
) {
    let mut buffer = [0_u8; 2048];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(size) => {
                let output = String::from_utf8_lossy(&buffer[..size]);
                for chunk in output.split_inclusive('\n') {
                    let chunk = chunk.trim_end_matches(['\r', '\n']);
                    if !chunk.is_empty() {
                        backend.emit_log(&operation_id, stream, chunk);
                    }
                }
            }
            Err(error) => {
                backend.emit_log(
                    &operation_id,
                    "stderr",
                    &format!("读取 Engine 输出失败：{error}"),
                );
                break;
            }
        }
    }
}

fn read_last_log_lines(path: &Path, limit: usize, max_bytes: u64) -> std::io::Result<Vec<String>> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let start = file_len.saturating_sub(max_bytes.max(1));
    file.seek(SeekFrom::Start(start))?;
    let mut bytes =
        Vec::with_capacity(usize::try_from(file_len.saturating_sub(start)).unwrap_or_default());
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = if start > 0 {
        text.split_once('\n').map(|(_, tail)| tail).unwrap_or("")
    } else {
        text.as_ref()
    };
    let lines = text.lines().collect::<Vec<_>>();
    Ok(lines[lines.len().saturating_sub(limit)..]
        .iter()
        .map(|line| (*line).to_string())
        .collect())
}

fn append_file(path: PathBuf) -> Result<File, String> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("无法打开 Engine 日志：{error}"))
}

#[cfg(unix)]
fn prepare_background_command(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
}

#[cfg(windows)]
fn prepare_background_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    command.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
}

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_command_window(_command: &mut Command) {}

fn validate_port(port: u16) -> Result<(), String> {
    (port >= 1024)
        .then_some(())
        .ok_or_else(|| "端口必须在 1024–65535 之间".to_string())
}

fn open_browser_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开系统浏览器：{error}"))
}

fn platform_name() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

fn package_version(package_root: &Path) -> Option<String> {
    let text = fs::read_to_string(package_root.join("package.json")).ok()?;
    let package: Value = serde_json::from_str(&text).ok()?;
    let raw = package.get("version")?.as_str()?;
    Version::parse(raw).ok().map(|version| version.to_string())
}

fn validate_engine_version(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let parsed = Version::parse(trimmed).map_err(|_| "Engine 版本号格式无效".to_string())?;
    if value != trimmed
        || parsed.to_string() != trimmed
        || trimmed.contains('+')
        || trimmed.len() > 80
    {
        return Err("Engine 版本号格式无效".to_string());
    }
    Ok(trimmed.to_string())
}

fn validate_managed_package(package_root: &Path, expected_version: &str) -> Result<(), String> {
    if package_version(package_root).as_deref() != Some(expected_version) {
        return Err(format!("Engine v{expected_version} 安装不完整或版本不匹配"));
    }
    let cli = package_root.join("src").join("cli").join("index.ts");
    if !cli.is_file() {
        return Err(format!("Engine v{expected_version} 缺少 CLI 入口"));
    }
    Ok(())
}

fn managed_versions_to_remove(
    installed_versions: &[String],
    active_version: &str,
    history_limit: usize,
) -> Vec<String> {
    installed_versions
        .iter()
        .filter(|version| version.as_str() != active_version)
        .skip(history_limit)
        .cloned()
        .collect()
}

fn config_dir() -> Option<PathBuf> {
    if let Some(raw) = std::env::var_os("OPENCODEX_HOME") {
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            return Some(path);
        }
    }
    dirs::home_dir().map(|home| home.join(".opencodex"))
}

fn configured_sidecar_models(config: &Value) -> HashMap<String, HashSet<String>> {
    let mut result = HashMap::new();
    let Some(providers) = config.get("providers").and_then(Value::as_object) else {
        return result;
    };
    for (provider, value) in providers {
        let ids = value
            .get("noVisionModels")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<HashSet<_>>();
        if !ids.is_empty() {
            result.insert(provider.clone(), ids);
        }
    }
    result
}

fn transfer_engine_needs_alignment(source_version: &str, target_version: Option<&str>) -> bool {
    target_version != Some(source_version)
}

fn provider_str<'a>(provider: &'a Value, key: &str) -> Option<&'a str> {
    provider.get(key).and_then(Value::as_str)
}

fn provider_disabled(provider: &Value) -> bool {
    provider
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn provider_has_api_key(provider: &Value) -> bool {
    provider_str(provider, "apiKey").is_some_and(|key| !key.trim().is_empty())
}

/// 镜像提供方的来源名：名称以 `-images` 结尾且去掉后缀后存在对应源提供方。
fn image_mirror_source<'a>(
    name: &'a str,
    providers: &serde_json::Map<String, Value>,
) -> Option<&'a str> {
    let source = name.strip_suffix(IMAGE_MIRROR_SUFFIX)?;
    (!source.is_empty() && providers.contains_key(source)).then_some(source)
}

/// 判断是否为 Switcher 创建的图片生成镜像提供方：名称带后缀、源存在，且形状为「openai-responses + 关闭在线模型 + 空模型列表」。
fn is_image_mirror_provider(
    name: &str,
    provider: &Value,
    providers: &serde_json::Map<String, Value>,
) -> bool {
    if image_mirror_source(name, providers).is_none() {
        return false;
    }
    provider_str(provider, "adapter") == Some(IMAGE_DIRECT_ADAPTER)
        && provider.get("liveModels").and_then(Value::as_bool) == Some(false)
        && provider
            .get("models")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
}

/// 由源提供方派生镜像提供方：复用地址、凭证与网络设置，但不暴露任何聊天模型。
fn build_image_mirror_provider(source: &Value) -> Value {
    let mut mirror = serde_json::Map::new();
    mirror.insert("adapter".into(), Value::String(IMAGE_DIRECT_ADAPTER.into()));
    mirror.insert("authMode".into(), Value::String("key".into()));
    for key in [
        "baseUrl",
        "apiKey",
        "apiKeyTransport",
        "headers",
        "allowPrivateNetwork",
        "upstreamHttpVersion",
    ] {
        if let Some(value) = source.get(key) {
            mirror.insert(key.into(), value.clone());
        }
    }
    mirror.insert("models".into(), Value::Array(Vec::new()));
    mirror.insert("liveModels".into(), Value::Bool(false));
    mirror.insert("newModelPolicy".into(), Value::String("off".into()));
    mirror.insert("disabled".into(), Value::Bool(false));
    Value::Object(mirror)
}

/// 列出可作为图片生成上游的提供方：启用中、API Key 鉴权、适配器为 openai-responses/openai-chat，排除镜像自身。
fn image_generation_options(config: &Value) -> Vec<ImageGenerationProviderOption> {
    let Some(providers) = config.get("providers").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut options = providers
        .iter()
        .filter(|(name, provider)| {
            !provider_disabled(provider) && !is_image_mirror_provider(name, provider, providers)
        })
        .filter(|(_, provider)| {
            provider_str(provider, "adapter")
                .is_some_and(|adapter| IMAGE_MIRRORABLE_ADAPTERS.contains(&adapter))
        })
        .filter(|(_, provider)| provider_str(provider, "authMode").is_none_or(|mode| mode == "key"))
        .map(|(name, provider)| {
            let adapter = provider_str(provider, "adapter").unwrap_or_default();
            ImageGenerationProviderOption {
                name: name.clone(),
                adapter: adapter.to_string(),
                base_url: provider_str(provider, "baseUrl")
                    .unwrap_or_default()
                    .to_string(),
                direct: adapter == IMAGE_DIRECT_ADAPTER,
                has_api_key: provider_has_api_key(provider),
                builtin: ENGINE_BUILTIN_PROVIDER_IDS.contains(&name.as_str()),
            }
        })
        .collect::<Vec<_>>();
    options.sort_by(|left, right| left.name.cmp(&right.name));
    options
}

/// 是否存在 OpenCodex 内置的 OpenAI 图片上游（ChatGPT 转发或 api.openai.com 的 API Key 提供方）。
fn openai_image_upstream_available(config: &Value) -> bool {
    config
        .get("providers")
        .and_then(Value::as_object)
        .and_then(|providers| providers.get("openai"))
        .is_some_and(|provider| {
            !provider_disabled(provider)
                && provider_str(provider, "adapter") == Some(IMAGE_DIRECT_ADAPTER)
        })
}

fn resolve_image_generation_settings(config: &Value) -> ImageGenerationSettings {
    let providers = config
        .get("providers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let configured_provider = config
        .pointer("/images/provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    // 若 images.provider 指向镜像提供方，则把选择映射回源提供方展示。
    let (provider, mirror_provider) = match configured_provider.as_deref() {
        Some(name)
            if providers
                .get(name)
                .is_some_and(|value| is_image_mirror_provider(name, value, &providers)) =>
        {
            (
                image_mirror_source(name, &providers).map(str::to_string),
                Some(name.to_string()),
            )
        }
        Some(name) => (Some(name.to_string()), None),
        None => (None, None),
    };
    ImageGenerationSettings {
        recent_requests: Vec::new(),
        provider,
        configured_provider,
        mirror_provider,
        timeout_ms: config.pointer("/images/timeoutMs").and_then(Value::as_u64),
        openai_upstream_available: openai_image_upstream_available(config),
        options: image_generation_options(config),
    }
}

const IMAGE_REQUEST_TAIL_BYTES: u64 = 2 * 1024 * 1024;
const IMAGE_REQUEST_LIMIT: usize = 5;

/// 从 usage.jsonl 尾部读取发往指定图片生成上游的最近请求（新到旧）。文件很大时只读最后 2MB。
fn read_recent_image_requests(path: &Path, provider: &str) -> Vec<ImageGenerationRecentRequest> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(length) = file.metadata().map(|meta| meta.len()) else {
        return Vec::new();
    };
    let start = length.saturating_sub(IMAGE_REQUEST_TAIL_BYTES);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut text = String::new();
    if file.read_to_string(&mut text).is_err() {
        return Vec::new();
    }
    let mut lines = text.lines();
    if start > 0 {
        // 从中间开始读，第一行大概率是被截断的半条记录。
        lines.next();
    }
    parse_recent_image_requests(lines, provider)
}

fn parse_recent_image_requests<'a>(
    lines: impl Iterator<Item = &'a str>,
    provider: &str,
) -> Vec<ImageGenerationRecentRequest> {
    let mut requests = lines
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| row.get("provider").and_then(Value::as_str) == Some(provider))
        .filter_map(|row| {
            Some(ImageGenerationRecentRequest {
                timestamp: row.get("timestamp").and_then(Value::as_i64)?,
                model: row
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                status: u16::try_from(row.get("status").and_then(Value::as_u64)?).ok()?,
                error_code: row
                    .get("errorCode")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                duration_ms: row.get("durationMs").and_then(Value::as_u64),
            })
        })
        .collect::<Vec<_>>();
    requests.sort_by_key(|request| std::cmp::Reverse(request.timestamp));
    requests.truncate(IMAGE_REQUEST_LIMIT);
    requests
}

fn validate_vision_sidecar_update(settings: &VisionSidecarUpdate) -> Result<(), String> {
    let model = settings.model.trim();
    if model.is_empty() || model.len() > 256 || model.chars().any(char::is_control) {
        return Err("图片描述模型名称无效".to_string());
    }
    if settings
        .backend
        .as_deref()
        .is_some_and(|backend| !matches!(backend, "openai" | "anthropic" | "routed"))
    {
        return Err("图片描述后端只支持 openai、anthropic 或 routed".to_string());
    }
    let namespaced = model.contains('/');
    if namespaced && settings.backend.as_deref() != Some("routed") {
        return Err("带 Provider 前缀的图片模型必须使用 routed 后端".to_string());
    }
    if !namespaced && settings.backend.as_deref() == Some("routed") {
        return Err("routed 后端必须选择带 Provider 前缀的图片模型".to_string());
    }
    Ok(())
}

fn config_is_initialized(directory: &Path) -> bool {
    let Ok(text) = fs::read_to_string(directory.join("config.json")) else {
        return false;
    };
    let Ok(config) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let Some(default_provider) = config
        .get("defaultProvider")
        .and_then(Value::as_str)
        .filter(|provider| !provider.trim().is_empty())
    else {
        return false;
    };
    config
        .get("providers")
        .and_then(Value::as_object)
        .is_some_and(|providers| providers.contains_key(default_provider))
}

fn codex_integration_is_enabled(directory: &Path) -> bool {
    let Ok(text) = fs::read_to_string(directory.join("config.json")) else {
        // Keep the Engine's backward-compatible default: an absent setting is
        // ON, and initialization validation reports a missing file separately.
        return true;
    };
    let Ok(config) = serde_json::from_str::<Value>(&text) else {
        return true;
    };
    config
        .get("clientIntegrations")
        .and_then(Value::as_object)
        .and_then(|integrations| integrations.get("codex"))
        .and_then(Value::as_bool)
        != Some(false)
}

fn request_json(port: u16, path: &str) -> Option<(u16, HealthBody)> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, HTTP_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(HTTP_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(HTTP_TIMEOUT)).ok()?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json\r\nConnection: close\r\n\r\n").ok()?;
    let mut response = Vec::with_capacity(2048);
    stream.take(64 * 1024).read_to_end(&mut response).ok()?;
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")?;
    let header = String::from_utf8_lossy(&response[..split]);
    let status = header
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse::<u16>()
        .ok()?;
    let body = serde_json::from_slice::<HealthBody>(&response[split + 4..]).ok()?;
    Some((status, body))
}

fn probe_health(port: u16) -> Option<HealthBody> {
    let (status, body) = request_json(port, "/healthz")?;
    (status == 200
        && body.service.as_deref() == Some("opencodex")
        && body.status.as_deref() == Some("ok"))
    .then_some(body)
}

fn probe_ready(port: u16) -> Option<HealthBody> {
    let (status, body) = request_json(port, "/readyz")?;
    (status == 200 && body.service.as_deref() == Some("opencodex")).then_some(body)
}

fn validate_switcher_import_request(request: &ImportSwitcherAccountsRequest) -> Result<(), String> {
    if request.source_ids.is_empty() {
        return Err("请至少选择一个账号".to_string());
    }
    if request.source_ids.len() > 1000
        || request
            .source_ids
            .iter()
            .any(|id| id.is_empty() || id.len() > 128)
    {
        return Err("导入请求包含过多账号或无效账号 ID".to_string());
    }
    Ok(())
}

fn account_binding_restart_mode(
    background_service: &BackgroundServiceState,
) -> AccountBindingRestartMode {
    if background_service.running {
        AccountBindingRestartMode::BackgroundService
    } else {
        AccountBindingRestartMode::Standalone
    }
}

fn isolated_instance_integration_action(
    action: &CommandAction,
    _instance_id: Option<&str>,
) -> bool {
    matches!(action, CommandAction::Sync | CommandAction::Restore)
}

fn display_action(action: &CommandAction) -> &'static str {
    match action {
        CommandAction::Init => "初始化",
        CommandAction::Start => "启动",
        CommandAction::Stop => "停止",
        CommandAction::Restart => "重启",
        CommandAction::Status => "状态检查",
        CommandAction::Doctor => "环境诊断",
        CommandAction::Sync => "配置同步",
        CommandAction::ServiceInstall => "后台服务安装",
        CommandAction::ServiceUninstall => "后台服务取消",
        CommandAction::Restore => "Codex 恢复",
        CommandAction::Uninstall => "卸载",
    }
}

fn service_references_directory(service: &BackgroundServiceState, directory: &Path) -> bool {
    if !service.installed {
        return false;
    }
    if service.references_unknown || service.referenced_cli_paths.is_empty() {
        return true;
    }
    service.referenced_cli_paths.iter().any(|path| {
        if cfg!(windows) {
            Path::new(&path.to_lowercase()).starts_with(directory.to_string_lossy().to_lowercase())
        } else {
            Path::new(path).starts_with(directory)
        }
    })
}

fn parse_engine_progress(line: &str) -> Option<EngineProgress> {
    let value: Value = serde_json::from_str(line).ok()?;
    let progress: EngineProgress =
        serde_json::from_value(value.get("engineProgress")?.clone()).ok()?;
    matches!(
        progress.stage.as_str(),
        "checking" | "downloading" | "dependencies" | "validating"
    )
    .then_some(progress)
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    fn image_config() -> Value {
        json!({
            "providers": {
                "openai": { "adapter": "openai-responses", "baseUrl": "https://chatgpt.com/backend-api/codex", "authMode": "forward", "disabled": true },
                "Zc": { "adapter": "openai-chat", "baseUrl": "https://relay.example/v1", "authMode": "key", "apiKey": "sk-secret", "allowPrivateNetwork": true, "noVisionModels": ["a"] },
                "Direct": { "adapter": "openai-responses", "baseUrl": "https://direct.example/v1", "apiKey": "sk-direct" },
                "Claude": { "adapter": "anthropic-messages", "baseUrl": "https://api.anthropic.com", "apiKey": "sk-ant" },
                "xai": { "adapter": "openai-responses", "baseUrl": "https://api.x.ai/v1", "authMode": "key", "apiKey": "xai-key" },
                "Zc-images": { "adapter": "openai-responses", "authMode": "key", "baseUrl": "https://relay.example/v1", "apiKey": "sk-secret", "models": [], "liveModels": false }
            },
            "images": { "provider": "Zc-images", "timeoutMs": 120000, "bridgeEnabled": false }
        })
    }

    #[test]
    fn image_generation_options_keep_keyed_openai_style_providers_and_hide_mirrors() {
        let options = super::image_generation_options(&image_config());
        let names = options
            .iter()
            .map(|option| option.name.as_str())
            .collect::<Vec<_>>();
        // 已禁用的 openai、anthropic 适配器和镜像自身都不出现；内置 xai 保留但标记 builtin。
        assert_eq!(names, vec!["Direct", "Zc", "xai"]);
        let zc = options.iter().find(|option| option.name == "Zc").unwrap();
        assert!(!zc.direct && zc.has_api_key && !zc.builtin);
        let direct = options
            .iter()
            .find(|option| option.name == "Direct")
            .unwrap();
        assert!(direct.direct);
        assert!(
            options
                .iter()
                .find(|option| option.name == "xai")
                .unwrap()
                .builtin
        );
    }

    #[test]
    fn image_generation_settings_map_mirror_back_to_source_provider() {
        let settings = super::resolve_image_generation_settings(&image_config());
        assert_eq!(settings.provider.as_deref(), Some("Zc"));
        assert_eq!(settings.configured_provider.as_deref(), Some("Zc-images"));
        assert_eq!(settings.mirror_provider.as_deref(), Some("Zc-images"));
        assert_eq!(settings.timeout_ms, Some(120_000));
        // ChatGPT 转发提供方已禁用，因此没有内置 OpenAI 图片上游。
        assert!(!settings.openai_upstream_available);
    }

    #[test]
    fn recent_image_requests_keep_only_the_configured_provider_newest_first() {
        let lines = [
            r#"{"provider":"Zc","model":"gpt-5.4","status":200,"timestamp":10}"#,
            r#"{"provider":"Zc-images","model":"gpt-image-2","status":502,"timestamp":20,"durationMs":11098,"errorCode":"upstream_server_error"}"#,
            "not json",
            r#"{"provider":"Zc-images","model":"gpt-image-2","status":200,"timestamp":30,"durationMs":9000}"#,
        ];
        let requests = super::parse_recent_image_requests(lines.into_iter(), "Zc-images");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].timestamp, 30);
        assert_eq!(requests[0].status, 200);
        assert_eq!(
            requests[1].error_code.as_deref(),
            Some("upstream_server_error")
        );
        assert_eq!(requests[1].duration_ms, Some(11098));
    }

    #[test]
    fn image_mirror_provider_copies_credentials_but_exposes_no_models() {
        let config = image_config();
        let source = &config["providers"]["Zc"];
        let mirror = super::build_image_mirror_provider(source);
        assert_eq!(mirror["adapter"], "openai-responses");
        assert_eq!(mirror["authMode"], "key");
        assert_eq!(mirror["apiKey"], "sk-secret");
        assert_eq!(mirror["baseUrl"], "https://relay.example/v1");
        assert_eq!(mirror["allowPrivateNetwork"], true);
        assert_eq!(mirror["liveModels"], false);
        assert_eq!(mirror["models"], json!([]));
        // 源提供方的图片描述配置不应被带到镜像里。
        assert!(mirror.get("noVisionModels").is_none());
        let providers = config["providers"].as_object().unwrap();
        assert!(super::is_image_mirror_provider(
            "Zc-images",
            &mirror,
            providers
        ));
        assert!(!super::is_image_mirror_provider("Zc", source, providers));
    }

    #[test]
    fn engine_progress_accepts_only_helper_download_stages() {
        let progress = super::parse_engine_progress(
            r#"{"engineProgress":{"stage":"downloading","downloadedBytes":10,"totalBytes":20}}"#,
        )
        .unwrap();
        assert_eq!(progress.downloaded_bytes, Some(10));
        assert_eq!(progress.total_bytes, Some(20));
        assert!(super::parse_engine_progress("network error").is_none());
        assert!(
            super::parse_engine_progress(r#"{"engineProgress":{"stage":"complete"}}"#).is_none()
        );
    }

    #[test]
    fn inactive_versions_referenced_by_service_are_protected() {
        let service = BackgroundServiceState {
            installed: true,
            referenced_cli_paths: vec!["/engines/2.40.0/node_modules/opencodex/cli.ts".into()],
            ..BackgroundServiceState::default()
        };
        assert!(super::service_references_directory(
            &service,
            std::path::Path::new("/engines/2.40.0")
        ));
        assert!(!super::service_references_directory(
            &service,
            std::path::Path::new("/engines/2.4")
        ));
        assert!(!super::service_references_directory(
            &service,
            std::path::Path::new("/engines/2.39.0")
        ));
        assert!(super::service_references_directory(
            &BackgroundServiceState {
                references_unknown: true,
                ..service
            },
            std::path::Path::new("/engines/2.39.0")
        ));
    }

    use super::{
        account_binding_restart_mode, build_engine_update_catalog, codex_integration_is_enabled,
        config_is_initialized, configured_sidecar_models, isolated_instance_integration_action,
        managed_versions_to_remove, read_last_log_lines, transfer_engine_needs_alignment,
        validate_engine_version, validate_managed_package, validate_port,
        validate_vision_sidecar_update, AccountBindingRestartMode, RemoteEngineCatalog,
        DEFAULT_PORT,
    };
    use crate::opencodex::models::{BackgroundServiceState, CommandAction, VisionSidecarUpdate};
    use chrono::Utc;
    #[cfg(unix)]
    use std::process::Command;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tempfile::tempdir;

    #[test]
    fn desktop_runtime_manifest_does_not_embed_an_engine() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../opencodex-engine/package.json")).unwrap();
        assert!(manifest["dependencies"]
            .get("@bitkyc08/opencodex")
            .is_none());
        assert!(manifest["dependencies"]["bun"].is_string());
    }

    #[test]
    fn reads_only_complete_lines_from_the_bounded_log_tail() {
        let dir = tempdir().expect("log tempdir");
        let path = dir.path().join("manager.log");
        fs::write(&path, "first-line\nsecond-line\nthird-line\nfourth-line\n")
            .expect("write manager log");

        let lines = read_last_log_lines(&path, 10, 25).expect("read log tail");

        assert_eq!(lines, ["third-line", "fourth-line"]);
    }

    #[test]
    fn account_binding_restores_the_original_service_lifecycle_mode() {
        assert_eq!(
            account_binding_restart_mode(&BackgroundServiceState {
                running: true,
                ..BackgroundServiceState::default()
            }),
            AccountBindingRestartMode::BackgroundService
        );
        assert_eq!(
            account_binding_restart_mode(&BackgroundServiceState::default()),
            AccountBindingRestartMode::Standalone
        );
    }

    #[test]
    fn all_instance_sync_and_restore_use_the_scoped_helper() {
        assert!(isolated_instance_integration_action(
            &CommandAction::Sync,
            Some("instance-custom")
        ));
        assert!(isolated_instance_integration_action(
            &CommandAction::Restore,
            Some("instance-custom")
        ));
        assert!(isolated_instance_integration_action(
            &CommandAction::Sync,
            Some(crate::instances::DEFAULT_INSTANCE_ID)
        ));
        assert!(!isolated_instance_integration_action(
            &CommandAction::Start,
            Some("instance-custom")
        ));
    }

    #[test]
    fn reads_only_configured_sidecar_model_ids() {
        let config = serde_json::json!({
            "providers": {
                "zbc": { "noVisionModels": ["gpt-5.6-sol", "gpt-5.6-terra", 7] },
                "native": { "noVisionModels": [] },
                "invalid": { "noVisionModels": "gpt-5.5" }
            }
        });
        let selected = configured_sidecar_models(&config);
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected.get("zbc").expect("zbc model set"),
            &["gpt-5.6-sol".to_string(), "gpt-5.6-terra".to_string()]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn transfer_aligns_missing_or_different_target_engines() {
        assert!(transfer_engine_needs_alignment("2.45.0", None));
        assert!(transfer_engine_needs_alignment("2.45.0", Some("2.48.0")));
        assert!(!transfer_engine_needs_alignment("2.45.0", Some("2.45.0")));
    }

    #[test]
    fn validates_coherent_vision_sidecar_model_and_backend_pairs() {
        assert!(validate_vision_sidecar_update(&VisionSidecarUpdate {
            model: "gpt-5.6-luna".to_string(),
            backend: Some("openai".to_string()),
            enabled: true,
        })
        .is_ok());
        assert!(validate_vision_sidecar_update(&VisionSidecarUpdate {
            model: "Zc/qwen3.8-max".to_string(),
            backend: Some("routed".to_string()),
            enabled: true,
        })
        .is_ok());
        assert!(validate_vision_sidecar_update(&VisionSidecarUpdate {
            model: "Zc/qwen3.8-max".to_string(),
            backend: Some("openai".to_string()),
            enabled: true,
        })
        .is_err());
        assert!(validate_vision_sidecar_update(&VisionSidecarUpdate {
            model: "gpt-5.6-luna".to_string(),
            backend: Some("routed".to_string()),
            enabled: false,
        })
        .is_err());
    }

    #[test]
    fn remote_catalog_failure_preserves_local_engine_versions() {
        let catalog = build_engine_update_catalog(
            Some("2.31.0".to_string()),
            "managed".to_string(),
            vec!["2.31.0".to_string(), "2.30.0".to_string()],
            Err("GitHub unavailable".to_string()),
        );

        assert_eq!(catalog.current_version.as_deref(), Some("2.31.0"));
        assert_eq!(catalog.current_source, "managed");
        assert_eq!(catalog.installed_versions, ["2.31.0", "2.30.0"]);
        assert!(catalog.releases.is_empty());
        assert_eq!(catalog.remote_error.as_deref(), Some("GitHub unavailable"));
    }

    #[test]
    fn accepts_only_canonical_engine_versions() {
        assert_eq!(validate_engine_version("2.28.0").as_deref(), Ok("2.28.0"));
        assert_eq!(
            validate_engine_version("2.28.0-preview.20260821").as_deref(),
            Ok("2.28.0-preview.20260821")
        );
        for invalid in ["v2.28.0", "2.28", "2.28.0+local", "../2.28.0", " 2.28.0"] {
            assert!(
                validate_engine_version(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn decodes_the_engine_release_helper_contract() {
        let catalog: RemoteEngineCatalog = serde_json::from_str(
            r#"{"releases":[{"version":"2.28.0","tag":"v2.28.0","name":"v2.28.0","prerelease":false,"publishedAt":"2026-08-21T00:00:00Z","url":"https://github.com/lidge-jun/opencodex/releases/tag/v2.28.0"}]}"#,
        )
        .expect("decode release catalog");
        assert_eq!(catalog.releases.len(), 1);
        assert_eq!(catalog.releases[0].version, "2.28.0");
        assert!(!catalog.releases[0].installed);
        assert!(!catalog.releases[0].active);
    }

    #[test]
    fn validates_a_managed_engine_entrypoint_and_version() {
        let temporary = std::env::temp_dir().join(format!(
            "opencodex-manager-engine-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_micros()
        ));
        let package = temporary.join("opencodex");
        std::fs::create_dir_all(package.join("src").join("cli")).expect("create package tree");
        std::fs::write(package.join("package.json"), r#"{"version":"2.28.0"}"#)
            .expect("write package manifest");
        assert!(validate_managed_package(&package, "2.28.0").is_err());
        std::fs::write(
            package.join("src").join("cli").join("index.ts"),
            "export {}",
        )
        .expect("write cli entrypoint");
        assert!(validate_managed_package(&package, "2.28.0").is_ok());
        assert!(validate_managed_package(&package, "2.29.0").is_err());
        std::fs::remove_dir_all(temporary).expect("remove temp directory");
    }

    #[test]
    fn retains_the_active_engine_and_three_recent_history_versions() {
        let installed = ["2.32.0", "2.31.0", "2.30.0", "2.29.0", "2.28.0"].map(ToString::to_string);
        assert_eq!(
            managed_versions_to_remove(&installed, "2.32.0", 3),
            ["2.28.0"]
        );
        assert_eq!(
            managed_versions_to_remove(&installed, "2.30.0", 3),
            ["2.28.0"]
        );
    }

    #[test]
    fn retains_three_recent_managed_versions_when_no_engine_is_active() {
        let installed = ["2.32.0", "2.31.0", "2.30.0", "2.29.0"].map(ToString::to_string);
        assert_eq!(managed_versions_to_remove(&installed, "", 3), ["2.29.0"]);
    }

    #[test]
    fn validates_non_privileged_ports() {
        assert!(validate_port(DEFAULT_PORT).is_ok());
        assert!(validate_port(1024).is_ok());
        assert!(validate_port(1023).is_err());
    }

    #[test]
    fn recognizes_only_a_complete_initialized_config() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "opencodex-manager-config-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create config test directory");

        assert!(!config_is_initialized(&directory));
        fs::write(
            directory.join("config.json"),
            r#"{"defaultProvider":"ollama","providers":{}}"#,
        )
        .expect("write incomplete config");
        assert!(!config_is_initialized(&directory));
        fs::write(
            directory.join("config.json"),
            r#"{"defaultProvider":"ollama","providers":{"ollama":{"baseUrl":"http://localhost:11434/v1"}}}"#,
        )
        .expect("write initialized config");
        assert!(config_is_initialized(&directory));

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn reads_the_durable_codex_integration_switch_with_absent_meaning_on() {
        let directory = std::env::temp_dir().join(format!(
            "opencodex-manager-integration-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_micros()
        ));
        fs::create_dir_all(&directory).expect("create integration test directory");

        fs::write(
            directory.join("config.json"),
            r#"{"clientIntegrations":{"codex":false}}"#,
        )
        .expect("write disabled integration config");
        assert!(!codex_integration_is_enabled(&directory));

        fs::write(
            directory.join("config.json"),
            r#"{"clientIntegrations":{}}"#,
        )
        .expect("write default integration config");
        assert!(codex_integration_is_enabled(&directory));

        fs::write(directory.join("config.json"), r#"{}"#).expect("write legacy integration config");
        assert!(codex_integration_is_enabled(&directory));

        fs::remove_dir_all(directory).expect("remove integration test directory");
    }

    #[cfg(unix)]
    #[test]
    fn bundled_runtime_runs_without_node_or_npm_on_path() {
        let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../opencodex-engine/node_modules/bun/bin/bun.exe");
        let output = Command::new(runtime)
            .arg("--version")
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
    }
}
