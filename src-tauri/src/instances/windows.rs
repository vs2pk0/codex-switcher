use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ProcessRow {
    process_id: u32,
    executable_path: Option<String>,
    command_line: Option<String>,
}

fn powershell(script: &str) -> Result<String, String> {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    hide_command_window(&mut command);
    let output = command
        .output()
        .map_err(|e| format!("查询 Windows 应用失败：{e}"))?;
    if !output.status.success() {
        return Err(format!(
            "查询 Windows 应用失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_start_matches('\u{feff}')
        .trim()
        .to_string())
}

// Only accept a desktop installation, never the similarly named CLI/app-server.
pub(super) fn desktop_in_directory(directory: &Path) -> Option<PathBuf> {
    if !directory.join("resources/app.asar").is_file() {
        return None;
    }
    ["ChatGPT.exe", "Codex.exe"]
        .into_iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
}

pub(super) fn discover() -> Option<PathBuf> {
    static CACHED: Mutex<Option<PathBuf>> = Mutex::new(None);
    let mut cached = CACHED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(path) = cached.as_ref().filter(|path| path.is_file()) {
        return Some(path.clone());
    }
    let location = powershell(
        r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$pkg = Get-AppxPackage -Name 'OpenAI.Codex' | Sort-Object Version -Descending | Select-Object -First 1
if ($pkg) { [Console]::Write($pkg.InstallLocation) }
"#,
    )
    .ok();
    let mut directories = Vec::new();
    if let Some(location) = location.filter(|value| !value.is_empty()) {
        directories.push(PathBuf::from(location).join("app"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        directories.push(PathBuf::from(&local).join("Programs/Codex"));
        directories.push(PathBuf::from(local).join("Codex"));
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            directories.push(PathBuf::from(root).join("Codex"));
        }
    }
    *cached = directories
        .iter()
        .find_map(|directory| desktop_in_directory(directory));
    cached.clone()
}

pub(super) fn executable(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    let valid_name = path.file_name().is_some_and(|name| {
        let name = name.to_string_lossy();
        name.eq_ignore_ascii_case("Codex.exe") || name.eq_ignore_ascii_case("ChatGPT.exe")
    });
    if !valid_name {
        return Err("请选择官方桌面程序 ChatGPT.exe 或 Codex.exe".to_string());
    }
    if path.is_file() {
        if let Some(executable) = path.parent().and_then(desktop_in_directory) {
            return Ok(executable);
        }
    }
    // Store updates replace the versioned installation directory.
    let is_store_path = path.components().any(|part| {
        part.as_os_str()
            .to_string_lossy()
            .to_ascii_lowercase()
            .starts_with("openai.codex_")
    });
    if is_store_path || path == Path::new("Codex.exe") {
        if let Some(executable) = discover() {
            return Ok(executable);
        }
    }
    Err(format!(
        "未找到 Codex 桌面程序，请重新选择官方 App：{}",
        path.display()
    ))
}

fn arguments(command: &str) -> Vec<String> {
    use windows_sys::Win32::{Foundation::LocalFree, UI::Shell::CommandLineToArgvW};
    let command: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
    let mut count = 0;
    unsafe {
        let argv = CommandLineToArgvW(command.as_ptr(), &mut count);
        if argv.is_null() {
            return Vec::new();
        }
        let result = std::slice::from_raw_parts(argv, count as usize)
            .iter()
            .map(|&arg| {
                let mut length = 0;
                while *arg.add(length) != 0 {
                    length += 1;
                }
                String::from_utf16_lossy(std::slice::from_raw_parts(arg, length))
            })
            .collect();
        LocalFree(argv.cast());
        result
    }
}

// Rust canonicalization adds an extended-length prefix. Chromium subsystems
// (including its GCM store) do not consistently accept that prefix.
pub(super) fn application_path(path: &str) -> String {
    let path = path.replace('/', "\\");
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(&path).to_string()
    }
}

fn normalized_path(path: &str) -> String {
    application_path(path).trim_end_matches('\\').to_lowercase()
}

fn store_root(path: &str) -> Option<PathBuf> {
    let path = PathBuf::from(normalized_path(path));
    let app = path.parent()?;
    let package = app.parent()?;
    if app.file_name()? != "app" || !package.file_name()?.to_str()?.starts_with("openai.codex_") {
        return None;
    }
    package.parent().map(Path::to_path_buf)
}

pub(super) fn can_activate_store_app(executable: &Path, error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
        && store_root(&executable.to_string_lossy()).is_some()
}

fn validate_store_activation(instance: &StoredCodexInstance) -> Result<(), String> {
    // Shell activation is brokered by Windows and cannot inherit our per-process
    // CODEX_HOME / CODEX_ELECTRON_USER_DATA_PATH overrides. Never open a different
    // profile while reporting that an isolated instance was launched.
    if instance.id != DEFAULT_INSTANCE_ID {
        return Err("商店激活只能打开系统默认实例，不能保留多开实例的独立启动环境".to_string());
    }
    let home = dirs::home_dir().ok_or_else(|| "无法确定用户主目录".to_string())?;
    if normalized_path(&instance.codex_home)
        != normalized_path(&home.join(".codex").to_string_lossy())
        || normalized_path(&instance.electron_data)
            != normalized_path(&default_electron_data().to_string_lossy())
    {
        return Err("商店激活不能保留自定义数据目录，请使用标准默认目录后重试".to_string());
    }
    Ok(())
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreApplication {
    app_user_model_id: String,
    executable_path: String,
}

fn store_app_id<'a>(
    executable: &Path,
    applications: &'a [StoreApplication],
) -> Result<&'a str, String> {
    let expected = normalized_path(&executable.to_string_lossy());
    applications
        .iter()
        .find(|app| normalized_path(&app.executable_path) == expected)
        .map(|app| app.app_user_model_id.as_str())
        .ok_or_else(|| {
            format!(
                "未找到此桌面程序对应的已注册商店入口：{}",
                executable.display()
            )
        })
}

fn registered_store_apps(force_refresh: bool) -> Result<Vec<StoreApplication>, String> {
    static CACHED: Mutex<Option<Vec<StoreApplication>>> = Mutex::new(None);
    if !force_refresh {
        let cached = CACHED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(apps) = cached.as_ref() {
            return Ok(apps.clone());
        }
    }
    // Read Application.Id from the registered manifest instead of assuming
    // "!App" or a display name, which can change between Codex/ChatGPT releases.
    let output = powershell(
        r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$apps = @(Get-AppxPackage -Name 'OpenAI.Codex' | ForEach-Object {
    $pkg = $_
    $manifest = Get-AppxPackageManifest -Package $pkg.PackageFullName
    foreach ($app in $manifest.Package.Applications.Application) {
        if ($app.Id -and $app.Executable) {
            [PSCustomObject]@{
                appUserModelId = $pkg.PackageFamilyName + '!' + $app.Id
                executablePath = Join-Path $pkg.InstallLocation $app.Executable
            }
        }
    }
})
ConvertTo-Json -InputObject $apps -Compress
"#,
    )?;
    let apps: Vec<StoreApplication> =
        serde_json::from_str(&output).map_err(|error| format!("解析商店应用入口失败：{error}"))?;
    *CACHED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(apps.clone());
    Ok(apps)
}

fn quote_workspace_argument(workspace: &str) -> String {
    // Always quote the one workspace argument, including trailing backslashes
    // and embedded quotes, using the CommandLineToArgvW escaping rules.
    let mut result = String::from("\"");
    let mut backslashes = 0;
    for ch in workspace.chars() {
        if ch == '\\' {
            backslashes += 1;
            continue;
        }
        if ch == '"' {
            result.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
        } else {
            result.extend(std::iter::repeat_n('\\', backslashes));
        }
        result.push(ch);
        backslashes = 0;
    }
    result.extend(std::iter::repeat_n('\\', backslashes * 2));
    result.push('"');
    result
}

fn activate_store_app(app_id: &str, workspace: Option<&str>) -> Result<(), String> {
    let target = format!(r"shell:AppsFolder\{app_id}");
    let parameters = workspace.map(|path| quote_workspace_argument(&application_path(path)));
    // Shell extensions can require STA COM. Use a fresh thread so a Tauri
    // worker's existing COM apartment does not change the activation behavior.
    thread::spawn(move || {
        use windows_sys::Win32::{
            System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
            UI::Shell::{
                ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
            },
        };
        let initialized =
            unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        if initialized < 0 {
            return Err(format!(
                "初始化商店激活组件失败（HRESULT 0x{initialized:08X}）"
            ));
        }
        let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let target: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
        let parameters: Option<Vec<u16>> =
            parameters.map(|value| value.encode_utf16().chain(Some(0)).collect());
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: operation.as_ptr(),
            lpFile: target.as_ptr(),
            lpParameters: parameters
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            nShow: 1,
            ..Default::default()
        };
        // Let Windows activate the registered app, as the Start menu does.
        // No PowerShell/cmd interpolation, elevation or package ACL changes.
        let result = if unsafe { ShellExecuteExW(&mut info) } != 0 {
            Ok(())
        } else {
            Err(format!(
                "系统商店入口启动失败：{}",
                std::io::Error::last_os_error()
            ))
        };
        unsafe { CoUninitialize() };
        result
    })
    .join()
    .map_err(|_| "商店应用激活线程异常退出".to_string())?
}

