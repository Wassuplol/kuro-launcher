//! Stage 05, configure: flags with a visible scope chain and a way back.
//!
//! Three pieces, each testable on its own:
//!
//! - `catalog` is Roblox's own served flag list and the default it hands out.
//! - `store` is the person's profiles and the scope chain that resolves them.
//! - `apply` writes the resolved set to the client's settings file as a lease,
//!   with a snapshot and a restore path for every way a session can end.
//!
//! Nothing here touches a running client, and applying is refused while one is
//! running, because a flag file is read at boot: writing it under a live client
//! produces a session whose flags do not match the file on disk.

pub mod apply;
pub mod catalog;
pub mod store;

pub use apply::Lease;
pub use store::{Row, Source, Store};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{FailureCode, LaunchError};
use crate::supply::{self, InstalledClient, Kind};

/// Where a flag file can live. Both are read by the client; the per-user one
/// survives an update, and the per-version one is replaced by Roblox when the
/// client updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// `%LOCALAPPDATA%\Roblox\ClientSettings\ClientAppSettings.json`
    User,
    /// `<Versions>\version-<hash>\ClientSettings\ClientAppSettings.json`
    Version,
}

impl Placement {
    pub fn label(self) -> &'static str {
        match self {
            Placement::User => "This Windows user",
            Placement::Version => "This client build",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Placement::User => "Survives a client update. One file for every build.",
            Placement::Version => "Replaced when Roblox updates the client.",
        }
    }
}

/// The file a flag write would touch, what is in it now, and which build it
/// belongs to when the placement is per-version.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub placement: Placement,
    pub label: String,
    pub note: String,
    pub path: String,
    pub exists: bool,
    /// What the person (or another tool) already put in that file.
    pub current: BTreeMap<String, Value>,
    /// The build this file sits inside, for the per-version placement.
    pub client: Option<String>,
}

/// `%LOCALAPPDATA%\Roblox\Versions` in, `%LOCALAPPDATA%\Roblox\ClientSettings\ClientAppSettings.json` out.
fn user_client_settings(versions: &Path) -> PathBuf {
    let base = versions.parent().unwrap_or(versions);
    base.join("ClientSettings").join("ClientAppSettings.json")
}

fn read_json(path: &Path) -> BTreeMap<String, Value> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, Value>>(&bytes).ok())
        .unwrap_or_default()
}

/// Resolves a placement to a real file. Reading it here is deliberate: the
/// screen shows what is actually on disk, not what this app remembers writing.
pub fn target(
    placement: Placement,
    installed: &[InstalledClient],
    pinned: &Option<String>,
) -> Result<Target, LaunchError> {
    let path = match placement {
        Placement::User => {
            let root = supply::roots()
                .into_iter()
                .find(|root| root.per_user)
                .ok_or_else(|| {
                    LaunchError::new(
                        FailureCode::NotFound,
                        "This machine has no per-user Roblox folder.",
                    )
                })?;
            // A supply root points at `Roblox\Versions`, and the settings
            // folder is its parent, not a child. Joining straight onto the root
            // produced `Roblox\Versions\ClientSettings`, a folder the client
            // never reads.
            user_client_settings(std::path::Path::new(&root.path))
        }
        Placement::Version => {
            let client = pick_client(installed, pinned).ok_or_else(|| {
                LaunchError::new(
                    FailureCode::ClientMissing,
                    "No Roblox client is installed, so there is no build to write into.",
                )
                .with_action("Open Roblox versions")
            })?;
            PathBuf::from(&client.path)
                .join("ClientSettings")
                .join("ClientAppSettings.json")
        }
    };

    let existing = std::fs::read(&path).ok();

    Ok(Target {
        placement,
        label: placement.label().to_string(),
        note: placement.note().to_string(),
        exists: existing.is_some(),
        current: existing
            .map(|bytes| {
                serde_json::from_slice::<BTreeMap<String, Value>>(&bytes).unwrap_or_default()
            })
            .unwrap_or_default(),
        path: path.to_string_lossy().to_string(),
        client: match placement {
            Placement::Version => pick_client(installed, pinned).map(|client| client.folder),
            Placement::User => None,
        },
    })
}

fn pick_client(installed: &[InstalledClient], pinned: &Option<String>) -> Option<InstalledClient> {
    let real = |client: &&InstalledClient| client.real && client.kind == Some(Kind::Player);

    let pinned_match = pinned
        .as_ref()
        .and_then(|name| installed.iter().filter(real).find(|c| &c.folder == name));

    pinned_match
        .or_else(|| installed.iter().filter(real).find(|c| c.live))
        .or_else(|| installed.iter().find(real))
        .cloned()
}

// ---- importing and exporting a profile ----

