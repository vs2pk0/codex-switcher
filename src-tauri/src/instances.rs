use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub const DEFAULT_INSTANCE_ID: &str = "default";
const INSTANCES_FILE: &str = "codex-instances.json";
const OPENCODEX_SECTION_MARKER: &str = "# Auto-injected by opencodex";
const OPENCODEX_JOURNAL_FILE: &str = "opencodex-journal.json";
static INSTANCE_OPERATION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoredCodexInstance {
    id: String,
    name: String,
    codex_home: String,
    electron_data: String,
    app_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    retired_data_paths: Vec<String>,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexInstance {
    pub id: String,
    pub name: String,
    pub codex_home: String,
    pub electron_data: String,
    pub app_path: String,
    pub workspace: Option<String>,
    pub created_at: i64,
    pub is_default: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub open_codex_connected: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexInstanceCapabilities {
    pub managed_instances_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodexInstanceLocation {
    pub id: String,
    pub name: String,
    pub codex_home: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCodexInstanceResult {
    pub instance_id: String,
    pub instance_name: String,
    pub deleted_paths: Vec<String>,
    pub deleted_backup_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCodexInstanceInput {
    pub id: Option<String>,
    pub name: String,
    pub codex_home: Option<String>,
    pub electron_data: Option<String>,
    pub app_path: Option<String>,
    pub workspace: Option<String>,
}

fn instances_path() -> PathBuf {
    crate::switcher_config_data_dir().join(INSTANCES_FILE)
}

fn default_codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
        .unwrap_or_else(|| PathBuf::from(".codex"))
}

fn default_electron_data() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("Codex")
}

fn default_app_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        for candidate in ["/Applications/ChatGPT.app", "/Applications/Codex.app"] {
            let path = PathBuf::from(candidate);
            if path.is_dir() {
                return path;
            }
        }
        PathBuf::from("/Applications/ChatGPT.app")
    }
    #[cfg(target_os = "windows")]
    {
        PathBuf::from("Codex.exe")
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        PathBuf::from("codex")
    }
}

fn default_instance() -> StoredCodexInstance {
    StoredCodexInstance {
        id: DEFAULT_INSTANCE_ID.to_string(),
        name: "系统默认实例".to_string(),
        codex_home: default_codex_home().to_string_lossy().to_string(),
        electron_data: default_electron_data().to_string_lossy().to_string(),
        app_path: default_app_path().to_string_lossy().to_string(),
        workspace: None,
        retired_data_paths: Vec::new(),
        created_at: 0,
    }
}

fn managed_instances_supported() -> bool {
    cfg!(target_os = "macos")
}

fn managed_instances_unavailable_error() -> String {
    "Codex 多开目前仅支持 macOS，其他平台暂不开放此功能".to_string()
}

fn read_stored_instances() -> Result<Vec<StoredCodexInstance>, String> {
    let path = instances_path();
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let content =
        fs::read_to_string(&path).map_err(|error| format!("读取 Codex 实例配置失败：{error}"))?;
    let instances = serde_json::from_str::<Vec<StoredCodexInstance>>(&content)
        .map_err(|error| format!("解析 Codex 实例配置失败：{error}"))?;
    Ok(instances
        .into_iter()
        .filter(|instance| instance.id != DEFAULT_INSTANCE_ID)
        .collect())
}

fn write_stored_instances(instances: &[StoredCodexInstance]) -> Result<(), String> {
    let path = instances_path();
    let parent = path
        .parent()
        .ok_or_else(|| "Codex 实例配置目录无效".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建实例配置目录失败：{error}"))?;
    let temporary = parent.join(format!(".{INSTANCES_FILE}.{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec_pretty(instances)
        .map_err(|error| format!("生成 Codex 实例配置失败：{error}"))?;
    fs::write(&temporary, bytes).map_err(|error| format!("写入 Codex 实例配置失败：{error}"))?;
    fs::rename(&temporary, &path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("保存 Codex 实例配置失败：{error}")
    })
}

fn clean_required(value: Option<String>, label: &str) -> Result<String, String> {
    let value = value.unwrap_or_default().trim().to_string();
    if value.is_empty() {
        Err(format!("{label}不能为空"))
    } else {
        Ok(value)
    }
}

fn absolute_directory(value: String, label: &str, create: bool) -> Result<String, String> {
    let path = PathBuf::from(&value);
    if !path.is_absolute() {
        return Err(format!("{label}必须使用绝对路径"));
    }
    if create {
        fs::create_dir_all(&path).map_err(|error| format!("创建{label}失败：{error}"))?;
    }
    if !path.is_dir() {
        return Err(format!("{label}不是有效目录：{}", path.display()));
    }
    fs::canonicalize(&path)
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|error| format!("解析{label}失败：{error}"))
}

fn normalize_optional_directory(
    value: Option<String>,
    label: &str,
) -> Result<Option<String>, String> {
    let Some(value) = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    absolute_directory(value, label, false).map(Some)
}

fn make_instance_id() -> String {
    let suffix: u32 = rand::thread_rng().gen();
    format!("instance-{}-{suffix:08x}", Utc::now().timestamp_millis())
}

fn validate_instance_id(id: &str) -> Result<(), String> {
    if id.len() > 128
        || id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("实例 ID 格式无效".to_string());
    }
    Ok(())
}

fn default_profile_root(id: &str) -> PathBuf {
    crate::switcher_data_dir().join("instances").join(id)
}

fn normalize_input(input: SaveCodexInstanceInput) -> Result<StoredCodexInstance, String> {
    let id = input
        .id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(make_instance_id);
    validate_instance_id(&id)?;
    if id == DEFAULT_INSTANCE_ID {
        return Err("系统默认实例不能修改".to_string());
    }
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("实例名称不能为空".to_string());
    }
    let root = default_profile_root(&id);
    let codex_home = absolute_directory(
        input
            .codex_home
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| root.join("codex-home").to_string_lossy().to_string()),
        "Codex Home",
        true,
    )?;
    let electron_data = absolute_directory(
        input
            .electron_data
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| root.join("electron-data").to_string_lossy().to_string()),
        "桌面数据目录",
        true,
    )?;
    if codex_home == electron_data {
        return Err("Codex Home 与桌面数据目录不能相同".to_string());
    }
    let app_path = clean_required(
        input
            .app_path
            .or_else(|| Some(default_app_path().to_string_lossy().to_string())),
        "官方 App 路径",
    )?;
    let app = PathBuf::from(&app_path);
    if !app.is_absolute() || !app.is_dir() {
        return Err(format!("官方 App 路径无效：{}", app.display()));
    }
    let app_path = fs::canonicalize(app)
        .map_err(|error| format!("解析官方 App 路径失败：{error}"))?
        .to_string_lossy()
        .to_string();
    ensure_isolated_desktop_data_supported(&app_path)?;
    let workspace = normalize_optional_directory(input.workspace, "工作区")?;
    Ok(StoredCodexInstance {
        id,
        name,
        codex_home,
        electron_data,
        app_path,
        workspace,
        retired_data_paths: Vec::new(),
        created_at: Utc::now().timestamp(),
    })
}

