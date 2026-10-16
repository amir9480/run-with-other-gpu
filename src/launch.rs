//! `--run <role> <exe>`: launch an app on a GPU and revert the preference when it exits.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use windows::core::{w, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_ELEVATION_REQUIRED, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::GetLongPathNameW;
use windows::Win32::System::SystemServices::JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO;
use windows::Win32::System::IO::{CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectAssociateCompletionPortInformation,
    SetInformationJobObject, JOBOBJECT_ASSOCIATE_COMPLETION_PORT,
};
use windows::Win32::System::Threading::{
    CreateProcessW, ResumeThread, WaitForSingleObject, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, INFINITE,
    PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::gpu::Role;
use crate::{prefs, state, util};

pub fn run(args: &[OsString]) -> i32 {
    match run_inner(args) {
        Ok(()) => 0,
        Err(e) => {
            util::error_box(&format!("{e:#}"));
            1
        }
    }
}

fn run_inner(args: &[OsString]) -> Result<()> {
    let [role, exe] = args else {
        bail!("Expected: --run <power-saving|high-performance> <exe>");
    };
    let role = role
        .to_str()
        .and_then(Role::from_arg)
        .ok_or_else(|| anyhow!("Unknown GPU role {:?}", role))?;
    let exe = full_path(Path::new(exe))?;
    let exe_name = exe
        .to_str()
        .ok_or_else(|| anyhow!("Path is not valid Unicode: {}", exe.display()))?
        .to_string();

    // Already the GPU Windows would pick: just launch, nothing to change or revert.
    // If another helper has the preference overridden right now, join it instead so
    // the shared refcount keeps the original safe.
    if !state::is_overridden(&exe_name) && prefs::effective_role(prefs::read(&exe_name).as_deref()) == role {
        let process = shell_execute(&exe, false)?;
        if let Some(h) = process {
            unsafe { let _ = CloseHandle(h); }
        }
        return Ok(());
    }

    state::acquire(&exe_name, role)?;
    let result = launch_and_wait(&exe);
    let released = state::release(&exe_name);
    result.with_context(|| format!("Failed to run {}", exe.display()))?;
    released
}

/// Absolute path with any 8.3 short names expanded, matching the image path DirectX looks up.
fn full_path(path: &Path) -> Result<PathBuf> {
    let abs = std::path::absolute(path).with_context(|| format!("Invalid path {}", path.display()))?;
    if !abs.is_file() {
        bail!("File not found: {}", abs.display());
    }
    let input = util::wide(&abs);
    let mut buf = vec![0u16; 32768];
    let len = unsafe { GetLongPathNameW(util::pcwstr(&input), Some(&mut buf)) } as usize;
    if len == 0 || len >= buf.len() {
        return Ok(abs);
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
}

/// Starts `exe` inside a job object and blocks until every process in the job has exited.
fn launch_and_wait(exe: &Path) -> Result<()> {
    unsafe {
        let job = Handle(CreateJobObjectW(None, None).context("CreateJobObject")?);
        let port = Handle(CreateIoCompletionPort(INVALID_HANDLE_VALUE, None, 0, 1).context("CreateIoCompletionPort")?);
        let info = JOBOBJECT_ASSOCIATE_COMPLETION_PORT { CompletionKey: job.0 .0, CompletionPort: port.0 };
        SetInformationJobObject(
            job.0,
            JobObjectAssociateCompletionPortInformation,
            &info as *const _ as *const _,
            std::mem::size_of_val(&info) as u32,
        )
        .context("SetInformationJobObject")?;

        let app = util::wide(exe);
        let mut cmdline = util::wide(format!("\"{}\"", exe.display()));
        let dir = exe.parent().map(util::wide);
        let startup = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
        let mut pi = PROCESS_INFORMATION::default();

        let created = CreateProcessW(
            util::pcwstr(&app),
            Some(PWSTR(cmdline.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            None,
            dir.as_deref().map(util::pcwstr).unwrap_or_default(),
            &startup,
            &mut pi,
        );

        match created {
            Ok(()) => {
                let process = Handle(pi.hProcess);
                let thread = Handle(pi.hThread);
                let in_job = AssignProcessToJobObject(job.0, process.0).is_ok();
                ResumeThread(thread.0);
                if in_job {
                    wait_for_empty_job(job.0, port.0)
                } else {
                    WaitForSingleObject(process.0, INFINITE);
                    Ok(())
                }
            }
            // The app's manifest requires admin: go through the shell to get a UAC prompt.
            // A non-elevated process can't put an elevated one in its job, so only the
            // launched process itself is tracked.
            Err(e) if e.code() == ERROR_ELEVATION_REQUIRED.to_hresult() => {
                if let Some(process) = shell_execute(exe, true)? {
                    let process = Handle(process);
                    WaitForSingleObject(process.0, INFINITE);
                }
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }
}

fn wait_for_empty_job(job: HANDLE, port: HANDLE) -> Result<()> {
    loop {
        let mut code = 0u32;
        let mut key = 0usize;
        let mut overlapped: *mut OVERLAPPED = std::ptr::null_mut();
        unsafe { GetQueuedCompletionStatus(port, &mut code, &mut key, &mut overlapped, INFINITE) }
            .context("GetQueuedCompletionStatus")?;
        if key == job.0 as usize && code == JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO {
            return Ok(());
        }
    }
}

/// Launches via the shell (handles UAC). Returns the process handle when `want_handle`.
fn shell_execute(exe: &Path, want_handle: bool) -> Result<Option<HANDLE>> {
    let file = util::wide(exe);
    let dir = exe.parent().map(util::wide);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | if want_handle { SEE_MASK_NOCLOSEPROCESS } else { Default::default() },
        lpVerb: w!("open"),
        lpFile: util::pcwstr(&file),
        lpDirectory: dir.as_deref().map(util::pcwstr).unwrap_or_default(),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.with_context(|| format!("Failed to start {}", exe.display()))?;
    Ok((want_handle && !info.hProcess.is_invalid()).then_some(info.hProcess))
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}
