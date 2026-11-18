//! The per-program graphics preference, and the launch-time re-application of it.
//!
//! Windows keeps one value per executable path in
//! `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`. The value's **name** is the
//! full image path and its **data** is a semicolon list of clauses, of which one
//! decides the card:
//!
//! - `GpuPreference=0` lets Windows decide,
//! - `GpuPreference=1` asks for the power-saving card,
//! - `GpuPreference=2` asks for the high-performance card.
//!
//! The entry names an exact file, which is why it belongs to the launch path and not
//! only to a button: a Roblox update installs a new `version-…` folder, and the entry
//! that was written for the old path keeps pointing at a file nothing runs any more.
//!
//! This is the same switch the person could set in Settings > System > Display >
//! Graphics. Nothing here reads a client process or touches a version folder.

use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    REG_VALUE_TYPE, RRF_RT_REG_SZ,
};

use crate::error::{FailureCode, LaunchError};

const SUBKEY: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Decide,
    PowerSaving,
    HighPerformance,
}

impl Choice {
    pub fn code(self) -> u32 {
        match self {
            Choice::Decide => 0,
            Choice::PowerSaving => 1,
            Choice::HighPerformance => 2,
        }
    }

    pub fn from_code(code: u32) -> Option<Choice> {
        match code {
            0 => Some(Choice::Decide),
            1 => Some(Choice::PowerSaving),
            2 => Some(Choice::HighPerformance),
            _ => None,
        }
    }

    /// What the setting is called where a person would meet it, which is Windows'
    /// own wording and not a rephrasing of it.
    pub fn label(self) -> &'static str {
        match self {
            Choice::Decide => "Let Windows decide",
            Choice::PowerSaving => "Power saving",
            Choice::HighPerformance => "High performance",
        }
    }
}

/// Replaces the `GpuPreference` clause and keeps every other one.
///
/// Windows writes more than one clause into this value on some machines
/// (`SwapEffectUpgradeEnable=1` sits beside the preference), and an overwrite would
/// drop a setting nobody here touched. Pure, so it is tested without a registry.
pub fn merge(raw: &str, choice: Choice) -> String {
    let mut clauses: Vec<String> = raw
        .split(';')
        .map(str::trim)
        .filter(|clause| !clause.is_empty())
        .filter(|clause| {
            !clause
                .split_once('=')
                .map(|(key, _)| key.trim().eq_ignore_ascii_case("GpuPreference"))
                .unwrap_or(false)
        })
        .map(str::to_string)
        .collect();

    clauses.push(format!("GpuPreference={}", choice.code()));
    let mut text = clauses.join(";");
    text.push(';');
    text
}

/// The clause a raw value carries, wherever it sits in the list.
pub fn parse(raw: &str) -> Option<Choice> {
    raw.split(';')
        .filter_map(|clause| clause.trim().split_once('='))
        .find(|(key, _)| key.trim().eq_ignore_ascii_case("GpuPreference"))
        .and_then(|(_, value)| value.trim().parse::<u32>().ok())
        .and_then(Choice::from_code)
}

/// What the entry for this executable says now. `None` is both "no entry" and "an
/// entry with no preference clause", which Windows treats the same way.
pub fn read(exe: &Path) -> Result<Option<Choice>, LaunchError> {
    Ok(raw(exe)?.as_deref().and_then(parse))
}

/// Writes the preference, or removes the entry for `Decide`.
pub fn write(exe: &Path, choice: Choice) -> Result<(), LaunchError> {
    let name = name_of(exe);
    if choice == Choice::Decide {
        return remove(&name);
    }

    let next = merge(raw(exe)?.unwrap_or_default().as_str(), choice);
    let subkey = wide(SUBKEY);
    let value = wide(&name);
    let data: Vec<u8> = wide(&next)
        .iter()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();

    let mut key = HKEY::default();
    // SAFETY: both names are null terminated, `key` is a live out-parameter, and the
    // key is closed below on every path after this point.
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    check(status, "create the graphics preference key")?;

    // SAFETY: `key` came from the call above, the value name is null terminated, and
    // the data is a UTF-16 string with its terminator. Closed below on both paths.
    let status = unsafe { RegSetValueExW(key, PCWSTR(value.as_ptr()), None, REG_SZ, Some(&data)) };
    let closed = unsafe { RegCloseKey(key) };
    check(closed, "close the graphics preference key")?;
    check(status, &format!("write the preference for {}", name))
}

