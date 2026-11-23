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
use std::time::{Duration, SystemTime};

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
    /// Roblox's own updater opened a tray client and left it running. Nothing was
    /// launched from this app, so this is not a launch that failed.
    Tray,
    /// The client logged a crash marker while it was running.
    Crashed,
}

/// One GPU the client saw, and whether it is the one it used.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Adapter {
    pub name: String,
    pub vram_mb: Option<u64>,
    pub chosen: bool,
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
    /// The endpoint from the handshake line, `<ip>:<port>`. This is the machine that
    /// actually answered; the join address above is often a relay that is not the
    /// game server and not something worth measuring.
    pub endpoint: Option<String>,
    /// Seconds from process start to the join completing, when the client said so.
    pub join_seconds: Option<u64>,
    /// Seconds between the join line and the last line the client wrote.
    ///
    /// This is the only honest answer to "how long was this person in a game". The
    /// client prewarms in the tray, and one log can start hours before it joins, so the
    /// length of the file is not play time. Zero when no server was ever reached.
    pub played_seconds: u64,
    /// The same join, in milliseconds. The client's own number is a float, and a
    /// healthy join is under a second, so the strip needs more than whole seconds.
    pub join_ms: Option<u64>,
    pub handshake: bool,
    pub adapter: Option<String>,
    pub api: Option<String>,
    pub vram_mb: Option<u64>,
    /// Every GPU the client enumerated, with the chosen one marked. This is what makes
    /// "the client is on the wrong card" observable instead of a guess.
    pub adapters: Vec<Adapter>,
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

