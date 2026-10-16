//! Classic Explorer context menu for .exe files (HKCU, no admin required).
//! On Windows 11 it appears under "Show more options".

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

use crate::{gpu, state, util};

const MENU_KEY: &str = r"Software\Classes\exefile\shell\RunWithOtherGpu";
const MENU_TITLE: &str = "Run with other graphic";
const EXE_NAME: &str = "run-with-other-gpu.exe";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn installed_exe() -> PathBuf {
    util::data_dir().join(EXE_NAME)
}

pub fn is_installed() -> bool {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(MENU_KEY).is_ok()
}

/// Copies this exe to LocalAppData and registers the menu with one entry per GPU role.
pub fn install() -> Result<Vec<gpu::Gpu>> {
    let target = installed_exe();
    copy_self_to(&target)?;
    let target_str = target.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(MENU_KEY);
    let (menu, _) = hkcu.create_subkey(MENU_KEY).context("creating context menu key")?;
    menu.set_value("MUIVerb", &MENU_TITLE)?;
    menu.set_value("SubCommands", &"")?;
    menu.set_value("Icon", &format!("\"{target_str}\",0"))?;

    let gpus = gpu::detect();
    for (i, g) in gpus.iter().enumerate() {
        let (item, _) = menu.create_subkey(format!(r"shell\{:02}{}", i + 1, g.role.arg()))?;
        item.set_value("MUIVerb", &g.menu_label())?;
        let (command, _) = item.create_subkey("command")?;
        command.set_value("", &format!("\"{target_str}\" --run {} \"%1\"", g.role.arg()))?;
    }

    notify_shell();
    Ok(gpus)
}

/// Removes the menu and the installed copy.
pub fn remove() -> Result<String> {
    match RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(MENU_KEY) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(e).context("removing context menu key");
        }
        _ => {}
    }
    notify_shell();

    let target = installed_exe();
    let _ = std::fs::remove_file(target.with_extension("old.exe"));
    let mut msg = String::from("Context menu removed.");
    if target.exists() && std::fs::remove_file(&target).is_err() {
        // Running from the installed copy (or a helper is still using it): delete once it exits.
        delete_later(&target);
        msg.push_str("\nThe installed copy will be deleted once it is no longer running.");
    }
    if !state::has_active_entries() {
        let _ = std::fs::remove_dir(util::data_dir());
    }
    Ok(msg)
}

fn copy_self_to(target: &Path) -> Result<()> {
    let me = std::env::current_exe().context("locating current exe")?;
    if same_path(&me, target) {
        return Ok(());
    }
    std::fs::create_dir_all(target.parent().unwrap()).context("creating install directory")?;
    if target.exists() && std::fs::remove_file(target).is_err() {
        // A running helper locks the file, but a running exe can still be renamed.
        let old = target.with_extension("old.exe");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(target, &old).context("replacing installed copy")?;
    }
    std::fs::copy(&me, target).with_context(|| format!("copying to {}", target.display()))?;
    Ok(())
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Retries deletion from a hidden cmd for up to a minute, so it succeeds after the exe exits.
fn delete_later(path: &Path) {
    let script = format!(
        "for /l %i in (1,1,30) do (del /f /q \"{0}\" 2>nul & if not exist \"{0}\" exit & ping -n 3 127.0.0.1 >nul)",
        path.display()
    );
    let _ = std::process::Command::new("cmd")
        .arg("/c")
        .raw_arg(script)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

fn notify_shell() {
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}