/// Writes only when the entry disagrees, so a launch that changes nothing writes
/// nothing. `Ok(true)` means this call is what changed it.
pub fn ensure(exe: &Path, choice: Choice) -> Result<bool, LaunchError> {
    if read(exe)? == Some(choice) {
        return Ok(false);
    }
    write(exe, choice)?;
    Ok(true)
}

fn raw(exe: &Path) -> Result<Option<String>, LaunchError> {
    let subkey = wide(SUBKEY);
    let name = wide(&name_of(exe));
    let mut buffer = vec![0u8; 2048];
    let mut size = buffer.len() as u32;
    let mut kind = REG_VALUE_TYPE::default();

    // SAFETY: the subkey and value names are null terminated and outlive the call,
    // and `size` describes the buffer exactly as the API documents it.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            Some(&mut kind),
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };

    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    check(status, &format!("read the preference for {}", name_of(exe)))?;

    let units: Vec<u16> = buffer[..size as usize]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    Ok(Some(String::from_utf16_lossy(&units)))
}

fn remove(name: &str) -> Result<(), LaunchError> {
    let subkey = wide(SUBKEY);
    let mut key = HKEY::default();

    // SAFETY: the subkey is null terminated, `key` is a live out-parameter, and the
    // handle is closed below on every path that opened it.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            &mut key,
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(status, "open the graphics preference key")?;

    let value = wide(name);
    // SAFETY: `key` was opened above and the value name is null terminated.
    let status = unsafe { RegDeleteValueW(key, PCWSTR(value.as_ptr())) };
    let closed = unsafe { RegCloseKey(key) };
    check(closed, "close the graphics preference key")?;

    // A value that is not there is the state this call exists to reach.
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(status, &format!("remove the preference for {name}"))
}

fn check(status: WIN32_ERROR, what: &str) -> Result<(), LaunchError> {
    if status == ERROR_SUCCESS {
        return Ok(());
    }
    Err(LaunchError::new(
        FailureCode::ConfigUnwritable,
        format!("Windows could not {what} (error {}).", status.0),
    )
    .with_action("Open Windows graphics settings"))
}

/// The value name is the path as given: Windows matches these entries by full path.
fn name_of(exe: &Path) -> String {
    exe.to_string_lossy().to_string()
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preference_clause_is_replaced_and_the_rest_kept() {
        assert_eq!(merge("", Choice::HighPerformance), "GpuPreference=2;");
        assert_eq!(
            merge("SwapEffectUpgradeEnable=1;", Choice::HighPerformance),
            "SwapEffectUpgradeEnable=1;GpuPreference=2;"
        );
        assert_eq!(
            merge("GpuPreference=1;SwapEffectUpgradeEnable=1;", Choice::Decide),
            "SwapEffectUpgradeEnable=1;GpuPreference=0;"
        );
    }

    #[test]
    fn the_clause_is_read_wherever_it_sits_and_however_it_is_cased() {
        assert_eq!(parse("GpuPreference=2;"), Some(Choice::HighPerformance));
        assert_eq!(
            parse("SwapEffectUpgradeEnable=1;gpupreference=1;"),
            Some(Choice::PowerSaving)
        );
        assert_eq!(parse("GpuPreference=0;"), Some(Choice::Decide));
        assert_eq!(parse("SwapEffectUpgradeEnable=1;"), None);
        assert_eq!(parse("GpuPreference=nonsense;"), None);
        assert_eq!(parse(""), None);
    }

    /// The one test that touches the real key, against a path that is deliberately
    /// not a program: the board's rule is to prove the write on a scratch name
    /// rather than on Roblox's own entry.
    #[test]
    fn a_scratch_path_round_trips_through_the_registry() {
        let probe = std::env::temp_dir().join("kuro-gpu-preference-probe.exe");
        let _ = write(&probe, Choice::Decide);

        assert_eq!(read(&probe).unwrap(), None, "a clean entry reads as none");
        assert!(ensure(&probe, Choice::HighPerformance).unwrap());
        assert_eq!(read(&probe).unwrap(), Some(Choice::HighPerformance));
        assert!(
            !ensure(&probe, Choice::HighPerformance).unwrap(),
            "the second write must be a no-op"
        );

        // Removing the entry leaves the key itself in place, which is what Windows
        // does too, and is what the screen reads as "no entry".
        let _ = write(&probe, Choice::Decide);
        assert_eq!(read(&probe).unwrap(), None);
    }
}
