//! The flag catalog.
//!
//! Roblox serves its own flag set, per application and per channel, from the
//! settings service. That is the only honest source for "official keys with
//! their defaults": nothing here is invented, and a key that the service does
//! not serve is marked as having no known default rather than given a guess.
//!
//! Measured: `application/PCDesktopClient` answers 200 with 25,584 keys, every
//! value a string (`"True"`, `"1000"`, `"Trace;7746948539"`). The desktop
//! client itself pulls the same catalogue at boot from
//! `clientsettingscdn.roblox.com/v2/settings-compressed/application/PCDesktopClient/bucket/<bucket>/<hash>.dcz`,
//! which the launcher does not use because the uncompressed endpoint is the
//! same data and needs no bucket discovery.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{FailureCode, LaunchError};
use crate::http;

const SERVICE: &str = "https://clientsettings.roblox.com/v2/settings/application/PCDesktopClient";
const APPLICATION: &str = "PCDesktopClient";

/// What a key's value is, which decides the control the screen draws. Roblox
/// encodes it in the name, so it is read rather than guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Flag,
    Int,
    String,
    Log,
    Number,
    Unknown,
}

impl Kind {
    pub fn of(key: &str) -> Self {
        let rest = key
            .strip_prefix("DF")
            .or_else(|| key.strip_prefix("SF"))
            .or_else(|| key.strip_prefix("F"))
            .or_else(|| key.strip_prefix("D"))
            .or_else(|| key.strip_prefix("S"))
            .or_else(|| key.strip_prefix("I"));

        match rest {
            Some(name) if name.starts_with("Flag") => Kind::Flag,
            Some(name) if name.starts_with("Int") => Kind::Int,
            Some(name) if name.starts_with("String") => Kind::String,
            Some(name) if name.starts_with("Log") => Kind::Log,
            Some(name) if name.starts_with("Float") || name.starts_with("Double") => Kind::Number,
            _ => Kind::Unknown,
        }
    }

    /// A key is only offered as editable when its name says what it holds.
    pub fn known(self) -> bool {
        self != Kind::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    /// Seconds since the epoch when the service answered.
    pub fetched: u64,
    pub application: String,
    /// Every served key with the value the service is handing out, which is the
    /// client's own default until someone overrides it.
    pub values: BTreeMap<String, String>,
}

fn path(root: &Path) -> PathBuf {
    root.join("flags").join("catalog.json")
}

pub fn load(root: &Path) -> Option<Catalog> {
    let bytes = std::fs::read(path(root)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn store(root: &Path, catalog: &Catalog) -> Result<(), LaunchError> {
    let file = path(root);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            LaunchError::new(
                FailureCode::ConfigUnwritable,
                format!("The flag folder could not be created: {e}"),
            )
        })?;
    }
    let tmp = file.with_extension("json.tmp");
    let body = serde_json::to_vec(catalog).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The flag catalog could not be encoded: {e}"),
        )
    })?;
    std::fs::write(&tmp, body).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The flag catalog could not be written: {e}"),
        )
    })?;
    std::fs::rename(&tmp, &file).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The flag catalog could not be replaced: {e}"),
        )
    })
}

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "applicationSettings")]
    settings: BTreeMap<String, String>,
}

/// Asks Roblox for the current catalogue and keeps it.
pub fn refresh(root: &Path) -> Result<Catalog, LaunchError> {
    let answer = http::get(SERVICE)?;

    if !answer.ok() {
        return Err(LaunchError::new(
            FailureCode::Network,
            format!("Roblox would not list its flags: {}", answer.reason()),
        )
        .with_action("Retry"));
    }

    let parsed: Answer = serde_json::from_str(&answer.body).map_err(|e| {
        LaunchError::new(
            FailureCode::Network,
            format!("Roblox answered in a shape this build does not know: {e}"),
        )
    })?;

    let catalog = Catalog {
        fetched: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        application: APPLICATION.to_string(),
        values: parsed.settings,
    };

    store(root, &catalog)?;
    Ok(catalog)
}

/// The catalogue, fetched on first use and reused afterwards.
pub fn ensure(root: &Path) -> Result<Catalog, LaunchError> {
    if let Some(catalog) = load(root) {
        return Ok(catalog);
    }
    refresh(root)
}

