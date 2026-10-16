#[cfg(not(windows))]
use std::process::Command;

pub(crate) fn open(url: &str) -> Result<(), String> {
    let url = validate_url(url)?;
    open_with_system(url)
}

fn validate_url(input: &str) -> Result<&str, String> {
    let input = input.trim();
    let parsed = url::Url::parse(input).map_err(|_| "链接格式无效".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("只能打开 http/https 链接".to_string());
    }
    if input.contains('\0') {
        return Err("链接包含无效字符".to_string());
    }
    Ok(input)
}

#[cfg(not(windows))]
fn run_opener(command: &mut Command) -> Result<(), String> {
    crate::system_command::prepare(command);
    let output = command
        .output()
        .map_err(|error| format!("系统浏览器打开失败：{error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = detail.trim();
    Err(if detail.is_empty() {
        format!("系统浏览器打开失败（{}）", output.status)
    } else {
        format!("系统浏览器打开失败：{detail}")
    })
}

#[cfg(target_os = "macos")]
fn open_with_system(url: &str) -> Result<(), String> {
    // Use the OS utility even when a GUI launch inherits a custom PATH.
    let default_result = run_opener(Command::new("/usr/bin/open").arg(url));
    if default_result.is_ok() {
        return default_result;
    }
    // A stale default-browser registration must not disable every About link.
    for browser in ["Safari", "Google Chrome", "Microsoft Edge", "Firefox"] {
        if run_opener(Command::new("/usr/bin/open").args(["-a", browser, url])).is_ok() {
            return Ok(());
        }
    }
    default_result
}

#[cfg(windows)]
fn open_with_system(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let target: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    // Avoid passing URLs through PowerShell/cmd argument parsing.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err(format!("系统浏览器打开失败（ShellExecuteW：{result}）"))
    }
}

#[cfg(all(not(target_os = "macos"), not(windows)))]
fn open_with_system(url: &str) -> Result<(), String> {
    run_opener(Command::new("xdg-open").arg(url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_external_links_without_changing_query_or_fragment() {
        let url = "https://github.com/vs2pk0/codex-account-switcher?a=1&b=2#readme";
        assert_eq!(validate_url(&format!(" {url} ")).unwrap(), url);
        for invalid in [
            "file:///tmp/a",
            "javascript:alert(1)",
            "not a url",
            "https://",
        ] {
            assert!(validate_url(invalid).is_err(), "{invalid}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn reports_opener_stderr_and_exit_status() {
        let error = run_opener(
            Command::new("/bin/sh").args(["-c", "printf 'browser unavailable' >&2; exit 1"]),
        )
        .unwrap_err();
        assert!(error.contains("browser unavailable"));
        assert!(run_opener(Command::new("/bin/sh").args(["-c", "exit 2"]))
            .unwrap_err()
            .contains("2"));
        assert!(run_opener(Command::new("/bin/sh").args(["-c", "exit 0"])).is_ok());
    }
}
