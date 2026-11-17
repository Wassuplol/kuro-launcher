//! What a Windows executable says about itself, and whether Windows trusts it.
//!
//! Two separate questions, deliberately not merged: the version resource names
//! a publisher and a build, and the Authenticode signature proves the bytes
//! came from someone Windows trusts. A client folder is only "real" when both
//! answers agree, so neither check alone is treated as proof.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Security::WinTrust::{
    WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
    WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_NONE,
    WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};

/// The publisher this launcher accepts as Roblox. Compared case-insensitively,
/// because the resource is free text and Roblox has shipped both casings.
pub const PUBLISHER: &str = "Roblox Corporation";

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

/// The fields of the version resource this app reads. Anything else is ignored
/// rather than surfaced, so the UI cannot show a value nobody verifies.
#[derive(Debug, Clone, Default)]
pub struct FileIdentity {
    pub company: Option<String>,
    /// Dotted build, for example `0.747.0.7471481`.
    pub version: Option<String>,
}

impl FileIdentity {
    pub fn claims_roblox(&self) -> bool {
        self.company
            .as_deref()
            .is_some_and(|name| name.trim().eq_ignore_ascii_case(PUBLISHER))
    }
}

/// Reads the version resource. Returns `None` when the file has none, which is
/// itself information: a folder without one is not a client.
pub fn read_identity(path: &Path) -> Option<FileIdentity> {
    let file = wide(path.as_os_str());

    // SAFETY: `file` is a NUL-terminated UTF-16 string that outlives both calls.
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(file.as_ptr()), None) };
    if size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; size as usize];
    // SAFETY: the buffer was allocated with exactly the size the first call
    // reported, which is the contract GetFileVersionInfoW expects.
    let ok = unsafe {
        GetFileVersionInfoW(
            PCWSTR(file.as_ptr()),
            None,
            size,
            buffer.as_mut_ptr().cast(),
        )
    };
    if ok.is_err() {
        return None;
    }

    // The block carries a language and a code page, in that order. Both are
    // needed: the key is the pair, and using one twice silently matches nothing.
    let (language, code_page) = query_translation(&buffer)?;
    let prefix = format!("\\StringFileInfo\\{language:04x}{code_page:04x}\\");

    Some(FileIdentity {
        company: query_string(&buffer, &format!("{prefix}CompanyName")),
        // Windows stores this as "0, 747, 0, 7471481". Dots are what every
        // other place in the app compares and prints, so it is normalised once
        // here rather than at each screen.
        version: query_string(&buffer, &format!("{prefix}FileVersion"))
            .or_else(|| query_string(&buffer, &format!("{prefix}ProductVersion")))
            .map(|value| value.replace(',', ".").replace(" ", "")),
    })
}

/// The language and code page the strings are stored under. Read rather than
/// assumed: English is common but not universal, and a wrong pair returns
/// nothing at all.
fn query_translation(buffer: &[u8]) -> Option<(u16, u16)> {
    let sub = wide(OsStr::new("\\VarFileInfo\\Translation"));
    let mut pointer: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut length: u32 = 0;

    // SAFETY: `buffer` is a valid version resource of the size reported by the
    // size call, and the out pointers are valid for the duration of the call.
    let ok = unsafe {
        VerQueryValueW(
            buffer.as_ptr().cast(),
            PCWSTR(sub.as_ptr()),
            &mut pointer,
            &mut length,
        )
    };
    if !ok.as_bool() || length < 4 {
        return None;
    }

    // The block is an array of {language, code page} pairs; the first is the one
    // the resource was written with.
    // SAFETY: length >= 4 means at least one full pair is present, so reading
    // two u16 values from the start of the block is in bounds.
    let language = unsafe { *(pointer as *const u16) };
    let code_page = unsafe { *((pointer as *const u16).add(1)) };
    Some((language, code_page))
}

fn query_string(buffer: &[u8], key: &str) -> Option<String> {
    let sub = wide(OsStr::new(key));
    let mut pointer: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut length: u32 = 0;

    // SAFETY: same contract as above; the returned pointer is inside `buffer`
    // and is only read while `buffer` is alive.
    let ok = unsafe {
        VerQueryValueW(
            buffer.as_ptr().cast(),
            PCWSTR(sub.as_ptr()),
            &mut pointer,
            &mut length,
        )
    };
    if !ok.as_bool() || pointer.is_null() || length == 0 {
        return None;
    }

    // SAFETY: the value block is `length` UTF-16 code units, NUL-terminated.
    let units = unsafe { std::slice::from_raw_parts(pointer as *const u16, length as usize) };
    let text = String::from_utf16_lossy(units);
    let trimmed = text.trim_matches(char::from(0)).trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// The outcome of asking Windows to validate a signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signature {
    /// A trusted chain validated the file.
    Trusted,
    /// No signature at all.
    Unsigned,
    /// A signature is present but did not validate.
    Rejected,
    /// The check itself could not run, for example a file that is locked.
    Unknown,
}

/// Asks Windows whether it trusts this file's Authenticode signature.
///
/// `WTD_CACHE_ONLY_URL_RETRIEVAL` keeps the call offline: it uses the local
/// certificate cache instead of fetching revocation lists, so a check during
/// startup never waits on the network. That is the honest trade: a revoked
/// certificate that is not yet cached will still read as trusted here, and the
/// caller says "Windows trusts this signature", not "this file is safe".
pub fn signature(path: &Path) -> Signature {
    let file = wide(path.as_os_str());

    let mut info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(file.as_ptr()),
        hFile: HANDLE(std::ptr::null_mut()),
        pgKnownSubject: std::ptr::null_mut(),
    };

    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut info },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_REVOCATION_CHECK_NONE,
        ..Default::default()
    };

    // The action id is passed by mutable pointer, so it lives in a local rather
    // than being borrowed from the constant.
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;

    // SAFETY: `data` points at a live WINTRUST_FILE_INFO for the whole call, as
    // WTD_CHOICE_FILE requires. The state is always closed below, including on
    // failure, so no handle is leaked.
    let status = unsafe {
        WinVerifyTrust(
            HWND(std::ptr::null_mut()),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };

    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: closing the verification state created by the call above.
    unsafe {
        WinVerifyTrust(
            HWND(std::ptr::null_mut()),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
    }

    // The codes Windows uses to say "there is no signature here at all",
    // separated from the codes that say "there is one and it is not good".
    // Anything else is reported as Unknown rather than guessed at, because a
    // launcher that calls an unchecked file "rejected" is as wrong as one that
    // calls it trusted.
    match status as u32 {
        0 => Signature::Trusted,
        0x800B_0100 // TRUST_E_NOSIGNATURE
        | 0x800B_0001 // TRUST_E_PROVIDER_UNKNOWN
        | 0x800B_0003 // TRUST_E_SUBJECT_FORM_UNKNOWN
        | 0x8009_200B // TRUST_E_NOSIGNATURE, as an NTSTATUS
        => Signature::Unsigned,
        0x800B_010C // TRUST_E_EXPLICIT_DISTRUST
        | 0x800B_0004 // TRUST_E_SUBJECT_NOT_TRUSTED
        | 0x800B_0111 // TRUST_E_BAD_DIGEST
        | 0x8009_6010 // TRUST_E_BAD_DIGEST, Win32 form
        | 0x800B_0109 // CERT_E_UNTRUSTEDROOT
        | 0x800B_0101 // CERT_E_EXPIRED
        | 0x800B_0107 // CERT_E_CHAINING
        => Signature::Rejected,
        _ => Signature::Unknown,
    }
}
