use super::models::{
    BackgroundServiceState, CommandAction, CommandFinishedEvent, CommandLogEvent, CommandStarted,
    DeleteEngineVersionRequest, DeleteSwitcherAccountRequest, EngineDeleteResult,
    EngineInstallResult, EngineRelease, EngineUpdateCatalog, HealthBody,
    ImportSwitcherAccountsRequest, InstallEngineVersionRequest, RunActionRequest,
    SwitcherAccountScan, SwitcherDeleteResult, SwitcherImportResult, SystemSnapshot,
    UpdateVisionModelsRequest, VisionModel, VisionModelCatalog, VisionModelsUpdateResult,
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
use std::io::{Read, Write};
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
    operation_busy: AtomicBool,
    interactive: Mutex<Option<InteractiveProcess>>,
    sequence: AtomicU64,
}

impl Backend {
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
            operation_busy: AtomicBool::new(false),
            interactive: Mutex::new(None),
            sequence: AtomicU64::new(1),
        })
    }

    fn begin_mutation(&self) -> Result<(), String> {
        self.operation_busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| "已有操作正在执行，请等待完成后重试".to_string())
    }

    fn finish_mutation(&self) {
        self.operation_busy.store(false, Ordering::Release);
    }

    fn operation_id(&self, action: &str) -> String {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        format!("{action}-{}-{sequence}", Utc::now().timestamp_millis())
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

    fn active_launcher(&self) -> Result<Launcher, String> {
        let program = self.bundled_runtime_path()?;
        let (package_root, source) = self
            .active_managed_version()
            .and_then(|version| {
                let package = self.managed_package_root(&version);
                (package_version(&package).as_deref() == Some(version.as_str())
                    && package.join("src").join("cli").join("index.ts").is_file())
                .then_some((package, "managed"))
            })
            .unwrap_or((self.bundled_package_root()?, "bundled"));
        let cli = package_root.join("src").join("cli").join("index.ts");
        if !cli.is_file() {
            return Err(format!("客户端内置 OpenCodex 源码缺失：{}", cli.display()));
        }
        let version = package_version(&package_root);
        Ok(Launcher {
            program,
            prefix_args: vec![cli.into_os_string()],
            working_dir: Some(package_root),
            version,
            source,
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

    fn bundled_package_root(&self) -> Result<PathBuf, String> {
        let package = self
            .bundled_engine_dir()?
            .join("node_modules")
            .join("@bitkyc08")
            .join("opencodex");
        package
            .is_dir()
            .then_some(package)
            .ok_or_else(|| "客户端内置 OpenCodex Engine 缺失，请重新安装完整客户端".to_string())
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
        for version in removable {
            let directory = self.managed_engine_root().join(&version);
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
        Err("客户端内置 Engine 资源不存在，请重新安装完整客户端".to_string())
    }

    fn command(&self, launcher: &Launcher, args: &[String]) -> Command {
        self.command_with_codex_home(launcher, args, None)
    }

    fn command_with_codex_home(
        &self,
        launcher: &Launcher,
        args: &[String],
        codex_home: Option<&Path>,
    ) -> Command {
        let mut command = Command::new(&launcher.program);
        command.args(&launcher.prefix_args).args(args);
        if let Some(working_dir) = launcher.working_dir.as_ref() {
            command.current_dir(working_dir);
        }
        command.env("NO_COLOR", "1").env("FORCE_COLOR", "0");
        if let Some(path) = codex_home {
            command.env("CODEX_HOME", path);
        }
        command
    }

    pub fn snapshot(&self) -> SystemSnapshot {
        let configured_port = read_configured_port().unwrap_or(DEFAULT_PORT);
        let runtime_port = read_runtime_port();
        let mut health = runtime_port.and_then(probe_health);
        if health.is_none() && runtime_port != Some(configured_port) {
            health = probe_health(configured_port);
        }
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
        let configuration = config_dir();
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

    pub fn get_vision_models(&self) -> Result<VisionModelCatalog, String> {
        let port = running_open_codex_port()
            .ok_or_else(|| "请先启动 OpenCodex 服务，再读取当前模型".to_string())?;
        let directory = config_dir().ok_or_else(|| "无法定位 OpenCodex 配置目录".to_string())?;
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

    fn background_service_state(&self) -> BackgroundServiceState {
        let result = (|| {
            let engine = self.bundled_engine_dir()?;
            let runtime = self.bundled_runtime_path()?;
            let helper = engine.join("manager-service-status.ts");
            if !helper.is_file() {
                return Err("客户端缺少后台服务状态组件".to_string());
            }
            let output = Command::new(runtime)
                .arg(helper)
                .arg("status")
                .current_dir(engine)
                .env("NO_COLOR", "1")
                .env("FORCE_COLOR", "0")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
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
        })();
        result.unwrap_or_else(|summary| BackgroundServiceState {
            summary,
            ..BackgroundServiceState::default()
        })
    }

    fn set_background_service_port(&self, port: u16) -> Result<(), String> {
        validate_port(port)?;
        let engine = self.bundled_engine_dir()?;
        let runtime = self.bundled_runtime_path()?;
        let helper = engine.join("manager-service-status.ts");
        if !helper.is_file() {
            return Err("客户端缺少后台服务管理组件".to_string());
        }
        let output = Command::new(runtime)
            .arg(helper)
            .arg("set-port")
            .arg(port.to_string())
            .current_dir(engine)
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
        let codex_home = if matches!(
            &request.action,
            CommandAction::Sync | CommandAction::Restore
        ) {
            Some(crate::instances::codex_home_for(
                request.instance_id.as_deref(),
            )?)
        } else {
            None
        };
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
    ) {
        let action_label = action.label();
        let result = if matches!(action, CommandAction::Start) {
            if probe_health(port).is_some() {
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
                if correct_version {
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
        let Ok(text) = fs::read_to_string(self.manager_log_path()) else {
            return Vec::new();
        };
        let lines: Vec<&str> = text.lines().collect();
        lines[lines.len().saturating_sub(limit)..]
            .iter()
            .map(|line| self.redact(line))
            .collect()
    }

    pub fn open_dashboard_window(&self, port: u16) -> Result<(), String> {
        validate_port(port)?;
        let url = tauri::Url::parse(&format!("http://127.0.0.1:{port}"))
            .map_err(|error| format!("Dashboard 地址无效：{error}"))?;
        if let Some(window) = self.app.get_webview_window("opencodex-dashboard") {
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
        WebviewWindowBuilder::new(&self.app, "opencodex-dashboard", WebviewUrl::External(url))
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
        self.begin_mutation()?;
        let result = (|| {
            if open_codex_service_running() {
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
        let bundled_version = self
            .bundled_package_root()
            .ok()
            .and_then(|package| package_version(&package));
        let installed_versions = self.installed_managed_versions();
        let remote = self.run_engine_update_helper::<RemoteEngineCatalog>("catalog", None);
        Ok(build_engine_update_catalog(
            current_version,
            current_source,
            bundled_version,
            installed_versions,
            remote,
        ))
    }

    pub fn install_engine_version(
        &self,
        request: InstallEngineVersionRequest,
    ) -> Result<EngineInstallResult, String> {
        let version = validate_engine_version(&request.version)?;
        self.begin_mutation()?;
        let result = (|| {
            if open_codex_service_running() {
                return Err("更新或切换 Engine 前请先停止 OpenCodex 服务".to_string());
            }
            let bundled_version = package_version(&self.bundled_package_root()?);
            if bundled_version.as_deref() == Some(version.as_str()) {
                self.write_active_engine(None)?;
                match self.prune_managed_engine_history("") {
                    Ok(removed) if !removed.is_empty() => self.persist_log(
                        "system",
                        &format!("已清理旧 Engine 版本：{}", removed.join("、")),
                    ),
                    Err(error) => self.persist_log("stderr", &error),
                    _ => {}
                }
                return Ok(EngineInstallResult {
                    version,
                    source: "bundled".to_string(),
                    message: "已切换到客户端内置 Engine".to_string(),
                });
            }

            let package = self.managed_package_root(&version);
            let already_installed = validate_managed_package(&package, &version).is_ok();
            if !already_installed {
                let input = serde_json::to_vec(&serde_json::json!({
                    "version": version,
                    "engineRoot": self.managed_engine_root(),
                }))
                .map_err(|error| format!("无法生成 Engine 安装请求：{error}"))?;
                let installed: EngineHelperInstallResult =
                    self.run_engine_update_helper("install", Some(&input))?;
                if installed.version != version {
                    return Err("Engine 更新器返回了错误的版本".to_string());
                }
                validate_managed_package(&package, &version)?;
            }
            self.write_active_engine(Some(&version))?;
            match self.prune_managed_engine_history(&version) {
                Ok(removed) if !removed.is_empty() => self.persist_log(
                    "system",
                    &format!("已清理旧 Engine 版本：{}", removed.join("、")),
                ),
                Err(error) => self.persist_log("stderr", &error),
                _ => {}
            }
            Ok(EngineInstallResult {
                version: version.clone(),
                source: "managed".to_string(),
                message: if already_installed {
                    format!("已切换到 Engine v{version}")
                } else {
                    format!("Engine v{version} 下载、校验并激活完成")
                },
            })
        })();
        self.finish_mutation();
        if let Ok(value) = &result {
            self.persist_log("system", &value.message);
        }
        result
    }

    pub fn delete_engine_version(
        &self,
        request: DeleteEngineVersionRequest,
    ) -> Result<EngineDeleteResult, String> {
        let version = validate_engine_version(&request.version)?;
        self.begin_mutation()?;
        let result = (|| {
            if open_codex_service_running() {
                return Err("删除 Engine 版本前请先停止 OpenCodex 服务".to_string());
            }
            if self.active_managed_version().as_deref() == Some(version.as_str()) {
                return Err("当前正在使用的 Engine 版本不能删除，请先切换到其他版本".to_string());
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
            self.persist_log("system", &value.message);
        }
        result
    }

    pub fn activate_bundled_engine(&self) -> Result<EngineInstallResult, String> {
        self.begin_mutation()?;
        let result = (|| {
            if open_codex_service_running() {
                return Err("回退 Engine 前请先停止 OpenCodex 服务".to_string());
            }
            let version = package_version(&self.bundled_package_root()?)
                .ok_or_else(|| "无法读取客户端内置 Engine 版本".to_string())?;
            self.write_active_engine(None)?;
            match self.prune_managed_engine_history("") {
                Ok(removed) if !removed.is_empty() => self.persist_log(
                    "system",
                    &format!("已清理旧 Engine 版本：{}", removed.join("、")),
                ),
                Err(error) => self.persist_log("stderr", &error),
                _ => {}
            }
            Ok(EngineInstallResult {
                version: version.clone(),
                source: "bundled".to_string(),
                message: format!("已回退到客户端内置 Engine v{version}"),
            })
        })();
        self.finish_mutation();
        if let Ok(value) = &result {
            self.persist_log("system", &value.message);
        }
        result
    }

    fn run_engine_update_helper<T: DeserializeOwned>(
        &self,
        action: &str,
        input: Option<&[u8]>,
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
        let mut child = command
            .spawn()
            .map_err(|error| format!("无法启动 Engine 更新器：{error}"))?;
        if let Some(bytes) = input {
            child
                .stdin
                .take()
                .ok_or_else(|| "无法打开 Engine 更新器输入".to_string())?
                .write_all(bytes)
                .map_err(|error| format!("无法发送 Engine 更新请求：{error}"))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|error| format!("等待 Engine 更新器失败：{error}"))?;
        if !output.status.success() {
            let detail = self.redact(String::from_utf8_lossy(&output.stderr).trim());
            return Err(if detail.is_empty() {
                "Engine 更新失败".to_string()
            } else {
                detail
            });
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("Engine 更新器返回了无效结果：{error}"))
    }

    fn run_switcher_helper<T: DeserializeOwned>(
        &self,
        action: &str,
        input: Option<&[u8]>,
    ) -> Result<T, String> {
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
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });
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
    bundled_version: Option<String>,
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
        release.installed = installed_versions.contains(&release.version)
            || bundled_version.as_deref() == Some(release.version.as_str());
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

fn read_runtime_port() -> Option<u16> {
    let text = fs::read_to_string(config_dir()?.join("runtime-port.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let port = value.get("port")?.as_u64()?;
    u16::try_from(port).ok().filter(|port| *port > 0)
}

fn read_configured_port() -> Option<u16> {
    let text = fs::read_to_string(config_dir()?.join("config.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let port = value.get("port")?.as_u64()?;
    u16::try_from(port).ok().filter(|port| *port > 0)
}

fn running_open_codex_port() -> Option<u16> {
    let configured = read_configured_port().unwrap_or(DEFAULT_PORT);
    let runtime = read_runtime_port();
    runtime
        .filter(|port| probe_health(*port).is_some())
        .or_else(|| probe_health(configured).map(|_| configured))
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

fn open_codex_service_running() -> bool {
    let configured_port = read_configured_port().unwrap_or(DEFAULT_PORT);
    let runtime_port = read_runtime_port();
    runtime_port.is_some_and(|port| probe_health(port).is_some())
        || probe_health(configured_port).is_some()
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

#[cfg(test)]
mod tests {
    use super::{
        build_engine_update_catalog, codex_integration_is_enabled, config_is_initialized,
        configured_sidecar_models, managed_versions_to_remove, package_version,
        validate_engine_version, validate_managed_package, validate_port, RemoteEngineCatalog,
        DEFAULT_PORT,
    };
    use chrono::Utc;
    use std::{
        fs,
        io::{Read, Write},
        path::PathBuf,
        process::{Command, Stdio},
        sync::mpsc::{self, Receiver},
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    #[cfg(unix)]
    fn wait_for_output(receiver: &Receiver<Vec<u8>>, transcript: &mut Vec<u8>, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if String::from_utf8_lossy(transcript).contains(needle) {
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let chunk = receiver.recv_timeout(remaining).unwrap_or_else(|error| {
                panic!(
                    "did not receive {needle:?}: {error}; output: {}",
                    String::from_utf8_lossy(transcript)
                )
            });
            transcript.extend_from_slice(&chunk);
        }
    }

    #[test]
    fn reads_the_pinned_bundled_engine_version() {
        let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("opencodex-engine")
            .join("node_modules")
            .join("@bitkyc08")
            .join("opencodex");
        assert_eq!(package_version(&package).as_deref(), Some("2.27.0"));
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
    fn remote_catalog_failure_preserves_local_engine_versions() {
        let catalog = build_engine_update_catalog(
            Some("2.31.0".to_string()),
            "managed".to_string(),
            Some("2.27.0".to_string()),
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
    fn retains_three_recent_managed_versions_when_using_bundled_engine() {
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
    fn bundled_init_completes_without_node_or_npm_on_path() {
        let engine = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("opencodex-engine");
        let package = engine
            .join("node_modules")
            .join("@bitkyc08")
            .join("opencodex");
        let runtime = engine
            .join("node_modules")
            .join("bun")
            .join("bin")
            .join("bun.exe");
        let cli = package.join("src").join("cli").join("index.ts");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        let config_dir = std::env::temp_dir().join(format!(
            "opencodex-manager-init-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&config_dir).expect("create isolated config directory");

        let mut child = Command::new(runtime)
            .arg(cli)
            .arg("init")
            .current_dir(package)
            .env("OPENCODEX_HOME", &config_dir)
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start bundled Engine init");

        let mut stdout = child.stdout.take().expect("open Engine stdout");
        let (stdout_sender, stdout_receiver) = mpsc::channel();
        let stdout_reader = thread::spawn(move || {
            let mut collected = Vec::new();
            let mut buffer = [0_u8; 2048];
            loop {
                let size = stdout.read(&mut buffer).expect("read Engine stdout");
                if size == 0 {
                    return collected;
                }
                let chunk = buffer[..size].to_vec();
                collected.extend_from_slice(&chunk);
                let _ = stdout_sender.send(chunk);
            }
        });
        let mut stderr = child.stderr.take().expect("open Engine stderr");
        let stderr_reader = thread::spawn(move || {
            let mut collected = Vec::new();
            stderr
                .read_to_end(&mut collected)
                .expect("read Engine stderr");
            collected
        });

        let mut stdin = child.stdin.take().expect("open Engine stdin");
        let mut transcript = Vec::new();
        for (prompt, answer) in [
            ("Select default provider (number): ", "24"),
            ("API key (usually blank — press Enter): ", ""),
            ("Default model (optional): ", ""),
            ("Proxy port [10100]: ", ""),
            ("Inject into Codex config.toml? [Y/n]: ", "n"),
            ("Install Codex autostart shim? [Y/n]: ", "n"),
        ] {
            wait_for_output(&stdout_receiver, &mut transcript, prompt);
            writeln!(stdin, "{answer}").expect("answer isolated init prompt");
            stdin.flush().expect("flush isolated init answer");
        }
        wait_for_output(&stdout_receiver, &mut transcript, "Setup complete");
        drop(stdin);
        let status = child.wait().expect("wait for Engine init");
        let stdout = stdout_reader.join().expect("join stdout reader");
        let stderr = stderr_reader.join().expect("join stderr reader");
        let _ = fs::remove_dir_all(&config_dir);

        assert!(
            status.success(),
            "init stderr: {}",
            String::from_utf8_lossy(&stderr)
        );
        assert!(
            String::from_utf8_lossy(&stdout).contains("Setup complete"),
            "init did not reach completion"
        );
    }
}
