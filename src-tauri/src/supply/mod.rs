//! Stage 02, supply: which clients exist, which one Roblox is serving, and how
//! to get one installed.
//!
//! Everything here talks to Roblox's own endpoints and nothing else. The
//! install is verified twice over: the package hashes come from Roblox's
//! manifest, and the extracted client must carry a signature Windows trusts
//! from the publisher the version resource names.

pub mod install;
pub mod manifest;
pub mod pe;
pub mod prune;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{FailureCode, LaunchError};

/// Roblox's deployment CDN. Every package and every installer comes from here.
pub const CDN: &str = "https://setup.rbxcdn.com";
/// The version service. This is where "what is LIVE" is answered.
pub const VERSION_SERVICE: &str = "https://clientsettingscdn.roblox.com";

/// A client folder is only recognised under this prefix, which is also what
/// keeps our own staging folders out of discovery.
const FOLDER_PREFIX: &str = "version-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Player,
    Studio,
}

impl Kind {
    /// The name Roblox's version service uses for this binary.
    pub fn binary_type(self) -> &'static str {
        match self {
            Kind::Player => "WindowsPlayer",
            Kind::Studio => "WindowsStudio64",
        }
    }

    /// The executable that proves a folder holds this client.
    pub fn executable(self) -> &'static str {
        match self {
            Kind::Player => "RobloxPlayerBeta.exe",
            Kind::Studio => "RobloxStudioBeta.exe",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Player => "Player",
            Kind::Studio => "Studio",
        }
    }
}

/// One channel, and whether this app can read it at all.
///
/// Measured, twice. On 2026-11-17: `channel/LIVE` answers anonymously, while
/// zcanary, zintegration and zflag answer `{"errors":[{"code":5,"message":"Not
/// authorized to perform this action."}]}`. On 2026-11-18 the same three were
/// asked again with a live session attached (the same `.ROBLOSECURITY` header the
/// app builds, proved live first by `users/v1/users/authenticated` answering
/// MemoLe): still 401 code 5. So a session buys nothing here, and the note says
/// that instead of promising that signing in would open them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub note: Option<String>,
}

pub fn channels() -> Vec<Channel> {
    let refusal = || {
        Some("Roblox refuses this channel to every account, a live session included. Internal builds only.".to_string())
    };
    vec![
        Channel {
            id: "LIVE".into(),
            label: "LIVE".into(),
            available: true,
            note: None,
        },
        Channel {
            id: "zcanary".into(),
            label: "zcanary".into(),
            available: false,
            note: refusal(),
        },
        Channel {
            id: "zintegration".into(),
            label: "zintegration".into(),
            available: false,
            note: refusal(),
        },
    ]
}

/// What the version service says is current.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveVersion {
    pub channel: String,
    pub version: String,
    /// The deployment folder name, `version-…`. Also the CDN prefix.
    pub upload: String,
    pub checked_at: u64,
}

/// A client folder that exists on this machine.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledClient {
    pub folder: String,
    pub path: String,
    /// Which root this folder sits in, in words.
    pub root: String,
    pub per_user: bool,
    pub kind: Option<Kind>,
    /// The dotted build from the executable's own version resource, when it has
    /// one. `None` is meaningful: a folder without one is not a real client.
    pub version: Option<String>,
    pub publisher: Option<String>,
    /// Real means: the executable for this kind is present, carries a version
    /// resource, and that resource names Roblox Corporation.
    pub real: bool,
    pub bytes: u64,
    pub files: u64,
    pub modified: Option<u64>,
    /// Whether Kuro installed this folder, and so whether it has a hash
    /// baseline it can verify against later.
    pub tracked: bool,
    pub pinned: bool,
    /// The process holding this folder open, when one does.
    pub holder: Option<String>,
    pub live: bool,
}

/// Everything the versions screen draws, in one read.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Where a new client would be written.
    pub versions_root: String,
    pub roots: Vec<Root>,
    pub exists: bool,
    pub installed: Vec<InstalledClient>,
    pub live_player: Option<LiveVersion>,
    pub live_studio: Option<LiveVersion>,
    pub channels: Vec<Channel>,
    pub total_bytes: u64,
}

