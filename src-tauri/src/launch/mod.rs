//! Stage 04, launch: turning "this game, as this account" into a running client.
//!
//! The order is fixed and each step has a named failure in `error.rs`:
//!
//! 1. parse the destination (`target`),
//! 2. pick the client that should run it, pinned first, then live, then newest,
//! 3. preflight: a saved session, an executable, and no second launch of the
//!    same request while one is being prepared,
//! 4. mint a ticket (`ticket`),
//! 5. build the launch string and spawn (`spawn`).
//!
//! Nothing here touches a running client, reads its memory or writes into a
//! version folder: the whole stage is a URI and a process.

pub mod spawn;
pub mod target;
pub mod ticket;

pub use target::{launch_uri, Mode, Target};

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::error::{FailureCode, LaunchError};
use crate::supply::{InstalledClient, Kind};

/// How long the same request is treated as already answered. Long enough to
/// swallow a double click, short enough that a deliberate second launch after
/// closing a game still works.
const REPEAT_WINDOW: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub mode: Mode,
    pub mode_label: String,
    pub place: Option<u64>,
    pub server: Option<String>,
    /// The folder name, which is what the versions screen calls this build.
    pub client: String,
    pub version: Option<String>,
    /// The executable, ready to spawn.
    pub path: String,
    pub launcher_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Launched {
    pub mode: Mode,
    pub mode_label: String,
    pub place: Option<u64>,
    pub server: Option<String>,
    pub account: String,
    pub client: String,
    pub version: Option<String>,
    /// True when an identical request was answered moments ago and no second
    /// client was spawned.
    pub reused: bool,
    pub detail: String,
    /// What happened to the flag file: written, or why it was left alone. A
    /// flag failure never stops a launch, so it is reported here instead.
    pub flags: Option<String>,
}

/// What the drawer shows for a pasted destination before anything is started.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub mode: Mode,
    pub mode_label: String,
    pub place: Option<u64>,
    pub server: Option<String>,
    pub client: Option<String>,
    pub version: Option<String>,
    pub summary: String,
}

/// Wall clock in milliseconds, which is what the launch string carries.
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// The client that should run this launch.
///
/// A pin wins, because that is what a pin is for. Otherwise the build Roblox is
/// serving, because that is the one that will not demand an update on start.
/// Otherwise the newest real client on the machine.
fn choose(installed: &[InstalledClient], pinned: &Option<String>) -> Option<InstalledClient> {
    let real = |client: &&InstalledClient| client.real && client.kind == Some(Kind::Player);

    let pinned_match = pinned
        .as_ref()
        .and_then(|name| installed.iter().filter(real).find(|c| &c.folder == name));

    pinned_match
        .or_else(|| installed.iter().filter(real).find(|c| c.live))
        .or_else(|| installed.iter().find(real))
        .cloned()
}

pub fn prepare(
    input: &str,
    installed: &[InstalledClient],
    pinned: &Option<String>,
) -> Result<Prepared, LaunchError> {
    let target = Target::parse(input)?;

    let client = choose(installed, pinned).ok_or_else(|| {
        LaunchError::new(
            FailureCode::ClientMissing,
            "No Roblox client is installed on this machine yet.",
        )
        .with_action("Open Roblox versions")
    })?;

    let exe = PathBuf::from(&client.path).join(Kind::Player.executable());
    if !exe.is_file() {
        return Err(LaunchError::new(
            FailureCode::ClientCorrupt,
            format!(
                "The client in {} has no player executable in it.",
                client.folder
            ),
        )
        .with_action("Check that version"));
    }

    Ok(Prepared {
        mode: target.mode(),
        mode_label: target.mode().label().to_string(),
        place: target.place(),
        server: target.server().map(str::to_string),
        client: client.folder,
        version: client.version,
        path: exe.to_string_lossy().to_string(),
        launcher_url: target.launcher_url(),
    })
}

