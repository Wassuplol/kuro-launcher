//! The black box: what this machine actually ran, read from the client's own logs.
//!
//! Roblox writes one `*_Player_<hash>_last.log` per launch into
//! `%LOCALAPPDATA%\Roblox\logs`. The client is talkative, and five of its lines are
//! the whole point of this module:
//!
//! - `! Joining game '<job>' place <id> at <address>` names the destination, the job
//!   it was given and the address it was handed.
//! - `Report game_join_loadtime: placeid:<id>, join_time:<s>, universeid:<id>`
//!   says how long the join actually took, and which universe it belonged to.
//! - `Handshake complete` proves a server answered. Without that line the client
//!   never reached a game, whatever the reason looks like from outside.
//! - `D3D11 Adapter: <name>` and `GfxFeatureLevel <level>` say which GPU the client
//!   actually chose, which is a different question from which GPU the machine has.
//! - `shutDown: (stage:<stage>)` says the client closed itself instead of dying.
//!
//! Everything here is derived, never invented. A log with no join line says it never
//! asked for a server; one with a join and no handshake says it stopped before
//! reaching one, and quotes the last failed request rather than guessing at a cause.
//!
//! The parser is deliberately total: a log it cannot understand produces a receipt
//! with `outcome: Ended` and no claims, not an error. A launcher that hides a broken
//! log is worse than one that shows a short row.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{FailureCode, LaunchError};
use crate::library;

/// How many receipts the page is allowed to carry. A launcher is not an archive.
pub const LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// A server answered and the client shut itself down.
    Left,
    /// A server answered, and the log stops without a shutdown line.
    Ended,
    /// The client asked for a place but never completed a handshake.
    NoHandshake,
    /// The client opened and never asked for a place at all.
    NeverJoined,
    /// The client logged a crash marker while it was running.
    Crashed,
}

/// One launch, as the client recorded it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    /// The log file stem: unique per launch, and what the row points at.
    pub id: String,
    pub version: String,
    /// Unix seconds, from the stamp the client put in the file name.
    pub started: u64,
    /// How long the client was alive, in seconds, from the log's own clock.
    pub seconds: u64,
    pub place: Option<u64>,
    /// Filled from the launcher's own history when it knows this place.
    pub name: Option<String>,
    pub universe: Option<u64>,
    pub job: Option<String>,
    pub server: Option<String>,
    /// Seconds from process start to the join completing, when the client said so.
    pub join_seconds: Option<u64>,
    pub handshake: bool,
    pub adapter: Option<String>,
    pub api: Option<String>,
    pub vram_mb: Option<u64>,
    pub shutdown_stage: Option<String>,
    /// How many failed HTTP requests the client logged, and the last one.
    pub http_errors: u64,
    pub last_http: Option<String>,
    pub outcome: Outcome,
    /// One sentence a person can read without knowing what a handshake is.
    pub verdict: String,
    pub action: Option<String>,
    pub lines: u64,
    pub bytes: u64,
    pub log_path: String,
}

/// `%LOCALAPPDATA%\Roblox\logs`, where every client on the machine writes.
pub fn logs_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("Roblox").join("logs"))
}

/// Every receipt this machine has a log for, newest first.
///
/// Reading the directory each time is on purpose: the logs are the source of truth,
/// the client rotates them itself, and a cache would show a launch the disk no longer
/// has. Twelve logs cost about 20 ms.
pub fn scan(data_root: &Path) -> Result<Vec<Receipt>, LaunchError> {
    let Some(dir) = logs_dir() else {
        return Ok(Vec::new());
    };

    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        // No logs folder is a machine that has never run the client, not a fault.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(LaunchError::new(
                FailureCode::ConfigUnreadable,
                format!("Roblox's log folder could not be read: {e}"),
            )
            .with_action("Open the data folder"))
        }
    };

    let names = known_names(data_root);
    let mut receipts: Vec<Receipt> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        // Crashpad writes its own log beside the player's, with the same stamp.
        if !file.ends_with(".log") || !file.contains("_Player_") || file.contains("CrashHandler") {
            continue;
        }
        if let Some(mut receipt) = parse(&path, file) {
            if let Some(place) = receipt.place {
                if let Some(name) = names.get(&place) {
                    receipt.name = Some(name.clone());
                }
            }
            receipts.push(receipt);
        }
    }

    receipts.sort_by(|a, b| b.started.cmp(&a.started).then(b.id.cmp(&a.id)));
    receipts.truncate(LIMIT);
    Ok(receipts)
}

