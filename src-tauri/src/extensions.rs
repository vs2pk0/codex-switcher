use base64::{engine::general_purpose::STANDARD, Engine};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, DirBuilder, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, LazyLock,
    },
    time::Duration,
};
use tauri_plugin_dialog::DialogExt;
use tempfile::{NamedTempFile, TempDir};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{oneshot, Mutex},
};
use zip::ZipArchive;

const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_WORKER_BYTES: u64 = 96 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 32 * 1024;
const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const PUBLIC_KEY: [u8; 32] = [
    0x4f, 0x2a, 0x3a, 0x76, 0xa0, 0xb8, 0xb3, 0x17, 0x84, 0x8a, 0xac, 0x78, 0xa7, 0x4b, 0x4d, 0xee,
    0x28, 0xc8, 0xff, 0xcb, 0x52, 0xc2, 0xbc, 0xe6, 0xf6, 0xe9, 0xd8, 0xca, 0xbe, 0xea, 0xec, 0x4f,
];

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Platform {
    id: String,
    label: String,
    login: LoginMode,
    #[serde(default)]
    login_options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon_png: Option<String>,
    #[serde(default)]
    local_import: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum LoginMode {
    Browser,
    BrowserCallback,
    Code,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    platforms: Vec<Platform>,
    #[serde(default)]
    account_actions: bool,
    #[serde(default)]
    usage: bool,
    #[serde(default)]
    usage_app_discovery: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    usage_icons: BTreeMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledExtension {
    #[serde(flatten)]
    extension: Extension,
    enabled: bool,
}

#[derive(Default, Serialize)]
pub struct ExtensionImportResult {
    installed: Vec<InstalledExtension>,
    failed: Vec<ExtensionImportFailure>,
}

#[derive(Serialize)]
struct ExtensionImportFailure {
    file: String,
    error: String,
}

#[derive(Deserialize)]
struct Manifest {
    protocol: u8,
    #[serde(flatten)]
    extension: Extension,
    targets: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Action {
    List,
    ImportCurrent,
    Import {
        json: String,
    },
    Refresh {
        account_id: String,
    },
    Edit {
        account_id: String,
        nickname: String,
        tags: Vec<String>,
    },
    SetPhone {
        account_id: String,
        phone: String,
    },
    SetPinned {
        account_id: String,
        pinned: bool,
    },
    Switch {
        account_id: String,
    },
    Export {
        account_id: String,
    },
    Remove {
        account_id: String,
    },
    LoginStart {
        option: Option<String>,
    },
    LoginFinish {
        login_id: String,
        code: Option<String>,
    },
    LoginCallback {
        login_id: String,
        code: String,
    },
    LoginCancel {
        login_id: String,
    },
    Usage {
        query: Value,
    },
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn valid_label(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 80 && !value.chars().any(char::is_control)
}

fn valid_icon(encoded: &str) -> bool {
    if encoded.len() > 4096 {
        return false;
    }
    let Ok(png) = STANDARD.decode(encoded) else {
        return false;
    };
    if png.len() < 33 || !png.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR") {
        return false;
    }
    (1..=64).contains(&u32::from_be_bytes(png[16..20].try_into().unwrap()))
        && (1..=64).contains(&u32::from_be_bytes(png[20..24].try_into().unwrap()))
}

fn target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    let extension = &manifest.extension;
    if manifest.protocol != 1
        || !valid_id(&extension.id)
        || !valid_label(&extension.name)
        || extension.version.len() > 40
        || semver::Version::parse(&extension.version).is_err()
        || (!extension.usage && extension.platforms.is_empty())
        || (extension.account_actions && extension.platforms.is_empty())
        || (extension.usage_app_discovery && !extension.usage)
        || extension.platforms.len() > 50
        || extension.usage_icons.len() > 50
        || (!extension.usage && !extension.usage_icons.is_empty())
        || !extension
            .usage_icons
            .iter()
            .all(|(id, icon)| valid_id(id) && valid_icon(icon))
        || manifest.targets.is_empty()
        || manifest.targets.len() > 6
    {
        return Err("插件清单无效，或需要更新的 Codex Switcher。".into());
    }
    let mut ids = HashSet::new();
    for platform in &extension.platforms {
        if !valid_id(&platform.id)
            || !ids.insert(&platform.id)
            || !valid_label(&platform.label)
            || platform
                .icon_png
                .as_deref()
                .is_some_and(|icon| !valid_icon(icon))
            || platform.login_options.len() > 10
            || !platform
                .login_options
                .iter()
                .all(|option| valid_label(option))
        {
            return Err("插件平台信息无效。".into());
        }
    }
    for (target, hash) in &manifest.targets {
        if !valid_id(target)
            || hash.len() != 64
            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("插件文件校验信息无效。".into());
        }
    }
    if !manifest.targets.contains_key(&target()) {
        return Err("这个插件包不适用于当前系统或处理器。".into());
    }
    Ok(())
}

fn read_entry(archive: &mut ZipArchive<File>, name: &str, max: u64) -> Result<Vec<u8>, String> {
    let entry = archive.by_name(name).map_err(|_| "插件包缺少必要文件。")?;
    if entry.size() > max {
        return Err("插件清单超出大小限制。".into());
    }
    let mut bytes = Vec::new();
    entry
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > max {
        return Err("插件清单超出大小限制。".into());
    }
    Ok(bytes)
}

fn io_error(error: io::Error) -> String {
    format!("读写插件失败（{}）。", error.kind())
}

struct Bundle {
    archive: ZipArchive<File>,
    manifest: Manifest,
}

impl Bundle {
    fn read(file: File) -> Result<Self, String> {
        if !file.metadata().map_err(io_error)?.is_file()
            || file.metadata().map_err(io_error)?.len() > MAX_ARCHIVE_BYTES
        {
            return Err("插件包无效或超出大小限制。".into());
        }
        let mut archive = ZipArchive::new(file).map_err(|_| "无法读取插件包。")?;
        if archive.len() > 9 {
            return Err("插件包内容无效。".into());
        }
        let manifest = read_entry(&mut archive, "manifest.json", MAX_MANIFEST_BYTES)?;
        let signature = read_entry(&mut archive, "manifest.sig", 64)?;
        UnparsedPublicKey::new(&ED25519, PUBLIC_KEY)
            .verify(&manifest, &signature)
            .map_err(|_| "插件签名无效，请使用可信来源提供的插件包。")?;
        let manifest: Manifest = serde_json::from_slice(&manifest).map_err(|_| "插件清单无效。")?;
        validate_manifest(&manifest)?;
        let mut names = HashSet::new();
        for index in 0..archive.len() {
            let entry = archive.by_index(index).map_err(|_| "插件包内容无效。")?;
            let name = entry.name();
            let allowed = matches!(name, "manifest.json" | "manifest.sig" | "NOTICE.md")
                || name
                    .strip_prefix("workers/")
                    .is_some_and(|key| manifest.targets.contains_key(key));
            if !entry.is_file() || !allowed || !names.insert(name.to_owned()) {
                return Err("插件包内容无效。".into());
            }
        }
        if manifest
            .targets
            .keys()
            .any(|key| !names.contains(&format!("workers/{key}")))
        {
            return Err("插件包缺少运行文件。".into());
        }
        Ok(Self { archive, manifest })
    }

    fn verify_worker(&mut self, mut destination: impl Write) -> Result<(), String> {
        let target = target();
        let mut worker = self
            .archive
            .by_name(&format!("workers/{target}"))
            .map_err(|_| "插件包缺少运行文件。")?;
        if worker.size() == 0 || worker.size() > MAX_WORKER_BYTES {
            return Err("插件运行文件大小无效。".into());
        }
        let expected = worker.size();
        let mut hasher = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        let mut size = 0;
        loop {
            let read = worker.read(&mut buffer).map_err(io_error)?;
            if read == 0 {
                break;
            }
            size += read as u64;
            if size > expected {
                return Err("插件运行文件大小无效。".into());
            }
            hasher.update(&buffer[..read]);
            destination.write_all(&buffer[..read]).map_err(io_error)?;
        }
        if size != expected
            || !format!("{:x}", hasher.finalize())
                .eq_ignore_ascii_case(&self.manifest.targets[&target])
        {
            return Err("插件运行文件校验失败。".into());
        }
        Ok(())
    }
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(io_error)
}

fn root() -> PathBuf {
    crate::switcher_data_dir().join("plugins")
}

fn installed_file(root: &Path, id: &str) -> PathBuf {
    extension_file(root, id, true)
}

fn extension_dir(root: &Path, enabled: bool) -> PathBuf {
    root.join(if enabled { "installed" } else { "disabled" })
}

fn extension_file(root: &Path, id: &str, enabled: bool) -> PathBuf {
    extension_dir(root, enabled).join(format!("{id}.csplugin"))
}

fn copy_bundle(root: &Path, source: &Path) -> Result<(NamedTempFile, Extension), String> {
    let directory = root.join("installed");
    create_private_dir(&directory)?;
    let mut staged = NamedTempFile::new_in(directory).map_err(io_error)?;
    let source = File::open(source).map_err(io_error)?;
    if !source.metadata().map_err(io_error)?.is_file() {
        return Err("请选择插件文件。".into());
    }
    let size = io::copy(&mut source.take(MAX_ARCHIVE_BYTES + 1), &mut staged).map_err(io_error)?;
    if size > MAX_ARCHIVE_BYTES {
        return Err("插件包超出大小限制。".into());
    }
    let mut bundle = Bundle::read(staged.reopen().map_err(io_error)?)?;
    bundle.verify_worker(io::sink())?;
    Ok((staged, bundle.manifest.extension))
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Reply>>>>;
static WORKERS: LazyLock<Mutex<HashMap<String, Arc<Worker>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Deserialize)]
struct Reply {
    id: u64,
    #[serde(default)]
    result: Value,
    error: Option<String>,
}