pub(super) fn launch_store_instance(
    instance: &StoredCodexInstance,
    executable: &Path,
) -> Result<CodexInstance, String> {
    validate_store_activation(instance)?;
    let mut applications = registered_store_apps(false)?;
    let app_id = match store_app_id(executable, &applications) {
        Ok(app_id) => app_id.to_string(),
        Err(_) => {
            applications = registered_store_apps(true)?;
            store_app_id(executable, &applications)?.to_string()
        }
    };
    activate_store_app(&app_id, instance.workspace.as_deref())?;
    // Shell success means the activation request was accepted, not that the
    // desktop is ready. Check the actual instance instead of the shell process.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let launched = checked_public_instance(instance.clone())?;
        if launched.running {
            return Ok(launched);
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "商店启动请求已发送，但未检测到实例“{}”运行，请从开始菜单检查应用能否打开",
                instance.name
            ));
        }
        thread::sleep(Duration::from_millis(80));
    }
}

fn executable_matches(expected: &Path, actual: &str) -> bool {
    let expected = normalized_path(&expected.to_string_lossy());
    let actual = normalized_path(actual);
    if expected == actual {
        return true;
    }
    let actual_name = Path::new(&actual)
        .file_name()
        .and_then(|name| name.to_str());
    matches!(actual_name, Some("chatgpt.exe" | "codex.exe"))
        && store_root(&expected).is_some_and(|root| Some(root) == store_root(&actual))
}

