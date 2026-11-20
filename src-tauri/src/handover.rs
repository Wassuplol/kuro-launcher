//! Roblox's own launch links, taken over and given back.
//!
//! `roblox-player:` and `roblox://` are what roblox.com hands Windows when somebody
//! presses Play. Roblox writes both into `HKCU\Software\Classes` and re-points them at
//! whichever version folder it installed last, so a launcher that wants to be *the*
//! launcher has to do three things: remember what was there, put itself in its place, and
//! be able to put the original back.
//!
//! Three rules shape this module.
//!
//! - **Nothing is deleted that was not ours.** The displaced command is copied into
//!   `HKCU\Software\Kuro Launcher\Schemes` before anything is written, and that copy is what
//!   "Give it back" restores and what the uninstaller reads after the data folder is gone.
//! - **A link this app cannot read still has to end somewhere.** It is handed to the handler
//!   that was there before, so a click always does what Roblox would have done rather than
//!   opening a window that says it could not parse something.
//! - **Studio keeps its links.** `launchmode:edit` and the `roblox-studio*` schemes are not
//!   taken: an edit launch opens the place editor, which this app has no business owning.

use std::path::Path;

use windows::core::{w, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::error::{FailureCode, LaunchError};
use crate::launch::target::Target;
use crate::links;

/// The two schemes roblox.com hands Windows for a game. Studio's two are deliberately not
/// here.
pub const SCHEMES: [&str; 2] = ["roblox-player", "roblox"];

/// The label Windows shows for the scheme once this app owns it.
const LABEL: &str = "URL:Roblox launch link, opened by KURO";

/// The remembered value that means "there was no handler here before". A distinct marker
/// rather than an empty string, because the uninstaller has to tell "this app took over a
/// scheme nobody owned" (delete it) from "this app never touched the scheme" (leave it).
pub const NONE: &str = "-";

/// One scheme, as the settings row needs it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scheme {
    pub scheme: String,
    /// What Windows would run right now, if anything.
    pub current: Option<String>,
    /// The command this app displaced, once it has taken the scheme.
    pub previous: Option<String>,
    /// Whether the current command runs the executable that asked.
    pub ours: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub exe: String,
    pub schemes: Vec<Scheme>,
    /// Something worth saying once: a takeover that Roblox overwrote and this app took
    /// back, or a link that had to be handed to Roblox's own handler.
    pub note: Option<String>,
}

pub fn state(exe: &Path, note: Option<String>) -> State {
    let schemes = SCHEMES
        .iter()
        .map(|scheme| {
            let current = links::state(scheme);
            Scheme {
                scheme: scheme.to_string(),
                ours: runs(current.as_deref(), exe),
                previous: links::remembered(scheme).filter(|value| value != NONE),
                current,
            }
        })
        .collect();

    State {
        exe: exe.to_string_lossy().to_string(),
        schemes,
        note,
    }
}

/// Whether one command string starts this executable.
pub fn runs(command: Option<&str>, exe: &Path) -> bool {
    let Some(command) = command else {
        return false;
    };
    match exe_of(command) {
        Some(path) => path.eq_ignore_ascii_case(&exe.to_string_lossy()),
        None => false,
    }
}

/// The executable a Windows command value names. Roblox writes `"C:\…\RobloxPlayerBeta.exe"
/// %1`, and a person or an installer may hand this app the same shape, so the quoted form is
/// read first and an unquoted first token is the fallback rather than the other way round.
pub fn exe_of(command: &str) -> Option<String> {
    let text = command.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(rest) = text.strip_prefix('"') {
        return rest.split('"').next().map(str::to_string);
    }
    text.split_whitespace().next().map(str::to_string)
}

/// Takes both schemes for this executable, remembering what was there first.
///
/// A scheme this app already owns is left alone: re-copying it would replace the remembered
/// original with this app's own command, and "Give it back" would then have nothing to give.
pub fn take(exe: &Path) -> Result<Vec<Scheme>, LaunchError> {
    for scheme in SCHEMES {
        let current = links::state(scheme);
        if !runs(current.as_deref(), exe) && links::remembered(scheme).is_none() {
            // `NONE` is written as well as a command, so the uninstaller can tell a scheme
            // this app took from one it never touched.
            links::remember(scheme, current.as_deref().unwrap_or(NONE));
        }
        links::write_scheme(
            scheme,
            &format!("\"{}\" \"%1\"", exe.to_string_lossy()),
            LABEL,
        )?;
    }
    Ok(state(exe, None).schemes)
}

