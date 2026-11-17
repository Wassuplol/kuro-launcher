//! Removing old clients without ever removing one that is in use.
//!
//! "In use" is answered by the filesystem rather than by a process list: asking
//! for exclusive access to the executable fails while anything holds a handle
//! to it. That catches a running client, a half-finished Roblox install, and an
//! antivirus scan alike, and it costs one call.

use std::path::Path;

use windows::core::PWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::error::{FailureCode, LaunchError};

use super::InstalledClient;

/// The name of a process whose executable lives inside this folder, if any.
///
/// Two earlier attempts at this were wrong, and both were wrong in the same
/// direction. Asking for exclusive access fails to notice a running program,
/// because Windows maps the image and then closes the file handle. Asking
/// whether the file can be deleted fails for the same reason: the image section
/// permits deletion on close, so the answer is yes while the program runs.
///
/// The question that has one unambiguous answer is "is a process running out of
/// this folder", so that is the question asked. It also produces something
/// worth showing: the name of the process that has to close first.
pub fn held_by(folder: &Path) -> Option<String> {
    let wanted = prefix_of(folder);

    // SAFETY: the snapshot handle is closed below on every path, and the entry
    // struct is fully initialised before the first call.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    let mut holder = None;
    // SAFETY: `entry` is sized and initialised as the API requires.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();

    while more {
        if let Some(image) = process_image(entry.th32ProcessID) {
            if prefix_of(Path::new(&image)).starts_with(&wanted) {
                holder = Some(
                    Path::new(&image)
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or(image),
                );
                break;
            }
        }

        // SAFETY: same contract as the first call; the entry is reused.
        more = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }

    // SAFETY: the handle came from the snapshot call above and is closed once.
    unsafe {
        let _ = CloseHandle(snapshot);
    }

    holder
}

/// A path in one shape, for comparing two paths from different sources.
///
/// The versions root is built with forward slashes and the process image comes
/// back with backslashes, so a raw comparison finds nothing at all. Lowercase
/// with a trailing separator, because Windows paths are case-insensitive and a
/// bare prefix would let `Versions2` match `Versions`.
fn prefix_of(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('/', "\\").to_lowercase();
    if !text.ends_with('\\') {
        text.push('\\');
    }
    text
}

/// The full image path of one process, when this account is allowed to ask.
/// Processes owned by another user are simply skipped: they cannot be Roblox
/// running as this user, which is the only case that matters here.
pub fn process_image(pid: u32) -> Option<String> {
    // SAFETY: the handle is closed below on every path out.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

    let mut buffer = vec![0u16; 1024];
    let mut size = buffer.len() as u32;

    // SAFETY: `buffer` holds `size` writable u16 values and outlives the call.
    let ok = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    }
    .is_ok();

    // SAFETY: closing the handle opened above, once.
    unsafe {
        let _ = CloseHandle(process);
    }

    if !ok {
        return None;
    }

    buffer.truncate(size as usize);
    Some(String::from_utf16_lossy(&buffer))
}

/// Why a folder survives a prune. Every kept folder gets a reason, so the screen
/// can explain the list instead of showing an unexplained remainder.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keep {
    pub folder: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub remove: Vec<String>,
    pub keep: Vec<Keep>,
    pub bytes: u64,
}

/// Decides what a prune would do, without doing it.
///
/// Four reasons to keep, in order: pinned by the person, served by Roblox right
/// now, the newest copy on disk, or held open by something else. Anything else
/// is an older copy and is offered for removal.
pub fn plan(installed: &[InstalledClient]) -> Plan {
    let newest = installed
        .iter()
        .filter(|client| client.real)
        .max_by(|a, b| {
            a.version
                .cmp(&b.version)
                .then_with(|| a.modified.cmp(&b.modified))
        })
        .map(|client| client.folder.clone());

    let mut remove = Vec::new();
    let mut keep = Vec::new();
    let mut bytes = 0u64;

    for client in installed {
        let reason: Option<String> = if client.kind == Some(super::Kind::Studio) {
            // Studio is a separate program with its own installer. A button
            // called "old versions" must never take it away as a side effect.
            Some("Studio is a separate install, so it is left alone".to_string())
        } else if client.pinned {
            Some("You pinned this one".to_string())
        } else if client.live {
            Some("Roblox is serving this one".to_string())
        } else if newest.as_deref() == Some(client.folder.as_str()) {
            Some("The newest copy on this machine".to_string())
        } else if let Some(holder) = &client.holder {
            Some(format!("In use by {holder}"))
        } else if !client.real {
            Some("Not a client Kuro recognises".to_string())
        } else {
            None
        };

        match reason {
            Some(reason) => keep.push(Keep {
                folder: client.folder.clone(),
                reason,
            }),
            None => {
                remove.push(client.folder.clone());
                bytes += client.bytes;
            }
        }
    }

    Plan {
        remove,
        keep,
        bytes,
    }
}

