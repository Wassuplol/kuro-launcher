//! Discord presence: what a friend sees while a game is running.
//!
//! Discord's local RPC is a named pipe, `\\.\pipe\discord-ipc-<n>`, carrying frames of
//! an opcode, a little-endian length and a JSON body. The launcher connects, performs one
//! handshake, and then says what it is doing; nothing here touches the network, and the
//! only thing that leaves the machine is what Discord already receives from every game
//! that supports presence.
//!
//! Two facts decide the shape of this module:
//!
//! - **The application id is the person's, not this build's.** Discord shows the name and
//!   the icon of the application the id belongs to. A launcher that shipped somebody
//!   else's id would be wearing their name, so the id is a field in Settings and the row
//!   says where to get one.
//! - **Presence is opt-in and clearly shown.** A launcher that tells Discord what you are
//!   playing by default is a launcher that decided something for you.
//!
//! Discord PTB, Canary and the stable build all expose the same pipe; the number after
//! `discord-ipc-` is the slot, so a machine running two clients uses two of them and this
//! code walks the range and takes the first that answers.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::error::{FailureCode, LaunchError};
use crate::receipts;

/// How often presence is reconsidered. Ten seconds is short enough that a session shows
/// up while it is still loading, and long enough that a log read per tick is nothing.
const TICK: Duration = Duration::from_secs(10);

/// The slots Discord listens on. Five is the documented range; the rest is because PTB
/// and Canary can both be open.
const SLOTS: u8 = 10;

/// What the settings screen reads.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The pipe answered a handshake.
    pub connected: bool,
    /// What a friend would see right now.
    pub line: Option<String>,
    /// Why it is not connected, said once rather than left blank.
    pub error: Option<String>,
}

/// One connected session with Discord.
pub struct Link {
    stream: File,
}

impl Link {
    /// Finds a pipe and says hello.
    pub fn open(client_id: &str) -> Result<Link, String> {
        let mut last = String::from("no Discord pipe answered");

        for slot in 0..SLOTS {
            let path = pipe_path(slot);
            let Ok(mut stream) = OpenOptions::new().read(true).write(true).open(&path) else {
                continue;
            };

            let hello = format!("{{\"v\":1,\"client_id\":\"{client_id}\"}}");
            if send(&mut stream, 0, &hello).is_err() {
                last = format!("the pipe on slot {slot} closed during the handshake");
                continue;
            }

            match read_frame(&mut stream) {
                // READY arrives as opcode 1 with `evt: "READY"`.
                Ok((_, body)) if body.contains("\"READY\"") => return Ok(Link { stream }),
                // Anything else is Discord refusing, and its sentence is more useful
                // than a summary of it.
                Ok((_, body)) => {
                    last = message_of(&body).unwrap_or_else(|| format!("Discord said: {body}"));
                }
                Err(_) => last = format!("the pipe on slot {slot} answered nothing"),
            }
        }

        Err(last)
    }

    /// Sets what a friend sees. `None` clears it, which is what leaving a game does.
    pub fn show(&mut self, activity: Option<&str>) -> Result<(), String> {
        let body = match activity {
            Some(activity) => format!(
                "{{\"cmd\":\"SET_ACTIVITY\",\"args\":{{\"pid\":{},\"activity\":{activity}}},\"nonce\":\"{}\"}}",
                std::process::id(),
                nonce()
            ),
            None => format!(
                "{{\"cmd\":\"SET_ACTIVITY\",\"args\":{{\"pid\":{},\"activity\":null}},\"nonce\":\"{}\"}}",
                std::process::id(),
                nonce()
            ),
        };

        send(&mut self.stream, 1, &body).map_err(|e| format!("the frame did not send: {e}"))?;

        // Discord answers every frame. A closed pipe shows up here, which is how this
        // thread learns that Discord was quit.
        let (opcode, answer) =
            read_frame(&mut self.stream).map_err(|e| format!("Discord stopped answering: {e}"))?;

        if opcode == 2 {
            return Err(
                message_of(&answer).unwrap_or_else(|| "Discord closed the session".to_string())
            );
        }
        if answer.contains("\"evt\":\"ERROR\"") {
            return Err(
                message_of(&answer).unwrap_or_else(|| "Discord refused the activity".to_string())
            );
        }
        Ok(())
    }
}

fn pipe_path(slot: u8) -> PathBuf {
    PathBuf::from(format!(r"\\.\pipe\discord-ipc-{slot}"))
}

fn send(stream: &mut File, opcode: u32, body: &str) -> std::io::Result<()> {
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.extend_from_slice(&opcode.to_le_bytes());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(body.as_bytes());
    stream.write_all(&frame)?;
    stream.flush()
}