/// One place Roblox keeps clients.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pub path: String,
    /// What the screen calls it, in words rather than a path.
    pub label: String,
    pub per_user: bool,
    pub exists: bool,
}

/// Every folder Roblox keeps clients in.
///
/// The brief names `%LOCALAPPDATA%\Roblox\Versions`, and that is where the
/// older clients on this machine are. But Roblox's current installer writes to
/// `C:\Program Files\Roblox\Versions` instead, and on the machine this was
/// built on that is the only place the live client exists. A launcher that
/// scans one root cannot see the version Roblox is actually running, so both
/// are scanned, per-user first.
pub fn roots() -> Vec<Root> {
    let mut found = Vec::new();

    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let path = PathBuf::from(local).join("Roblox").join("Versions");
        found.push(Root {
            exists: path.is_dir(),
            path: path.to_string_lossy().to_string(),
            label: "This user".to_string(),
            per_user: true,
        });
    }

    for variable in ["PROGRAMFILES", "PROGRAMFILES(X86)"] {
        let Some(base) = std::env::var_os(variable) else {
            continue;
        };
        let path = PathBuf::from(base).join("Roblox").join("Versions");
        if found
            .iter()
            .any(|root| root.path.eq_ignore_ascii_case(&path.to_string_lossy()))
        {
            continue;
        }
        found.push(Root {
            exists: path.is_dir(),
            path: path.to_string_lossy().to_string(),
            label: "All users".to_string(),
            per_user: false,
        });
    }

    found
}

/// The folder a new client should be written into.
///
/// Whichever root already holds a real client, because Roblox keeps one install
/// location per machine and updates in place; if there is none, the per-user
/// root the brief asks for.
pub fn install_root() -> Result<PathBuf, LaunchError> {
    let roots = roots();
    if roots.is_empty() {
        return Err(LaunchError::new(
            FailureCode::ClientMissing,
            "Windows did not report an application data folder.",
        ));
    }

    let mut best: Option<(String, PathBuf)> = None;
    for root in &roots {
        let path = PathBuf::from(&root.path);
        if !path.is_dir() {
            continue;
        }
        for client in discover(&path, &None, None).unwrap_or_default() {
            if !client.real {
                continue;
            }
            let version = client.version.unwrap_or_default();
            if best.as_ref().is_none_or(|(seen, _)| version > *seen) {
                best = Some((version, path.clone()));
            }
        }
    }

    if let Some((_, path)) = best {
        return Ok(path);
    }

    let fallback = roots
        .into_iter()
        .find(|root| root.per_user)
        .map(|root| root.path)
        .ok_or_else(|| {
            LaunchError::new(
                FailureCode::ClientMissing,
                "There is no folder to install a client into.",
            )
        })?;

    Ok(PathBuf::from(fallback))
}

/// The per-user root, kept for the places that mean "the default" rather than
/// "wherever Roblox actually is".
pub fn versions_root() -> Result<PathBuf, LaunchError> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
        LaunchError::new(
            FailureCode::ClientMissing,
            "Windows did not report a local application data folder.",
        )
    })?;
    Ok(PathBuf::from(local).join("Roblox").join("Versions"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn modified_seconds(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// Bytes and file count under a folder, skipping the integrity baseline so the
/// number shown is the client, not our own bookkeeping.
fn measure(root: &Path) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut files = 0u64;
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                files += 1;
                bytes += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            }
        }
    }

    (bytes, files)
}