// ---------------------------------------------------------------------------
// Snapshots, and the diff between two of them
// ---------------------------------------------------------------------------

/// How many catalogues are kept. One is about 1.5 MB, and a Roblox version churns
/// weekly, so six is a couple of months of answers and not a disk leak.
const KEEP: usize = 6;

/// How many rows one side of a diff returns. A release can touch thousands of keys,
/// and a list nobody can read is not a diff anyone uses.
const CAP: usize = 200;

/// One stored catalogue, named by the client version it was served for.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: String,
    /// Seconds since the epoch when this catalogue was fetched.
    pub at: u64,
    pub keys: usize,
}

fn snapshots_dir(root: &Path) -> PathBuf {
    root.join("flags").join("catalogs")
}

/// A version as a file name: dots and digits survive, anything else becomes a dash.
fn snapshot_name(version: &str) -> String {
    let cleaned: String = version
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.is_empty() {
        "unversioned".to_string()
    } else {
        cleaned
    }
}

/// Keeps the catalogue under the version it was served for, and drops the oldest one
/// once there are more than `KEEP`.
///
/// The version is the live build at the moment of the fetch, which is the honest label:
/// Roblox serves one bucket, and that bucket is what the current client boots with.
pub fn keep_snapshot(root: &Path, version: &str, catalog: &Catalog) -> Result<(), LaunchError> {
    let dir = snapshots_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The catalogue folder could not be created: {e}"),
        )
    })?;

    let file = dir.join(format!("{}.json", snapshot_name(version)));
    let tmp = file.with_extension("json.tmp");
    let body = serde_json::to_vec(catalog).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The catalogue could not be encoded: {e}"),
        )
    })?;
    std::fs::write(&tmp, body).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The catalogue could not be written: {e}"),
        )
    })?;
    std::fs::rename(&tmp, &file).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The catalogue could not be replaced: {e}"),
        )
    })?;

    prune(&dir);
    Ok(())
}

/// Oldest out first, by modification time.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect();

    if files.len() <= KEEP {
        return;
    }

    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, path) in files.into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(path);
    }
}

/// Every stored catalogue, newest first.
pub fn snapshots(root: &Path) -> Vec<Snapshot> {
    let Ok(entries) = std::fs::read_dir(snapshots_dir(root)) else {
        return Vec::new();
    };

    let mut list: Vec<Snapshot> = entries
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .filter_map(|entry| {
            let version = entry.path().file_stem()?.to_str()?.to_string();
            let catalog = read_catalog(&entry.path())?;
            Some(Snapshot {
                version,
                at: catalog.fetched,
                keys: catalog.values.len(),
            })
        })
        .collect();

    list.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.at));
    list
}

fn read_catalog(file: &Path) -> Option<Catalog> {
    let bytes = std::fs::read(file).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// One key that differs between two catalogues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub key: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub from: Snapshot,
    pub to: Snapshot,
    pub added: Vec<Change>,
    pub removed: Vec<Change>,
    pub changed: Vec<Change>,
    pub added_count: usize,
    pub removed_count: usize,
    pub changed_count: usize,
    /// True when a list was cut to `CAP`.
    pub truncated: bool,
}