fn matches(instance: &StoredCodexInstance, executable: &Path, row: &ProcessRow) -> bool {
    let (Some(path), Some(command)) = (&row.executable_path, &row.command_line) else {
        return false;
    };
    if !executable_matches(executable, path) {
        return false;
    }
    let args = arguments(command);
    if args.is_empty()
        || args
            .iter()
            .any(|arg| arg.starts_with("--type=") || arg == "--type")
    {
        return false;
    }
    let user_data = args.iter().enumerate().find_map(|(index, arg)| {
        arg.strip_prefix("--user-data-dir=")
            .map(str::to_string)
            .or_else(|| {
                (arg == "--user-data-dir")
                    .then(|| args.get(index + 1).cloned())
                    .flatten()
            })
    });
    match user_data {
        Some(path) => normalized_path(&path) == normalized_path(&instance.electron_data),
        None => instance.id == DEFAULT_INSTANCE_ID,
    }
}

fn pid_in_rows(instance: &StoredCodexInstance, rows: &[ProcessRow]) -> Result<Option<u32>, String> {
    // An uninstalled application is not a process-query failure. Keep the old
    // absolute path so an already-running process can still be recognized.
    let executable =
        executable(&instance.app_path).unwrap_or_else(|_| PathBuf::from(&instance.app_path));
    let saved = Path::new(&instance.app_path);
    for row in rows {
        if row.command_line.is_none()
            && row.executable_path.as_deref().is_some_and(|path| {
                executable_matches(&executable, path) || executable_matches(saved, path)
            })
        {
            return Err(format!(
                "无法读取 Codex 进程 PID {} 的实例信息",
                row.process_id
            ));
        }
        if matches(instance, &executable, row) || matches(instance, saved, row) {
            return Ok(Some(row.process_id));
        }
    }
    Ok(None)
}