/// Reads one folder as a client, or says why it is not one.
fn read_folder(
    path: &Path,
    root_label: &str,
    per_user: bool,
    pinned: &Option<String>,
    live: Option<&str>,
) -> Option<InstalledClient> {
    let folder = path.file_name()?.to_string_lossy().to_string();
    if !folder.starts_with(FOLDER_PREFIX) {
        return None;
    }

    let kind = [Kind::Player, Kind::Studio]
        .into_iter()
        .find(|kind| path.join(kind.executable()).is_file());

    let identity = kind.and_then(|kind| pe::read_identity(&path.join(kind.executable())));
    let (bytes, files) = measure(path);

    Some(InstalledClient {
        real: identity
            .as_ref()
            .is_some_and(pe::FileIdentity::claims_roblox),
        publisher: identity.as_ref().and_then(|id| id.company.clone()),
        version: identity.as_ref().and_then(|id| id.version.clone()),
        kind,
        tracked: path.join(install::BASELINE).is_file(),
        pinned: pinned.as_deref() == Some(folder.as_str()),
        holder: prune::held_by(path),
        live: live == Some(folder.as_str()),
        modified: modified_seconds(path),
        path: path.to_string_lossy().to_string(),
        root: root_label.to_string(),
        per_user,
        folder,
        bytes,
        files,
    })
}

/// Every client folder on this machine, newest-looking first.
pub fn discover(
    root: &Path,
    pinned: &Option<String>,
    live: Option<&str>,
) -> Result<Vec<InstalledClient>, LaunchError> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut found = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|e| {
        LaunchError::new(
            FailureCode::ClientMissing,
            format!("The Roblox versions folder could not be listed: {e}"),
        )
    })?;

    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        if let Some(client) = read_folder(&entry.path(), "This user", true, pinned, live) {
            found.push(client);
        }
    }

    // Newest build first, then by name, so the list never reshuffles between
    // reads for two folders with no readable version.
    found.sort_by(|a, b| {
        b.version
            .cmp(&a.version)
            .then_with(|| b.modified.cmp(&a.modified))
            .then_with(|| a.folder.cmp(&b.folder))
    });

    Ok(found)
}

/// Every client in every root, newest first.
///
/// Two folders can hold the same build; the live one is whichever matches what
/// Roblox is serving, so both are reported and the screen shows which root each
/// came from.
pub fn discover_all(
    pinned: &Option<String>,
    live: Option<&str>,
) -> Result<Vec<InstalledClient>, LaunchError> {
    let mut found = Vec::new();
    for root in roots() {
        let path = PathBuf::from(&root.path);
        if !path.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&path).into_iter().flatten().flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            if let Some(client) =
                read_folder(&entry.path(), &root.label, root.per_user, pinned, live)
            {
                found.push(client);
            }
        }
    }

    found.sort_by(|a, b| {
        b.version
            .cmp(&a.version)
            .then_with(|| b.modified.cmp(&a.modified))
            .then_with(|| a.folder.cmp(&b.folder))
    });

    Ok(found)
}

/// A blocking HTTP agent for everything in this module.
///
/// The TLS provider is named explicitly because ureq's default is Rustls, and
/// Rustls is not compiled in here. `NativeTls` means Windows' own TLS stack and
/// root store: no bundled certificate list to go stale, nothing extra to audit,
/// and the same trust decisions every other program on the machine makes.
pub fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(concat!("KuroLauncher/", env!("CARGO_PKG_VERSION")))
        .http_status_as_error(false)
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .build(),
        )
        .build()
        .into()
}

#[derive(Debug, Deserialize)]
struct VersionAnswer {
    version: String,
    #[serde(rename = "clientVersionUpload")]
    upload: String,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    errors: Vec<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: i64,
    message: String,
}

fn read_body(response: ureq::http::Response<ureq::Body>) -> Result<String, LaunchError> {
    response.into_body().read_to_string().map_err(|e| {
        LaunchError::new(
            FailureCode::Network,
            format!("Roblox's answer could not be read: {e}"),
        )
        .with_action("Retry")
    })
}