struct Worker {
    extension: Extension,
    input: Mutex<ChildStdin>,
    pending: Pending,
    serial: AtomicU64,
    alive: Arc<AtomicBool>,
    // Kill the worker before removing its executable on Windows.
    child: Child,
    _directory: TempDir,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

fn unpack_worker(root: &Path, id: &str) -> Result<(TempDir, PathBuf, Extension), String> {
    let mut bundle = Bundle::read(File::open(installed_file(root, id)).map_err(io_error)?)?;
    if bundle.manifest.extension.id != id {
        return Err("插件标识与安装文件不符。".into());
    }
    let directory = tempfile::Builder::new()
        .prefix("codex-switcher-plugin-")
        .tempdir()
        .map_err(io_error)?;
    let executable = directory.path().join(if cfg!(windows) {
        "worker.exe"
    } else {
        "worker"
    });
    let file = File::create(&executable).map_err(io_error)?;
    bundle.verify_worker(&file)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o700))
            .map_err(io_error)?;
    }
    Ok((directory, executable, bundle.manifest.extension))
}

impl Worker {
    async fn start(root: PathBuf, id: String) -> Result<Self, String> {
        let data_dir = root.join("data").join(&id);
        let (directory, executable, extension) = tauri::async_runtime::spawn_blocking(move || {
            create_private_dir(&root.join("data").join(&id))?;
            unpack_worker(&root, &id)
        })
        .await
        .map_err(|_| "无法读取插件。")??;
        let mut command = Command::new(&executable);
        command
            .env("CODEX_SWITCHER_PLUGIN_DATA_DIR", &data_dir)
            .env("COCKPIT_TOOLS_DATA_DIR", &data_dir)
            .env_remove("COCKPIT_TOOLS_TEST_DATA_DIR")
            .env_remove("COCKPIT_TEST_DATA_DIR")
            .current_dir(directory.path())
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command.spawn().map_err(|_| "无法启动插件。")?;
        let input = child.stdin.take().ok_or("无法连接插件。")?;
        let output = child.stdout.take().ok_or("无法连接插件。")?;
        let pending: Pending = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));
        let (reader_pending, reader_alive) = (pending.clone(), alive.clone());
        tauri::async_runtime::spawn(async move {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = Vec::new();
                let read = (&mut reader)
                    .take((MAX_RESPONSE_BYTES + 1) as u64)
                    .read_until(b'\n', &mut line)
                    .await;
                if !matches!(read, Ok(size) if size > 0 && size <= MAX_RESPONSE_BYTES)
                    || line.last() != Some(&b'\n')
                {
                    break;
                }
                let Ok(reply) = serde_json::from_slice::<Reply>(&line) else {
                    break;
                };
                if let Some(sender) = reader_pending.lock().await.remove(&reply.id) {
                    let _ = sender.send(reply);
                }
            }
            reader_alive.store(false, Ordering::Release);
            reader_pending.lock().await.clear();
        });
        Ok(Self {
            extension,
            input: Mutex::new(input),
            pending,
            serial: AtomicU64::new(1),
            alive,
            child,
            _directory: directory,
        })
    }

    async fn request(&self, platform: Option<&str>, action: Action) -> Result<Value, String> {
        let duration = match &action {
            Action::LoginFinish { .. } => Duration::from_secs(900),
            Action::Usage { query } if query["operation"] == "sync" => Duration::from_secs(900),
            Action::Usage { .. } => Duration::from_secs(150),
            _ => Duration::from_secs(90),
        };
        let id = self.serial.fetch_add(1, Ordering::Relaxed);
        let mut message = serde_json::to_vec(&serde_json::json!({
            "id": id, "platform": platform, "action": action
        }))
        .map_err(|_| "插件请求无效。")?;
        if message.len() > MAX_REQUEST_BYTES {
            return Err("插件请求超出大小限制。".into());
        }
        message.push(b'\n');
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);
        let written = tokio::time::timeout(Duration::from_secs(5), async {
            self.input.lock().await.write_all(&message).await
        })
        .await;
        if !matches!(written, Ok(Ok(()))) {
            self.alive.store(false, Ordering::Release);
            self.pending.lock().await.remove(&id);
            return Err("插件连接已断开，请重试。".into());
        }
        let result = tokio::time::timeout(duration, receiver).await;
        self.pending.lock().await.remove(&id);
        let reply = result
            .map_err(|_| "等待插件响应超时，请重试。")?
            .map_err(|_| "插件已退出，请重试。")?;
        match reply.error {
            Some(error) => Err(error),
            None => Ok(reply.result),
        }
    }
}

