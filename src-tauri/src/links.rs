//! Party links: a `kuro://` address that carries a place, one exact server and one
//! account, and opens this app on the right launch.
//!
//! Why a scheme and not a shared game link: Roblox's own links cannot name an account,
//! and a game link that names a server is long enough that nobody pastes it twice. The
//! payload here is three facts the launcher already understands.
//!
//! Registration is written at runtime into `HKCU\Software\Classes`, not by the installer,
//! and that is a decision rather than an accident:
//!
//! - it needs no administrator, so a portable copy can register itself;
//! - it is one value the person can remove from this app, which is a better promise
//!   than "reinstall to undo it";
//! - and it points at whichever executable is running, so a moved folder re-registers
//!   instead of leaving a dead link behind.
//!
//! Nothing here launches anything. It turns one string into three facts, and the launch
//! path decides what to do with them.

use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    REG_VALUE_TYPE, RRF_RT_REG_SZ,
};

use crate::error::{FailureCode, LaunchError};

/// The scheme this app answers to. Short, and the product's own name.
pub const SCHEME: &str = "kuro";

/// What a link carries. Every field is optional because a link is text from outside and
/// half a link is a real thing to receive.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Party {
    pub place: Option<u64>,
    /// The job id: Roblox's own name for one exact server instance.
    pub server: Option<String>,
    pub account: Option<u64>,
}

impl Party {
    /// What the Launch page should be given, in the shape it already reads.
    pub fn target(&self) -> Option<String> {
        match (self.place, &self.server) {
            (Some(place), Some(server)) => {
                Some(format!("roblox://placeId={place}&gameInstanceId={server}"))
            }
            (Some(place), None) => Some(place.to_string()),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.place.is_none() && self.server.is_none() && self.account.is_none()
    }
}

/// Percent-encodes a query value. Everything a job id is made of survives unchanged, and
/// everything else is escaped rather than trusted.
fn escape(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn unescape(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() + 1 {
            let hex = value.get(index + 1..index + 3).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// The link for a party. A party with nothing in it has no link, which is the honest
/// answer rather than `kuro://join?` that opens an empty page.
pub fn url_for(party: &Party) -> Option<String> {
    if party.place.is_none() && party.server.is_none() {
        return None;
    }

    let mut query: Vec<String> = Vec::new();
    if let Some(place) = party.place {
        query.push(format!("place={place}"));
    }
    if let Some(server) = &party.server {
        query.push(format!("server={}", escape(server)));
    }
    if let Some(account) = party.account {
        query.push(format!("account={account}"));
    }

    Some(format!("{SCHEME}://join?{}", query.join("&")))
}

/// Reads a `kuro://` address. Anything else, including another scheme, is `None`.
pub fn parse(url: &str) -> Option<Party> {
    let rest = url
        .trim()
        .strip_prefix(&format!("{SCHEME}://"))
        .or_else(|| {
            url.trim()
                .strip_prefix(&format!("{}://", SCHEME.to_uppercase()))
        })?;

    // `join?…`, `join`, or a bare query are all accepted: the first is what this app
    // writes, and the other two are what a person typing by hand produces.
    let query = match rest.split_once('?') {
        Some((_, query)) => query,
        None => rest,
    };

    let mut party = Party::default();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key.trim().to_ascii_lowercase().as_str() {
            "place" | "placeid" => party.place = value.trim().parse().ok(),
            "server" | "job" | "gameinstanceid" => {
                let server = unescape(value.trim());
                if !server.is_empty() {
                    party.server = Some(server);
                }
            }
            "account" | "user" | "userid" => party.account = value.trim().parse().ok(),
            _ => {}
        }
    }

    if party.is_empty() {
        return None;
    }
    Some(party)
}

/// The first `kuro://` address in a command line, which is how Windows hands one over.
pub fn from_args<'a>(args: impl IntoIterator<Item = &'a String>) -> Option<Party> {
    args.into_iter().find_map(|arg| parse(arg))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

fn classes(scheme: &str) -> String {
    format!(r"Software\Classes\{scheme}")
}

fn command_key(scheme: &str) -> String {
    format!(r"Software\Classes\{scheme}\shell\open\command")
}

/// What Windows would run for this scheme right now, if anything.
pub fn state(scheme: &str) -> Option<String> {
    read_value(HKEY_CURRENT_USER, &command_key(scheme), "")
        .ok()
        .flatten()
}

/// Registers the scheme for this executable. Anonymous in the registry but not here: the
/// command names the exact file, so a second copy of the app is a second registration
/// and the last one wins, which is what a person who just ran a newer build expects.
pub fn register(scheme: &str, exe: &Path) -> Result<String, LaunchError> {
    let command = format!("\"{}\" \"%1\"", exe.to_string_lossy());

    let root = classes(scheme);
    write_value(&root, "", "URL:KURO launch link")?;
    write_value(&root, "URL Protocol", "")?;
    write_value(&command_key(scheme), "", &command)?;

    Ok(command)
}

/// Removes the whole tree for this scheme.
pub fn unregister(scheme: &str) -> Result<(), LaunchError> {
    let subkey = wide(&classes(scheme));

    // SAFETY: the subkey is null terminated; a missing key is the state this call exists
    // to reach, so it is not an error.
    let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr())) };
    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    Err(failure(status, "remove the link registration"))
}

