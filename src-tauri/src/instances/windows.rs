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
    let mut cached = CACHED.lock().ok()?;
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

fn process_rows() -> Result<Vec<ProcessRow>, String> {
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
