use std::path::PathBuf;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

pub const APP_NAME: &str = "Run with other GPU";

/// Release tag injected by CI (e.g. `v1.2.0`); `dev` for local builds.
pub const VERSION: &str = match option_env!("APP_VERSION") {
    Some(v) => v,
    None => "dev",
};

/// `%LOCALAPPDATA%\RunWithOtherGpu` — holds the installed copy and the recovery state.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("RunWithOtherGpu")
}

/// Reuse the parent terminal's console (if any) so CLI output is visible.
pub fn attach_console() {
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

pub fn error_box(message: &str) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from(APP_NAME),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// NUL-terminated UTF-16 buffer for Win32 calls that need a mutable or long-lived string.
pub fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn pcwstr(buf: &[u16]) -> PCWSTR {
    PCWSTR(buf.as_ptr())
}