/// Puts the displaced handlers back.
///
/// `fallback` is the Roblox client this machine would run today. It is only used when the
/// remembered path is gone from disk, because Roblox removes old version folders and a
/// restored command naming a deleted file is a link that opens nothing.
pub fn give_back(fallback: Option<&Path>) -> Result<Vec<Scheme>, LaunchError> {
    for scheme in SCHEMES {
        // Never taken by this app means never touched: the honest action is to leave the
        // value exactly as Windows has it.
        let Some(previous) = links::remembered(scheme) else {
            continue;
        };

        if previous == NONE {
            links::unregister(scheme)?;
            links::forget_remembered(scheme);
            continue;
        }

        let remembered_exe = exe_of(&previous).filter(|path| Path::new(path).is_file());
        match (remembered_exe, fallback) {
            (Some(_), _) => links::write_scheme(scheme, &previous, "URL:Roblox launch link")?,
            (None, Some(fallback)) => links::write_scheme(
                scheme,
                &format!("\"{}\" %1", fallback.to_string_lossy()),
                "URL:Roblox launch link",
            )?,
            (None, None) => links::unregister(scheme)?,
        }
        links::forget_remembered(scheme);
    }

    let exe = std::env::current_exe().unwrap_or_default();
    Ok(state(&exe, None).schemes)
}

/// Writes this app back into every scheme it owns whose command changed underneath it.
///
/// Roblox's installer re-registers its own schemes on update, which silently returns every
/// game link to the stock launcher. Re-asserting on start is what makes the takeover survive
/// an update instead of quietly ending at the next one.
pub fn reassert(exe: &Path) -> Result<Vec<String>, LaunchError> {
    let mut rewritten = Vec::new();
    for scheme in SCHEMES {
        if links::remembered(scheme).is_none() {
            continue;
        }
        if runs(links::state(scheme).as_deref(), exe) {
            continue;
        }
        links::write_scheme(
            scheme,
            &format!("\"{}\" \"%1\"", exe.to_string_lossy()),
            LABEL,
        )?;
        rewritten.push((*scheme).to_string());
    }
    Ok(rewritten)
}

/// Hands a link to the handler that was there before this app took the scheme.
///
/// The URI is passed as one argument, never through `cmd /c start`, because a shell
/// re-parses the `&` characters inside `placelauncherurl` and opens the wrong thing.
pub fn forward(raw: &str) -> Result<(), LaunchError> {
    let previous = SCHEMES
        .iter()
        .find_map(|scheme| links::remembered(scheme))
        .and_then(|command| exe_of(&command))
        .filter(|exe| Path::new(exe).is_file());

    match previous {
        Some(exe) => {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;

            std::process::Command::new(&exe)
                .arg(raw)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map(|_| ())
                .map_err(|e| {
                    LaunchError::new(
                        FailureCode::ClientMissing,
                        format!("Roblox's own launcher at {exe} would not start: {e}"),
                    )
                })
        }
        None => open(raw),
    }
}

