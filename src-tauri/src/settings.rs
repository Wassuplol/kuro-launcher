use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{FailureCode, LaunchError};

pub const SCHEMA: u32 = 1;
const NAME_LIMIT: usize = 24;

/// Why the person is here. Each answer changes which defaults the app opens
/// with, so it is stored rather than inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    LaunchFaster,
    ManageAccounts,
    TunePerformance,
    BuildingSomething,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Identity {
    pub name: String,
    pub genres: Vec<String>,
    pub intent: Option<Intent>,
    pub onboarded: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Intro {
    pub instant_start: bool,
}

/// What the version manager remembers between runs. Only the pin: everything
/// else about the clients is read from disk each time, because Roblox changes
/// it without telling us.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Supply {
    /// A client folder name, `version-…`, that a prune must never remove.
    pub pinned: Option<String>,
}

/// What the launch pipeline remembers. Nothing here is inferred: both switches
/// change what happens on the machine, so both are asked for.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Launching {
    /// Hold `ROBLOX_singletonMutex` for the life of the app, so a second client
    /// can start beside the first. Off by default: it changes how the machine
    /// behaves for every Roblox program, not only this one.
    pub multi_instance: bool,
}

/// The assistant's connection. The key is stored **sealed** with DPAPI, the same
/// way a session cookie is, and is never returned to the page: the screen only ever
/// learns whether one is set.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Assistant {
    /// Empty means "use the environment, then the default endpoint".
    pub base_url: String,
    /// Empty means "use the default model", which is named in `assistant.rs`.
    pub model: String,
    /// Hex DPAPI blob. Never plaintext, and never read back by the page.
    pub key_sealed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub schema: u32,
    pub identity: Identity,
    pub intro: Intro,
    pub supply: Supply,
    pub launch: Launching,
    pub assistant: Assistant,
    pub advanced: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            identity: Identity::default(),
            intro: Intro::default(),
            supply: Supply::default(),
            launch: Launching::default(),
            assistant: Assistant::default(),
            advanced: false,
        }
    }
}

/// The single data root: accounts, profiles, cache and logs all live under it.
///
/// A file or folder named `portable` beside the exe moves the whole root next
/// to the exe, which is what a USB copy needs. Everything else uses the
/// per-user app data directory.
pub fn data_root() -> Result<PathBuf, LaunchError> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("portable").exists() || dir.join("portable.txt").exists() {
                return Ok(dir.join("data"));
            }
        }
    }

    let appdata = std::env::var_os("APPDATA").ok_or_else(|| {
        LaunchError::new(
            FailureCode::Internal,
            "Windows did not report an application data folder.",
        )
    })?;

    Ok(PathBuf::from(appdata).join("com.wassuplol.kuro"))
}

pub fn settings_path(root: &Path) -> PathBuf {
    root.join("settings.json")
}

pub fn load(root: &Path) -> Result<Settings, LaunchError> {
    let path = settings_path(root);
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            LaunchError::new(
                FailureCode::ConfigUnreadable,
                format!("Your settings file could not be read: {e}"),
            )
            .with_action("Reset your answers")
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(LaunchError::new(
            FailureCode::ConfigUnreadable,
            format!("Your settings file could not be opened: {e}"),
        )),
    }
}

/// Writes through a temporary file so a crash mid-write cannot leave a half
/// settings file behind.
pub fn save(root: &Path, next: &Settings) -> Result<(), LaunchError> {
    fs::create_dir_all(root).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The data folder could not be created: {e}"),
        )
    })?;

    let path = settings_path(root);
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(next).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("Settings could not be encoded: {e}"),
        )
    })?;

    fs::write(&tmp, body).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("Settings could not be written: {e}"),
        )
    })?;

    fs::rename(&tmp, &path).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("Settings could not be replaced: {e}"),
        )
    })?;

    Ok(())
}

/// Trims and caps the display name exactly as the onboarding field promises.
/// What the page is allowed to read: everything except the sealed assistant key.
///
/// The seal is ciphertext, but it is still nobody's business outside this process,
/// and keeping it out of the payload is what makes "write-only" a fact instead of a
/// description.
pub fn for_page(mut settings: Settings) -> Settings {
    settings.assistant.key_sealed.clear();
    settings
}

/// Puts the stored seal back after the page sends settings it never saw the seal in.
/// Without this, every save from the settings drawer would delete the key.
pub fn keep_seal(next: &mut Settings, stored: &str) {
    next.assistant.key_sealed = stored.to_string();
}

pub fn normalise(mut next: Settings) -> Settings {
    next.schema = SCHEMA;
    next.identity.name = next
        .identity
        .name
        .trim()
        .chars()
        .take(NAME_LIMIT)
        .collect::<String>();
    next.identity.genres.truncate(12);
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_never_receives_the_sealed_key_but_the_file_keeps_it() {
        let mut stored = Settings::default();
        stored.assistant.key_sealed = "abcd1234".to_string();

        let seen = for_page(stored.clone());
        assert!(
            seen.assistant.key_sealed.is_empty(),
            "the page must not read the seal"
        );

        // What the page sends back is the same object with an empty seal, and the
        // save path has to put the stored one back.
        let mut incoming = seen;
        keep_seal(&mut incoming, &stored.assistant.key_sealed);
        assert_eq!(incoming.assistant.key_sealed, "abcd1234");
    }
}