/// Walks two served catalogues and says what moved.
///
/// A key that appeared is `added`, one that stopped being served is `removed`, and one
/// whose served value differs is `changed`. Nothing here interprets why: Roblox decides
/// those, and a launcher that guesses at intent is a launcher that lies politely.
pub fn compare(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> (Vec<Change>, Vec<Change>, Vec<Change>) {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for (key, value) in after {
        match before.get(key) {
            None => added.push(Change {
                key: key.clone(),
                before: None,
                after: Some(value.clone()),
            }),
            Some(old) if old != value => changed.push(Change {
                key: key.clone(),
                before: Some(old.clone()),
                after: Some(value.clone()),
            }),
            Some(_) => {}
        }
    }

    for (key, value) in before {
        if !after.contains_key(key) {
            removed.push(Change {
                key: key.clone(),
                before: Some(value.clone()),
                after: None,
            });
        }
    }

    (added, removed, changed)
}

pub fn diff(root: &Path, from: &str, to: &str) -> Result<Diff, LaunchError> {
    let list = snapshots(root);
    let find = |version: &str| {
        list.iter()
            .find(|snapshot| snapshot.version == version)
            .cloned()
            .ok_or_else(|| {
                LaunchError::new(
                    FailureCode::NotFound,
                    format!("There is no catalogue stored for {version}."),
                )
                .with_action("Read the flag list again")
            })
    };

    let left = find(from)?;
    let right = find(to)?;

    let dir = snapshots_dir(root);
    let before =
        read_catalog(&dir.join(format!("{}.json", snapshot_name(from)))).ok_or_else(|| {
            LaunchError::new(
                FailureCode::ConfigUnreadable,
                format!("The catalogue for {from} could not be read."),
            )
        })?;
    let after =
        read_catalog(&dir.join(format!("{}.json", snapshot_name(to)))).ok_or_else(|| {
            LaunchError::new(
                FailureCode::ConfigUnreadable,
                format!("The catalogue for {to} could not be read."),
            )
        })?;

    let (added, removed, changed) = compare(&before.values, &after.values);
    let truncated = added.len() > CAP || removed.len() > CAP || changed.len() > CAP;

    Ok(Diff {
        from: left,
        to: right,
        added_count: added.len(),
        removed_count: removed.len(),
        changed_count: changed.len(),
        added: added.into_iter().take(CAP).collect(),
        removed: removed.into_iter().take(CAP).collect(),
        changed: changed.into_iter().take(CAP).collect(),
        truncated,
    })
}

/// One search result. `default` is absent when Roblox does not serve the key,
/// which is a real state: a locally set flag can be honoured and still have no
/// published default.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub key: String,
    pub default: Option<String>,
    pub kind: Kind,
    /// Whether the client will take this key from a local file. False does not
    /// mean the key is wrong, only that a local override of it is discarded —
    /// the screen has to say which, because silently offering both is the lie
    /// the allowlist created.
    pub honoured: bool,
}