/// Reads a destination without starting anything, so the drawer can say what a
/// pasted link will do before a game opens on top of the window.
pub fn reading(
    input: &str,
    installed: &[InstalledClient],
    pinned: &Option<String>,
) -> Result<Reading, LaunchError> {
    let prepared = prepare(input, installed, pinned)?;

    let summary = match prepared.mode {
        Mode::Normal => match prepared.place {
            Some(place) => format!("Opens place {place} in any server."),
            None => "Opens Roblox in any server.".to_string(),
        },
        Mode::Server => format!(
            "Opens one exact server of place {}.",
            prepared.place.unwrap_or_default()
        ),
        Mode::Private => format!(
            "Opens the private server for place {}.",
            prepared.place.unwrap_or_default()
        ),
        Mode::Follow => "Follows a friend into the place they are in.".to_string(),
    };

    Ok(Reading {
        mode: prepared.mode,
        mode_label: prepared.mode_label,
        place: prepared.place,
        server: prepared.server,
        client: Some(prepared.client),
        version: prepared.version,
        summary,
    })
}

// ---- the duplicate guard ----

struct Guard {
    in_flight: bool,
    last: Option<(String, Instant)>,
}

static GUARD: Mutex<Guard> = Mutex::new(Guard {
    in_flight: false,
    last: None,
});

/// Reserves the right to launch. `Ok(true)` means this exact request was just
/// answered and no client should be spawned for it.
pub fn begin(key: &str) -> Result<bool, LaunchError> {
    let mut guard = GUARD.lock().map_err(|_| {
        LaunchError::new(
            FailureCode::Internal,
            "The launch lock was poisoned by an earlier fault.",
        )
    })?;

    if guard.in_flight {
        return Err(LaunchError::new(
            FailureCode::AlreadyRunning,
            "A launch is already being prepared. Give it a second.",
        ));
    }

    if let Some((last, at)) = &guard.last {
        if last == key && at.elapsed() < REPEAT_WINDOW {
            return Ok(true);
        }
    }

    guard.in_flight = true;
    Ok(false)
}

/// Records the launch as done and opens the window for the next one.
pub fn settle(key: &str) {
    if let Ok(mut guard) = GUARD.lock() {
        guard.in_flight = false;
        guard.last = Some((key.to_string(), Instant::now()));
    }
}

/// Releases the reservation without recording a launch, for every failure path.
pub fn release() {
    if let Ok(mut guard) = GUARD.lock() {
        guard.in_flight = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(folder: &str, version: &str, live: bool, real: bool) -> InstalledClient {
        InstalledClient {
            folder: folder.to_string(),
            path: format!("C:/clients/{folder}"),
            root: "This user".to_string(),
            per_user: true,
            kind: Some(Kind::Player),
            version: Some(version.to_string()),
            publisher: Some("Roblox Corporation".to_string()),
            real,
            bytes: 0,
            files: 0,
            modified: None,
            tracked: false,
            pinned: false,
            holder: None,
            live,
        }
    }

    #[test]
    fn a_pin_beats_the_live_build() {
        let installed = vec![
            client("version-live", "0.700", true, true),
            client("version-pinned", "0.690", false, true),
        ];
        let chosen = choose(&installed, &Some("version-pinned".to_string())).unwrap();
        assert_eq!(chosen.folder, "version-pinned");
    }

    #[test]
    fn the_live_build_beats_a_newer_folder() {
        let installed = vec![
            client("version-newer", "0.710", false, true),
            client("version-live", "0.700", true, true),
        ];
        let chosen = choose(&installed, &None).unwrap();
        assert_eq!(chosen.folder, "version-live");
    }

    #[test]
    fn a_folder_that_is_not_a_real_client_is_never_chosen() {
        let installed = vec![client("version-fake", "0.700", true, false)];
        assert!(choose(&installed, &None).is_none());
    }

    #[test]
    fn a_pin_that_is_not_installed_falls_back_to_the_live_build() {
        let installed = vec![client("version-live", "0.700", true, true)];
        let chosen = choose(&installed, &Some("version-gone".to_string())).unwrap();
        assert_eq!(chosen.folder, "version-live");
    }

    #[test]
    fn the_duplicate_guard_keeps_one_launch_at_a_time() {
        // The guard is process-wide, so this test owns it for its duration.
        assert!(!begin("a|1818").unwrap());
        assert_eq!(
            begin("a|1818").unwrap_err().code,
            FailureCode::AlreadyRunning
        );
        settle("a|1818");
        // The same request inside the window is answered, not spawned again.
        assert!(begin("a|1818").unwrap());
        // A different request is a new launch.
        assert!(!begin("b|1818").unwrap());
        release();
    }
}
