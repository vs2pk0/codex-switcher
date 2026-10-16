use std::process::Command;

/// Build commands for OS utilities, not for launching the user's Codex runtime.
pub(crate) fn new(program: &str) -> Command {
    #[cfg(target_os = "macos")]
    let program = match program {
        "sqlite3" => "/usr/bin/sqlite3",
        "touch" => "/usr/bin/touch",
        "open" => "/usr/bin/open",
        "osascript" => "/usr/bin/osascript",
        "ps" => "/bin/ps",
        "kill" => "/bin/kill",
        other => other,
    };
    let mut command = Command::new(program);
    prepare(&mut command);
    command
}

pub(crate) fn prepare(command: &mut Command) {
    // An injected arm64 library cannot load in Apple's arm64e utilities. Remove
    // injection only for this child, preserving the app and user's environment.
    #[cfg(target_os = "macos")]
    command.env_remove("DYLD_INSERT_LIBRARIES");
    #[cfg(not(target_os = "macos"))]
    let _ = command;
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn native_sqlite_runs_with_an_incompatible_injection_removed() {
        let mut command = new("sqlite3");
        assert_eq!(command.get_program(), "/usr/bin/sqlite3");
        command.env("DYLD_INSERT_LIBRARIES", "/missing/timezone-shim.dylib");
        prepare(&mut command);
        let output = command
            .args([":memory:", "SELECT 6 * 7;"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
    }

    #[test]
    fn native_process_detection_runs_with_an_incompatible_injection_removed() {
        let mut command = new("ps");
        command.env("DYLD_INSERT_LIBRARIES", "/missing/timezone-shim.dylib");
        prepare(&mut command);
        let pid = std::process::id().to_string();
        let output = command.args(["-p", &pid, "-o", "pid="]).output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), pid);
    }
}
