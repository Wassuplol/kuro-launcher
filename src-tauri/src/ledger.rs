//! The playtime ledger: what the logs would forget.
//!
//! Roblox keeps a handful of logs and deletes the rest, so every total read straight
//! from that folder silently shrinks over time. This store keeps one line per session
//! keyed by the log's own id, which is the only thing that makes "since records began"
//! an honest sentence.
//!
//! What is recorded is what the log said: the place, the start, how long the client was
//! alive, and whether a server ever answered. A session that never reached a server is
//! kept as a session, because a launch that failed is still a launch.
//!
//! The ledger starts the first time this app reads the log folder. Everything before
//! that is gone, and the screen says so rather than backdating a total.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{FailureCode, LaunchError};
use crate::receipts::Receipt;

pub const SCHEMA: u32 = 1;
const STORE: &str = "sessions.json";

/// How many sessions the ledger holds. A log folder holds twelve; this holds years.
const LIMIT: usize = 20_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Entry {
    /// The log's own id, which is what makes this an upsert rather than a counter.
    pub id: String,
    pub place: Option<u64>,
    pub name: Option<String>,
    /// Unix seconds.
    pub started: u64,
    /// Seconds the client was alive, as the log's own clock measured it.
    pub seconds: u64,
    /// Whether a server answered. Time only counts when one did.
    pub reached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Store {
    pub schema: u32,
    pub entries: Vec<Entry>,
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
    root.join(STORE)
}

pub fn load(root: &Path) -> Result<Store, LaunchError> {
    match fs::read(path(root)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            LaunchError::new(
                FailureCode::ConfigUnreadable,
                format!("Your playtime ledger could not be read: {e}"),
            )
            .with_action("Open the data folder")
        }),
        // No file is a machine that has never been read before.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(e) => Err(LaunchError::new(
            FailureCode::ConfigUnreadable,
            format!("Your playtime ledger could not be opened: {e}"),
        )),
    }
}

pub fn save(root: &Path, store: &Store) -> Result<(), LaunchError> {
    let body = serde_json::to_vec(store).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The ledger could not be encoded: {e}"),
        )
    })?;

    let file = path(root);
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, body).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The ledger could not be written: {e}"),
        )
    })?;
    fs::rename(&tmp, &file).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The ledger could not be replaced: {e}"),
        )
    })
}

/// Folds the receipts in hand into the ledger, and answers how many were new.
///
/// Only a real change writes the file, because this runs on every read of the log
/// folder and a launcher that rewrites a store each time the page opens is a launcher
/// wearing out a disk for nothing.
pub fn absorb(root: &Path, receipts: &[Receipt]) -> Result<usize, LaunchError> {
    let mut store = load(root)?;
    let mut added = 0usize;
    let mut changed = false;

    for receipt in receipts {
        match store
            .entries
            .iter_mut()
            .find(|entry| entry.id == receipt.id)
        {
            Some(entry) => {
                // A session absorbed while it was still running has no length yet, and
                // the log that closes it is the one that knows how long it lasted.
                if receipt.handshake && receipt.seconds > entry.seconds {
                    entry.seconds = receipt.seconds;
                    changed = true;
                }
                if entry.name.is_none() {
                    if let Some(name) = &receipt.name {
                        entry.name = Some(name.clone());
                        changed = true;
                    }
                }
                if entry.place.is_none() && receipt.place.is_some() {
                    entry.place = receipt.place;
                    changed = true;
                }
            }
            None => {
                store.entries.push(Entry {
                    id: receipt.id.clone(),
                    place: receipt.place,
                    name: receipt.name.clone(),
                    started: receipt.started,
                    seconds: if receipt.handshake {
                        receipt.seconds
                    } else {
                        0
                    },
                    reached: receipt.handshake,
                });
                added += 1;
            }
        }
    }

    if added > 0 || changed {
        store.entries.sort_by(|a, b| a.started.cmp(&b.started));
        // Oldest out first: a ledger is a record of what happened, and a session from
        // three years ago is the one to lose if one has to go.
        if store.entries.len() > LIMIT {
            let drop = store.entries.len() - LIMIT;
            store.entries.drain(0..drop);
        }
        save(root, &store)?;
    }

    Ok(added)
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub seconds: u64,
    pub sessions: u64,
    /// Places played, not launched at: a session that never reached a server is not a
    /// place somebody was in.
    pub places: u64,
    /// Days with at least one session, counted in UTC dates, which is the same calendar
    /// the log file names use.
    pub days: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub place: u64,
    pub name: Option<String>,
    pub seconds: u64,
    pub sessions: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Longest {
    pub place: Option<u64>,
    pub name: Option<String>,
    pub seconds: u64,
    pub started: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    /// `YYYY-MM-DD`, in UTC: the calendar the log file names already use.
    pub date: String,
    pub seconds: u64,
}

/// Everything the year-in-review card says.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ledger {
    pub all: Totals,
    pub year: u32,
    pub this_year: Totals,
    /// The first and last session the ledger holds, unix seconds.
    pub first: Option<u64>,
    pub last: Option<u64>,
    pub top: Vec<Place>,
    pub year_top: Vec<Place>,
    pub longest: Option<Longest>,
    pub busiest: Option<Day>,
}

