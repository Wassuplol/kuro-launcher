//! Starting the client.
//!
//! Three facts decide everything here, all measured rather than assumed:
//!
//! - The client is spawned with the launch string as its only argument. A shell
//!   wrapper (`cmd /c start`) re-parses the `&` characters inside it and opens
//!   the wrong thing, so the argument is passed verbatim.
//! - The player guards itself with a **session local** named mutex,
//!   `ROBLOX_singletonMutex`. Holding it first makes every later launch spawn a
//!   real process; `Global\` is a different object and does nothing, which is why
//!   copied snippets silently fail.
//! - The app is a windowed binary, so no console appears. `CREATE_NO_WINDOW` is
//!   still set, because the bootstrapper inside the client would otherwise be
//!   free to open one on a machine with a console attached.

use std::path::Path;

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

use crate::error::{FailureCode, LaunchError};
use crate::supply::prune;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Holds the singleton mutex for as long as the app is running.
///
/// Dropping it hands the lock back, which is what stops a second client from
/// spawning, so this is kept in app state rather than created and forgotten.
pub struct Singleton {
    handle: HANDLE,
    /// False when another program already held the mutex. Enabling the switch
    /// then changes nothing until that program exits, and the screen says so
    /// instead of claiming a lock it did not take.
    pub created: bool,
}

// SAFETY: a kernel handle is not tied to the thread that created it, and
// `CloseHandle` may be called from any thread. The value is only ever moved
// between threads inside a `Mutex`.
unsafe impl Send for Singleton {}

impl Drop for Singleton {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful `CreateMutexW` and is closed
        // exactly once, here.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

/// Creates the session-local mutex and takes it.
pub fn hold_singleton() -> Result<Singleton, LaunchError> {
    // SAFETY: no security attributes, initial ownership requested, and the name
    // is a static wide string with a terminator.
    let handle =
        unsafe { CreateMutexW(None, true, w!("Local\\ROBLOX_singletonMutex")) }.map_err(|e| {
            LaunchError::new(
                FailureCode::Internal,
                format!("The multi-instance lock could not be created: {e}"),
            )
        })?;

    // `CreateMutexW` succeeds for an existing mutex too, and only the last error
    // separates "I made this" from "someone else already had it".
    // SAFETY: reading the thread's last error immediately after the call.
    let created = unsafe { GetLastError() } != ERROR_ALREADY_EXISTS;

    Ok(Singleton { handle, created })
}

/// Whether a game client is running right now.
///
/// The check exists because the second launch of the day is the one that
/// silently focuses the first window: reporting that honestly beats a Play
/// button that appears to do nothing.
pub fn player_running() -> bool {
    const NAME: &str = "RobloxPlayerBeta.exe";

    let Ok(snapshot) = (unsafe {
        windows::Win32::System::Diagnostics::ToolHelp::CreateToolhelp32Snapshot(
            windows::Win32::System::Diagnostics::ToolHelp::TH32CS_SNAPPROCESS,
            0,
        )
    }) else {
        return false;
    };

    let mut entry = windows::Win32::System::Diagnostics::ToolHelp::PROCESSENTRY32W {
        dwSize: std::mem::size_of::<windows::Win32::System::Diagnostics::ToolHelp::PROCESSENTRY32W>(
        ) as u32,
        ..Default::default()
    };

    // SAFETY: `entry` is sized and initialised as the API requires, and the
    // snapshot is closed below.
    let mut more = unsafe {
        windows::Win32::System::Diagnostics::ToolHelp::Process32FirstW(snapshot, &mut entry)
    }
    .is_ok();

    let mut found = false;
    while more {
        if let Some(image) = prune::process_image(entry.th32ProcessID) {
            if Path::new(&image)
                .file_name()
                .map(|name| name.to_string_lossy().eq_ignore_ascii_case(NAME))
                .unwrap_or(false)
            {
                found = true;
                break;
            }
        }
        // SAFETY: same snapshot, same entry, called in the documented loop.
        more = unsafe {
            windows::Win32::System::Diagnostics::ToolHelp::Process32NextW(snapshot, &mut entry)
        }
        .is_ok();
    }

    // SAFETY: the snapshot handle is no longer used.
    let _ = unsafe { CloseHandle(snapshot) };
    found
}

/// Starts one client, detached, with the launch string as its only argument.
///
/// The child is returned rather than dropped: a flag lease is released when it
/// exits, and that is the only moment the caller knows the session is over.
pub fn spawn(exe: &Path, uri: &str) -> Result<std::process::Child, LaunchError> {
    use std::os::windows::process::CommandExt;

    std::process::Command::new(exe)
        .arg(uri)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| {
            LaunchError::new(
                FailureCode::ClientMissing,
                format!(
                    "The client at {} could not be started: {e}",
                    exe.to_string_lossy()
                ),
            )
            .with_action("Open Roblox versions")
        })
}
