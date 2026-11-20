//! What to launch, read out of whatever was pasted.
//!
//! People arrive with four different strings: a bare place id, a game page URL,
//! a private server invite, or a `roblox://` deep link from Discord. All four
//! describe the same thing, so they are normalised here once and the rest of the
//! pipeline only ever sees a `Target`.
//!
//! Pure: no network, no filesystem. Everything in this file is covered by tests,
//! because a launch URI that is one character wrong opens the wrong game.

use serde::Serialize;

use crate::error::{FailureCode, LaunchError};

/// The five rows in the brief's table, minus `linkCode`, which is a parameter
/// any of them can carry rather than a destination of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Any server. Matchmaking decides.
    Normal,
    /// One exact server, when the id came from a server list or a share link.
    Server,
    /// A private server, with the code from the invite.
    Private,
    /// Follow a friend into whatever they are in.
    Follow,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Normal => "Any server",
            Mode::Server => "One server",
            Mode::Private => "Private server",
            Mode::Follow => "Follow a friend",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    place: Option<u64>,
    server: Option<String>,
    access: Option<String>,
    user: Option<u64>,
}

fn refused(detail: &str) -> LaunchError {
    LaunchError::new(
        FailureCode::NotFound,
        format!("That is not something this build can launch: {detail}"),
    )
    .with_action("Paste a game link or a place id")
}

impl Target {
    pub fn parse(raw: &str) -> Result<Self, LaunchError> {
        let text = raw.trim();
        if text.is_empty() {
            return Err(refused("it is empty"));
        }

        // A whole launch URI pasted back in, in either spelling the site uses: the
        // long `roblox-player:` one and the short `roblox:` one. Only its launcher URL
        // matters, and the prefix is not enough on its own: `roblox://placeId=1` starts
        // with the same word and is a deep link, so a launch string has to name a launch.
        let launch_string = ["roblox-player:", "roblox:"]
            .into_iter()
            .find_map(|prefix| text.strip_prefix(prefix))
            .filter(|rest| rest.contains("launchmode:") || rest.contains("placelauncherurl:"));

        if let Some(rest) = launch_string {
            if let Some(url) = rest
                .split('+')
                .find_map(|part| part.strip_prefix("placelauncherurl:"))
            {
                return Self::parse(&decode(url));
            }
            return Err(refused("it is a launch string with no destination in it"));
        }

        // A bare id is a place id. Universe ids are resolved by the screen that
        // asks Roblox for a game's own name, not guessed at here.
        if text.chars().all(|c| c.is_ascii_digit()) {
            let place = text.parse().map_err(|_| refused("that id is too large"))?;
            return Ok(Self {
                place: Some(place),
                server: None,
                access: None,
                user: None,
            });
        }

        // An explicit, unguessable form for a person who has one.
        if let Some(rest) = text.strip_prefix("user:") {
            let user = rest
                .trim()
                .parse()
                .map_err(|_| refused("that is not a user id"))?;
            return Ok(Self {
                place: None,
                server: None,
                access: None,
                user: Some(user),
            });
        }

        // `roblox://placeId=1818&gameInstanceId=..` carries its parameters with
        // no path and no question mark, so the scheme decides how it is split.
        let (path, query): (String, String) = match text.strip_prefix("roblox://") {
            Some(rest) => match rest.split_once('?') {
                Some((path, query)) => (path.to_string(), query.to_string()),
                None if rest.contains('=') => (String::new(), rest.to_string()),
                None => (rest.to_string(), String::new()),
            },
            None => match text.split_once('?') {
                Some((path, query)) => (path.to_string(), query.to_string()),
                None => (text.to_string(), String::new()),
            },
        };
        let params = parse_query(&query);

        let place = params
            .get("placeId")
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| place_in_path(&path));

        let server = params
            .get("gameInstanceId")
            .or_else(|| params.get("gameId"))
            .filter(|value| is_instance(value))
            .cloned();

        let access = params
            .get("privateServerLinkCode")
            .or_else(|| params.get("linkCode"))
            // The name `PlaceLauncher.ashx` itself uses for the same thing, which is the
            // one an intercepted site link carries.
            .or_else(|| params.get("accessCode"))
            .filter(|value| !value.is_empty())
            .cloned();