/// How many places the card lists. Three fits beside the numbers without a scroll.
const TOP: usize = 3;

pub fn view(root: &Path) -> Result<Ledger, LaunchError> {
    let store = load(root)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(summarise(&store, now))
}

pub fn summarise(store: &Store, now: u64) -> Ledger {
    let year = year_of(now);

    let mut all = Totals::default();
    let mut mine = Totals::default();
    let mut all_places: Vec<Place> = Vec::new();
    let mut year_places: Vec<Place> = Vec::new();
    let mut all_days: Vec<u64> = Vec::new();
    let mut year_days: Vec<(u64, u64)> = Vec::new();
    let mut this_year_days: Vec<u64> = Vec::new();
    let mut longest: Option<Longest> = None;
    let mut busiest: Option<Day> = None;

    for entry in &store.entries {
        let day = entry.started / 86_400;
        let is_this_year = year_of(entry.started) == year;

        all.sessions += 1;
        if !all_days.contains(&day) {
            all_days.push(day);
        }

        if is_this_year {
            mine.sessions += 1;
            if !this_year_days.contains(&day) {
                this_year_days.push(day);
            }
        }

        // Time and places only count where a server answered: a failed launch is a
        // session, not an hour in a game.
        if !entry.reached || entry.seconds == 0 {
            continue;
        }

        all.seconds += entry.seconds;
        if is_this_year {
            mine.seconds += entry.seconds;
        }

        if let Some(place) = entry.place {
            add_place(&mut all_places, place, entry);
            if is_this_year {
                add_place(&mut year_places, place, entry);
            }
        }

        if is_this_year {
            let slot = year_days.iter_mut().find(|(at, _)| *at == day);
            match slot {
                Some((_, seconds)) => *seconds += entry.seconds,
                None => year_days.push((day, entry.seconds)),
            }

            if longest
                .as_ref()
                .map(|best| entry.seconds > best.seconds)
                .unwrap_or(true)
            {
                longest = Some(Longest {
                    place: entry.place,
                    name: entry.name.clone(),
                    seconds: entry.seconds,
                    started: entry.started,
                });
            }
        }
    }

    if let Some((day, seconds)) = year_days.into_iter().max_by_key(|(_, seconds)| *seconds) {
        busiest = Some(Day {
            date: date_of(day * 86_400),
            seconds,
        });
    }

    all.places = all_places.len() as u64;
    all.days = all_days.len() as u64;
    mine.places = year_places.len() as u64;
    mine.days = this_year_days.len() as u64;

    all_places.sort_by(|a, b| b.seconds.cmp(&a.seconds).then(a.place.cmp(&b.place)));
    year_places.sort_by(|a, b| b.seconds.cmp(&a.seconds).then(a.place.cmp(&b.place)));

    Ledger {
        all,
        year,
        this_year: mine,
        first: store.entries.iter().map(|entry| entry.started).min(),
        last: store.entries.iter().map(|entry| entry.started).max(),
        top: all_places.into_iter().take(TOP).collect(),
        year_top: year_places.into_iter().take(TOP).collect(),
        longest,
        busiest,
    }
}

fn add_place(list: &mut Vec<Place>, place: u64, entry: &Entry) {
    match list.iter_mut().find(|item| item.place == place) {
        Some(item) => {
            item.seconds += entry.seconds;
            item.sessions += 1;
            if item.name.is_none() {
                item.name = entry.name.clone();
            }
        }
        None => list.push(Place {
            place,
            name: entry.name.clone(),
            seconds: entry.seconds,
            sessions: 1,
        }),
    }
}

/// The calendar year a moment falls in, in UTC.
fn year_of(at: u64) -> u32 {
    let (year, _, _) = civil_from_days((at / 86_400) as i64);
    year as u32
}