fn validate_unique(
    candidate: &StoredCodexInstance,
    existing: &[StoredCodexInstance],
) -> Result<(), String> {
    let managed_root = default_profile_root(&candidate.id);
    for (path, label) in std::iter::once((&candidate.codex_home, "Codex Home"))
        .chain(std::iter::once((&candidate.electron_data, "桌面数据目录")))
        .chain(
            candidate
                .retired_data_paths
                .iter()
                .map(|path| (path, "历史实例数据目录")),
        )
    {
        let path = normalize_stored_path(Path::new(path), label)?;
        validate_owned_data_target(&path, candidate, existing, &managed_root)?;
    }
    Ok(())
}

fn normalize_stored_path(path: &Path, label: &str) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("{label}必须使用绝对路径"));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(format!("{label}不能包含 . 或 .. 路径段"));
    }
    if path.exists() {
        fs::canonicalize(path).map_err(|error| format!("解析{label}失败：{error}"))
    } else {
        Ok(path.to_path_buf())
    }
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn normal_component_count(path: &Path) -> usize {
    path.components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count()
}

fn validate_owned_data_target(
    target: &Path,
    instance: &StoredCodexInstance,
    existing: &[StoredCodexInstance],
    managed_root: &Path,
) -> Result<(), String> {
    if normal_component_count(target) < 3 {
        return Err(format!(
            "实例数据目录范围过大，拒绝使用：{}",
            target.display()
        ));
    }

    if let Some(home) = dirs::home_dir() {
        let home = normalize_stored_path(&home, "用户目录")?;
        if target == home || home.starts_with(target) {
            return Err(format!(
                "实例数据目录不能等于或包含用户目录：{}",
                target.display()
            ));
        }
    }

    let switcher_root = normalize_stored_path(&crate::switcher_data_dir(), "应用数据目录")?;
    let normalized_managed_root = normalize_stored_path(managed_root, "实例托管目录")?;
    let inside_managed_root = target == managed_root
        || target.starts_with(managed_root)
        || target == normalized_managed_root
        || target.starts_with(&normalized_managed_root);
    if !inside_managed_root && paths_overlap(target, &switcher_root) {
        return Err(format!(
            "自定义实例目录不能占用 Codex Switcher 的共享数据目录：{}",
            target.display()
        ));
    }

    let default = default_instance();
    for (protected, label) in [
        (&default.codex_home, "系统默认 Codex Home"),
        (&default.electron_data, "系统默认桌面数据目录"),
    ] {
        let protected = normalize_stored_path(Path::new(protected), label)?;
        if paths_overlap(target, &protected) {
            return Err(format!(
                "实例数据目录不能与{label}重叠：{}",
                target.display()
            ));
        }
    }

    for owner in std::iter::once(instance)
        .chain(std::iter::once(&default))
        .chain(existing.iter())
    {
        for (protected, label) in [
            (Some(owner.app_path.as_str()), "官方 App"),
            (owner.workspace.as_deref(), "工作区"),
        ] {
            let Some(protected) = protected else {
                continue;
            };
            let protected = normalize_stored_path(Path::new(protected), label)?;
            if paths_overlap(target, &protected) {
                return Err(format!(
                    "实例数据目录不能与实例“{}”的{label}重叠：{}",
                    owner.name,
                    target.display()
                ));
            }
        }

        if owner.id == instance.id {
            continue;
        }
        for (other_path, label) in std::iter::once((&owner.codex_home, "Codex Home"))
            .chain(std::iter::once((&owner.electron_data, "桌面数据目录")))
            .chain(
                owner
                    .retired_data_paths
                    .iter()
                    .map(|path| (path, "历史实例数据目录")),
            )
        {
            let other_path = normalize_stored_path(Path::new(other_path), label)?;
            if paths_overlap(target, &other_path) {
                return Err(format!(
                    "实例数据目录不能与实例“{}”的数据目录重叠：{}",
                    owner.name,
                    target.display()
                ));
            }
        }
    }
    Ok(())
}

fn instance_deletion_roots(
    instance: &StoredCodexInstance,
    existing: &[StoredCodexInstance],
) -> Result<Vec<PathBuf>, String> {
    validate_instance_id(&instance.id)?;
    let instances_root = crate::switcher_data_dir().join("instances");
    let managed_root = default_profile_root(&instance.id);
    if managed_root.parent() != Some(instances_root.as_path()) {
        return Err("实例托管目录校验失败，已拒绝删除".to_string());
    }

    let mut candidates = vec![managed_root.clone()];
    for (value, label) in std::iter::once((&instance.codex_home, "Codex Home"))
        .chain(std::iter::once((&instance.electron_data, "桌面数据目录")))
        .chain(
            instance
                .retired_data_paths
                .iter()
                .map(|path| (path, "历史实例数据目录")),
        )
    {
        candidates.push(normalize_stored_path(Path::new(value), label)?);
    }
    for target in &candidates {
        validate_owned_data_target(target, instance, existing, &managed_root)?;
    }

    candidates.sort_by_key(|path| normal_component_count(path));
    let mut roots = Vec::<PathBuf>::new();
    for candidate in candidates {
        if !roots.iter().any(|root| candidate.starts_with(root)) {
            roots.push(candidate);
        }
    }
    Ok(roots)
}