/// Place names from the launcher's own history, so a receipt reads "Brookhaven"
/// instead of an id. Best effort: a launch made outside this app has no name here,
/// and the row falls back to the id rather than inventing one.
fn known_names(data_root: &Path) -> HashMap<u64, String> {
    // The cache first, then the launcher's own history on top: a name written at
    // launch time is the one the person saw.
    let mut map = cached_names(data_root);
    for entry in library::load(data_root).entries {
        if let Some(name) = entry.name {
            if !name.is_empty() {
                map.insert(entry.place, name);
            }
        }
    }
    map
}

/// Parses one log. `None` means the file name carried no usable timestamp.
pub fn parse(path: &Path, file: &str) -> Option<Receipt> {
    let stamp = stamp_from_name(file)?;
    let version = file.split('_').next().unwrap_or_default().to_string();

    let raw = fs::read(path).ok()?;
    let bytes = raw.len() as u64;
    let text = String::from_utf8_lossy(&raw);

    let mut lines = 0u64;
    let mut seconds = 0u64;
    let mut place = None;
    let mut universe = None;
    let mut job = None;
    let mut server = None;
    let mut join_seconds = None;
    let mut handshake = false;
    let mut adapter = None;
    let mut api = None;
    let mut vram_mb = None;
    let mut shutdown_stage = None;
    let mut http_errors = 0u64;
    let mut last_http = None;
    let mut crash = false;

    for line in text.lines() {
        lines += 1;
        if let Some(rel) = relative_seconds(line) {
            seconds = seconds.max(rel);
        }

        if let Some(start) = line.find("! Joining game '") {
            let rest = &line[start + "! Joining game '".len()..];
            if let Some(end) = rest.find('\'') {
                job = Some(rest[..end].to_string());
                let tail = &rest[end..];
                if let Some(at) = tail.find(" place ") {
                    place = digits(&tail[at + 7..]);
                }
                if let Some(at) = tail.find(" at ") {
                    let address = tail[at + 4..].trim();
                    let address = address.split_whitespace().next().unwrap_or("");
                    if !address.is_empty() {
                        server = Some(address.to_string());
                    }
                }
            }
        }

        if line.contains("Report game_join_loadtime") {
            if let Some(at) = line.find("placeid:") {
                place = place.or_else(|| digits(&line[at + 8..]));
            }
            if let Some(at) = line.find("universeid:") {
                universe = digits(&line[at + 11..]);
            }
            if let Some(at) = line.find("join_time:") {
                let rest = &line[at + 10..];
                let end = rest
                    .find(|c: char| !c.is_ascii_digit() && c != '.')
                    .unwrap_or(rest.len());
                join_seconds = rest[..end]
                    .parse::<f64>()
                    .ok()
                    .map(|value| value.round() as u64);
            }
        }

        if line.contains("Handshake complete") {
            handshake = true;
        }

        // The client logs both the adapter list and the one it chose; the chosen one
        // is the line without "Vendor" in it.
        if let Some(at) = line.find("D3D11 Adapter: ") {
            let rest = line[at + "D3D11 Adapter: ".len()..].trim();
            if rest.starts_with("Vendor ") {
                if let Some(vram) = rest.find("VRAM ") {
                    vram_mb = digits(&rest[vram + 5..]);
                }
            } else if !rest.is_empty() && adapter.is_none() {
                adapter = Some(rest.to_string());
            }
        }

        if let Some(at) = line.find("GfxFeatureLevel ") {
            let rest = line[at + "GfxFeatureLevel ".len()..].trim();
            let level = rest.split_whitespace().next().unwrap_or("");
            if !level.is_empty() {
                api = Some(level.to_string());
            }
        }
        if line.contains("Vulkan: Using extension") || line.contains("Vulkan Adapter") {
            api = Some("Vulkan".to_string());
        }

        if let Some(at) = line.find("shutDown: (stage:") {
            let rest = &line[at + "shutDown: (stage:".len()..];
            let end = rest.find(')').unwrap_or(rest.len());
            shutdown_stage = Some(rest[..end].to_string());
        }

        if line.contains("[DFLog::HttpTraceError]") {
            http_errors += 1;
            let status = line
                .find("status:")
                .and_then(|at| digits(&line[at + 7..]))
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let url = line
                .find("url:{ \"")
                .and_then(|at| {
                    let rest = &line[at + 7..];
                    rest.find('"').map(|end| rest[..end].to_string())
                })
                .unwrap_or_default();
            last_http = Some(if url.is_empty() {
                status
            } else {
                format!("{status} {}", host_of(&url))
            });
        }

        if line.contains("FLog::CrashHandler")
            || line.contains("!! Fatal")
            || line.contains("Fatal error:")
            || line.contains("Roblox has crashed")
        {
            crash = true;
        }
    }

    let seconds = seconds.max(join_seconds.unwrap_or(0));
    let outcome = if crash {
        Outcome::Crashed
    } else if handshake {
        if shutdown_stage.is_some() {
            Outcome::Left
        } else {
            Outcome::Ended
        }
    } else if place.is_some() {
        Outcome::NoHandshake
    } else {
        Outcome::NeverJoined
    };

    let verdict = match outcome {
        Outcome::Left => format!("Ran for {} and left cleanly.", span(seconds)),
        Outcome::Ended => format!(
            "Ran for {}. The log stops without a shutdown line, which is what closing the window looks like.",
            span(seconds)
        ),
        Outcome::NoHandshake => {
            let mut text = "Stopped before it reached a server.".to_string();
            if let Some(last) = &last_http {
                text.push_str(&format!(
                    " {} failed request{}, the last was {last}.",
                    http_errors,
                    if http_errors == 1 { "" } else { "s" }
                ));
            }
            text
        }
        Outcome::NeverJoined => {
            "The client opened but never asked for a place. Usually the bootstrapper handing over.".to_string()
        }
        Outcome::Crashed => {
            "The client logged a crash while it was running.".to_string()
        }
    };

    let action = match outcome {
        Outcome::NoHandshake => Some("Check the network".to_string()),
        Outcome::Crashed => Some("Open the log".to_string()),
        _ => None,
    };

    Some(Receipt {
        id: file
            .trim_end_matches("_last.log")
            .trim_end_matches(".log")
            .to_string(),
        version,
        started: stamp,
        seconds,
        place,
        name: None,
        universe,
        job,
        server,
        join_seconds,
        handshake,
        adapter,
        api,
        vram_mb,
        shutdown_stage,
        http_errors,
        last_http,
        outcome,
        verdict,
        action,
        lines,
        bytes,
        log_path: path.to_string_lossy().to_string(),
    })
}