        let user = params
            .get("userId")
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| user_in_path(&path));

        if user.is_some() {
            return Ok(Self {
                place,
                server: None,
                access,
                user,
            });
        }

        let Some(place) = place else {
            return Err(refused("no place id is in there"));
        };

        Ok(Self {
            place: Some(place),
            server,
            access,
            user: None,
        })
    }

    pub fn mode(&self) -> Mode {
        if self.user.is_some() {
            Mode::Follow
        } else if self.access.is_some() {
            Mode::Private
        } else if self.server.is_some() {
            Mode::Server
        } else {
            Mode::Normal
        }
    }

    pub fn place(&self) -> Option<u64> {
        self.place
    }

    pub fn server(&self) -> Option<&str> {
        self.server.as_deref()
    }

    pub fn user(&self) -> Option<u64> {
        self.user
    }

    /// The PlaceLauncher URL this target resolves to.
    pub fn launcher_url(&self) -> String {
        let base = "https://assetgame.roblox.com/game/PlaceLauncher.ashx";

        match self.mode() {
            Mode::Follow => format!(
                "{base}?request=RequestFollowUser&userId={}&isPlayTogetherGame=false",
                self.user.unwrap_or_default()
            ),
            Mode::Private => format!(
                "{base}?request=RequestPrivateGame&placeId={}&accessCode={}&isPlayTogetherGame=false",
                self.place.unwrap_or_default(),
                self.access.clone().unwrap_or_default()
            ),
            Mode::Server => format!(
                "{base}?request=RequestGameJob&placeId={}&gameId={}&isPlayTogetherGame=false",
                self.place.unwrap_or_default(),
                self.server.clone().unwrap_or_default()
            ),
            Mode::Normal => format!(
                "{base}?request=RequestGame&placeId={}&isPlayTogetherGame=false",
                self.place.unwrap_or_default()
            ),
        }
    }
}

/// The launch string the client itself accepts.
///
/// `ShellExecuteW` hands this to the registered handler, and the handler calls
/// the player with it as the only argument. Nothing here is optional except the
/// locale pair, which is sent explicitly so a machine set to another language
/// still gets English game UI rather than a surprise.
pub fn launch_uri(ticket: &str, launcher: &str, now_ms: u128) -> String {
    let tracker = now_ms % 10_000_000_000;

    format!(
        "roblox-player:1+launchmode:play+gameinfo:{ticket}+launchtime:{now_ms}\
         +placelauncherurl:{}+browserTrackerId:{tracker}+roboat:1\
         +robloxLocale:en_US+gameLocale:en_US+channel:+LaunchExp:InApp",
        encode(launcher)
    )
}

/// Percent-encodes everything that is not unreserved. The launcher URL sits
/// inside a colon-and-plus delimited string, so `:`, `/`, `?`, `=` and `&` all
/// have to be escaped or the client reads them as separators.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).to_string()
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((key.to_string(), decode(value)))
        })
        .collect()
}

/// `/games/1818/Crossroads` and `roblox.com/games/1818` both carry the place.
fn place_in_path(path: &str) -> Option<u64> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let index = parts.iter().position(|part| *part == "games")?;
    parts.get(index + 1)?.parse().ok()
}

/// `/users/123/profile` is a friend, which the follow mode can use.
fn user_in_path(path: &str) -> Option<u64> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let index = parts.iter().position(|part| *part == "users")?;
    parts.get(index + 1)?.parse().ok()
}