struct HandleGuard(windows_sys::Win32::Foundation::HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

fn query_process_image_path(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> Result<Option<String>, ()> {
    use windows_sys::Win32::System::Threading::QueryFullProcessImageNameW;
    let mut capacity = 1024u32;
    while capacity <= 32768 {
        let mut buf = vec![0u16; capacity as usize];
        let mut size = capacity;
        if unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) } != 0 {
            return Ok(Some(String::from_utf16_lossy(&buf[..size as usize])));
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(122) => capacity *= 2,
            Some(6 | 87 | 299) => return Ok(None),
            _ => return Err(()),
        }
    }
    Err(())
}

fn query_process_command_line(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> Result<Option<String>, ()> {
    use windows_sys::{
        Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation},
        Win32::Foundation::UNICODE_STRING,
    };
    const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC0000004_u32 as i32;
    const STATUS_BUFFER_TOO_SMALL: i32 = 0xC0000023_u32 as i32;
    const STATUS_BUFFER_OVERFLOW: i32 = 0x80000005_u32 as i32;
    const STATUS_PROCESS_IS_TERMINATING: i32 = 0xC000010A_u32 as i32;

    let mut byte_len = 2048usize;
    for _ in 0..4 {
        let words = byte_len.div_ceil(8).max(4);
        let mut buf = vec![0u64; words];
        let mut ret_len = 0u32;
        let status = unsafe {
            NtQueryInformationProcess(
                handle,
                ProcessCommandLineInformation,
                buf.as_mut_ptr().cast(),
                (words * 8) as u32,
                &mut ret_len,
            )
        };
        if status >= 0 {
            let us = unsafe { &*(buf.as_ptr() as *const UNICODE_STRING) };
            if us.Length == 0 || us.Buffer.is_null() {
                return Ok(Some(String::new()));
            }
            if us.Length % 2 != 0 {
                return Err(());
            }
            let buf_start = buf.as_ptr() as usize;
            let buf_end = buf_start.saturating_add(words * 8);
            let str_start = us.Buffer as usize;
            let str_end = str_start.saturating_add(us.Length as usize);
            if str_start < buf_start || str_end > buf_end {
                return Err(());
            }
            let slice =
                unsafe { std::slice::from_raw_parts(us.Buffer, (us.Length / 2) as usize) };
            return Ok(Some(String::from_utf16_lossy(slice)));
        }
        if status == STATUS_PROCESS_IS_TERMINATING {
            return Ok(None);
        }
        if matches!(
            status,
            STATUS_INFO_LENGTH_MISMATCH | STATUS_BUFFER_TOO_SMALL | STATUS_BUFFER_OVERFLOW
        ) {
            let next = (ret_len as usize).max(words * 8 * 2);
            if next > 1024 * 1024 {
                return Err(());
            }
            byte_len = next + 16;
            continue;
        }
        return Err(());
    }
    Err(())
}