/// "1h 12m", "6m 41s", "48s". Rounded, never two units of noise.
pub fn span(seconds: u64) -> String {
    if seconds >= 3600 {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        if minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {minutes}m")
        }
    } else if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

/// The leading digits of `text`, as a number.
fn digits(text: &str) -> Option<u64> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    text[..end].parse().ok()
}

/// The host of an absolute URL, for a line that already names the status.
fn host_of(url: &str) -> &str {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    rest.split('/').next().unwrap_or(rest)
}

/// The relative-seconds field every log line carries as its second column.
fn relative_seconds(line: &str) -> Option<u64> {
    let mut parts = line.split(',');
    parts.next()?;
    let field = parts.next()?;
    field.parse::<f64>().ok().map(|value| value.round() as u64)
}

/// The UTC stamp in `0.748.0.7481526_20261118T162027Z_Player_4DADF_last.log`.
fn stamp_from_name(file: &str) -> Option<u64> {
    let token = file
        .split('_')
        .find(|part| part.len() == 16 && part.ends_with('Z') && part.contains('T'))?;
    let year: i64 = token.get(0..4)?.parse().ok()?;
    let month: i64 = token.get(4..6)?.parse().ok()?;
    let day: i64 = token.get(6..8)?.parse().ok()?;
    let hour: i64 = token.get(9..11)?.parse().ok()?;
    let minute: i64 = token.get(11..13)?.parse().ok()?;
    let second: i64 = token.get(13..15)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some((days * 86_400 + hour * 3600 + minute * 60 + second).max(0) as u64)
}