fn open(raw: &str) -> Result<(), LaunchError> {
    let uri = wide(raw);
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(uri.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW answers above 32 on success; anything at or below it is the failure
    // code cast through a handle-shaped return, which is what this API does.
    if result.0 as usize > 32 {
        return Ok(());
    }
    Err(LaunchError::new(
        FailureCode::Internal,
        format!(
            "Windows would not open that link (ShellExecute code {}).",
            result.0 as usize
        ),
    ))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// What a command line turned out to hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    /// A `kuro://` party link. The page reads it as it always has.
    Party(links::Party),
    /// A Roblox link with a destination this app can read.
    Launch(String),
    /// A Roblox link it will not own, and why, so the caller can hand it over.
    Other { raw: String, why: String },
}

/// The first argument that is a link of either family.
pub fn incoming(args: &[String]) -> Option<Incoming> {
    args.iter().find_map(|arg| classify(arg.trim()))
}

fn classify(raw: &str) -> Option<Incoming> {
    if raw.is_empty() {
        return None;
    }

    if let Some(party) = links::parse(raw) {
        return Some(Incoming::Party(party));
    }

    let lower = raw.to_ascii_lowercase();
    let native = lower.starts_with("roblox-player:")
        || lower.starts_with("roblox:")
        || lower.starts_with("roblox://");
    if !native {
        return None;
    }

    // An edit launch opens the place editor. Studio's links were never taken, and a game
    // launcher has no business pretending to be one.
    if let Some(mode) = launch_mode(raw) {
        if !mode.eq_ignore_ascii_case("play") {
            return Some(Incoming::Other {
                raw: raw.to_string(),
                why: format!("it asks for {mode} mode, not a game"),
            });
        }
    }

    match Target::parse(raw) {
        Ok(_) => Some(Incoming::Launch(raw.to_string())),
        Err(error) => Some(Incoming::Other {
            raw: raw.to_string(),
            why: error.message,
        }),
    }
}

fn launch_mode(raw: &str) -> Option<String> {
    raw.split('+').find_map(|part| {
        part.strip_prefix("launchmode:")
            .map(|value| value.trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play_link() -> String {
        "roblox-player:1+launchmode:play+gameinfo:ABCDEF+launchtime:1700000000000\
         +placelauncherurl:https%3A%2F%2Fassetgame.roblox.com%2Fgame%2FPlaceLauncher.ashx%3Frequest%3DRequestGameJob%26placeId%3D189707%26gameId%3D2c0a4b1e-8f10-4d3a-9e2b-1a2b3c4d5e6f%26isPlayTogetherGame%3Dfalse\
         +browsertrackerid:12345+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp"
            .to_string()
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn a_play_link_is_a_launch_and_it_carries_the_exact_server() {
        let incoming = incoming(&args(&["kuro-launcher.exe", &play_link()])).unwrap();
        match incoming {
            Incoming::Launch(raw) => {
                let target = Target::parse(&raw).unwrap();
                assert_eq!(target.place(), Some(189_707));
                assert_eq!(
                    target.server(),
                    Some("2c0a4b1e-8f10-4d3a-9e2b-1a2b3c4d5e6f")
                );
                assert_eq!(target.mode(), crate::launch::Mode::Server);
            }
            other => panic!("a play link should be a launch, got {other:?}"),
        }
    }

    #[test]
    fn an_edit_link_is_left_to_roblox() {
        let incoming = incoming(&args(&[
            "roblox-player:1+launchmode:edit+gameinfo:X+placelauncherurl:https%3A%2F%2Fassetgame.roblox.com%2F",
        ]))
        .unwrap();
        assert!(matches!(incoming, Incoming::Other { .. }));
    }

    #[test]
    fn a_studio_scheme_is_not_ours() {
        assert_eq!(
            incoming(&args(&["roblox-studio:1+launchmode:edit+placeId:1818"])),
            None
        );
    }

    #[test]
    fn a_deep_link_with_a_place_is_a_launch() {
        let incoming = incoming(&args(&["roblox://placeId=1818&gameInstanceId=abc"])).unwrap();
        assert!(matches!(incoming, Incoming::Launch(_)));
    }

    #[test]
    fn a_deep_link_with_nothing_in_it_is_handed_over_rather_than_guessed() {
        let incoming = incoming(&args(&["roblox://profile"])).unwrap();
        assert!(matches!(incoming, Incoming::Other { .. }));
    }

    #[test]
    fn a_party_link_stays_a_party_link() {
        let incoming = incoming(&args(&["kuro://join?place=1818"])).unwrap();
        assert_eq!(
            incoming,
            Incoming::Party(links::Party {
                place: Some(1818),
                server: None,
                account: None,
            })
        );
    }

    #[test]
    fn the_executable_is_read_out_of_a_quoted_command() {
        assert_eq!(
            exe_of(r#""C:\Program Files\Roblox\Versions\version-abc\RobloxPlayerBeta.exe" %1"#)
                .as_deref(),
            Some(r"C:\Program Files\Roblox\Versions\version-abc\RobloxPlayerBeta.exe")
        );
        assert_eq!(
            exe_of(r"C:\Roblox\RobloxPlayerLauncher.exe %1").as_deref(),
            Some(r"C:\Roblox\RobloxPlayerLauncher.exe")
        );
        assert_eq!(exe_of("   "), None);
    }

    #[test]
    fn ours_compares_the_path_not_the_whole_command() {
        let exe = Path::new(r"C:\Apps\kuro-launcher.exe");
        assert!(runs(Some(r#""C:\Apps\kuro-launcher.exe" "%1""#), exe));
        assert!(runs(Some(r#""c:\apps\KURO-launcher.EXE" "%1""#), exe));
        assert!(!runs(Some(r#""C:\Other\kuro-launcher.exe" "%1""#), exe));
        assert!(!runs(None, exe));
    }
}