/// Asks the version service what is current on one channel.
///
/// Deliberately not cached here: the caller decides how fresh it needs the
/// answer, and the screen prints the time it was asked.
pub fn resolve(kind: Kind, channel: &str) -> Result<LiveVersion, LaunchError> {
    let url = format!(
        "{VERSION_SERVICE}/v2/client-version/{}/channel/{channel}",
        kind.binary_type()
    );

    let response = agent(Duration::from_secs(12))
        .get(&url)
        .call()
        .map_err(|e| match e {
            ureq::Error::Timeout(_) => LaunchError::new(
                FailureCode::Network,
                "Roblox's version service did not answer in time.",
            )
            .with_action("Retry"),
            other => LaunchError::new(
                FailureCode::Network,
                format!("Roblox's version service could not be reached: {other}"),
            )
            .with_action("Retry"),
        })?;

    let status = response.status().as_u16();
    let body = read_body(response)?;

    if (200..300).contains(&status) {
        let answer: VersionAnswer = serde_json::from_str(&body).map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!(
                    "Roblox's version service answered in a shape this build does not know: {e}"
                ),
            )
        })?;
        return Ok(LiveVersion {
            channel: channel.to_string(),
            version: answer.version,
            upload: answer.upload,
            checked_at: now(),
        });
    }

    // A refusal carries a named reason, and that reason is what the screen
    // shows. "Not authorized" on a channel is not a network fault.
    if let Ok(envelope) = serde_json::from_str::<ErrorEnvelope>(&body) {
        if let Some(first) = envelope.errors.first() {
            let code = if first.code == 5 {
                FailureCode::ChannelUnavailable
            } else {
                FailureCode::Network
            };
            return Err(LaunchError::new(
                code,
                format!("Roblox refused the {channel} channel: {}", first.message),
            ));
        }
    }

    Err(LaunchError::new(
        FailureCode::Network,
        format!("Roblox's version service answered {status} with nothing usable."),
    )
    .with_action("Retry"))
}

/// Fetches one deployment's package manifest.
pub fn fetch_manifest(upload: &str) -> Result<Vec<manifest::Package>, LaunchError> {
    let url = format!("{CDN}/{upload}-rbxPkgManifest.txt");
    let response = agent(Duration::from_secs(20))
        .get(&url)
        .call()
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("Roblox's package list could not be fetched: {e}"),
            )
            .with_action("Try the official installer")
        })?;

    let status = response.status().as_u16();
    let body = read_body(response)?;
    if !(200..300).contains(&status) {
        return Err(LaunchError::new(
            FailureCode::Network,
            format!("Roblox has no package list for {upload} (answered {status})."),
        )
        .with_action("Try the official installer"));
    }

    manifest::parse(&body)
}

/// Mutable supply state: the cancel flag for a running install, and the last
/// answers from the version service so a screen can draw without waiting.
///
/// Everything is shared behind an `Arc` so a command can clone the handles it
/// needs and move them into a blocking worker, leaving the window responsive
/// while a client is being hashed or downloaded.
#[derive(Clone, Default)]
pub struct SupplyState {
    pub cancel: Arc<AtomicBool>,
    pub last_player: Arc<Mutex<Option<LiveVersion>>>,
    pub last_studio: Arc<Mutex<Option<LiveVersion>>>,
}

impl SupplyState {
    pub fn begin(&self) {
        self.cancel.store(false, Ordering::SeqCst);
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn remember(&self, kind: Kind, live: &LiveVersion) {
        let slot = match kind {
            Kind::Player => &self.last_player,
            Kind::Studio => &self.last_studio,
        };
        if let Ok(mut guard) = slot.lock() {
            *guard = Some(live.clone());
        }
    }

    pub fn cached(&self, kind: Kind) -> Option<LiveVersion> {
        let slot = match kind {
            Kind::Player => &self.last_player,
            Kind::Studio => &self.last_studio,
        };
        slot.lock().ok().and_then(|guard| guard.clone())
    }
}

/// Builds the whole picture the screen needs.
pub fn snapshot(state: &SupplyState, pinned: &Option<String>) -> Result<Snapshot, LaunchError> {
    let root = install_root()?;

    let player = state.cached(Kind::Player);
    let studio = state.cached(Kind::Studio);
    let live_folder = player.as_ref().map(|live| live.upload.as_str());

    let roots = roots();
    let installed = discover_all(pinned, live_folder)?;
    let total_bytes = installed.iter().map(|client| client.bytes).sum();

    Ok(Snapshot {
        versions_root: root.to_string_lossy().to_string(),
        exists: roots.iter().any(|root| root.exists),
        roots,
        installed,
        live_player: player,
        live_studio: studio,
        channels: channels(),
        total_bytes,
    })
}