/// A server id is a GUID-shaped string. Anything else in a `gameId` parameter is
/// not one, and passing it on would land in matchmaking while claiming to be an
/// exact server.
fn is_instance(value: &str) -> bool {
    value.len() >= 8
        && value.len() <= 64
        && value.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        && value.chars().any(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(input: &str) -> Target {
        Target::parse(input).expect("should parse")
    }

    #[test]
    fn takes_all_four_shapes_people_arrive_with() {
        assert_eq!(parsed("1818").place(), Some(1818));

        let game = parsed("https://www.roblox.com/games/1818/Crossroads");
        assert_eq!(game.place(), Some(1818));
        assert_eq!(game.mode(), Mode::Normal);

        let private = parsed(
            "https://www.roblox.com/games/1818/Crossroads?privateServerLinkCode=ABC123def456",
        );
        assert_eq!(private.mode(), Mode::Private);

        let server = parsed(
            "https://www.roblox.com/games/1818/Crossroads?gameInstanceId=2c0a4b1e-8f10-4d3a-9e2b-1a2b3c4d5e6f",
        );
        assert_eq!(server.mode(), Mode::Server);
        assert_eq!(
            server.server(),
            Some("2c0a4b1e-8f10-4d3a-9e2b-1a2b3c4d5e6f")
        );

        let deep = parsed("roblox://placeId=1818&gameInstanceId=abcdef0123456789");
        assert_eq!(deep.place(), Some(1818));
        assert_eq!(deep.mode(), Mode::Server);

        let follow = parsed("roblox://follow?userId=156");
        assert_eq!(follow.mode(), Mode::Follow);
        assert_eq!(follow.user(), Some(156));

        let profile = parsed("https://www.roblox.com/users/156/profile");
        assert_eq!(profile.mode(), Mode::Follow);
        assert_eq!(profile.user(), Some(156));
    }

    #[test]
    fn reads_a_launch_string_back_to_its_destination() {
        let launcher = "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestGame&placeId=1818&isPlayTogetherGame=false";
        let uri = launch_uri("TICKET", launcher, 1_700_000_000_000);
        let parsed = Target::parse(&uri).expect("a launch string is a target too");
        assert_eq!(parsed.place(), Some(1818));
        assert_eq!(parsed.mode(), Mode::Normal);
    }

    #[test]
    fn refuses_what_cannot_be_a_destination() {
        assert!(Target::parse("").is_err());
        assert!(Target::parse("https://www.roblox.com/home").is_err());
        assert!(Target::parse("hello").is_err());
    }

    #[test]
    fn builds_the_four_launcher_urls() {
        assert_eq!(
            parsed("1818").launcher_url(),
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestGame&placeId=1818&isPlayTogetherGame=false"
        );
        assert_eq!(
            parsed("roblox://placeId=1818&gameInstanceId=abcdef0123456789").launcher_url(),
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestGameJob&placeId=1818&gameId=abcdef0123456789&isPlayTogetherGame=false"
        );
        assert_eq!(
            parsed("https://www.roblox.com/games/1818/x?privateServerLinkCode=CODE").launcher_url(),
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestPrivateGame&placeId=1818&accessCode=CODE&isPlayTogetherGame=false"
        );
        assert_eq!(
            parsed("roblox://follow?userId=156").launcher_url(),
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestFollowUser&userId=156&isPlayTogetherGame=false"
        );
    }

    #[test]
    fn builds_the_launch_string_the_client_expects() {
        let uri = launch_uri(
            "TICKET",
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestGame&placeId=1818",
            1_700_000_000_123,
        );

        assert!(uri.starts_with("roblox-player:1+launchmode:play"));
        assert!(uri.contains("+gameinfo:TICKET"));
        assert!(uri.contains("+launchtime:1700000000123"));
        assert!(uri.contains("placelauncherurl:https%3A%2F%2Fassetgame.roblox.com%2Fgame%2FPlaceLauncher.ashx%3Frequest%3DRequestGame%26placeId%3D1818"));
        assert!(uri.contains("+roboat:1"));
        assert!(uri.contains("+robloxLocale:en_US"));
        assert!(uri.ends_with("+LaunchExp:InApp"));
        // The separator characters must not survive inside the encoded value.
        let encoded = uri
            .split("placelauncherurl:")
            .nth(1)
            .and_then(|rest| rest.split("+browserTrackerId:").next())
            .unwrap();
        assert!(!encoded.contains('&'));
        assert!(!encoded.contains(':'));
    }

    #[test]
    fn round_trips_percent_encoding() {
        let text = "https://a.example/b?c=d&e=f g";
        let encoded = encode(text);
        assert_eq!(encoded, "https%3A%2F%2Fa.example%2Fb%3Fc%3Dd%26e%3Df%20g");
        assert_eq!(decode(&encoded), text);
    }
}
