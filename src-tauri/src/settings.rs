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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub schema: u32,
    pub identity: Identity,
    pub intro: Intro,
    pub advanced: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            identity: Identity::default(),
            intro: Intro::default(),
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