/// Days since 1970-01-01 for a civil date, by Howard Hinnant's algorithm. Written
/// out rather than pulled in as a dependency: this is the only date arithmetic in
/// the crate, and a date crate for one subtraction is a bad trade.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let shifted = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * shifted + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

// ---------------------------------------------------------------------------
// Place names
// ---------------------------------------------------------------------------

/// Where a place id resolves to, so a receipt reads "Natural Disaster Survival"
/// instead of "Place 189707". Kept on disk because a name does not change and the
/// page should still read well offline.
const PLACES: &str = "places.json";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Resolved {
    universe: u64,
    name: String,
    at: u64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Places {
    schema: u32,
    places: HashMap<String, Resolved>,
}

fn places_path(root: &Path) -> PathBuf {
    root.join(PLACES)
}

fn read_places(root: &Path) -> Places {
    fs::read(places_path(root))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Places>(&bytes).ok())
        .unwrap_or_default()
}

/// Cached names for the places asked about, without any network call.
pub fn cached_names(root: &Path) -> HashMap<u64, String> {
    read_places(root)
        .places
        .into_iter()
        .filter_map(|(place, resolved)| place.parse().ok().map(|place| (place, resolved.name)))
        .collect()
}

/// Resolves places the cache does not know yet, then answers for everything asked.
///
/// Best effort by design: this page exists to work when Roblox does not answer, so a
/// failed lookup leaves the row as a place id rather than failing the screen. At most
/// `BATCH` new places are resolved per call, and every answer is kept forever.
pub fn names(root: &Path, wanted: &[u64]) -> HashMap<u64, String> {
    const BATCH: usize = 10;

    let mut store = read_places(root);
    let mut answered: HashMap<u64, String> = HashMap::new();
    let mut tries = 0usize;
    let mut changed = false;

    for &place in wanted {
        let key = place.to_string();
        if let Some(resolved) = store.places.get(&key) {
            answered.insert(place, resolved.name.clone());
            continue;
        }
        if tries >= BATCH {
            continue;
        }
        tries += 1;
        if let Some((universe, name)) = resolve_place(place) {
            store.places.insert(
                key,
                Resolved {
                    universe,
                    name: name.clone(),
                    at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                },
            );
            answered.insert(place, name);
            changed = true;
        }
    }

    if changed {
        store.schema = 1;
        let _ = fs::create_dir_all(root);
        let _ = fs::write(
            places_path(root),
            serde_json::to_vec_pretty(&store).unwrap_or_default(),
        );
    }

    answered
}

#[derive(serde::Deserialize)]
struct UniverseAnswer {
    #[serde(rename = "universeId")]
    universe: u64,
}

#[derive(serde::Deserialize)]
struct GamesAnswer {
    data: Vec<GameAnswer>,
}

#[derive(serde::Deserialize)]
struct GameAnswer {
    #[serde(default)]
    id: u64,
    name: String,
}