fn short_path_hash(path: &Path) -> String {
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .take(12)
        .collect()
}

fn remove_owned_path(path: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("读取待删除数据失败（{}）：{error}", path.display())),
    };
    let result = if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    result
        .map(|_| true)
        .map_err(|error| format!("删除实例数据失败（{}）：{error}", path.display()))
}

fn session_backup_source_instance_id(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut manifest = archive.by_name("backup.json").ok()?;
    if manifest.size() > 1024 * 1024 {
        return None;
    }
    let mut content = String::new();
    manifest.read_to_string(&mut content).ok()?;
    serde_json::from_str::<serde_json::Value>(&content)
        .ok()?
        .pointer("/sourceInstance/id")?
        .as_str()
        .map(str::to_string)
}

fn attributed_session_backups(instance_id: &str) -> Result<Vec<PathBuf>, String> {
    let backup_dir = crate::switcher_data_dir().join("session");
    let entries = match fs::read_dir(&backup_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("读取会话备份目录失败：{error}")),
    };
    let mut backups = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("读取会话备份失败：{error}"))?;
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|error| format!("读取会话备份类型失败：{error}"))?
            .is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("zip")
        {
            continue;
        }
        if session_backup_source_instance_id(&path).as_deref() == Some(instance_id) {
            backups.push(path);
        }
    }
    Ok(backups)
}

fn collect_session_path_hashes(codex_home: &Path) -> Result<HashSet<String>, String> {
    let mut hashes = HashSet::new();
    let mut pending = ["sessions", "archived_sessions"]
        .into_iter()
        .map(|name| codex_home.join(name))
        .collect::<Vec<_>>();
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "读取实例会话目录失败（{}）：{error}",
                    directory.display()
                ))
            }
        };
        for entry in entries {
            let entry = entry.map_err(|error| format!("读取实例会话文件失败：{error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("读取实例会话文件类型失败：{error}"))?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("jsonl")
            {
                hashes.insert(short_path_hash(&entry.path()));
            }
        }
    }
    collect_trashed_session_path_hashes(codex_home, &mut hashes)?;
    Ok(hashes)
}

fn collect_trashed_session_path_hashes(
    codex_home: &Path,
    hashes: &mut HashSet<String>,
) -> Result<(), String> {
    let trash_hash = short_path_hash(codex_home);
    let trash_dirs = [
        crate::switcher_data_dir()
            .join("session-trash")
            .join(trash_hash),
        codex_home.join(".codex-switcher").join("session-trash"),
    ];
    for trash_dir in trash_dirs {
        let entries = match fs::read_dir(&trash_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "读取实例会话回收站失败（{}）：{error}",
                    trash_dir.display()
                ))
            }
        };
        for entry in entries {
            let entry = entry.map_err(|error| format!("读取会话回收站记录失败：{error}"))?;
            if !entry
                .file_type()
                .map_err(|error| format!("读取会话回收站记录类型失败：{error}"))?
                .is_file()
                || entry.path().extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }
            let content = fs::read_to_string(entry.path())
                .map_err(|error| format!("读取会话回收站元数据失败：{error}"))?;
            let Some(original_path) = serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|value| {
                    value
                        .get("originalPath")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
            else {
                continue;
            };
            let original_path = PathBuf::from(original_path);
            if original_path.is_absolute()
                && !original_path
                    .components()
                    .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
                && original_path.starts_with(codex_home)
            {
                hashes.insert(short_path_hash(&original_path));
            }
        }
    }
    Ok(())
}

fn attributed_session_edit_backups(
    session_hashes: &HashSet<String>,
) -> Result<Vec<PathBuf>, String> {
    if session_hashes.is_empty() {
        return Ok(Vec::new());
    }
    let backup_dir = crate::switcher_data_dir().join("session-edit-backups");
    let entries = match fs::read_dir(&backup_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("读取会话编辑备份目录失败：{error}")),
    };
    let mut backups = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("读取会话编辑备份失败：{error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("读取会话编辑备份类型失败：{error}"))?
            .is_file()
        {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if session_hashes
            .iter()
            .any(|hash| name.ends_with(&format!("-{hash}.jsonl")))
        {
            backups.push(entry.path());
        }
    }
    Ok(backups)
}

fn app_executable(instance: &StoredCodexInstance) -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let macos = PathBuf::from(&instance.app_path)
            .join("Contents")
            .join("MacOS");
        for name in ["ChatGPT", "Codex"] {
            let candidate = macos.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        let candidate = fs::read_dir(&macos)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.is_file());
        candidate.ok_or_else(|| format!("App 缺少可执行文件：{}", macos.display()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(PathBuf::from(&instance.app_path))
    }
}

#[cfg(target_os = "macos")]
fn ensure_isolated_desktop_data_supported(app_path: &str) -> Result<(), String> {
    const MARKER: &[u8] = b"CODEX_ELECTRON_USER_DATA_PATH";
    const CHUNK_SIZE: usize = 1024 * 1024;
    let asar_path = PathBuf::from(app_path)
        .join("Contents")
        .join("Resources")
        .join("app.asar");
    let mut file = File::open(&asar_path).map_err(|error| {
        format!(
            "无法检查官方 App 的多开兼容性（{}）：{error}",
            asar_path.display()
        )
    })?;
    let mut chunk = vec![0_u8; CHUNK_SIZE];
    let mut overlap = Vec::new();
    loop {
        let count = file
            .read(&mut chunk)
            .map_err(|error| format!("读取官方 App 兼容性信息失败：{error}"))?;
        if count == 0 {
            break;
        }
        let mut searchable = Vec::with_capacity(overlap.len() + count);
        searchable.extend_from_slice(&overlap);
        searchable.extend_from_slice(&chunk[..count]);
        if searchable
            .windows(MARKER.len())
            .any(|window| window == MARKER)
        {
            return Ok(());
        }
        let keep = MARKER.len().saturating_sub(1).min(searchable.len());
        overlap.clear();
        overlap.extend_from_slice(&searchable[searchable.len() - keep..]);
    }
    Err("当前官方 App 未检测到独立桌面数据目录能力，暂时不能安全多开".to_string())
}