#[tauri::command]
pub async fn extension_list() -> Result<Vec<InstalledExtension>, String> {
    let _guard = WORKERS.lock().await;
    tauri::async_runtime::spawn_blocking(|| list_installed(&root()))
        .await
        .map_err(|_| "无法读取已安装的插件。")?
}

fn list_installed(root: &Path) -> Result<Vec<InstalledExtension>, String> {
    let mut list = Vec::new();
    for enabled in [true, false] {
        let entries = match fs::read_dir(extension_dir(root, enabled)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io_error(error)),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("csplugin") {
                continue;
            }
            if let Ok(bundle) = File::open(&path).map_err(io_error).and_then(Bundle::read) {
                let extension = bundle.manifest.extension;
                if path.file_stem().and_then(|value| value.to_str()) == Some(extension.id.as_str())
                {
                    list.push(InstalledExtension { extension, enabled });
                }
            }
        }
    }
    list.sort_by(|left, right| {
        left.extension
            .name
            .cmp(&right.extension.name)
            .then(left.extension.id.cmp(&right.extension.id))
    });
    Ok(list)
}

#[tauri::command]
pub async fn extension_import(app: tauri::AppHandle) -> Result<ExtensionImportResult, String> {
    let paths = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Codex Switcher Plugin", &["csplugin"])
            .blocking_pick_files()
    })
    .await
    .map_err(|_| "无法选择插件包。")?;
    let mut result = ExtensionImportResult::default();
    for path in paths.into_iter().flatten() {
        let Ok(path) = path.into_path() else {
            result.failed.push(ExtensionImportFailure {
                file: "插件包".into(),
                error: "请选择本地插件包。".into(),
            });
            continue;
        };
        match install_extension(path.clone()).await {
            Ok(extension) => result.installed.push(extension),
            Err(error) => result.failed.push(ExtensionImportFailure {
                file: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                error,
            }),
        }
    }
    Ok(result)
}