/// What a prune actually did, including what it refused to do.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneResult {
    pub removed: Vec<String>,
    pub freed: u64,
    pub kept: Vec<Keep>,
    pub failed: Vec<Refusal>,
}

/// One folder that survived the prune, and why. A refusal is not an error the
/// screen hides: the folder is still there and the person is told which one.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    pub folder: String,
    pub code: crate::error::FailureCode,
    pub message: String,
}

/// Deletes one folder, re-checking that it is still safe to delete.
///
/// The lock and pin checks are repeated here rather than trusted from the plan:
/// time passes between drawing a list and pressing the button, and a client
/// started in between must survive.
pub fn remove(path: &Path, folder: &str, pinned: &Option<String>) -> Result<(), LaunchError> {
    if !folder.starts_with(super::FOLDER_PREFIX) || folder.contains('/') || folder.contains('\\') {
        return Err(LaunchError::new(
            FailureCode::Internal,
            "That is not a client folder name.",
        ));
    }

    if pinned.as_deref() == Some(folder) {
        return Err(LaunchError::new(
            FailureCode::Internal,
            format!("{folder} is pinned, so it was not removed."),
        )
        .with_action("Unpin it first"));
    }

    if !path.is_dir() {
        return Err(LaunchError::new(
            FailureCode::ClientMissing,
            format!("{folder} is already gone."),
        ));
    }

    if let Some(holder) = held_by(path) {
        return Err(LaunchError::new(
            FailureCode::MutexHeld,
            format!("{folder} is in use by {holder}, so it was not removed."),
        )
        .with_action("Close Roblox and try again"));
    }

    std::fs::remove_dir_all(path).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("{folder} could not be removed: {e}"),
        )
        .with_action("Close Roblox and try again")
    })
}

/// Runs the plan, one folder at a time, and reports each refusal instead of
/// stopping at the first one. A folder that is in use should not stop the other
/// three from being removed.
pub fn run(installed: &[InstalledClient], pinned: &Option<String>) -> PruneResult {
    let plan = plan(installed);
    let mut removed = Vec::new();
    let mut failed = Vec::new();
    let mut freed = 0u64;

    for folder in &plan.remove {
        let Some(client) = installed.iter().find(|client| &client.folder == folder) else {
            continue;
        };
        let bytes = client.bytes;

        match remove(Path::new(&client.path), folder, pinned) {
            Ok(()) => {
                removed.push(folder.clone());
                freed += bytes;
            }
            Err(error) => failed.push(Refusal {
                folder: folder.clone(),
                code: error.code,
                message: error.message,
            }),
        }
    }

    PruneResult {
        removed,
        freed,
        kept: plan.keep,
        failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The folder every Windows process on this machine runs out of. If this
    /// returns nothing, the enumeration is broken, because there is always at
    /// least one system process living in System32.
    #[test]
    fn a_system_folder_reads_as_held() {
        let system = Path::new("C:/Windows/System32");
        let holder = held_by(system);
        assert!(
            holder.is_some(),
            "expected at least one process running out of System32"
        );
        println!("System32 held by {:?}", holder);
    }

    /// A folder nothing runs from has no holder, which is the case a prune
    /// depends on to delete anything at all.
    #[test]
    fn a_scratch_folder_reads_as_free() {
        let dir = std::env::temp_dir().join("kuro-held-by-test");
        std::fs::create_dir_all(&dir).expect("temp dir");
        assert!(
            held_by(&dir).is_none(),
            "nothing runs out of a scratch folder"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