fn write_value(key: &str, name: &str, value: &str) -> Result<(), LaunchError> {
    let subkey = wide(key);
    let value_name = wide(name);
    let data: Vec<u8> = wide(value)
        .iter()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();

    let mut handle = HKEY::default();
    // SAFETY: both strings are null terminated, `handle` is a live out-parameter, and the
    // key is closed below on every path.
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            None,
            &mut handle,
            None,
        )
    };
    check(status, "create the link registration")?;

    // SAFETY: the handle came from the call above, the name is null terminated, and the
    // data is a UTF-16 string with its terminator.
    let status = unsafe {
        RegSetValueExW(
            handle,
            PCWSTR(value_name.as_ptr()),
            None,
            REG_SZ,
            Some(&data),
        )
    };
    let closed = unsafe { RegCloseKey(handle) };
    check(closed, "close the link registration")?;
    check(status, "write the link registration")
}

fn read_value(root: HKEY, key: &str, name: &str) -> Result<Option<String>, LaunchError> {
    let subkey = wide(key);
    let value_name = wide(name);
    let mut buffer = vec![0u8; 2048];
    let mut size = buffer.len() as u32;
    let mut kind = REG_VALUE_TYPE::default();

    // SAFETY: names are null terminated and outlive the call; `size` describes the buffer.
    let status = unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value_name.as_ptr()),
            RRF_RT_REG_SZ,
            Some(&mut kind),
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };

    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    check(status, "read the link registration")?;

    let units: Vec<u16> = buffer[..size as usize]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    Ok(Some(String::from_utf16_lossy(&units)))
}

fn check(status: WIN32_ERROR, what: &str) -> Result<(), LaunchError> {
    if status == ERROR_SUCCESS {
        return Ok(());
    }
    Err(failure(status, what))
}

fn failure(status: WIN32_ERROR, what: &str) -> LaunchError {
    LaunchError::new(
        FailureCode::ConfigUnwritable,
        format!("Windows could not {what} (error {}).", status.0),
    )
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_link_carries_three_facts_and_reads_back_whole() {
        let party = Party {
            place: Some(920_587_237),
            server: Some("2c0a4b1e-8f10-4d3a-9e2b-1a2b3c4d5e6f".to_string()),
            account: Some(663_048_626),
        };

        let url = url_for(&party).unwrap();
        assert_eq!(
            url,
            "kuro://join?place=920587237&server=2c0a4b1e-8f10-4d3a-9e2b-1a2b3c4d5e6f&account=663048626"
        );
        assert_eq!(parse(&url), Some(party));
    }

    #[test]
    fn a_half_link_is_read_for_the_half_it_has() {
        let place_only = parse("kuro://join?place=1818").unwrap();
        assert_eq!(place_only.place, Some(1818));
        assert_eq!(place_only.server, None);
        assert_eq!(place_only.target().as_deref(), Some("1818"));

        let job_only = parse("kuro://join?server=abc123").unwrap();
        assert_eq!(job_only.server.as_deref(), Some("abc123"));
        // No place means nothing the launch path can open: a job id alone is not a place.
        assert_eq!(job_only.target(), None);
    }

    #[test]
    fn the_shapes_a_person_types_are_read_too() {
        assert_eq!(parse("kuro://join").map(|p| p.is_empty()), None);
        assert_eq!(
            parse("kuro://placeId=1818&gameInstanceId=abc")
                .unwrap()
                .server
                .as_deref(),
            Some("abc")
        );
        assert_eq!(
            parse("kuro://join?place=1818&job=zzz").unwrap().place,
            Some(1818)
        );
        assert_eq!(parse("https://roblox.com/games/1818"), None);
        assert_eq!(parse("not a link"), None);
    }

    #[test]
    fn a_server_that_needs_escaping_survives_the_round_trip() {
        let party = Party {
            place: Some(1),
            server: Some("a b&c=d".to_string()),
            account: None,
        };
        let url = url_for(&party).unwrap();
        assert!(url.contains("server=a%20b%26c%3Dd"));
        assert_eq!(parse(&url), Some(party));
    }

    #[test]
    fn the_command_line_is_searched_for_a_link() {
        let args: Vec<String> = vec![
            "kuro-launcher.exe".to_string(),
            "--open".to_string(),
            "kuro://join?place=1818".to_string(),
        ];
        // The executable's own name is first and is not a link.
        assert_eq!(from_args(&args).unwrap().place, Some(1818));
        assert_eq!(from_args(&Vec::<String>::new()), None);
    }

    /// The one test that touches the real class tree, under a scheme that is deliberately
    /// not `kuro`: proving the write does not mean registering this app on the machine
    /// that runs the suite.
    #[test]
    fn a_scratch_scheme_registers_reads_back_and_unregisters() {
        let scheme = "kuro-test-probe";
        let exe = PathBuf::from(r"C:\nowhere\kuro-probe.exe");

        unregister(scheme).unwrap();
        assert_eq!(state(scheme), None);

        let command = register(scheme, &exe).unwrap();
        assert_eq!(command, "\"C:\\nowhere\\kuro-probe.exe\" \"%1\"");
        assert_eq!(state(scheme).as_deref(), Some(command.as_str()));

        unregister(scheme).unwrap();
        assert_eq!(state(scheme), None);
    }
}