pub fn search(catalog: &Catalog, query: &str, limit: usize) -> Vec<Hit> {
    let needle = query.trim().to_ascii_lowercase();

    let mut hits: Vec<Hit> = catalog
        .values
        .iter()
        .filter(|(key, _)| needle.is_empty() || key.to_ascii_lowercase().contains(&needle))
        .take(limit)
        .map(|(key, value)| Hit {
            key: key.clone(),
            default: Some(value.clone()),
            kind: Kind::of(key),
            honoured: super::allow::honours(key),
        })
        .collect();

    // A query that names a key Roblox does not serve is still offered, with no
    // default, because that is exactly the case a person is asking about when
    // they type a name they read somewhere. `DFIntTaskSchedulerTargetFps` is the
    // proof that this branch matters: it is absent from the served list and
    // people arrive looking for it, and the row is also the proof that "offered"
    // must never mean "works" — the client drops it, because it is not on the
    // allowlist.
    let typed = query.trim();
    let typed_is_key = Kind::of(typed).known() && typed.len() > 6;
    let already = hits.iter().any(|hit| hit.key == typed);

    if typed_is_key && !already {
        hits.push(Hit {
            key: typed.to_string(),
            default: None,
            kind: Kind::of(typed),
            honoured: super::allow::honours(typed),
        });
    }

    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diff_names_what_appeared_what_left_and_what_moved() {
        let before = BTreeMap::from([
            ("FFlagStays".to_string(), "True".to_string()),
            ("FFlagLeaves".to_string(), "True".to_string()),
            ("FFlagMoves".to_string(), "True".to_string()),
        ]);
        let after = BTreeMap::from([
            ("FFlagStays".to_string(), "True".to_string()),
            ("FFlagMoves".to_string(), "False".to_string()),
            ("FFlagArrives".to_string(), "1000".to_string()),
        ]);

        let (added, removed, changed) = compare(&before, &after);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].key, "FFlagArrives");
        assert_eq!(added[0].before, None);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].key, "FFlagLeaves");
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].key, "FFlagMoves");
        assert_eq!(changed[0].before.as_deref(), Some("True"));
        assert_eq!(changed[0].after.as_deref(), Some("False"));
    }

    #[test]
    fn two_snapshots_can_be_kept_and_diffed_from_disk() {
        let root = std::env::temp_dir().join(format!("kuro-catalogs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let older = Catalog {
            fetched: 1_700_000_000,
            application: "PCDesktopClient".to_string(),
            values: BTreeMap::from([("FFlagA".to_string(), "True".to_string())]),
        };
        let newer = Catalog {
            fetched: 1_700_086_400,
            application: "PCDesktopClient".to_string(),
            values: BTreeMap::from([
                ("FFlagA".to_string(), "False".to_string()),
                ("FFlagB".to_string(), "1".to_string()),
            ]),
        };

        keep_snapshot(&root, "0.748.0.7481526", &older).unwrap();
        keep_snapshot(&root, "0.750.0.7500001", &newer).unwrap();

        let list = snapshots(&root);
        assert_eq!(list.len(), 2);
        // Newest first, which is the order the screen offers.
        assert_eq!(list[0].version, "0.750.0.7500001");
        assert_eq!(list[0].keys, 2);

        let result = diff(&root, "0.748.0.7481526", "0.750.0.7500001").unwrap();
        assert_eq!(result.added_count, 1);
        assert_eq!(result.changed_count, 1);
        assert_eq!(result.removed_count, 0);
        assert_eq!(result.changed[0].key, "FFlagA");
        assert!(!result.truncated);

        // A version that was never stored is a named failure, not an empty diff.
        assert_eq!(
            diff(&root, "0.748.0.7481526", "0.999").unwrap_err().code,
            FailureCode::NotFound
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_the_newest_catalogues_are_kept() {
        let root = std::env::temp_dir().join(format!("kuro-catalogs-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        for index in 0..(KEEP + 3) {
            let catalog = Catalog {
                fetched: index as u64,
                application: "PCDesktopClient".to_string(),
                values: BTreeMap::new(),
            };
            keep_snapshot(&root, &format!("0.7{index}.0"), &catalog).unwrap();
            // Modification time is what the prune sorts by, and a loop this fast can
            // land two files in the same tick.
            std::thread::sleep(std::time::Duration::from_millis(15));
        }

        assert_eq!(snapshots(&root).len(), KEEP);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_the_type_out_of_the_name() {
        assert_eq!(Kind::of("FFlagDebugDisplayFPS"), Kind::Flag);
        assert_eq!(Kind::of("DFFlagSoundServiceMockOutputDevices"), Kind::Flag);
        assert_eq!(Kind::of("FIntFontSizePadding"), Kind::Int);
        assert_eq!(Kind::of("DFIntTaskSchedulerTargetFps"), Kind::Int);
        assert_eq!(Kind::of("FStringVoiceUserAgencyIXPLayerName"), Kind::String);
        assert_eq!(Kind::of("FLogNetworkTiming"), Kind::Log);
        assert_eq!(Kind::of("FFloatSomeThreshold"), Kind::Number);
        // Anything else is not offered as editable.
        assert_eq!(Kind::of("LayeredClothingMode"), Kind::Unknown);
        assert!(!Kind::of("Sawdust").known());
    }

    #[test]
    fn finds_keys_and_offers_an_unserved_one_rather_than_nothing() {
        let mut values = BTreeMap::new();
        values.insert("FFlagAlpha".to_string(), "True".to_string());
        values.insert("DFIntBeta".to_string(), "30".to_string());
        let catalog = Catalog {
            fetched: 0,
            application: "PCDesktopClient".to_string(),
            values,
        };

        let hits = search(&catalog, "alpha", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].default.as_deref(), Some("True"));
        assert_eq!(hits[0].kind, Kind::Flag);

        // A key Roblox does not serve comes back with no default rather than
        // being silently dropped, even when its name matches nothing served.
        let hits = search(&catalog, "DFIntTaskSchedulerTargetFps", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].default, None);
        assert_eq!(hits[0].kind, Kind::Int);

        // A partial query is a substring search and does not invent a key.
        let hits = search(&catalog, "alp", 10);
        assert_eq!(hits.len(), 1);
        assert!(hits.iter().all(|hit| hit.key == "FFlagAlpha"));
    }
}