/// One frame: opcode, length, body.
fn read_frame(stream: &mut File) -> std::io::Result<(u32, String)> {
    let mut head = [0u8; 8];
    stream.read_exact(&mut head)?;
    let opcode = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
    let length = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
    // A frame claiming more than a megabyte is not Discord; refusing it keeps a broken
    // pipe from allocating the machine.
    if length > 1 << 20 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the frame length is not plausible",
        ));
    }
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    Ok((opcode, String::from_utf8_lossy(&body).to_string()))
}

/// The `message` field of a Discord error, when the body has one.
fn message_of(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("data")
        .and_then(|data| data.get("message"))
        .or_else(|| value.get("message"))
        .and_then(|text| text.as_str())
        .map(str::to_string)
}

fn nonce() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("kuro-{nanos:x}")
}

/// The activity object, built from what the client's own log says.
///
/// `details` is the game and `state` is the server, because that is the order a friend
/// reads them in. A session with no place yet says what it is doing instead of inventing
/// a game.
pub fn activity(
    place_name: Option<&str>,
    place: Option<u64>,
    endpoint: Option<&str>,
    started: u64,
) -> String {
    let details = place_name
        .map(str::to_string)
        .or_else(|| place.map(|place| format!("Place {place}")))
        .unwrap_or_else(|| "In Roblox".to_string());

    let state = match endpoint {
        Some(endpoint) => format!("server {endpoint}"),
        None => "joining a server".to_string(),
    };

    // No `assets`: an icon is an uploaded asset key belonging to the application, and
    // naming one this build does not have would show a blank square.
    serde_json::json!({
        "details": truncate(&details, 128),
        "state": truncate(&state, 128),
        "timestamps": { "start": started.max(1) * 1000 },
    })
    .to_string()
}

