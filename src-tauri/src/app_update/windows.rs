// canonicalize() adds a verbatim prefix on Windows. NSIS and the Windows shell
// expect ordinary drive/UNC paths even though filesystem APIs accept both.
fn shell_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    }
}

#[cfg(target_os = "windows")]
pub(super) fn open(path: &std::path::Path) -> Result<(), String> {
    let target = shell_path(
        path.to_str()
            .ok_or_else(|| "更新安装包路径包含无效字符".to_string())?,
    );
    let directory = path
        .parent()
        .and_then(|parent| parent.to_str())
        .map(shell_path);
    // A fresh STA thread lets shell extensions initialize COM independently of
    // Tauri's workers. ShellExecute also honors an installer's elevation manifest.
    std::thread::spawn(move || {
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
                "初始化安装包启动组件失败（HRESULT 0x{initialized:08X}）"
            ));
        }
        let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let target: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
        let directory: Option<Vec<u16>> =
            directory.map(|value| value.encode_utf16().chain(Some(0)).collect());
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: operation.as_ptr(),
            lpFile: target.as_ptr(),
            lpDirectory: directory
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            nShow: 1,
            ..Default::default()
        };
        let result = if unsafe { ShellExecuteExW(&mut info) } != 0 {
            Ok(())
        } else {
            Err(format!(
                "打开更新安装包失败: {}",
                std::io::Error::last_os_error()
            ))
        };
        unsafe { CoUninitialize() };
        result
    })
    .join()
    .map_err(|_| "安装包启动线程异常退出".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_drive_paths_are_compatible_with_the_installer() {
        assert_eq!(
            shell_path(
                r"\\?\C:\Users\测试 用户\Downloads\Codex Switcher\updates\0.1.44\Codex Switcher.exe"
            ),
            r"C:\Users\测试 用户\Downloads\Codex Switcher\updates\0.1.44\Codex Switcher.exe"
        );
    }

    #[test]
    fn canonical_unc_paths_keep_the_network_share() {
        assert_eq!(
            shell_path(r"\\?\UNC\server\share\Codex Switcher.msi"),
            r"\\server\share\Codex Switcher.msi"
        );
    }

    #[test]
    fn ordinary_paths_are_preserved() {
        for path in [r"C:\Users\测试 用户\setup.exe", r"\\server\share\setup.msi"] {
            assert_eq!(shell_path(path), path);
        }
    }
}