/// Fast in-process Win32 snapshot (~0.05ms) avoiding cold `powershell.exe` + WMI spawns.
/// Falls back to PowerShell/CIM only if an active Codex/ChatGPT process cannot be inspected.
fn native_process_rows() -> Option<Vec<ProcessRow>> {
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
    };
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return None;
    }
    let _snapshot = HandleGuard(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut pids = Vec::new();
    const MAX_SNAPSHOT_PROCESSES: usize = 65_536;
    if unsafe { Process32FirstW(snapshot, &mut entry) } != 0 {
        let mut scanned = 0usize;
        loop {
            scanned += 1;
            if scanned > MAX_SNAPSHOT_PROCESSES {
                return None;
            }
            let len = entry
                .szExeFile
                .iter()
                .position(|&ch| ch == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case("Codex.exe") || name.eq_ignore_ascii_case("ChatGPT.exe") {
                pids.push(entry.th32ProcessID);
            }
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                break;
            }
        }
    }
    if pids.is_empty() {
        return Some(Vec::new());
    }
    let mut rows = Vec::with_capacity(pids.len());
    for pid in pids {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            if std::io::Error::last_os_error().raw_os_error() == Some(87) {
                continue;
            }
            return None;
        }
        let _handle = HandleGuard(handle);
        let executable_path = match query_process_image_path(handle) {
            Ok(Some(path)) => path,
            Ok(None) => continue,
            Err(()) => return None,
        };
        let command_line = match query_process_command_line(handle) {
            Ok(Some(cmd)) => cmd,
            Ok(None) => continue,
            Err(()) => return None,
        };
        rows.push(ProcessRow {
            process_id: pid,
            executable_path: Some(executable_path),
            command_line: Some(command_line),
        });
    }
    Some(rows)
}

pub(super) fn wait_for_exit(pid: u32, timeout: Duration) {
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if !handle.is_null() {
        let _handle = HandleGuard(handle);
        let millis = timeout.as_millis().min(u32::MAX as u128) as u32;
        unsafe {
            WaitForSingleObject(handle, millis);
        }
    }
}

fn process_rows() -> Result<Vec<ProcessRow>, String> {
    if let Some(rows) = native_process_rows() {
        return Ok(rows);
    }
    let output = powershell(
        r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$rows = @(Get-CimInstance Win32_Process -Filter "Name = 'Codex.exe' OR Name = 'ChatGPT.exe'" | Select-Object ProcessId,ExecutablePath,CommandLine)
ConvertTo-Json -InputObject $rows -Compress
"#,
    )?;
    serde_json::from_str(&output).map_err(|error| format!("解析 Windows 进程信息失败：{error}"))
}

pub(super) fn live_pid(instance: &StoredCodexInstance) -> Result<Option<u32>, String> {
    pid_in_rows(instance, &process_rows()?)
}