/// The session in progress, if there is one.
///
/// The client writes one log per launch and keeps appending while it runs, so the
/// newest log with a fresh modification time is the session happening now. A log
/// untouched for half an hour is a finished one, and answering with it would put
/// yesterday's session on a strip that claims to be live.
pub fn live(data_root: &Path) -> Option<Receipt> {
    const FRESH: Duration = Duration::from_secs(30 * 60);

    let dir = logs_dir()?;
    let mut newest: Option<(SystemTime, PathBuf, String)> = None;

    for entry in fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        let Some(file) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file.ends_with(".log") || !file.contains("_Player_") || file.contains("CrashHandler") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        if newest
            .as_ref()
            .map(|(at, _, _)| modified > *at)
            .unwrap_or(true)
        {
            newest = Some((modified, path.clone(), file.to_string()));
        }
    }

    let (modified, path, file) = newest?;
    if modified.elapsed().unwrap_or_default() > FRESH {
        return None;
    }

    let mut receipt = parse(&path, &file)?;
    if let Some(place) = receipt.place {
        if let Some(name) = known_names(data_root).get(&place) {
            receipt.name = Some(name.clone());
        }
    }
    Some(receipt)
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
    let mut place = None;
    let mut universe = None;
    let mut job = None;
    let mut server = None;
    let mut endpoint = None;
    let mut join_ms: Option<u64> = None;
    let mut join_seconds: Option<u64> = None;
    let mut handshake = false;
    let mut adapter = None;
    let mut adapters: Vec<Adapter> = Vec::new();
    let mut api = None;
    let mut vram_mb = None;
    let mut shutdown_stage = None;
    let mut http_errors = 0u64;
    let mut last_http = None;
    let mut crash = false;
    let mut tray = false;
    let mut no_reload = false;
    let mut settings_failed = false;

    let mut first_stamp: Option<i64> = None;
    let mut last_stamp: Option<i64> = None;
    let mut joined_stamp: Option<i64> = None;

    for line in text.lines() {
        lines += 1;
        if let Some(stamp) = line_stamp(line) {
            first_stamp.get_or_insert(stamp);
            last_stamp = Some(stamp);
        }

        if let Some(start) = line.find("! Joining game '") {
            // The join line's own stamp is when play started, whatever the client was
            // doing before it.
            joined_stamp = joined_stamp.or_else(|| line_stamp(line));
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
                // Milliseconds, because a healthy join takes under a second: the client
                // writes a float here, and rounding it to whole seconds turned every fast
                // join into zero, which is a number nobody can show.
                join_ms = rest[..end]
                    .parse::<f64>()
                    .ok()
                    .map(|value| (value * 1000.0).round() as u64);
                join_seconds = join_ms.map(|ms| (ms + 500) / 1000);
            }
        }

        if line.contains("Handshake complete") {
            handshake = true;
            // `… [DFLog::RbxTransportRnaExpConnection] 128.116.5.33:63242. Handshake
            // complete.` The endpoint sits right before the sentence, and it is the only
            // place the client names the server that answered rather than a relay.
            if endpoint.is_none() {
                if let Some(at) = line.find(" Handshake complete") {
                    let token = line[..at]
                        .split_whitespace()
                        .last()
                        .unwrap_or("")
                        .trim_end_matches('.');
                    if is_endpoint(token) {
                        endpoint = Some(token.to_string());
                    }
                }
            }
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

        // `Adapter[0] info: Description=<name>, VendorId=..., DedicatedVideoMemory=<bytes>`
        if let Some(at) = line.find("Adapter[") {
            if line.contains(" info: ") {
                let rest = &line[at..];
                let name = rest
                    .find("Description=")
                    .map(|start| {
                        let tail = &rest[start + "Description=".len()..];
                        let end = tail.find(", VendorId=").unwrap_or(tail.len());
                        tail[..end].trim().to_string()
                    })
                    .unwrap_or_default();
                let vram = rest
                    .find("DedicatedVideoMemory=")
                    .and_then(|start| digits(&rest[start + "DedicatedVideoMemory=".len()..]))
                    .map(|bytes| bytes / (1024 * 1024))
                    .filter(|mb| *mb > 0);
                if !name.is_empty() {
                    adapters.push(Adapter {
                        name,
                        vram_mb: vram,
                        chosen: false,
                    });
                }
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

        // The bootstrapper's own no-reload guard: it found the app already running, wrote
        // this and destroyed itself in under a tenth of a second. Nothing was asked for
        // because there was nothing to start, not because a launch went wrong.
        if line.contains("NoReload Detector: App NoReload") {
            no_reload = true;
        }

        // Roblox's own updater starts a tray client that sits in the notification area
        // waiting for a launch it may never receive, and it writes a player log like any
        // other. Without this marker every sleepy updater is counted as a launch that
        // failed here, and a machine that played nothing all day reads as six failures.
        if line.contains("Tray treatment is ON")
            || line.contains("Systray initialized in")
            || line.contains("AppState/TrayMode")
        {
            tray = true;
        }

        // The client's first act is to fetch its flags from clientsettingscdn. Behind a
        // filter that breaks that TLS handshake the fetch fails, the client writes a few
        // kilobytes and exits without ever reading the launch URI, and from the outside
        // that is indistinguishable from a launch this app got wrong.
        if line.contains("Failed to download or apply critical settings")
            || line.contains("SslConnectFail")
        {
            settings_failed = true;
        }
    }

    // A failed request to the settings host is the same fact from the other side, and it
    // is the one network fault worth naming: nothing about this app or that place is wrong.
    let settings_failed = settings_failed
        || last_http
            .as_deref()
            .is_some_and(|last| last.contains("clientsettings"));

    if let Some(chosen) = &adapter {
        for item in adapters.iter_mut() {
            item.chosen = item.name == *chosen;
        }
    }

    // Wall clock from the log's own stamps. The comma field that used to be read here is
    // not seconds since this log started: Roblox writes lines from more than one clock
    // into the same file, and a twenty minute session carried values over 9,000 there.
    let seconds = match (first_stamp, last_stamp) {
        (Some(first), Some(last)) => (last - first).max(0) as u64,
        _ => 0,
    }
    .max(join_seconds.unwrap_or(0));

    let played_seconds = if handshake {
        match (joined_stamp, last_stamp) {
            (Some(joined), Some(last)) => (last - joined).max(0) as u64,
            _ => 0,
        }
    } else {
        0
    };
    let outcome = if crash {
        Outcome::Crashed
    } else if handshake {
        if shutdown_stage.is_some() {
            Outcome::Left
        } else {
            Outcome::Ended
        }
    } else if tray {
        Outcome::Tray
    } else if place.is_some() {
        Outcome::NoHandshake
    } else {
        Outcome::NeverJoined
    };

    // The duration is on the row already, so the sentence carries the reason instead
    // of repeating the number: say a fact once.
    let verdict = match outcome {
        Outcome::Left => "Left cleanly.".to_string(),
        Outcome::Ended => {
            "The log stops without a shutdown line, which is what closing the window looks like."
                .to_string()
        }
        Outcome::Tray => {
            "Roblox's own updater opened a tray client and left it waiting. Nothing was launched \
             from here."
                .to_string()
        }
        Outcome::NoHandshake if settings_failed => {
            "The client could not fetch its settings from Roblox's CDN, so no server was reached. \
             That is the network, not this app."
                .to_string()
        }
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
        Outcome::NeverJoined if settings_failed => {
            "The client stopped while fetching its settings from Roblox's CDN, before it read a \
             place. Nothing about that place is wrong."
                .to_string()
        }
        Outcome::NeverJoined if no_reload => {
            "Another client was already running, so Roblox's own no-reload guard made this one \
             step aside."
                .to_string()
        }
        Outcome::NeverJoined => {
            "The client opened but never asked for a place. Usually the bootstrapper handing over."
                .to_string()
        }
        Outcome::Crashed => "The client logged a crash while it was running.".to_string(),
    };

    let action = match outcome {
        Outcome::NoHandshake => Some("Check the network".to_string()),
        Outcome::NeverJoined if settings_failed => Some("Check the network".to_string()),
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
        endpoint,
        join_seconds,
        join_ms,
        played_seconds,
        handshake,
        adapter,
        api,
        vram_mb,
        adapters,
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

/// Whether a token is `<ipv4>:<port>`, which is the shape the handshake line carries.
/// A bare address is not one: the join line's relay address has no port and is not the
/// server, so it must not be mistaken for a measurement target.
fn is_endpoint(token: &str) -> bool {
    match token.rsplit_once(':') {
        Some((ip, port)) => ip.parse::<std::net::Ipv4Addr>().is_ok() && port.parse::<u16>().is_ok(),
        None => false,
    }
}

/// The relative-seconds field every log line carries as its second column.
/// The ISO stamp every line starts with, as seconds since the epoch.
///
/// `2026-11-18T20:15:33.957Z,13.9,...` The stamp is the only field in a Roblox log that
/// means the same thing on every line, which is why the session's length is measured
/// between the first and the last of them.
fn line_stamp(line: &str) -> Option<i64> {
    let head = line.get(..20)?;
    if head.as_bytes().get(10) != Some(&b'T') {
        return None;
    }

    let year: i64 = head.get(0..4)?.parse().ok()?;
    let month: i64 = head.get(5..7)?.parse().ok()?;
    let day: i64 = head.get(8..10)?.parse().ok()?;
    let hour: i64 = head.get(11..13)?.parse().ok()?;
    let minute: i64 = head.get(14..16)?.parse().ok()?;
    let second: i64 = head.get(17..19)?.parse().ok()?;

    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
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

/// Every place this machine has already resolved, newest first.
///
/// Read from disk only. The stage offers the last destinations it can name without a
/// network call, because the one thing a launcher's home should never do is wait on
/// Roblox to say what you played an hour ago.
pub fn known_places(root: &Path) -> Vec<(u64, u64, String, u64)> {
    let mut rows: Vec<(u64, u64, String, u64)> = read_places(root)
        .places
        .into_iter()
        .filter_map(|(place, resolved)| {
            place
                .parse()
                .ok()
                .map(|place| (place, resolved.universe, resolved.name, resolved.at))
        })
        .collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.3));
    rows
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

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kuro-places-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The stage offers the last destinations this machine already named, newest first,
    /// with no network call. A key that is not a number is dropped rather than shown.
    #[test]
    fn known_places_come_back_newest_first() {
        let dir = scratch("order");
        let json = r#"{"schema":1,"places":{
            "189707":{"universe":65241,"name":"Natural Disaster Survival","at":100},
            "10449761463":{"universe":3808081382,"name":"The Strongest Battlegrounds","at":300},
            "not-a-place":{"universe":1,"name":"Broken","at":900}
        }}"#;
        fs::write(dir.join("places.json"), json).unwrap();

        let rows = known_places(&dir);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, 10_449_761_463);
        assert_eq!(rows[0].2, "The Strongest Battlegrounds");
        assert_eq!(rows[1].0, 189_707);
        assert_eq!(rows[1].1, 65_241);

        let _ = fs::remove_dir_all(&dir);
    }

    /// A missing file is an empty list, not an error: a fresh install has to draw.
    #[test]
    fn no_places_file_is_no_destinations() {
        let dir = scratch("absent");
        assert!(known_places(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    const CLEAN: &str = "\
2026-11-18T16:13:44.389Z,0.389293,b668,6,Info [DFLog::Mimalloc] mi_option_reserve_os_memory=0
2026-11-18T16:13:44.581Z,0.581731,b668,6 [FLog::Graphics] Adapter[0] info: Description=NVIDIA GeForce RTX 5070 Ti, VendorId=0x10de, DeviceId=0x2c05, DedicatedVideoMemory=16771973120, DedicatedSystemMemory=0
2026-11-18T16:13:44.581Z,0.581731,b668,6 [FLog::Graphics] Adapter[1] info: Description=AMD Radeon(TM) Graphics, VendorId=0x1002, DeviceId=0x13c0, DedicatedVideoMemory=2119966720, DedicatedSystemMemory=0
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

    /// The updater's own tray client, taken from a real one: it fetches flags, opens the
    /// notification area and waits. It wrote 36 lines and asked for no place.
    const TRAY: &str = "\
2026-11-23T17:39:56.090Z,0.090495,57b8,6,Warning [FLog::RobloxStarterNetworkStarterModule] userAgent: Roblox/WinInetRobloxApp/0.748.0.7481526 (GlobalDist; RobloxDirectDownload) AppState/TrayMode
2026-11-23T17:39:56.170Z,0.170362,6f8c,6,Warning [FLog::FlagFetchingStarterModule] Successfully loaded flags
2026-11-23T17:39:56.189Z,0.189500,57b8,6,Warning [FLog::Systray] Tray treatment is ON)
2026-11-23T17:39:56.189Z,0.189500,57b8,6,Info [FLog::Systray] Systray initialized in sync mode
2026-11-23T17:39:56.189Z,0.189500,551c,6,Info [FLog::Systray] Starting background update timers
2026-11-23T17:39:56.467Z,0.467557,84ec,6,Info [FLog::Systray] No new version available, current version is 0.748.0.7481526
";

    /// A client that never got past its own flag fetch. Shape taken from the lines the
    /// client writes behind a filter that breaks TLS to clientsettingscdn.
    const NO_SETTINGS: &str = "\
2026-11-23T16:44:02.000Z,0.000000,1111,6,Warning [FLog::FlagFetchingStarterModule] Fetching flags from CDN, attempt: 0
2026-11-23T16:44:02.400Z,0.400000,1111,6,Error [FLog::FlagFetchingStarterModule] fetch flag exception: HttpError: SslConnectFail
2026-11-23T16:44:03.900Z,1.900000,1111,6,Error [FLog::RobloxStarter] Failed to download or apply critical settings
";

    /// A client that quit because one was already running. Eight lines, no place, and
    /// nothing wrong: the bootstrapper's own guard did it.
    const NO_RELOAD: &str = "\
2026-11-23T16:42:43.092Z,0.092839,3f38,6,Warning [FLog::RobloxStarter] Starting module: Logging
2026-11-23T16:42:43.094Z,0.094839,3f38,6,Warning [FLog::Output] NoReload Detector: warm start adjustment: 101
2026-11-23T16:42:43.094Z,0.094839,3f38,6,Warning [FLog::Output] NoReload Detector: App NoReload!
2026-11-23T16:42:43.095Z,0.095839,3f38,6,Warning [FLog::RobloxStarter] RobloxStarter destroyed
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
        assert_eq!(
            receipt.endpoint.as_deref(),
            Some("128.116.5.33:63242"),
            "the handshake line names the server that answered, not the relay"
        );
        assert!(receipt.handshake);
        assert_eq!(
            receipt.adapter.as_deref(),
            Some("NVIDIA GeForce RTX 5070 Ti")
        );
        assert_eq!(receipt.vram_mb, Some(15_995));
        assert_eq!(receipt.api.as_deref(), Some("D3D11.1"));
        assert_eq!(receipt.adapters.len(), 2);
        assert_eq!(receipt.adapters[0].name, "NVIDIA GeForce RTX 5070 Ti");
        assert_eq!(receipt.adapters[0].vram_mb, Some(15_995));
        assert!(receipt.adapters[0].chosen);
        assert_eq!(receipt.adapters[1].name, "AMD Radeon(TM) Graphics");
        assert_eq!(receipt.adapters[1].vram_mb, Some(2_021));
        assert!(!receipt.adapters[1].chosen);
        assert_eq!(receipt.shutdown_stage.as_deref(), Some("UGCGame"));
        assert_eq!(receipt.join_seconds, Some(117));
        assert_eq!(receipt.seconds, 402);
        assert_eq!(receipt.outcome, Outcome::Left);
        assert_eq!(receipt.verdict, "Left cleanly.");
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

    /// Roblox's updater opens a tray client on its own schedule. Counting that as a launch
    /// this app made put six failures on a page for an afternoon in which nothing failed.
    #[test]
    fn a_tray_client_is_not_a_failed_launch() {
        let dir = std::env::temp_dir().join("kuro-receipts-tray");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let name = "0.748.0.7481526_20261123T173956Z_Player_4798E_last.log";
        let path = write(&dir, name, TRAY);

        let receipt = parse(&path, name).expect("a receipt");
        assert_eq!(receipt.outcome, Outcome::Tray);
        assert_eq!(receipt.place, None);
        assert!(!receipt.handshake);
        assert!(receipt.verdict.contains("tray client"));
        assert!(receipt.verdict.contains("Nothing was launched from here"));
        assert_eq!(receipt.action, None, "there is nothing to fix");
    }

    /// The one network fault worth naming, because the remedy is the network: the client
    /// never read the launch URI at all, so no destination or account was ever refused.
    #[test]
    fn a_settings_fetch_failure_names_the_cdn_instead_of_the_place() {
        let dir = std::env::temp_dir().join("kuro-receipts-cdn");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let name = "0.748.0.7481526_20261123T164402Z_Player_11111_last.log";
        let path = write(&dir, name, NO_SETTINGS);

        let receipt = parse(&path, name).expect("a receipt");
        assert!(!receipt.handshake);
        assert_eq!(receipt.outcome, Outcome::NeverJoined);
        assert!(receipt.verdict.contains("settings from Roblox's CDN"));
        assert_eq!(receipt.action.as_deref(), Some("Check the network"));
    }

    #[test]
    fn a_client_that_stepped_aside_says_so_instead_of_blaming_a_handover() {
        let dir = std::env::temp_dir().join("kuro-receipts-noreload");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let name = "0.748.0.7481526_20261123T164243Z_Player_95CCC_last.log";
        let path = write(&dir, name, NO_RELOAD);

        let receipt = parse(&path, name).expect("a receipt");
        assert_eq!(receipt.outcome, Outcome::NeverJoined);
        assert!(!receipt.handshake);
        assert!(receipt.verdict.contains("no-reload guard made this one"));
        assert_eq!(receipt.action, None);
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
    fn an_endpoint_is_an_address_with_a_port_and_nothing_else() {
        assert!(is_endpoint("128.116.5.33:63242"));
        // The join address is a relay with no port, and measuring it would be measuring
        // the wrong machine.
        assert!(!is_endpoint("10.206.10.135"));
        assert!(!is_endpoint("128.116.5.33:"));
        assert!(!is_endpoint("scid=2a75dd38"));
        assert!(!is_endpoint(""));
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