#[cfg(not(target_os = "macos"))]
fn ensure_isolated_desktop_data_supported(_app_path: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
fn process_rows() -> Vec<(u32, String)> {
    let output = Command::new("ps").args(["-axo", "pid=,command="]).output();
    let Ok(output) = output else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let split = trimmed.find(char::is_whitespace)?;
            let pid = trimmed[..split].parse::<u32>().ok()?;
            Some((pid, trimmed[split..].trim().to_string()))
        })
        .collect()
}

#[cfg(any(windows, test))]
fn parse_windows_tasklist_codex_pids(output: &str) -> Vec<u32> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.trim().trim_matches('"').split("\",\"");
            let image_name = fields.next()?;
            let pid = fields.next()?;
            image_name
                .eq_ignore_ascii_case("Codex.exe")
                .then(|| pid.parse::<u32>().ok())
                .flatten()
        })
        .collect()
}

#[cfg(windows)]
fn windows_default_codex_pids() -> Vec<u32> {
    let mut command = Command::new("tasklist");
    command.args(["/FI", "IMAGENAME eq Codex.exe", "/FO", "CSV", "/NH"]);
    hide_command_window(&mut command);
    let Ok(output) = command.output() else {
        return Vec::new();
    };
    parse_windows_tasklist_codex_pids(&String::from_utf8_lossy(&output.stdout))
}

fn launch_user_data_arg(instance: &StoredCodexInstance) -> Option<String> {
    (instance.id != DEFAULT_INSTANCE_ID)
        .then(|| format!("--user-data-dir={}", instance.electron_data))
}

fn process_matches_instance(
    instance: &StoredCodexInstance,
    executable: &str,
    command: &str,
) -> bool {
    if !command.starts_with(executable) {
        return false;
    }
    let user_data_arg = format!("--user-data-dir={}", instance.electron_data);
    if instance.id == DEFAULT_INSTANCE_ID {
        !command.contains("--user-data-dir=") || command.contains(&user_data_arg)
    } else {
        command.contains(&user_data_arg)
    }
}

fn live_pid(instance: &StoredCodexInstance) -> Option<u32> {
    #[cfg(windows)]
    {
        if instance.id != DEFAULT_INSTANCE_ID {
            return None;
        }
        windows_default_codex_pids().into_iter().next()
    }
    #[cfg(not(windows))]
    {
        let executable = app_executable(instance).ok()?.to_string_lossy().to_string();
        process_rows().into_iter().find_map(|(pid, command)| {
            process_matches_instance(instance, &executable, &command).then_some(pid)
        })
    }
}

fn public_instance(instance: StoredCodexInstance) -> CodexInstance {
    let pid = live_pid(&instance);
    let open_codex_connected = codex_home_has_opencodex_routing(Path::new(&instance.codex_home));
    CodexInstance {
        id: instance.id.clone(),
        name: instance.name,
        codex_home: instance.codex_home,
        electron_data: instance.electron_data,
        app_path: instance.app_path,
        workspace: instance.workspace,
        created_at: instance.created_at,
        is_default: instance.id == DEFAULT_INSTANCE_ID,
        running: pid.is_some(),
        pid,
        open_codex_connected,
    }
}

fn instance_location(instance: StoredCodexInstance) -> CodexInstanceLocation {
    CodexInstanceLocation {
        id: instance.id,
        name: instance.name,
        codex_home: PathBuf::from(instance.codex_home),
    }
}

fn codex_home_has_opencodex_routing(codex_home: &Path) -> bool {
    let Ok(config) = fs::read_to_string(codex_home.join("config.toml")) else {
        return false;
    };
    if marker_owned_openai_base_url(&config) {
        return true;
    }

    let Ok(document) = config.parse::<toml_edit::Document>() else {
        return false;
    };
    let legacy_provider = document
        .get("model_provider")
        .and_then(toml_edit::Item::as_str)
        == Some("opencodex")
        && document
            .get("model_providers")
            .and_then(toml_edit::Item::as_table_like)
            .and_then(|providers| providers.get("opencodex"))
            .and_then(toml_edit::Item::as_table_like)
            .and_then(|provider| provider.get("base_url"))
            .and_then(toml_edit::Item::as_str)
            .is_some();
    if legacy_provider {
        return true;
    }

    let Some(active_base_url) = document
        .get("openai_base_url")
        .and_then(toml_edit::Item::as_str)
    else {
        return false;
    };
    let Ok(journal) = fs::read_to_string(codex_home.join(OPENCODEX_JOURNAL_FILE)) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&journal)
        .ok()
        .is_some_and(|value| {
            value.get("version").and_then(serde_json::Value::as_u64) == Some(1)
                && value
                    .get("injectedOpenaiBaseUrl")
                    .and_then(serde_json::Value::as_str)
                    == Some(active_base_url)
        })
}

fn marker_owned_openai_base_url(config: &str) -> bool {
    let mut previous_was_marker = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            break;
        }
        if previous_was_marker
            && trimmed
                .split_once('=')
                .is_some_and(|(key, _)| key.trim() == "openai_base_url")
        {
            return true;
        }
        previous_was_marker = trimmed == OPENCODEX_SECTION_MARKER;
    }
    false
}

