//! Per-app GPU preference, as written by Settings > System > Display > Graphics.

use anyhow::{Context, Result};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

use crate::gpu::Role;

const KEY: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";
const FIELD: &str = "GpuPreference=";

/// Raw preference string for `exe` (e.g. `"SwapEffectUpgradeEnable=1;GpuPreference=2;"`).
pub fn read(exe: &str) -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(KEY, KEY_READ)
        .ok()?
        .get_value(exe)
        .ok()
}

/// The role Windows will use for an app with this preference string. No entry, or
/// "Let Windows decide" (0), is treated as power saving.
pub fn effective_role(raw: Option<&str>) -> Role {
    let pref = raw.and_then(|raw| {
        raw.split(';')
            .find_map(|kv| kv.trim().strip_prefix(FIELD))
            .and_then(|v| v.trim().parse::<u32>().ok())
    });
    if pref == Some(Role::HighPerformance.pref_value()) {
        Role::HighPerformance
    } else {
        Role::PowerSaving
    }
}

/// `raw` with only its `GpuPreference=` field replaced, keeping any other fields.
pub fn with_role(raw: Option<&str>, role: Role) -> String {
    let mut out: String = raw
        .unwrap_or("")
        .split(';')
        .map(str::trim)
        .filter(|kv| !kv.is_empty() && !kv.starts_with(FIELD))
        .map(|kv| format!("{kv};"))
        .collect();
    out.push_str(&format!("{FIELD}{};", role.pref_value()));
    out
}

pub fn write(exe: &str, value: &str) -> Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(KEY)
        .context("opening UserGpuPreferences")?;
    key.set_value(exe, &value.to_string())
        .with_context(|| format!("writing GPU preference for {exe}"))
}

/// Puts back `original`, or deletes the value if the app had no preference before.
pub fn restore(exe: &str, original: Option<&str>) -> Result<()> {
    match original {
        Some(value) => write(exe, value),
        None => {
            let key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(KEY, KEY_SET_VALUE) {
                Ok(key) => key,
                Err(_) => return Ok(()),
            };
            match key.delete_value(exe) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    Err(e).with_context(|| format!("removing GPU preference for {exe}"))
                }
                _ => Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_role_defaults_to_power_saving() {
        assert_eq!(effective_role(None), Role::PowerSaving);
        assert_eq!(effective_role(Some("GpuPreference=0;")), Role::PowerSaving);
        assert_eq!(effective_role(Some("GpuPreference=1;")), Role::PowerSaving);
        assert_eq!(effective_role(Some("SwapEffectUpgradeEnable=1;")), Role::PowerSaving);
        assert_eq!(
            effective_role(Some("SwapEffectUpgradeEnable=1;GpuPreference=2;")),
            Role::HighPerformance
        );
    }

    #[test]
    fn with_role_keeps_other_fields() {
        assert_eq!(with_role(None, Role::HighPerformance), "GpuPreference=2;");
        assert_eq!(
            with_role(Some("GpuPreference=1;SwapEffectUpgradeEnable=1;"), Role::HighPerformance),
            "SwapEffectUpgradeEnable=1;GpuPreference=2;"
        );
    }
}
