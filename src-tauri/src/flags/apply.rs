//! Writing flags to the client, and putting the machine back.
//!
//! The model is a lease, not a permanent edit. A flag file is written before a
//! client starts, and the previous state is restored when that client exits, when
//! the launcher exits, and on the next launcher start if the last run ended in a
//! crash. That is the only shape that keeps "restore after crash" honest: a
//! permanent edit has nothing to restore.
//!
//! Every write goes through a temporary file and a rename in the same folder, so
//! a client starting mid-write sees either the old file or the new one, never a
//! half-written one. The bytes written are hashed and the hash is kept, so a file
//! somebody edited by hand is recognised instead of silently overwritten.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::store::Row;
use crate::error::{FailureCode, LaunchError};

/// Where the lease stands.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lease {
    pub at: u64,
    pub path: String,
    pub keys: usize,
    /// The file that was there before, or `None` when there was none. Reverting
    /// deletes the file in that case rather than leaving an empty object.
    pub backup: Option<String>,
    /// SHA-256 of the bytes this app wrote.
    pub hash: String,
}

fn journal(root: &Path) -> PathBuf {
    root.join("flags").join("state.json")
}

fn backup_dir(root: &Path) -> PathBuf {
    root.join("flags").join("backup")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn lease(root: &Path) -> Option<Lease> {
    let bytes = std::fs::read(journal(root)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_lease(root: &Path, lease: &Lease) -> Result<(), LaunchError> {
    let file = journal(root);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            LaunchError::new(
                FailureCode::ConfigUnwritable,
                format!("The flag folder could not be created: {e}"),
            )
        })?;
    }
    let body = serde_json::to_vec_pretty(lease).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The flag lease could not be encoded: {e}"),
        )
    })?;
    std::fs::write(&file, body).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The flag lease could not be written: {e}"),
        )
    })
}

fn clear_lease(root: &Path) {
    let _ = std::fs::remove_file(journal(root));
}

/// Replaces a file in one step on the same volume.
fn replace(path: &Path, bytes: &[u8]) -> Result<(), LaunchError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            LaunchError::new(
                FailureCode::ConfigUnwritable,
                format!("The client settings folder could not be created: {e}"),
            )
        })?;
    }
    let tmp = path.with_extension("json.kuro-tmp");
    std::fs::write(&tmp, bytes).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("Flags could not be written: {e}"),
        )
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        // Leaving the temporary behind would make the next attempt write into a
        // folder the client also scans.
        let _ = std::fs::remove_file(&tmp);
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("Flags could not be put in place: {e}"),
        )
    })
}

/// The value as the client wants to read it: a string, always.
///
/// Measured on 0.748.0.7481526: `"FFlagDebugGraphicsPreferVulkan": "True"` makes
/// the client load `vulkan-1.dll`, and `: true` in the same key leaves it on
/// D3D11. So the file this app writes holds `"True"` and `"240"`, not `true` and
/// `240`. The store keeps the typed value because the screen needs the kind; the
/// conversion happens exactly here, at the edge.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

/// Writes the resolved rows and records what was there before.
pub fn apply(root: &Path, target: &Path, rows: &[Row]) -> Result<Lease, LaunchError> {
    let existing = std::fs::read(target).ok();

    let backup = match &existing {
        Some(bytes) => {
            let folder = backup_dir(root);
            std::fs::create_dir_all(&folder).map_err(|e| {
                LaunchError::new(
                    FailureCode::ConfigUnwritable,
                    format!("The flag backup folder could not be created: {e}"),
                )
            })?;
            let file = folder.join(format!("ClientAppSettings.{}.json", now()));
            std::fs::write(&file, bytes).map_err(|e| {
                LaunchError::new(
                    FailureCode::ConfigUnwritable,
                    format!("The settings already on this machine could not be backed up: {e}"),
                )
            })?;
            Some(file.to_string_lossy().to_string())
        }
        None => None,
    };

    let mut map = Map::new();
    for row in rows {
        map.insert(row.key.clone(), Value::String(text(&row.value)));
    }
    let body = serde_json::to_vec_pretty(&Value::Object(map)).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("Those flags could not be encoded: {e}"),
        )
    })?;

    replace(target, &body)?;

    let lease = Lease {
        at: now(),
        path: target.to_string_lossy().to_string(),
        keys: rows.len(),
        backup,
        hash: hash(&body),
    };
    write_lease(root, &lease)?;
    Ok(lease)
}