pub(super) fn list(instances: Vec<StoredCodexInstance>) -> Result<Vec<CodexInstance>, String> {
    // One fresh snapshot for the entire list; never cache process identity for
    // mutations, where a stale PID could target a different process.
    let rows = process_rows()?;
    instances
        .iter()
        .map(|instance| {
            let pid = pid_in_rows(instance, &rows)?;
            Ok(public_instance_with_pid(instance.clone(), pid))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_activation_fallback_only_handles_denied_desktop_package_launches() {
        let desktop = Path::new("C:/Program Files/WindowsApps/OpenAI.Codex_2/app/ChatGPT.exe");
        let denied = std::io::Error::from_raw_os_error(5);
        assert!(can_activate_store_app(desktop, &denied));
        assert!(!can_activate_store_app(
            desktop,
            &std::io::Error::from_raw_os_error(2)
        ));
        for other in [
            "C:/Program Files/Codex/ChatGPT.exe",
            "C:/Program Files/WindowsApps/OpenAI.Codex_2/app/resources/codex.exe",
            "C:/Program Files/WindowsApps/Other.App_2/app/ChatGPT.exe",
        ] {
            assert!(!can_activate_store_app(Path::new(other), &denied));
        }
    }

    #[test]
    fn store_activation_cannot_redirect_an_isolated_or_custom_home_instance() {
        let mut instance = default_instance();
        instance.codex_home = dirs::home_dir()
            .unwrap()
            .join(".codex")
            .to_string_lossy()
            .into();
        assert!(validate_store_activation(&instance).is_ok());
        instance.id = "work".into();
        assert!(validate_store_activation(&instance)
            .unwrap_err()
            .contains("多开实例"));
        instance.id = DEFAULT_INSTANCE_ID.into();
        instance.codex_home.push_str("-custom");
        assert!(validate_store_activation(&instance).is_err());
        instance.codex_home.truncate(instance.codex_home.len() - 7);
        instance.electron_data.push_str("-custom");
        assert!(validate_store_activation(&instance).is_err());
    }

    #[test]
    fn store_activation_uses_the_registered_id_for_the_exact_desktop_executable() {
        let apps: Vec<StoreApplication> = serde_json::from_value(serde_json::json!([
            {
                "appUserModelId": "OpenAI.Codex_publisher!Helper",
                "executablePath": "C:/Program Files/WindowsApps/OpenAI.Codex_2/app/resources/codex.exe"
            },
            {
                "appUserModelId": "OpenAI.Codex_publisher!ChatGPT",
                "executablePath": "C:/Program Files/WindowsApps/OpenAI.Codex_2/app/ChatGPT.exe"
            },
            {
                "appUserModelId": "OpenAI.Codex_publisher!App",
                "executablePath": "D:/WindowsApps/OpenAI.Codex_1/app/Codex.exe"
            }
        ])).unwrap();
        assert_eq!(
            store_app_id(
                Path::new(r"\\?\C:\Program Files\WindowsApps\OpenAI.Codex_2\app\ChatGPT.exe"),
                &apps
            )
            .unwrap(),
            "OpenAI.Codex_publisher!ChatGPT"
        );
        assert_eq!(
            store_app_id(
                Path::new("d:/windowsapps/openai.codex_1/app/codex.exe"),
                &apps
            )
            .unwrap(),
            "OpenAI.Codex_publisher!App"
        );
        assert!(store_app_id(
            Path::new("C:/Program Files/WindowsApps/OpenAI.Codex_1/app/ChatGPT.exe"),
            &apps
        )
        .is_err());
        assert!(store_app_id(Path::new("C:/Codex/ChatGPT.exe"), &[]).is_err());
    }

    #[test]
    fn store_workspace_arguments_round_trip_without_shell_interpolation() {
        for workspace in [
            r"C:\用户\工作 A",
            r"C:\",
            r"C:\Work Space\",
            r"\\server\share\工作\",
            r#"C:\work\a\"quoted"#,
            r"C:\work\$(& other);'project",
        ] {
            let command = format!("ChatGPT.exe {}", quote_workspace_argument(workspace));
            assert_eq!(arguments(&command), vec!["ChatGPT.exe", workspace]);
        }
    }

    #[test]
    fn process_identity_excludes_helpers_cli_and_neighbor_profiles() {
        let executable = Path::new("C:/Program Files/Codex/ChatGPT.exe");
        let mut instance = default_instance();
        instance.id = "work".into();
        instance.electron_data = "C:/Profiles/Work One".into();
        let mut row = ProcessRow {
            process_id: 42,
            executable_path: Some(executable.to_string_lossy().into()),
            command_line: Some(
                r#""C:\Program Files\Codex\ChatGPT.exe" "--user-data-dir=C:\Profiles\Work One""#
                    .into(),
            ),
        };
        assert!(matches(&instance, executable, &row));
        row.command_line
            .as_mut()
            .unwrap()
            .push_str(" --type=renderer");
        assert!(!matches(&instance, executable, &row));
        row.command_line = Some(
            r#""C:\Program Files\Codex\ChatGPT.exe" "--user-data-dir=C:\Profiles\Work One extra""#
                .into(),
        );
        assert!(!matches(&instance, executable, &row));
        row.command_line = Some(
            r#""C:\Program Files\Codex\ChatGPT.exe" --user-data-dir "c:\profiles\work one""#.into(),
        );
        assert!(matches(&instance, executable, &row));
        instance.id = DEFAULT_INSTANCE_ID.into();
        instance.electron_data = "C:/Default".into();
        assert!(!matches(&instance, executable, &row));
        row.command_line = Some(r#""C:\Program Files\Codex\ChatGPT.exe""#.into());
        assert!(matches(&instance, executable, &row));
        row.executable_path = Some("C:/CLI/codex.exe".into());
        assert!(!matches(&instance, executable, &row));
    }

    #[test]
    fn desktop_discovery_requires_resources_and_prefers_the_actual_desktop_host() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Codex.exe"), []).unwrap();
        assert!(desktop_in_directory(root.path()).is_none());
        fs::create_dir(root.path().join("resources")).unwrap();
        fs::write(root.path().join("resources/app.asar"), []).unwrap();
        assert_eq!(
            desktop_in_directory(root.path()),
            Some(root.path().join("Codex.exe"))
        );
        fs::write(root.path().join("ChatGPT.exe"), []).unwrap();
        assert_eq!(
            desktop_in_directory(root.path()),
            Some(root.path().join("ChatGPT.exe"))
        );
    }

    #[test]
    fn windows_arguments_preserve_unicode_spaces_and_verbatim_paths() {
        let args = arguments(r#""C:\App\ChatGPT.exe" "--user-data-dir=\\?\C:\用户\工作 A""#);
        assert_eq!(args[1], r"--user-data-dir=\\?\C:\用户\工作 A");
        assert_eq!(
            normalized_path(r"\\?\C:\用户\工作 A"),
            normalized_path("c:/用户/工作 A")
        );
        assert_eq!(
            application_path(r"\\?\UNC\server\share\profile"),
            r"\\server\share\profile"
        );
    }

    #[test]
    fn store_updates_recognize_the_previous_desktop_but_not_its_cli() {
        let current = Path::new("C:/Program Files/WindowsApps/OpenAI.Codex_2/app/ChatGPT.exe");
        assert!(executable_matches(
            current,
            "C:/Program Files/WindowsApps/OpenAI.Codex_1/app/ChatGPT.exe"
        ));
        assert!(!executable_matches(
            current,
            "C:/Program Files/WindowsApps/OpenAI.Codex_1/app/resources/codex.exe"
        ));
        assert!(!executable_matches(
            current,
            "D:/WindowsApps/OpenAI.Codex_1/app/ChatGPT.exe"
        ));
        assert!(!executable_matches(
            current,
            "C:/Program Files/WindowsApps/Other.App_1/app/ChatGPT.exe"
        ));
    }

    #[test]
    #[ignore = "starts two installed desktop clients with temporary profiles"]
    fn desktop_instances_launch_stop_and_restart_independently() {
        struct RunningInstances(Vec<StoredCodexInstance>);
        impl Drop for RunningInstances {
            fn drop(&mut self) {
                for instance in &self.0 {
                    let _ = stop_stored(instance);
                }
            }
        }
        let root = tempfile::tempdir().unwrap();
        let default = default_instance();
        let baseline = live_pid(&default).unwrap();
        let mut running = RunningInstances(Vec::new());
        for name in ["实例 A", "instance B"] {
            let mut instance = default.clone();
            instance.id = format!("smoke-{}", running.0.len());
            instance.codex_home = root.path().join(name).join("home").to_string_lossy().into();
            instance.electron_data = root
                .path()
                .join(name)
                .join("desktop")
                .to_string_lossy()
                .into();
            fs::create_dir_all(&instance.codex_home).unwrap();
            fs::create_dir_all(&instance.electron_data).unwrap();
            instance.codex_home = fs::canonicalize(&instance.codex_home)
                .unwrap()
                .to_string_lossy()
                .into();
            instance.electron_data = fs::canonicalize(&instance.electron_data)
                .unwrap()
                .to_string_lossy()
                .into();
            running.0.push(instance);
            assert!(launch_stored(running.0.last().unwrap()).unwrap().running);
        }
        let first = &running.0[0];
        let second = &running.0[1];
        let second_pid = live_pid(second).unwrap().unwrap();
        assert!(launch_stored(first).unwrap_err().contains("已在运行"));
        stop_stored(first).unwrap();
        assert_eq!(live_pid(first).unwrap(), None);
        assert_eq!(live_pid(second).unwrap(), Some(second_pid));
        assert!(launch_stored(first).unwrap().running);
        assert_eq!(live_pid(&default).unwrap(), baseline);
        assert_eq!(live_pid(second).unwrap(), Some(second_pid));
    }
}