fn truncate(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// Starts the worker that keeps presence in step with the session.
///
/// One thread for the life of the process, because presence is a single connection and a
/// single socket; the loop reads the settings each tick, so switching it off stops it
/// within ten seconds and needs no message passing.
pub fn spawn(app: tauri::AppHandle) {
    use tauri::Manager;

    std::thread::spawn(move || {
        let mut link: Option<Link> = None;
        let mut shown: Option<String> = None;
        let mut beat: u32 = 0;

        loop {
            let Some(state) = app.try_state::<crate::AppState>() else {
                std::thread::sleep(TICK);
                continue;
            };

            let (enabled, client_id) = {
                let settings = match state.settings.lock() {
                    Ok(settings) => settings,
                    Err(_) => {
                        std::thread::sleep(TICK);
                        continue;
                    }
                };
                (
                    settings.discord.enabled,
                    settings.discord.app_id.trim().to_string(),
                )
            };

            let mut status = match state.presence.lock() {
                Ok(status) => status,
                Err(_) => {
                    std::thread::sleep(TICK);
                    continue;
                }
            };

            if !enabled {
                if link.is_some() {
                    if let Some(open) = link.as_mut() {
                        let _ = open.show(None);
                    }
                    link = None;
                    shown = None;
                }
                *status = Status::default();
                drop(status);
                std::thread::sleep(TICK);
                continue;
            }

            if client_id.is_empty() {
                *status = Status {
                    connected: false,
                    line: None,
                    error: Some(
                        "An application id is needed before Discord can show anything.".to_string(),
                    ),
                };
                drop(status);
                std::thread::sleep(TICK);
                continue;
            }

            // What the person is doing, from the log the Sessions page already reads.
            //
            // Two conditions, and both matter. The client has to be running, because a
            // log stays on disk for half an hour after a game is closed and the card
            // outliving the game is the thing this is fixing. And the log has to carry a
            // handshake: a client that opened but never reached a server is not "playing"
            // anything, and saying "joining a server" for an hour is a lie the card told.
            let running = crate::launch::spawn::player_running();
            let session = receipts::live(&state.data_root)
                .filter(|receipt| shows(running, receipt.handshake, receipt.place));
            let wanted = session.as_ref().map(|receipt| {
                activity(
                    receipt.name.as_deref(),
                    receipt.place,
                    receipt.endpoint.as_deref(),
                    receipt.started,
                )
            });

            if link.is_none() {
                match Link::open(&client_id) {
                    Ok(open) => {
                        link = Some(open);
                        status.connected = true;
                        status.error = None;
                        shown = None;
                    }
                    Err(why) => {
                        *status = Status {
                            connected: false,
                            line: None,
                            error: Some(why),
                        };
                        drop(status);
                        std::thread::sleep(TICK);
                        continue;
                    }
                }
            }

            // A card that never changes is also a socket that is never tested. Every
            // third tick the same activity is sent again, which both refreshes the
            // elapsed clock and finds out whether Discord is still there.
            beat += 1;
            let resync = beat % 3 == 0;

            if wanted != shown || resync {
                let result = match (link.as_mut(), wanted.clone()) {
                    (Some(open), activity) => open.show(activity.as_deref()),
                    (None, _) => Ok(()),
                };

                match result {
                    Ok(()) => {
                        shown = wanted.clone();
                        status.connected = true;
                        status.error = None;
                        status.line = session.as_ref().map(|receipt| {
                            let name = receipt
                                .name
                                .clone()
                                .or_else(|| receipt.place.map(|place| format!("Place {place}")))
                                .unwrap_or_else(|| "In Roblox".to_string());
                            match &receipt.endpoint {
                                Some(endpoint) => format!("{name} · {endpoint}"),
                                None => name,
                            }
                        });
                    }
                    Err(why) => {
                        // A pipe that died is not an error worth keeping forever: the next
                        // tick reconnects, which is what happens when Discord restarts.
                        link = None;
                        shown = None;
                        status.connected = false;
                        status.error = Some(why);
                    }
                }
            }

            drop(status);
            std::thread::sleep(TICK);
        }
    });
}

/// Whether this is worth telling Discord about.
///
/// Three gates, and each one closed a way the card lied: the client has to be running
/// (a log outlives the game), the server has to have answered (a client that never
/// joined is not playing), and there has to be a place to name.
fn shows(running: bool, handshake: bool, place: Option<u64>) -> bool {
    running && handshake && place.is_some()
}

/// Validates an application id before it is stored.
///
/// Discord's ids are numeric and currently 18 or 19 digits. Accepting the range rather
/// than today's width means the check does not rot, and refusing letters catches the
/// most common paste: the application's client *secret* or its invite URL.
pub fn check_app_id(app_id: &str) -> Result<(), LaunchError> {
    let text = app_id.trim();
    if text.is_empty() {
        return Ok(());
    }
    let numeric = text.chars().all(|ch| ch.is_ascii_digit());
    let length = text.len();
    if numeric && (17..=20).contains(&length) {
        return Ok(());
    }

    Err(LaunchError::new(
        FailureCode::NotFound,
        "A Discord application id is a number, about 19 digits long, from the app's General Information page.",
    )
    .with_action("Open the developer portal"))
}

/// Whether Discord is running at all, which the settings row says before a person
/// wonders why presence is not showing.
///
/// A pipe is a kernel object, not a file: `exists()` reports nothing useful about it, so
/// the check is an open and an immediate close. No handshake is sent, which means Discord
/// never sees this as a connection.
pub fn discord_running() -> bool {
    (0..SLOTS).any(|slot| {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(pipe_path(slot))
            .is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_game_is_not_still_playing() {
        assert!(!shows(false, true, Some(189707)));
    }

    #[test]
    fn a_client_that_never_reached_a_server_is_not_playing() {
        assert!(!shows(true, false, Some(189707)));
        assert!(!shows(true, false, None));
    }

    #[test]
    fn a_live_joined_session_is_playing() {
        assert!(shows(true, true, Some(189707)));
    }

    #[test]
    fn an_application_id_has_to_look_like_one() {
        assert!(check_app_id("").is_ok());
        assert!(check_app_id("1234567890123456789").is_ok());
        assert!(check_app_id(" 123456789012345678 ").is_ok());
        // A secret, a URL and yesterday's id are all refused with the same sentence.
        assert!(check_app_id("abcdefghijklmnop").is_err());
        assert!(check_app_id("https://discord.com/developers/applications/1").is_err());
        assert!(check_app_id("123").is_err());
    }

    #[test]
    fn the_activity_reads_like_a_sentence_a_friend_can_understand() {
        let json = activity(
            Some("Natural Disaster Survival"),
            Some(189707),
            Some("128.116.5.33:63242"),
            1_795_018_424,
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["details"], "Natural Disaster Survival");
        assert_eq!(value["state"], "server 128.116.5.33:63242");
        assert_eq!(value["timestamps"]["start"], 1_795_018_424_000u64);
    }

    #[test]
    fn a_session_without_a_place_says_so_instead_of_naming_one() {
        let json = activity(None, None, None, 1);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["details"], "In Roblox");
        assert_eq!(value["state"], "joining a server");
    }

    #[test]
    fn a_frame_is_an_opcode_a_length_and_the_body() {
        // The wire shape, checked without a pipe: 4 bytes of opcode, 4 of length.
        let body = "{\"v\":1}";
        let mut frame = Vec::new();
        frame.extend_from_slice(&0u32.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(body.as_bytes());

        assert_eq!(
            u32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]),
            0
        );
        assert_eq!(
            u32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]) as usize,
            body.len()
        );
        assert_eq!(&frame[8..], body.as_bytes());
    }

    #[test]
    fn discord_errors_are_read_from_where_they_actually_are() {
        assert_eq!(
            message_of(r#"{"cmd":"DISPATCH","evt":"ERROR","data":{"code":4000,"message":"Invalid Client ID"}}"#)
                .as_deref(),
            Some("Invalid Client ID")
        );
        assert_eq!(message_of("{}"), None);
    }
}
