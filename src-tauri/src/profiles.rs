//! Launch profiles: one tile that remembers everything a launch needs.
//!
//! A tile is a named destination plus the answers that would otherwise be picked
//! again every time: which account, which flag profile, which client build, and what
//! KURO does with its own window once the game is up. Every one of those already
//! exists elsewhere in the app; this store only remembers the combination.
//!
//! What a tile is **not**: it is not a copy of a flag profile, and it does not pin
//! anything. Editing the flags behind a tile changes what it launches with, which is
//! the point of naming them rather than freezing them.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{FailureCode, LaunchError};

pub const SCHEMA: u32 = 1;
const STORE: &str = "profiles.json";
const NAME_LIMIT: usize = 40;

/// What happens to KURO's own window once the client is running. The game's window is
/// Roblox's business and is never touched; this is the launcher stepping out of the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum After {
    /// Leave the window where it is.
    #[default]
    Stay,
    /// Minimise, so the client has the screen. The process stays, which is what keeps
    /// a flag lease alive until the game exits.
    Minimize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LaunchProfile {
    /// Stable key. Built from the name once, then kept, so renaming a tile does not
    /// orphan anything that points at it.
    pub id: String,
    pub name: String,
    /// A place id, a game link, a private invite or a server link, exactly as typed.
    pub target: String,
    pub account: Option<u64>,
    /// A flag profile name, used on top of the scope chain rather than instead of it.
    pub flags: Option<String>,
    /// A client folder, `version-…`, to run instead of the pinned or live build.
    pub client: Option<String>,
    pub after: After,
    /// Unix seconds of the last launch, so the newest tile sorts first.
    pub at: u64,
    pub used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Store {
    pub schema: u32,
    pub entries: Vec<LaunchProfile>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            entries: Vec::new(),
        }
    }
}

fn path(root: &Path) -> PathBuf {
    root.join("launch").join(STORE)
}

pub fn load(root: &Path) -> Result<Store, LaunchError> {
    match fs::read(path(root)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            LaunchError::new(
                FailureCode::ConfigUnreadable,
                format!("Your launch tiles could not be read: {e}"),
            )
            .with_action("Open the data folder")
        }),
        // No file is a machine that has never saved a tile.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(e) => Err(LaunchError::new(
            FailureCode::ConfigUnreadable,
            format!("Your launch tiles could not be opened: {e}"),
        )),
    }
}

/// Writes through a temporary file, so a crash mid-write cannot leave a half store.
pub fn save(root: &Path, store: &Store) -> Result<(), LaunchError> {
    let file = path(root);
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| {
            LaunchError::new(
                FailureCode::ConfigUnwritable,
                format!("The launch folder could not be created: {e}"),
            )
        })?;
    }

    let body = serde_json::to_vec_pretty(store).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The tiles could not be encoded: {e}"),
        )
    })?;

    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, body).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The tiles could not be written: {e}"),
        )
    })?;
    fs::rename(&tmp, &file).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The tiles could not be replaced: {e}"),
        )
    })
}

/// A name turned into a key: letters and digits kept, everything else a hyphen.
///
/// Non-ASCII letters are kept rather than dropped, because a tile named in Russian is
/// as legitimate as one named in English and `tile-2` would say nothing about it.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "tile".to_string()
    } else {
        out
    }
}

pub fn find<'a>(store: &'a Store, id: &str) -> Option<&'a LaunchProfile> {
    store.entries.iter().find(|entry| entry.id == id)
}

/// Saves a tile, creating or replacing one.
///
/// A tile saved with an empty id takes the key its name suggests. Two tiles named the
/// same thing get distinct keys instead of one quietly overwriting the other, and
/// saving under an id that already exists replaces that tile rather than adding a
/// second one with the same key.
pub fn upsert(
    store: &mut Store,
    mut incoming: LaunchProfile,
) -> Result<LaunchProfile, LaunchError> {
    incoming.name = incoming.name.trim().chars().take(NAME_LIMIT).collect();
    incoming.target = incoming.target.trim().to_string();

    if incoming.name.is_empty() {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "A tile needs a name.",
        ));
    }
    if incoming.target.is_empty() {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "A tile needs a destination.",
        ));
    }
    if incoming.filters_empty() {
        incoming.flags = None;
        incoming.client = None;
    }

    let existing = if incoming.id.is_empty() {
        // Same name, same slot: saving "Bedwars" twice updates one tile.
        store
            .entries
            .iter()
            .position(|entry| entry.name.eq_ignore_ascii_case(&incoming.name))
    } else {
        store
            .entries
            .iter()
            .position(|entry| entry.id == incoming.id)
    };

    let taken = |store: &Store, id: &str, except: Option<usize>| {
        store
            .entries
            .iter()
            .enumerate()
            .any(|(index, entry)| entry.id == id && Some(index) != except)
    };

    let id = match existing {
        Some(index) => store.entries[index].id.clone(),
        None => {
            let base = if incoming.id.is_empty() {
                slug(&incoming.name)
            } else {
                slug(&incoming.id)
            };
            let mut candidate = base.clone();
            let mut serial = 2;
            while taken(store, &candidate, None) {
                candidate = format!("{base}-{serial}");
                serial += 1;
            }
            candidate
        }
    };

    incoming.id = id;

    match existing {
        Some(index) => {
            // Launch history belongs to the tile, not to the edit: saving a new name
            // for the same destination must not make it look like a tile nobody used.
            incoming.used = store.entries[index].used;
            incoming.at = store.entries[index].at.max(incoming.at);
            store.entries[index] = incoming.clone();
        }
        None => store.entries.push(incoming.clone()),
    }

    Ok(incoming)
}

