//! Friendly error when the Visual C++ runtime is missing.
//!
//! build.rs delay-loads VCRUNTIME140, VCRUNTIME140_1 and MSVCP140, so the exe starts
//! without them and the first call into a missing DLL lands in this hook instead of
//! the loader's "VCRUNTIME140.dll was not found" error.
//!
//! The hook may run before `main` and before the runtime is available, so it must not
//! use std (allocation, formatting) — only static strings and plain Win32 calls.

use std::ffi::c_void;

use windows::core::{w, PCWSTR};
use windows::Win32::System::Threading::ExitProcess;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONERROR, MB_YESNO, SW_SHOWNORMAL};

// `dliNotification` values from delayimp.h.
const DLI_FAIL_LOAD_LIBRARY: u32 = 3;
const DLI_FAIL_GET_PROC: u32 = 4;

unsafe extern "system" fn on_delay_load_failure(notify: u32, _info: *mut c_void) -> *mut c_void {
    if notify == DLI_FAIL_LOAD_LIBRARY || notify == DLI_FAIL_GET_PROC {
        unsafe {
            let answer = MessageBoxW(
                None,
                w!("This app needs the Microsoft Visual C++ Redistributable (x64), which is missing or out of date.\n\n\
                    Download: https://aka.ms/vs/17/release/vc_redist.x64.exe\n\n\
                    Download it now?"),
                w!("Run with other GPU"),
                MB_YESNO | MB_ICONERROR,
            );
            if answer == IDYES {
                ShellExecuteW(
                    None,
                    w!("open"),
                    w!("https://aka.ms/vs/17/release/vc_redist.x64.exe"),
                    PCWSTR::null(),
                    PCWSTR::null(),
                    SW_SHOWNORMAL,
                );
            }
            ExitProcess(1);
        }
    }
    std::ptr::null_mut()
}

/// Picked up by the delay-load helper in delayimp.lib (`PfnDliHook __pfnDliFailureHook2`).
#[no_mangle]
#[used]
pub static __pfnDliFailureHook2: unsafe extern "system" fn(u32, *mut c_void) -> *mut c_void = on_delay_load_failure;