pub fn resolve_instance(instance_id: &str) -> Result<CodexInstance, String> {
    let id = instance_id.trim();
    if id.is_empty() || id == DEFAULT_INSTANCE_ID {
        return Ok(public_instance(default_instance()));
    }
    if !managed_instances_supported() {
        return Err(managed_instances_unavailable_error());
    }
    read_stored_instances()?
        .into_iter()
        .find(|instance| instance.id == id)
        .map(public_instance)
        .ok_or_else(|| "Codex 实例不存在".to_string())
}

pub fn codex_home_for(instance_id: Option<&str>) -> Result<PathBuf, String> {
    Ok(PathBuf::from(
        resolve_instance(instance_id.unwrap_or(DEFAULT_INSTANCE_ID))?.codex_home,
    ))
}

pub(crate) fn resolve_usage_instance_locations(
    instance_id: Option<&str>,
    all_instances: bool,
) -> Result<Vec<CodexInstanceLocation>, String> {
    if !all_instances {
        return stored_instance(instance_id.unwrap_or(DEFAULT_INSTANCE_ID))
            .map(instance_location)
            .map(|instance| vec![instance]);
    }
    let mut instances = vec![default_instance()];
    if managed_instances_supported() {
        instances.extend(read_stored_instances()?);
    }
    Ok(instances.into_iter().map(instance_location).collect())
}

#[tauri::command]
pub fn list_codex_instances() -> Result<Vec<CodexInstance>, String> {
    let mut result = vec![public_instance(default_instance())];
    if managed_instances_supported() {
        result.extend(read_stored_instances()?.into_iter().map(public_instance));
    }
    Ok(result)
}

#[tauri::command]
pub fn get_codex_instance_capabilities() -> CodexInstanceCapabilities {
    CodexInstanceCapabilities {
        managed_instances_supported: managed_instances_supported(),
    }
}

#[tauri::command]
pub fn save_codex_instance(input: SaveCodexInstanceInput) -> Result<CodexInstance, String> {
    if !managed_instances_supported() {
        return Err(managed_instances_unavailable_error());
    }
    let _guard = INSTANCE_OPERATION_LOCK
        .lock()
        .map_err(|_| "Codex 实例配置锁已损坏".to_string())?;
    let mut stored = read_stored_instances()?;
    let previous = input
        .id
        .as_deref()
        .and_then(|id| stored.iter().find(|item| item.id == id))
        .cloned();
    if previous
        .as_ref()
        .is_some_and(|item| live_pid(item).is_some())
    {
        return Err("请先停止实例再修改配置".to_string());
    }
    let mut candidate = normalize_input(input)?;
    if let Some(previous) = previous.as_ref() {
        candidate.created_at = previous.created_at;
        candidate.retired_data_paths = previous.retired_data_paths.clone();
        for path in [&previous.codex_home, &previous.electron_data] {
            if path != &candidate.codex_home
                && path != &candidate.electron_data
                && !candidate.retired_data_paths.contains(path)
            {
                candidate.retired_data_paths.push(path.clone());
            }
        }
        candidate
            .retired_data_paths
            .retain(|path| path != &candidate.codex_home && path != &candidate.electron_data);
    }
    validate_unique(&candidate, &stored)?;
    if let Some(index) = stored.iter().position(|item| item.id == candidate.id) {
        stored[index] = candidate.clone();
    } else {
        stored.push(candidate.clone());
    }
    write_stored_instances(&stored)?;
    Ok(public_instance(candidate))
}

#[tauri::command]
pub fn delete_codex_instance(instance_id: String) -> Result<DeleteCodexInstanceResult, String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        return Err("系统默认实例不能删除".to_string());
    }
    let _guard = INSTANCE_OPERATION_LOCK
        .lock()
        .map_err(|_| "Codex 实例配置锁已损坏".to_string())?;
    let mut stored = read_stored_instances()?;
    let instance = stored
        .iter()
        .find(|item| item.id == instance_id)
        .cloned()
        .ok_or_else(|| "Codex 实例不存在".to_string())?;

    let deletion_roots = instance_deletion_roots(&instance, &stored)?;
    let codex_home = normalize_stored_path(Path::new(&instance.codex_home), "Codex Home")?;
    let session_hashes = collect_session_path_hashes(&codex_home)?;
    let session_backups = attributed_session_backups(&instance.id)?;
    let session_edit_backups = attributed_session_edit_backups(&session_hashes)?;
    let mut related_directories = vec![
        crate::switcher_data_dir()
            .join("config-backups")
            .join(short_path_hash(&codex_home)),
        crate::switcher_data_dir()
            .join("session-trash")
            .join(short_path_hash(&codex_home)),
    ];
    if let Some(path) = crate::usage::instance_usage_cache_dir(&instance.id) {
        related_directories.push(path);
    }

    stop_stored(&instance)?;
    let _usage_guard = crate::usage::lock_usage_data()?;

    let mut deleted_paths = Vec::new();
    for path in deletion_roots.iter().chain(related_directories.iter()) {
        if remove_owned_path(path)? {
            deleted_paths.push(path.to_string_lossy().to_string());
        }
    }
    let mut deleted_backup_count = 0usize;
    for path in session_backups.iter().chain(session_edit_backups.iter()) {
        if remove_owned_path(path)? {
            deleted_backup_count += 1;
        }
    }
    stored.retain(|item| item.id != instance_id);
    write_stored_instances(&stored)?;
    Ok(DeleteCodexInstanceResult {
        instance_id: instance.id,
        instance_name: instance.name,
        deleted_paths,
        deleted_backup_count,
    })
}