async fn install_extension(path: PathBuf) -> Result<InstalledExtension, String> {
    let root = root();
    let staging_root = root.clone();
    let (file, extension) =
        tauri::async_runtime::spawn_blocking(move || copy_bundle(&staging_root, &path))
            .await
            .map_err(|_| "无法导入插件。")??;
    let mut workers = WORKERS.lock().await;
    require_idle(&workers, &extension.id)?;
    let id = extension.id.clone();
    let enabled = tauri::async_runtime::spawn_blocking(move || {
        let disabled = extension_file(&root, &id, false);
        let enabled = !disabled.try_exists().map_err(io_error)?;
        let destination = if enabled {
            installed_file(&root, &id)
        } else {
            disabled
        };
        file.as_file().sync_all().map_err(io_error)?;
        file.persist(destination)
            .map_err(|error| io_error(error.error))?;
        Ok::<bool, String>(enabled)
    })
    .await
    .map_err(|_| "无法保存插件。")??;
    workers.remove(&extension.id);
    Ok(InstalledExtension { extension, enabled })
}

fn require_idle(workers: &HashMap<String, Arc<Worker>>, id: &str) -> Result<(), String> {
    if workers
        .get(id)
        .is_some_and(|worker| Arc::strong_count(worker) > 1)
    {
        return Err("插件正在执行操作，请稍后再试。".into());
    }
    Ok(())
}

