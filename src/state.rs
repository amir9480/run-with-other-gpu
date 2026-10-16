//! Crash-safe bookkeeping of GPU preferences we changed, stored in
//! `%LOCALAPPDATA%\RunWithOtherGpu\state.json`.
//!
//! Each changed exe records its original preference and the helper processes
//! ("owners") currently relying on the change. The last owner to finish restores
//! the original; owners that died without cleaning up are detected by PID +
//! creation time and pruned on the next start of any instance.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, STILL_ACTIVE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcess, GetCurrentProcessId, GetExitCodeProcess, GetProcessTimes, OpenProcess,
    ReleaseMutex, WaitForSingleObject, INFINITE, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::gpu::Role;
use crate::{prefs, util};

#[derive(Serialize, Deserialize, Default)]
struct State {
    entries: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    /// Exe path exactly as used for the registry value name.
    exe: String,
    /// Preference string before we touched it; `None` means there was no value.
    original: Option<String>,
    /// Preference string we last wrote.
    applied: String,
    owners: Vec<Owner>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
struct Owner {
    pid: u32,
    created: u64,
}

/// Switches `exe` to `role` and registers this process as an owner of the change.
pub fn acquire(exe: &str, role: Role) -> Result<()> {
    let _lock = Lock::acquire()?;
    let mut state = load();
    prune(&mut state);

    let entry = state.entries.entry(key(exe)).or_insert_with(|| {
        let original = prefs::read(exe);
        Entry { exe: exe.to_string(), applied: original.clone().unwrap_or_default(), original, owners: Vec::new() }
    });
    entry.applied = prefs::with_role(entry.original.as_deref(), role);
    entry.owners.push(Owner::current());
    let applied = entry.applied.clone();

    // Persist before touching the registry, so a crash in between is still recoverable.
    save(&state)?;
    prefs::write(exe, &applied)
}

/// Unregisters this process; restores the original preference if no owners remain.
pub fn release(exe: &str) -> Result<()> {
    let _lock = Lock::acquire()?;
    let mut state = load();
    let me = Owner::current();
    if let Some(entry) = state.entries.get_mut(&key(exe)) {
        entry.owners.retain(|o| *o != me);
    }
    prune(&mut state);
    save(&state)
}

/// Restores preferences whose owners are all gone (e.g. the helper crashed).
pub fn recover_stale() -> Result<()> {
    let _lock = Lock::acquire()?;
    let mut state = load();
    if prune(&mut state) {
        save(&state)?;
    }
    Ok(())
}

/// Whether another helper currently has `exe`'s preference overridden.
pub fn is_overridden(exe: &str) -> bool {
    load().entries.contains_key(&key(exe))
}

pub fn has_active_entries() -> bool {
    !load().entries.is_empty()
}

/// Drops dead owners and restores entries left without owners. Returns whether anything changed.
fn prune(state: &mut State) -> bool {
    let mut changed = false;
    state.entries.retain(|_, entry| {
        let before = entry.owners.len();
        entry.owners.retain(Owner::is_alive);
        changed |= entry.owners.len() != before;
        if !entry.owners.is_empty() {
            return true;
        }
        changed = true;
        // Only revert if the value is still ours; if the user changed it in Settings
        // meanwhile, leave their choice alone.
        if prefs::read(&entry.exe).as_deref() == Some(entry.applied.as_str()) {
            if prefs::restore(&entry.exe, entry.original.as_deref()).is_err() {
                return true; // keep it so we retry next time
            }
        }
        false
    });
    changed
}

fn key(exe: &str) -> String {
    exe.to_lowercase()
}

fn path() -> PathBuf {
    util::data_dir().join("state.json")
}

fn load() -> State {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(state: &State) -> Result<()> {
    let path = path();
    if state.entries.is_empty() {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e).context("removing state file"),
            _ => return Ok(()),
        }
    }
    std::fs::create_dir_all(util::data_dir()).context("creating data directory")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(state)?).context("writing state file")?;
    std::fs::rename(&tmp, &path).context("replacing state file")
}

impl Owner {
    fn current() -> Owner {
        let pid = unsafe { GetCurrentProcessId() };
        let created = creation_time(unsafe { GetCurrentProcess() }).unwrap_or(0);
        Owner { pid, created }
    }

    fn is_alive(&self) -> bool {
        unsafe {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, self.pid) else {
                return false;
            };
            let mut code = 0u32;
            let running = GetExitCodeProcess(handle, &mut code).is_ok() && code == STILL_ACTIVE.0 as u32;
            // A matching creation time rules out PID reuse.
            let same = creation_time(handle) == Some(self.created);
            let _ = CloseHandle(handle);
            running && same
        }
    }
}

fn creation_time(process: HANDLE) -> Option<u64> {
    let (mut created, mut exit, mut kernel, mut user) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetProcessTimes(process, &mut created, &mut exit, &mut kernel, &mut user).ok()? };
    Some(((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
}

/// Cross-process lock around state read-modify-write.
struct Lock(HANDLE);

impl Lock {
    fn acquire() -> Result<Lock> {
        unsafe {
            let handle = CreateMutexW(None, false, w!("Local\\RunWithOtherGpu.State")).context("creating state mutex")?;
            let wait = WaitForSingleObject(handle, INFINITE);
            if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
                let _ = CloseHandle(handle);
                anyhow::bail!("waiting for state mutex failed");
            }
            Ok(Lock(handle))
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}