fn launch_stored(instance: &StoredCodexInstance) -> Result<CodexInstance, String> {
    if instance.id != DEFAULT_INSTANCE_ID && !managed_instances_supported() {
        return Err(managed_instances_unavailable_error());
    }
    if let Some(pid) = live_pid(instance) {
        return Err(format!("实例已在运行（PID {pid}）"));
    }
    #[cfg(target_os = "macos")]
    {
        if instance.id != DEFAULT_INSTANCE_ID {
            ensure_isolated_desktop_data_supported(&instance.app_path)?;
        }
        fs::create_dir_all(&instance.codex_home)
            .map_err(|error| format!("创建 Codex Home 失败：{error}"))?;
        fs::create_dir_all(&instance.electron_data)
            .map_err(|error| format!("创建桌面数据目录失败：{error}"))?;
        let log_path = PathBuf::from(&instance.codex_home).join("codex-switcher-launch.log");
        let stdout = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&log_path)
            .map_err(|error| format!("创建实例启动日志失败：{error}"))?;
        let stderr = stdout
            .try_clone()
            .map_err(|error| format!("复制实例日志句柄失败：{error}"))?;
        let mut command = Command::new(app_executable(instance)?);
        if let Some(user_data_arg) = launch_user_data_arg(instance) {
            command.arg(user_data_arg);
        }
        command
            .env("CODEX_HOME", &instance.codex_home)
            .env("CODEX_ELECTRON_USER_DATA_PATH", &instance.electron_data)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        if let Some(workspace) = instance.workspace.as_ref() {
            command.arg(workspace);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("启动 Codex 实例失败：{error}"))?;
        thread::sleep(Duration::from_millis(550));
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("检查 Codex 实例状态失败：{error}"))?
        {
            let detail = fs::read_to_string(&log_path)
                .ok()
                .and_then(|content| {
                    content
                        .lines()
                        .rev()
                        .find(|line| !line.trim().is_empty())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| format!("进程立即退出：{status}"));
            return Err(format!(
                "启动 Codex 实例失败：{detail}；日志：{}",
                log_path.display()
            ));
        }
        thread::spawn(move || {
            let _ = child.wait();
        });
        resolve_instance(&instance.id)
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("powershell");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            crate::account::WINDOWS_CODEX_START_SCRIPT,
        ]);
        hide_command_window(&mut command);
        let status = command
            .status()
            .map_err(|error| format!("启动 Codex 失败：{error}"))?;
        if !status.success() {
            return Err("启动 Codex 失败：未找到桌面应用，请确认已安装并可正常打开".to_string());
        }
        thread::sleep(Duration::from_millis(500));
        resolve_instance(DEFAULT_INSTANCE_ID)
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = instance;
        Err("多开实例启动目前仅支持 macOS".to_string())
    }
}

#[tauri::command]
pub fn launch_codex_instance(instance_id: String) -> Result<CodexInstance, String> {
    let _guard = INSTANCE_OPERATION_LOCK
        .lock()
        .map_err(|_| "Codex 实例操作锁已损坏".to_string())?;
    let instance = stored_instance(&instance_id)?;
    launch_stored(&instance)
}

fn stored_instance(instance_id: &str) -> Result<StoredCodexInstance, String> {
    if instance_id.trim().is_empty() || instance_id == DEFAULT_INSTANCE_ID {
        return Ok(default_instance());
    }
    if !managed_instances_supported() {
        return Err(managed_instances_unavailable_error());
    }
    read_stored_instances()?
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| "Codex 实例不存在".to_string())
}

