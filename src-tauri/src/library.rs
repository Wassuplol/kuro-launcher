//! What this machine played.
//!
//! Roblox has no anonymous "recently played" endpoint, so this is the launcher's own
//! record: what it launched, when, and as whom. That is a narrower claim than "what I
//! played" and it is the only one that can be true, which is the whole argument for
//! keeping it here instead of guessing at a shelf.
//!
//! The file is `history.json` in the data root, rewritten whole through a temporary
//! file and a rename, because a half-written log would be read back as a lost
//! history rather than as a fault. The same place played twice in a row is one entry
//! with a newer time, and the list is capped: a launcher is not an archive.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{FailureCode, LaunchError};

const SCHEMA: u32 = 1;
/// Enough to cover a session of jumping between games, not so many that the list
/// becomes a second Library.
pub const CAP: usize = 40;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|seconds| seconds.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Played {
    /// Unix seconds, written here rather than trusted from the page.
    #[serde(default)]
    pub at: u64,
    pub place: u64,
    #[serde(default)]
    pub universe: Option<u64>,
    /// The name as the page knew it. Stored so the row still reads after Roblox is
    /// unreachable, which is exactly when somebody opens a library.
    #[serde(default)]
    pub name: Option<String>,
    /// "normal", "private", "server": the mode this launch actually used.
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub account: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
    pub schema: u32,
    pub entries: Vec<Played>,
}

impl Default for Log {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            entries: Vec::new(),
        }
    }
}

fn path(root: &Path) -> PathBuf {
    root.join("history.json")
}

/// The log, or an empty one. A log that cannot be parsed is treated as empty on
/// purpose: it holds play history, not settings, and refusing to open the library
/// because of it would be a worse failure than losing it.
pub fn load(root: &Path) -> Log {
    fs::read_to_string(path(root))
        .ok()
        .and_then(|text| serde_json::from_str::<Log>(&text).ok())
        .filter(|log| log.schema == SCHEMA)
        .unwrap_or_default()
}

fn save(root: &Path, log: &Log) -> Result<(), LaunchError> {
    fs::create_dir_all(root).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The play history folder could not be created: {e}."),
        )
    })?;

    let target = path(root);
    let temporary = target.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(log).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The play history could not be serialised: {e}."),
        )
    })?;

    // Same folder, rename over: a reader sees the old file or the new one.
    fs::write(&temporary, text)
        .and_then(|_| fs::rename(&temporary, &target))
        .map_err(|e| {
            LaunchError::new(
                FailureCode::ConfigUnwritable,
                format!("The play history could not be written: {e}."),
            )
        })
}

/// Records one launch. The time is set here, so a page cannot backdate an entry.
pub fn record(root: &Path, mut entry: Played) -> Result<Log, LaunchError> {
    entry.at = now();

    let mut log = load(root);
    log.entries.retain(|seen| {
        !(seen.place == entry.place
            && seen.universe == entry.universe
            && seen.account == entry.account)
    });
    log.entries.insert(0, entry);
    log.entries.truncate(CAP);

    save(root, &log)?;
    Ok(log)
}

pub fn clear(root: &Path) -> Result<Log, LaunchError> {
    let log = Log::default();
    save(root, &log)?;
    Ok(log)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kuro-history-{name}-{}", now()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn entry(place: u64, name: &str) -> Played {
        Played {
            at: 0,
            place,
            universe: Some(place + 1),
            name: Some(name.to_string()),
            mode: "normal".to_string(),
            account: Some(7),
        }
    }

    #[test]
    fn the_same_place_twice_is_one_entry_and_the_newer_time_wins() {
        let root = scratch("dedupe");
        record(&root, entry(1, "first")).unwrap();
        let log = record(&root, entry(1, "second")).unwrap();

        assert_eq!(log.entries.len(), 1);
        assert_eq!(log.entries[0].name.as_deref(), Some("second"));
        assert!(
            log.entries[0].at > 0,
            "the clock is set here, not by the page"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_log_is_capped_and_newest_first() {
        let root = scratch("cap");
        for place in 0..(CAP as u64 + 5) {
            record(&root, entry(place, "played")).unwrap();
        }
        let log = load(&root);

        assert_eq!(log.entries.len(), CAP);
        assert_eq!(log.entries[0].place, CAP as u64 + 4);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_corrupt_log_reads_as_empty_rather_than_failing_the_page() {
        let root = scratch("corrupt");
        fs::create_dir_all(&root).unwrap();
        fs::write(path(&root), "{ not json").unwrap();

        assert!(load(&root).entries.is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