/// `YYYY-MM-DD` in UTC, which is the calendar the log file names already use.
fn date_of(at: u64) -> String {
    let (year, month, day) = civil_from_days((at / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Days since 1970-01-01 back to a civil date, by Howard Hinnant's algorithm. The
/// inverse of the one in `receipts`, written here so this module stays independent of
/// how that one parses file names.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipts::Outcome;

    fn receipt(id: &str, started: u64, seconds: u64, reached: bool) -> Receipt {
        Receipt {
            id: id.to_string(),
            version: "0.748.0.7481526".to_string(),
            started,
            seconds,
            place: Some(920_587_237),
            name: Some("Jailbreak".to_string()),
            universe: Some(1),
            job: None,
            server: None,
            endpoint: None,
            join_seconds: None,
            handshake: reached,
            adapter: None,
            api: None,
            vram_mb: None,
            adapters: Vec::new(),
            shutdown_stage: None,
            http_errors: 0,
            last_http: None,
            outcome: if reached {
                Outcome::Left
            } else {
                Outcome::NoHandshake
            },
            verdict: String::new(),
            action: None,
            lines: 0,
            bytes: 0,
            log_path: String::new(),
        }
    }

    #[test]
    fn the_same_session_twice_is_one_line() {
        let root = std::env::temp_dir().join(format!("kuro-ledger-once-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        assert_eq!(
            absorb(&root, &[receipt("a", 1_700_000_000, 600, true)]).unwrap(),
            1
        );
        assert_eq!(
            absorb(&root, &[receipt("a", 1_700_000_000, 600, true)]).unwrap(),
            0
        );
        assert_eq!(load(&root).unwrap().entries.len(), 1);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_session_read_while_it_was_running_is_lengthened_by_the_closed_log() {
        let root = std::env::temp_dir().join(format!("kuro-ledger-live-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        absorb(&root, &[receipt("a", 1_700_000_000, 60, true)]).unwrap();
        absorb(&root, &[receipt("a", 1_700_000_000, 900, true)]).unwrap();
        assert_eq!(load(&root).unwrap().entries[0].seconds, 900);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_launch_that_never_reached_a_server_counts_as_a_session_and_not_as_time() {
        let store = Store {
            schema: SCHEMA,
            entries: vec![
                Entry {
                    id: "a".to_string(),
                    place: None,
                    name: None,
                    started: 1_700_000_000,
                    seconds: 0,
                    reached: false,
                },
                Entry {
                    id: "b".to_string(),
                    place: Some(920_587_237),
                    name: Some("Jailbreak".to_string()),
                    started: 1_700_003_600,
                    seconds: 600,
                    reached: true,
                },
            ],
        };

        let ledger = summarise(&store, 1_700_010_000);
        assert_eq!(ledger.all.sessions, 2);
        assert_eq!(ledger.all.seconds, 600);
        assert_eq!(ledger.all.places, 1);
        assert_eq!(ledger.all.days, 1);
    }

    #[test]
    fn the_card_counts_only_this_year_where_it_says_this_year() {
        // 2025-12-31T23:00Z and 2026-01-01T01:00Z: two hours apart, different years.
        let store = Store {
            schema: SCHEMA,
            entries: vec![
                Entry {
                    id: "old".to_string(),
                    place: Some(1),
                    name: Some("Old".to_string()),
                    started: 1_767_222_000,
                    seconds: 3_600,
                    reached: true,
                },
                Entry {
                    id: "new".to_string(),
                    place: Some(2),
                    name: Some("New".to_string()),
                    started: 1_767_229_200,
                    seconds: 1_200,
                    reached: true,
                },
            ],
        };

        let ledger = summarise(&store, 1_767_240_000);
        assert_eq!(year_of(1_767_222_000), 2025);
        assert_eq!(year_of(1_767_229_200), 2026);
        assert_eq!(ledger.year, 2026);
        assert_eq!(ledger.all.seconds, 4_800);
        assert_eq!(ledger.this_year.seconds, 1_200);
        assert_eq!(ledger.this_year.sessions, 1);
        assert_eq!(ledger.year_top.len(), 1);
        assert_eq!(ledger.year_top[0].name.as_deref(), Some("New"));
        assert_eq!(ledger.longest.as_ref().unwrap().seconds, 1_200);
    }

    #[test]
    fn a_date_reads_back_as_the_date_it_came_from() {
        assert_eq!(date_of(1_795_018_424), "2026-11-18");
        assert_eq!(year_of(1_795_018_424), 2026);
        assert_eq!(date_of(0), "1970-01-01");
    }

    #[test]
    fn the_busiest_day_is_the_day_with_the_most_time_in_it() {
        let day = 20_400; // a fixed civil day, whatever it is
        let store = Store {
            schema: SCHEMA,
            entries: vec![
                Entry {
                    id: "a".to_string(),
                    place: Some(1),
                    name: None,
                    started: day * 86_400 + 3_600,
                    seconds: 600,
                    reached: true,
                },
                Entry {
                    id: "b".to_string(),
                    place: Some(1),
                    name: None,
                    started: day * 86_400 + 7_200,
                    seconds: 900,
                    reached: true,
                },
                Entry {
                    id: "c".to_string(),
                    place: Some(1),
                    name: None,
                    started: (day + 1) * 86_400 + 3_600,
                    seconds: 100,
                    reached: true,
                },
            ],
        };

        let ledger = summarise(&store, (day + 2) * 86_400);
        let busiest = ledger.busiest.expect("a busiest day");
        assert_eq!(busiest.seconds, 1_500);
        assert_eq!(busiest.date, date_of(day * 86_400));
    }
}