fn exports_dir(root: &Path) -> PathBuf {
    root.join("flags").join("exports")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFile {
    pub name: String,
    pub path: String,
    pub keys: usize,
}

/// Writes a profile as a file a person can keep, send or diff.
pub fn export(root: &Path, profile: &store::Profile) -> Result<PathBuf, LaunchError> {
    let folder = exports_dir(root);
    std::fs::create_dir_all(&folder).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The exports folder could not be created: {e}"),
        )
    })?;

    let safe: String = profile
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let file = folder.join(format!("{safe}.json"));
    let body = serde_json::json!({
        "profile": profile.name,
        "notes": profile.notes,
        "values": profile.values,
    });
    let text = serde_json::to_vec_pretty(&body).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("That profile could not be encoded: {e}"),
        )
    })?;

    std::fs::write(&file, text).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("That profile could not be written: {e}"),
        )
    })?;

    Ok(file)
}

pub fn exports(root: &Path) -> Vec<ExportFile> {
    let folder = exports_dir(root);
    let mut found: Vec<ExportFile> = std::fs::read_dir(&folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext.eq_ignore_ascii_case("json"))
                .unwrap_or(false)
        })
        .map(|entry| {
            let path = entry.path();
            let keys = read_json(&path).len();
            ExportFile {
                name: path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .unwrap_or_default(),
                path: path.to_string_lossy().to_string(),
                keys,
            }
        })
        .collect();

    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub name: String,
    pub values: BTreeMap<String, Value>,
    /// Keys whose value contradicts the shape the name promises. They are
    /// reported and left out rather than written and silently ignored by the
    /// client.
    pub refused: Vec<String>,
}

/// Reads a profile file. Accepts the shape this app exports and a bare
/// `{key: value}` map, which is what a file downloaded from anywhere else is.
pub fn import(path: &Path) -> Result<Imported, LaunchError> {
    let bytes = std::fs::read(path).map_err(|e| {
        LaunchError::new(
            FailureCode::NotFound,
            format!("That file could not be read: {e}"),
        )
    })?;

    let parsed: Value = serde_json::from_slice(&bytes).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnreadable,
            format!("That file is not JSON: {e}"),
        )
    })?;

    let (name, raw) = match parsed {
        Value::Object(mut map) => match map.remove("values") {
            Some(Value::Object(values)) => {
                let name = map
                    .remove("profile")
                    .and_then(|value| value.as_str().map(str::to_string))
                    .or_else(|| {
                        path.file_stem()
                            .map(|stem| stem.to_string_lossy().to_string())
                    })
                    .unwrap_or_else(|| "imported".to_string());
                (name, values)
            }
            // No wrapper: the whole object is the flag map.
            _ => {
                let name = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .unwrap_or_else(|| "imported".to_string());
                let mut values = serde_json::Map::new();
                for (key, value) in map {
                    values.insert(key, value);
                }
                (name, values)
            }
        },
        _ => {
            return Err(LaunchError::new(
                FailureCode::ConfigUnreadable,
                "That file holds no flags: it is not an object.",
            ))
        }
    };

    let mut values = BTreeMap::new();
    let mut refused = Vec::new();
    for (key, value) in raw {
        match store::check_value(&key, &value) {
            Ok(()) => {
                values.insert(key, value);
            }
            Err(_) => refused.push(key),
        }
    }

    Ok(Imported {
        name,
        values,
        refused,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_per_user_target_sits_beside_the_versions_folder_not_inside_it() {
        let got = user_client_settings(Path::new(r"C:\Users\dodia\AppData\Local\Roblox\Versions"));
        assert_eq!(
            got,
            PathBuf::from(
                r"C:\Users\dodia\AppData\Local\Roblox\ClientSettings\ClientAppSettings.json"
            )
        );
    }

    #[test]
    fn imports_a_bare_flag_map_and_a_wrapped_profile() {
        let dir = std::env::temp_dir().join(format!("kuro-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let bare = dir.join("forum-fastflags.json");
        std::fs::write(
            &bare,
            br#"{"DFIntTaskSchedulerTargetFps": 240, "FFlagThing": true}"#,
        )
        .unwrap();
        let imported = import(&bare).unwrap();
        assert_eq!(imported.name, "forum-fastflags");
        assert_eq!(imported.values.len(), 2);
        assert!(imported.refused.is_empty());

        let wrapped = dir.join("mine.json");
        std::fs::write(
            &wrapped,
            br#"{"profile":"Mine","values":{"FFlagThing":false,"FFlagBad":"not a bool"}}"#,
        )
        .unwrap();
        let imported = import(&wrapped).unwrap();
        assert_eq!(imported.name, "Mine");
        assert_eq!(imported.values.len(), 1);
        assert_eq!(imported.refused, vec!["FFlagBad".to_string()]);
    }

    #[test]
    fn refuses_a_file_that_is_not_a_flag_map() {
        let dir = std::env::temp_dir().join(format!("kuro-import-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.json");
        std::fs::write(&file, b"[1,2,3]").unwrap();
        assert!(import(&file).is_err());
    }
}