fn move_extension(root: &Path, id: &str, enabled: bool) -> Result<(), String> {
    let source = extension_file(root, id, !enabled);
    let destination = extension_file(root, id, enabled);
    if destination.try_exists().map_err(io_error)? {
        return Ok(());
    }
    create_private_dir(&extension_dir(root, enabled))?;
    fs::rename(source, destination).map_err(io_error)
}

#[tauri::command]
pub async fn extension_set_enabled(extension_id: String, enabled: bool) -> Result<(), String> {
    if !valid_id(&extension_id) {
        return Err("插件标识无效。".into());
    }
    let mut workers = WORKERS.lock().await;
    require_idle(&workers, &extension_id)?;
    let id = extension_id.clone();
    tauri::async_runtime::spawn_blocking(move || move_extension(&root(), &id, enabled))
        .await
        .map_err(|_| "无法更改插件状态。")??;
    workers.remove(&extension_id);
    Ok(())
}

#[tauri::command]
pub async fn extension_uninstall(extension_id: String) -> Result<(), String> {
    if !valid_id(&extension_id) {
        return Err("插件标识无效。".into());
    }
    let mut workers = WORKERS.lock().await;
    require_idle(&workers, &extension_id)?;
    let id = extension_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = root();
        let path = installed_file(&root, &id);
        match fs::remove_file(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::remove_file(extension_file(&root, &id, false)).map_err(io_error)
            }
            result => result.map_err(io_error),
        }
    })
    .await
    .map_err(|_| "无法卸载插件。")??;
    workers.remove(&extension_id);
    Ok(())
}

#[tauri::command]
pub async fn extension_call(
    extension_id: String,
    platform: Option<String>,
    action: Action,
) -> Result<Value, String> {
    if !valid_id(&extension_id) {
        return Err("插件标识无效。".into());
    }
    let worker = {
        let mut workers = WORKERS.lock().await;
        if !workers
            .get(&extension_id)
            .is_some_and(|worker| worker.alive.load(Ordering::Acquire))
        {
            workers.remove(&extension_id);
            let worker = Worker::start(root(), extension_id.clone()).await?;
            workers.insert(extension_id.clone(), Arc::new(worker));
        }
        Arc::clone(&workers[&extension_id])
    };
    let allowed = match &action {
        Action::Usage { query } => {
            platform.is_none()
                && worker.extension.usage
                && (query["operation"] != "applications" || worker.extension.usage_app_discovery)
        }
        _ => platform.as_ref().is_some_and(|id| {
            worker.extension.platforms.iter().any(|item| {
                &item.id == id
                    && match &action {
                        Action::ImportCurrent => item.local_import,
                        Action::Edit { .. }
                        | Action::SetPhone { .. }
                        | Action::SetPinned { .. }
                        | Action::Switch { .. }
                        | Action::Export { .. } => worker.extension.account_actions,
                        _ => true,
                    }
            })
        }),
    };
    if !allowed {
        return Err("插件未提供此功能。".into());
    }
    worker.request(platform.as_deref(), action).await
}