/// place id to universe id, then universe id to its name. Two measured endpoints:
/// `apis.roblox.com/universes/v1/places/<id>/universe` answers anonymously, and
/// `gameId`-keyed `multiget-place-details` does not (it wants a session), which is
/// why the name is fetched by universe instead.
fn resolve_place(place: u64) -> Option<(u64, String)> {
    let universe: UniverseAnswer = get_json(&format!(
        "https://apis.roblox.com/universes/v1/places/{place}/universe"
    ))?;
    let games: GamesAnswer = get_json(&format!(
        "https://games.roblox.com/v1/games?universeIds={}",
        universe.universe
    ))?;
    let first = games.data.into_iter().next()?;
    // Roblox answers with a placeholder row, `id: 0`, rather than a 404 for an id it
    // does not know. A placeholder in a receipt is worse than the id it replaced, and
    // a leading `[` is not a placeholder: real games carry event tags like
    // "[HALLOWEEN] The Strongest Battlegrounds".
    if first.id == 0 || first.name.is_empty() || first.name == "[TITLE UNAVAILABLE]" {
        return None;
    }
    Some((universe.universe, first.name))
}

fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Option<T> {
    let answer = crate::http::get(url).ok()?;
    if !answer.ok() {
        return None;
    }
    serde_json::from_str(&answer.body).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const CLEAN: &str = "\
2026-11-18T16:13:44.389Z,0.389293,b668,6,Info [DFLog::Mimalloc] mi_option_reserve_os_memory=0
2026-11-18T16:13:45.206Z,1.206065,b668,6 [FLog::Graphics] D3D11 Adapter: NVIDIA GeForce RTX 5070 Ti
2026-11-18T16:13:45.206Z,1.206065,b668,6 [FLog::Graphics] D3D11 Adapter: Vendor 10de Device 2c05 VRAM 15995 RAM 46696
2026-11-18T16:13:45.219Z,1.219940,b668,6,Warning [FLog::Graphics] D3D11: GfxCardName NVIDIA GeForce RTX 5070 Ti GfxCardVendor 10de GfxFeatureLevel D3D11.1
2026-11-18T16:13:45.300Z,1.300000,95f4,6,Info [FLog::Network] ! Joining game 'f2e8bfce-f3e5-4f58-adfc-d18b5cdeacbf' place 10449761463 at 10.206.10.135
2026-11-18T16:15:41.571Z,117.571426,95f4,6 [FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:10449761463, join_time:116.50002209398429898, universeid:3808081382, referral_page:DeepLink
2026-11-18T16:15:41.683Z,117.683708,c9d4,6,Info [DFLog::RbxTransportRnaExpConnection] 128.116.5.33:63242. Handshake complete. scid=2a75dd382b2927c844b2de22bd2fa2839dcb6ad7
2026-11-18T16:20:25.019Z,401.019470,b668,6 [FLog::SingleSurfaceApp] shutDown: (stage:UGCGame).
2026-11-18T16:20:26.293Z,402.293640,b668,6,Info [DFLog::SystemDialogHandler] Platform handler was destroyed.
";

    const BOOTSTRAPPER: &str = "\
2026-11-18T16:20:27.359Z,0.359342,2420,12 [DFLog::HttpTraceError] HttpResponse(#3) status:404 Not Found bodySize:0 url:{ \"https://apis.roblox.com/\" } ip:128.116.5.3
2026-11-18T16:20:27.375Z,0.375947,2420,12 [DFLog::HttpTraceError] HttpResponse(#1) status:404 Not Found bodySize:0 url:{ \"https://apis.roblox.com/\" } ip:128.116.5.3
2026-11-18T16:20:28.000Z,1.000000,2420,6,Info [FLog::WndProcessCheck] waitForNewPlayerProcess new waiting for mutex result is 0X000000
";

    const STOPPED: &str = "\
2026-11-18T10:10:08.000Z,0.008278,4b78,6,Info [FLog::Output] start
2026-11-18T10:10:08.400Z,0.400000,4b78,6 [FLog::Network] ! Joining game 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' place 920587237 at 128.116.4.1
2026-11-18T10:10:09.900Z,1.900000,4b78,12 [DFLog::HttpTraceError] HttpResponse(#9) status:401 Not Found bodySize:0 url:{ \"https://apis.roblox.com/authentication-service/v1/tickets\" } ip:128.116.4.2
";

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(body.as_bytes()).unwrap();
        path
    }

    #[test]
    fn reads_a_clean_session_end_to_end() {
        let dir = std::env::temp_dir().join("kuro-receipts-clean");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let name = "0.748.0.7481526_20261118T161344Z_Player_297EB_last.log";
        let path = write(&dir, name, CLEAN);

        let receipt = parse(&path, name).expect("a receipt");
        assert_eq!(receipt.version, "0.748.0.7481526");
        assert_eq!(receipt.place, Some(10_449_761_463));
        assert_eq!(receipt.universe, Some(3_808_081_382));
        assert_eq!(
            receipt.job.as_deref(),
            Some("f2e8bfce-f3e5-4f58-adfc-d18b5cdeacbf")
        );
        assert_eq!(receipt.server.as_deref(), Some("10.206.10.135"));
        assert!(receipt.handshake);
        assert_eq!(
            receipt.adapter.as_deref(),
            Some("NVIDIA GeForce RTX 5070 Ti")
        );
        assert_eq!(receipt.vram_mb, Some(15_995));
        assert_eq!(receipt.api.as_deref(), Some("D3D11.1"));
        assert_eq!(receipt.shutdown_stage.as_deref(), Some("UGCGame"));
        assert_eq!(receipt.join_seconds, Some(117));
        assert_eq!(receipt.seconds, 402);
        assert_eq!(receipt.outcome, Outcome::Left);
        assert_eq!(receipt.verdict, "Ran for 6m 42s and left cleanly.");
        assert_eq!(receipt.action, None);
        assert_eq!(receipt.started, 1_795_018_424);
    }

    #[test]
    fn a_bootstrapper_log_never_claims_a_session() {
        let dir = std::env::temp_dir().join("kuro-receipts-boot");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let name = "0.748.0.7481526_20261118T162027Z_Player_2E49B_last.log";
        let path = write(&dir, name, BOOTSTRAPPER);

        let receipt = parse(&path, name).expect("a receipt");
        assert_eq!(receipt.place, None);
        assert!(!receipt.handshake);
        assert_eq!(receipt.http_errors, 2);
        assert_eq!(receipt.outcome, Outcome::NeverJoined);
        assert!(receipt.verdict.contains("never asked for a place"));
    }

    #[test]
    fn a_join_without_a_handshake_quotes_the_last_failure() {
        let dir = std::env::temp_dir().join("kuro-receipts-stopped");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let name = "0.748.0.7481526_20261118T101008Z_Player_0A8A0_last.log";
        let path = write(&dir, name, STOPPED);

        let receipt = parse(&path, name).expect("a receipt");
        assert_eq!(receipt.place, Some(920_587_237));
        assert!(!receipt.handshake);
        assert_eq!(receipt.outcome, Outcome::NoHandshake);
        assert_eq!(receipt.last_http.as_deref(), Some("401 apis.roblox.com"));
        assert!(receipt
            .verdict
            .contains("Stopped before it reached a server"));
        assert!(receipt.verdict.contains("1 failed request"));
        assert_eq!(receipt.action.as_deref(), Some("Check the network"));
    }

    #[test]
    fn the_stamp_in_the_file_name_is_the_start() {
        assert_eq!(
            stamp_from_name("0.748.0.7481526_20261118T162027Z_Player_4DADF_last.log"),
            Some(1_795_018_827)
        );
        assert_eq!(stamp_from_name("no stamp here.log"), None);
        assert_eq!(
            stamp_from_name("0.748.0.7481526_20261318T162027Z_bad.log"),
            None
        );
    }

    #[test]
    fn durations_read_like_a_person_wrote_them() {
        assert_eq!(span(48), "48s");
        assert_eq!(span(401), "6m 41s");
        assert_eq!(span(3_600), "1h");
        assert_eq!(span(4_320), "1h 12m");
    }
}