/// Removes one tile. False means it was already gone, which is not an error.
pub fn remove(store: &mut Store, id: &str) -> bool {
    let before = store.entries.len();
    store.entries.retain(|entry| entry.id != id);
    store.entries.len() != before
}

/// Marks a tile as the one that just ran, which is also how it sorts.
pub fn note_used(store: &mut Store, id: &str, at: u64) {
    if let Some(entry) = store.entries.iter_mut().find(|entry| entry.id == id) {
        entry.at = at;
        entry.used = entry.used.saturating_add(1);
    }
}

/// Newest first, then by name: a tile saved today is the one being used today.
pub fn ordered(store: &Store) -> Vec<LaunchProfile> {
    let mut list = store.entries.clone();
    list.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.name.cmp(&b.name)));
    list
}

impl LaunchProfile {
    fn filters_empty(&self) -> bool {
        self.flags
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
            && self
                .client
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(name: &str, target: &str) -> LaunchProfile {
        LaunchProfile {
            name: name.to_string(),
            target: target.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_name_becomes_a_key_that_survives_renaming() {
        assert_eq!(slug("My Tile!"), "my-tile");
        assert_eq!(slug("  Bed  Wars  "), "bed-wars");
        assert_eq!(slug("Мой тайл"), "мой-тайл");
        assert_eq!(slug("!!!"), "tile");
    }

    #[test]
    fn saving_the_same_name_twice_updates_one_tile() {
        let mut store = Store::default();
        upsert(
            &mut store,
            tile("Bedwars", "https://www.roblox.com/games/1"),
        )
        .unwrap();
        let saved = upsert(
            &mut store,
            tile("Bedwars", "https://www.roblox.com/games/2"),
        )
        .unwrap();

        assert_eq!(store.entries.len(), 1);
        assert_eq!(saved.id, "bedwars");
        assert_eq!(store.entries[0].target, "https://www.roblox.com/games/2");
    }

    #[test]
    fn two_names_that_share_a_key_get_distinct_keys() {
        let mut store = Store::default();
        let first = upsert(&mut store, tile("Bed Wars", "1")).unwrap();
        let second = upsert(&mut store, tile("bed-wars", "2")).unwrap();

        assert_eq!(first.id, "bed-wars");
        assert_eq!(second.id, "bed-wars-2");
    }

    #[test]
    fn a_tile_with_no_name_or_no_destination_is_refused() {
        let mut store = Store::default();
        assert_eq!(
            upsert(&mut store, tile("", "1")).unwrap_err().code,
            FailureCode::NotFound
        );
        assert_eq!(
            upsert(&mut store, tile("Name", "  ")).unwrap_err().code,
            FailureCode::NotFound
        );
        assert!(store.entries.is_empty());
    }

    #[test]
    fn the_launch_count_belongs_to_the_tile_not_the_edit() {
        let mut store = Store::default();
        let saved = upsert(&mut store, tile("Bedwars", "1")).unwrap();
        note_used(&mut store, &saved.id, 100);
        note_used(&mut store, &saved.id, 200);

        let edited = upsert(&mut store, tile("Bedwars", "1")).unwrap();
        assert_eq!(edited.used, 2);
        assert_eq!(edited.at, 200);
    }

    #[test]
    fn the_newest_tile_sorts_first() {
        let mut store = Store::default();
        let a = upsert(&mut store, tile("A", "1")).unwrap();
        let b = upsert(&mut store, tile("B", "2")).unwrap();
        note_used(&mut store, &a.id, 10);
        note_used(&mut store, &b.id, 20);

        let ids: Vec<String> = ordered(&store).into_iter().map(|entry| entry.id).collect();
        assert_eq!(ids, vec![b.id, a.id]);
    }

    #[test]
    fn a_tile_round_trips_through_a_file() {
        let root = std::env::temp_dir().join(format!("kuro-tiles-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);

        let mut store = Store::default();
        let saved = upsert(
            &mut store,
            LaunchProfile {
                name: "Bedwars".to_string(),
                target: "https://www.roblox.com/games/1".to_string(),
                account: Some(42),
                after: After::Minimize,
                ..Default::default()
            },
        )
        .unwrap();
        save(&root, &store).unwrap();

        let read = load(&root).unwrap();
        let back = find(&read, &saved.id).unwrap();
        assert_eq!(back.target, "https://www.roblox.com/games/1");
        assert_eq!(back.account, Some(42));
        assert_eq!(back.after, After::Minimize);
        assert_eq!(back.name, "Bedwars");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_file_is_an_empty_store_and_a_broken_one_is_not() {
        let root = std::env::temp_dir().join(format!("kuro-tiles-broken-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        assert!(load(&root).unwrap().entries.is_empty());

        fs::create_dir_all(root.join("launch")).unwrap();
        fs::write(root.join("launch").join(STORE), b"{ not json").unwrap();
        assert_eq!(load(&root).unwrap_err().code, FailureCode::ConfigUnreadable);
        let _ = fs::remove_dir_all(&root);
    }
}