/// Puts the previous state back.
///
/// Returns the path that was restored, or `None` when there was nothing to do.
/// A file that no longer matches what this app wrote is kept as a second backup
/// before the restore, because the person may have edited it after us and
/// dropping that edit to satisfy a rule would be the wrong trade.
pub fn revert(root: &Path) -> Result<Option<String>, LaunchError> {
    let Some(lease) = lease(root) else {
        return Ok(None);
    };

    let target = PathBuf::from(&lease.path);

    if let Ok(current) = std::fs::read(&target) {
        if hash(&current) != lease.hash {
            let folder = backup_dir(root);
            let _ = std::fs::create_dir_all(&folder);
            let _ = std::fs::write(
                folder.join(format!("ClientAppSettings.{}.replaced.json", now())),
                &current,
            );
        }
    }

    match &lease.backup {
        Some(path) => {
            let bytes = std::fs::read(path).map_err(|e| {
                LaunchError::new(
                    FailureCode::ConfigUnreadable,
                    format!("The backup of the previous settings could not be read: {e}"),
                )
            })?;
            replace(&target, &bytes)?;
        }
        None => {
            // There was no file before, so there must be none after.
            match std::fs::remove_file(&target) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(LaunchError::new(
                        FailureCode::ConfigUnwritable,
                        format!("The flag file could not be removed: {e}"),
                    ))
                }
            }
        }
    }

    clear_lease(root);
    Ok(Some(lease.path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::catalog::Kind;
    use crate::flags::store::Source;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kuro-flags-{name}-{}", now()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rows() -> Vec<Row> {
        vec![
            Row {
                key: "FFlagOne".to_string(),
                value: json!(true),
                source: Source::Account,
                profile: "mine".to_string(),
                kind: Kind::Flag,
            },
            Row {
                key: "DFIntTwo".to_string(),
                value: json!(60),
                source: Source::Global,
                profile: "global".to_string(),
                kind: Kind::Int,
            },
        ]
    }

    #[test]
    fn applies_then_puts_back_a_file_that_was_not_there() {
        let root = scratch("absent");
        let target = root.join("ClientSettings").join("ClientAppSettings.json");

        let written = apply(&root, &target, &rows()).unwrap();
        assert_eq!(written.keys, 2);
        assert!(written.backup.is_none());

        let written: Value = serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
        // Strings, not booleans and numbers: the client ignores the other
        // spelling on the same key. Measured, see `text`.
        assert_eq!(written["FFlagOne"], json!("True"));
        assert_eq!(written["DFIntTwo"], json!("60"));

        assert!(revert(&root).unwrap().is_some());
        assert!(!target.exists());
        assert!(lease(&root).is_none());
    }

    #[test]
    fn applies_then_restores_what_was_already_there() {
        let root = scratch("existing");
        let target = root.join("ClientSettings").join("ClientAppSettings.json");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, br#"{"FFlagTheirs": true}"#).unwrap();

        let written = apply(&root, &target, &rows()).unwrap();
        assert!(written.backup.is_some());
        assert!(!std::fs::read_to_string(&target)
            .unwrap()
            .contains("FFlagTheirs"));

        revert(&root).unwrap();
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            r#"{"FFlagTheirs": true}"#
        );
    }

    #[test]
    fn a_file_edited_after_the_write_is_kept_before_the_restore() {
        let root = scratch("edited");
        let target = root.join("ClientSettings").join("ClientAppSettings.json");
        apply(&root, &target, &rows()).unwrap();

        // Somebody else rewrites it while the lease is live.
        std::fs::write(&target, br#"{"FFlagHandEdited": true}"#).unwrap();
        revert(&root).unwrap();

        // The original state is back, and their version survives in a backup.
        assert!(!target.exists());
        let kept: Vec<_> = std::fs::read_dir(root.join("flags").join("backup"))
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".replaced.json"))
            .collect();
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn reverting_with_no_lease_does_nothing() {
        let root = scratch("empty");
        assert!(revert(&root).unwrap().is_none());
    }

    #[test]
    fn the_written_file_is_valid_json_with_the_values_the_rows_carried() {
        let root = scratch("json");
        let target = root.join("ClientSettings").join("ClientAppSettings.json");
        apply(&root, &target, &rows()).unwrap();

        let text = std::fs::read_to_string(&target).unwrap();
        let parsed: serde_json::Map<String, Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed.get("FFlagTwo"), None);
        assert_eq!(parsed.get("FFlagOne"), Some(&json!("True")));
    }

    #[test]
    fn every_value_reaches_the_file_as_a_string() {
        assert_eq!(text(&json!(true)), "True");
        assert_eq!(text(&json!(false)), "False");
        assert_eq!(text(&json!(240)), "240");
        assert_eq!(text(&json!("Trace;1234")), "Trace;1234");
    }
}