fn stop_stored(instance: &StoredCodexInstance) -> Result<(), String> {
    #[cfg(windows)]
    {
        if instance.id != DEFAULT_INSTANCE_ID {
            return Ok(());
        }
        let pid = live_pid(instance);
        let mut command = Command::new("taskkill");
        command.args(["/IM", "Codex.exe", "/T", "/F"]);
        hide_command_window(&mut command);
        let status = command
            .status()
            .map_err(|error| format!("停止实例失败：{error}"))?;
        if !status.success() && pid.is_some() && live_pid(instance).is_some() {
            return Err(format!("停止 Codex 进程 PID {} 失败", pid.unwrap()));
        }
        for _ in 0..30 {
            if live_pid(instance).is_none() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        return Err(format!(
            "Codex 进程 PID {} 未在超时时间内退出",
            pid.unwrap_or_default()
        ));
    }

    #[cfg(not(windows))]
    {
        let Some(pid) = live_pid(instance) else {
            return Ok(());
        };
        #[cfg(unix)]
        unsafe {
            if libc::kill(pid as i32, libc::SIGTERM) != 0 {
                return Err(format!(
                    "停止实例 PID {pid} 失败：{}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        for _ in 0..30 {
            if live_pid(instance).is_none() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(format!("实例 PID {pid} 未在超时时间内退出"))
    }
}

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[tauri::command]
pub fn stop_codex_instance(instance_id: String) -> Result<(), String> {
    let _guard = INSTANCE_OPERATION_LOCK
        .lock()
        .map_err(|_| "Codex 实例操作锁已损坏".to_string())?;
    stop_stored(&stored_instance(&instance_id)?)
}

#[tauri::command]
pub fn restart_codex_instance(instance_id: String) -> Result<CodexInstance, String> {
    let _guard = INSTANCE_OPERATION_LOCK
        .lock()
        .map_err(|_| "Codex 实例操作锁已损坏".to_string())?;
    let instance = stored_instance(&instance_id)?;
    stop_stored(&instance)?;
    thread::sleep(Duration::from_millis(250));
    launch_stored(&instance)
}

pub fn run_with_instance_restarted<T>(
    instance_id: &str,
    action: impl FnOnce(&CodexInstance) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = INSTANCE_OPERATION_LOCK
        .lock()
        .map_err(|_| "Codex 实例操作锁已损坏".to_string())?;
    let stored = stored_instance(instance_id)?;
    let public = public_instance(stored.clone());
    if public.running {
        stop_stored(&stored)?;
    }
    let action_result = action(&public);
    let start_result = launch_stored(&stored).map(|_| ());
    match (action_result, start_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(action_error), Err(start_error)) => Err(format!(
            "{action_error}；同时未能重新启动实例“{}”：{start_error}",
            stored.name
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn default_instance_uses_reserved_id() {
        let instance = default_instance();
        assert_eq!(instance.id, DEFAULT_INSTANCE_ID);
        assert!(!instance.codex_home.is_empty());
        assert!(!instance.electron_data.is_empty());
    }

    #[test]
    fn detects_only_opencodex_owned_instance_routing() {
        let marker_home = tempfile::tempdir().expect("marker home");
        fs::write(
            marker_home.path().join("config.toml"),
            concat!(
                "model = \"gpt-5.5\"\n",
                "# Auto-injected by opencodex\n",
                "openai_base_url = \"http://127.0.0.1:15800/v1\"\n",
                "[features]\nresponses_websockets = true\n",
            ),
        )
        .expect("write marker config");
        assert!(codex_home_has_opencodex_routing(marker_home.path()));

        let user_home = tempfile::tempdir().expect("user home");
        fs::write(
            user_home.path().join("config.toml"),
            "openai_base_url = \"https://gateway.example/v1\"\n",
        )
        .expect("write user config");
        assert!(!codex_home_has_opencodex_routing(user_home.path()));

        fs::write(
            user_home.path().join("config.toml"),
            concat!(
                "# Auto-injected by opencodex\n",
                "openai_base_url_backup = \"https://gateway.example/v1\"\n",
            ),
        )
        .expect("write similar user config key");
        assert!(!codex_home_has_opencodex_routing(user_home.path()));

        let legacy_home = tempfile::tempdir().expect("legacy home");
        fs::write(
            legacy_home.path().join("config.toml"),
            concat!(
                "model_provider = \"opencodex\"\n",
                "[model_providers.opencodex]\n",
                "base_url = \"http://127.0.0.1:15800/v1\"\n",
            ),
        )
        .expect("write legacy config");
        assert!(codex_home_has_opencodex_routing(legacy_home.path()));
    }

    #[test]
    fn journal_recognizes_opencodex_routing_after_marker_is_rewritten() {
        let home = tempfile::tempdir().expect("journal home");
        fs::write(
            home.path().join("config.toml"),
            "openai_base_url = \"http://127.0.0.1:15800/v1\"\n",
        )
        .expect("write rewritten config");
        fs::write(
            home.path().join(OPENCODEX_JOURNAL_FILE),
            serde_json::json!({
                "version": 1,
                "injectedOpenaiBaseUrl": "http://127.0.0.1:15800/v1"
            })
            .to_string(),
        )
        .expect("write journal");

        assert!(codex_home_has_opencodex_routing(home.path()));

        fs::write(
            home.path().join("config.toml"),
            "openai_base_url = \"https://changed.example/v1\"\n",
        )
        .expect("replace with user routing");
        assert!(!codex_home_has_opencodex_routing(home.path()));

        fs::write(
            home.path().join("config.toml"),
            "openai_base_url = \"http://127.0.0.1:15800/v1\"\n",
        )
        .expect("restore injected routing");
        fs::write(
            home.path().join(OPENCODEX_JOURNAL_FILE),
            serde_json::json!({
                "version": 2,
                "injectedOpenaiBaseUrl": "http://127.0.0.1:15800/v1"
            })
            .to_string(),
        )
        .expect("write unsupported journal");
        assert!(!codex_home_has_opencodex_routing(home.path()));
    }

    #[test]
    fn generated_instance_ids_are_not_default() {
        assert_ne!(make_instance_id(), DEFAULT_INSTANCE_ID);
    }

    #[test]
    fn capabilities_match_the_supported_platform() {
        assert_eq!(
            get_codex_instance_capabilities().managed_instances_supported,
            cfg!(target_os = "macos")
        );
    }

    #[test]
    fn windows_tasklist_parser_only_returns_codex_processes() {
        let output = concat!(
            "\"Codex.exe\",\"1234\",\"Console\",\"1\",\"100,000 K\"\n",
            "\"codex.EXE\",\"5678\",\"Console\",\"1\",\"80,000 K\"\n",
            "\"Codex Switcher.exe\",\"9999\",\"Console\",\"1\",\"50,000 K\"\n",
        );
        assert_eq!(parse_windows_tasklist_codex_pids(output), vec![1234, 5678]);
    }

    #[test]
    fn process_matching_keeps_default_and_managed_instances_isolated() {
        let executable = "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT";
        let default = default_instance();
        let managed = StoredCodexInstance {
            id: "instance-work".to_string(),
            name: "work".to_string(),
            codex_home: "/tmp/work-codex-home".to_string(),
            electron_data: "/tmp/work-electron-data".to_string(),
            app_path: default.app_path.clone(),
            workspace: None,
            retired_data_paths: Vec::new(),
            created_at: 1,
        };
        let legacy_default_command = executable.to_string();
        let explicit_default_command =
            format!("{executable} --user-data-dir={}", default.electron_data);
        let managed_command = format!("{executable} --user-data-dir={}", managed.electron_data);

        assert!(process_matches_instance(
            &default,
            executable,
            &legacy_default_command
        ));
        assert!(process_matches_instance(
            &default,
            executable,
            &explicit_default_command
        ));
        assert!(!process_matches_instance(
            &default,
            executable,
            &managed_command
        ));
        assert!(process_matches_instance(
            &managed,
            executable,
            &managed_command
        ));
        assert!(!process_matches_instance(
            &managed,
            executable,
            &explicit_default_command
        ));
        assert_eq!(launch_user_data_arg(&default), None);
        assert_eq!(
            launch_user_data_arg(&managed),
            Some(format!("--user-data-dir={}", managed.electron_data))
        );
    }

    #[test]
    fn instance_ids_cannot_escape_the_managed_directory() {
        assert!(validate_instance_id("instance-123_abc").is_ok());
        assert!(validate_instance_id("../../outside").is_err());
        assert!(validate_instance_id("instance/child").is_err());
    }

    #[test]
    fn deleting_instance_removes_owned_data_and_attributed_backups() {
        let instance_id = format!("instance-delete-{}", rand::thread_rng().gen::<u32>());
        let profile_root = default_profile_root(&instance_id);
        let retired_data = tempfile::tempdir().unwrap();
        fs::write(retired_data.path().join("old-data"), b"retired").unwrap();
        let retired_data_path = fs::canonicalize(retired_data.path()).unwrap();
        let codex_home = profile_root.join("codex-home");
        let electron_data = profile_root.join("electron-data");
        let session_path = codex_home
            .join("sessions")
            .join("2026")
            .join("session.jsonl");
        fs::create_dir_all(session_path.parent().unwrap()).unwrap();
        fs::create_dir_all(&electron_data).unwrap();
        fs::write(&session_path, b"{}\n").unwrap();
        fs::write(electron_data.join("Cookies"), b"desktop-data").unwrap();

        let codex_home = fs::canonicalize(&codex_home).unwrap();
        let electron_data = fs::canonicalize(&electron_data).unwrap();
        let session_path = fs::canonicalize(&session_path).unwrap();
        let instance = StoredCodexInstance {
            id: instance_id.clone(),
            name: "待删除实例".to_string(),
            codex_home: codex_home.to_string_lossy().to_string(),
            electron_data: electron_data.to_string_lossy().to_string(),
            app_path: default_app_path().to_string_lossy().to_string(),
            workspace: None,
            retired_data_paths: vec![retired_data_path.to_string_lossy().to_string()],
            created_at: Utc::now().timestamp(),
        };
        write_stored_instances(std::slice::from_ref(&instance)).unwrap();

        let config_backup_dir = crate::switcher_data_dir()
            .join("config-backups")
            .join(short_path_hash(&codex_home));
        let session_trash_dir = crate::switcher_data_dir()
            .join("session-trash")
            .join(short_path_hash(&codex_home));
        fs::create_dir_all(&config_backup_dir).unwrap();
        fs::create_dir_all(&session_trash_dir).unwrap();
        fs::write(config_backup_dir.join("config.toml.bak"), b"backup").unwrap();
        fs::write(session_trash_dir.join("session.jsonl"), b"trash").unwrap();
        let trashed_original_path = codex_home
            .join("sessions")
            .join("2026")
            .join("trashed-session.jsonl");
        fs::write(
            session_trash_dir.join("trashed-session.json"),
            serde_json::json!({ "originalPath": trashed_original_path })
                .to_string()
                .as_bytes(),
        )
        .unwrap();

        let edit_backup_dir = crate::switcher_data_dir().join("session-edit-backups");
        fs::create_dir_all(&edit_backup_dir).unwrap();
        let edit_backup = edit_backup_dir.join(format!(
            "20260825-turn-delete-{}.jsonl",
            short_path_hash(&session_path)
        ));
        fs::write(&edit_backup, b"edit-backup").unwrap();
        let trashed_edit_backup = edit_backup_dir.join(format!(
            "20260825-message-delete-{}.jsonl",
            short_path_hash(&trashed_original_path)
        ));
        fs::write(&trashed_edit_backup, b"trashed-edit-backup").unwrap();

        let manual_backup_dir = crate::switcher_data_dir().join("session");
        fs::create_dir_all(&manual_backup_dir).unwrap();
        let manual_backup =
            manual_backup_dir.join(format!("codex-session-backup-{instance_id}-20260825.zip"));
        let file = File::create(&manual_backup).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file("backup.json", zip::write::FileOptions::default())
            .unwrap();
        archive
            .write_all(
                serde_json::json!({
                    "backupMode": "manual",
                    "sourceInstance": { "id": instance_id }
                })
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        archive.finish().unwrap();

        let result = delete_codex_instance(instance_id.clone()).unwrap();
        assert_eq!(result.instance_id, instance_id);
        assert_eq!(result.deleted_backup_count, 3);
        assert!(!profile_root.exists());
        assert!(!codex_home.exists());
        assert!(!electron_data.exists());
        assert!(!config_backup_dir.exists());
        assert!(!session_trash_dir.exists());
        assert!(!retired_data_path.exists());
        assert!(!manual_backup.exists());
        assert!(!edit_backup.exists());
        assert!(!trashed_edit_backup.exists());
        assert!(read_stored_instances().unwrap().is_empty());
    }

    #[test]
    fn manual_backup_attribution_uses_manifest_instead_of_filename_prefix() {
        let backup_dir = crate::switcher_data_dir().join("session");
        fs::create_dir_all(&backup_dir).unwrap();
        let misleading = backup_dir.join("codex-session-backup-instance-alpha-child.zip");
        let file = File::create(&misleading).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file("backup.json", zip::write::FileOptions::default())
            .unwrap();
        archive
            .write_all(br#"{"sourceInstance":{"id":"instance-alpha-child"}}"#)
            .unwrap();
        archive.finish().unwrap();

        assert!(attributed_session_backups("instance-alpha")
            .unwrap()
            .is_empty());
        assert_eq!(
            attributed_session_backups("instance-alpha-child").unwrap(),
            vec![misleading]
        );
    }

    #[test]
    fn deletion_rejects_a_data_directory_that_overlaps_the_workspace() {
        let root = tempfile::tempdir().unwrap();
        let data_root = root.path().join("owned-data");
        let workspace = data_root.join("project");
        fs::create_dir_all(&workspace).unwrap();
        let instance = StoredCodexInstance {
            id: format!("instance-unsafe-{}", rand::thread_rng().gen::<u32>()),
            name: "危险目录实例".to_string(),
            codex_home: data_root.to_string_lossy().to_string(),
            electron_data: root
                .path()
                .join("electron-data")
                .to_string_lossy()
                .to_string(),
            app_path: default_app_path().to_string_lossy().to_string(),
            workspace: Some(workspace.to_string_lossy().to_string()),
            retired_data_paths: Vec::new(),
            created_at: Utc::now().timestamp(),
        };
        let error =
            instance_deletion_roots(&instance, std::slice::from_ref(&instance)).unwrap_err();
        assert!(error.contains("工作区"));
        assert!(workspace.exists());
    }

    #[test]
    fn system_default_instance_cannot_be_deleted() {
        assert_eq!(
            delete_codex_instance(DEFAULT_INSTANCE_ID.to_string()).unwrap_err(),
            "系统默认实例不能删除"
        );
    }
}
