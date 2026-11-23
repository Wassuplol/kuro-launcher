//! The friend list, and what those friends are doing right now.
//!
//! Four endpoints, four budgets, all measured on 2026-11-18 rather than assumed:
//!
//! - `friends.roblox.com/v1/users/{id}/friends` **needs the session**: anonymously it
//!   answers 403 `Request Context Failure`, and with the cookie it answers
//!   `x-ratelimit-limit: 20, 20;w=60`. Twenty a minute is the tightest budget in the
//!   app, so the id list is cached for two minutes.
//! - That answer carries **ids and empty names**: 200 entries, every `name` and
//!   `displayName` an empty string, and no next cursor. Names come from
//!   `users.roblox.com/v1/users` (POST, 100 ids a call, `100, 1000;w=1`).
//! - `thumbnails.roblox.com/v1/users/avatar-headshot` takes 100 ids a call at 500 a
//!   minute, which is cheap enough that every row has a face.
//! - `presence.roblox.com/v1/presence/users` (POST) accepts **one id anonymously** and
//!   **50 without a session**, then answers 400 `Too many User Ids being sent in the
//!   request`; with the session cookie it takes **200 per call**. That ceiling is why
//!   the cookie is passed here even though the single-user case works without it.
//!   For somebody in a game it returns the game's name in `lastLocation`, its
//!   `placeId`, its `universeId` and — the useful one — a `gameId` that is the exact
//!   server, so "Join" can put a person in their friend's server rather than in a
//!   random one.
//!
//! Presence is deliberately not cached: it is the part that changes. Everything else
//! is, because nothing else does.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::discovery::{Budget, Cache, Class};
use crate::error::{FailureCode, LaunchError};
use crate::http;

const FRIENDS: &str = "https://friends.roblox.com/v1/users/";
const USERS: &str = "https://users.roblox.com/v1/users";
const PRESENCE: &str = "https://presence.roblox.com/v1/presence/users";
const HEADS: &str = "https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds=";

/// Measured: 100 ids are accepted by `users` and by `avatar-headshot`.
const PER_CALL: usize = 100;
/// Measured: 200 ids are accepted by `presence/users` **with the session**, and 50
/// without one. The cookie is not optional here, so neither is the chunk.
const PRESENCE_PER_CALL: usize = 200;
/// The friend list allows twenty calls a minute; two minutes of cache keeps a page
/// that is opened and closed from spending that.
const FRIENDS_TTL: Duration = Duration::from_secs(120);

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|seconds| seconds.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Deserialize)]
struct RawFriend {
    id: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFriends {
    #[serde(default)]
    data: Vec<RawFriend>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUser {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    has_verified_badge: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RawUsers {
    #[serde(default)]
    data: Vec<RawUser>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPresence {
    user_id: u64,
    #[serde(default)]
    user_presence_type: u8,
    #[serde(default)]
    last_location: String,
    #[serde(default)]
    place_id: Option<u64>,
    #[serde(default)]
    universe_id: Option<u64>,
    /// The exact server, which is what a join into a friend's game needs.
    #[serde(default)]
    game_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPresences {
    #[serde(default)]
    user_presences: Vec<RawPresence>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawThumb {
    #[serde(default)]
    target_id: u64,
    #[serde(default)]
    image_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawThumbs {
    #[serde(default)]
    data: Vec<RawThumb>,
}

/// What one friend is doing, in the four states Roblox publishes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    /// 0 offline, 1 online, 2 in a game, 3 in Studio. Kept as the wire number so the
    /// screen can sort on it without parsing a label.
    pub kind: u8,
    pub label: String,
    pub place: Option<u64>,
    pub universe: Option<u64>,
    /// The exact server, when they are in one.
    pub server: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Friend {
    pub user: u64,
    pub name: String,
    pub display: String,
    pub verified: bool,
    pub headshot: Option<String>,
    pub presence: Presence,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Roster {
    pub friends: Vec<Friend>,
    pub in_game: usize,
    pub online: usize,
    pub offline: usize,
    pub fetched: u64,
    pub cached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn status(presence: Option<&RawPresence>) -> Presence {
    let Some(presence) = presence else {
        return Presence {
            kind: 0,
            label: "Offline".to_string(),
            place: None,
            universe: None,
            server: None,
        };
    };

    let label = match presence.user_presence_type {
        2 => {
            if presence.last_location.trim().is_empty() {
                "In a game".to_string()
            } else {
                format!("In {}", presence.last_location.trim())
            }
        }
        3 => "In Studio".to_string(),
        1 => {
            if presence.last_location.trim().is_empty() || presence.last_location == "Website" {
                "Online".to_string()
            } else {
                format!("Online · {}", presence.last_location.trim())
            }
        }
        _ => "Offline".to_string(),
    };

    let in_game = presence.user_presence_type == 2;
    Presence {
        kind: presence.user_presence_type,
        label,
        place: if in_game { presence.place_id } else { None },
        universe: if in_game { presence.universe_id } else { None },
        server: if in_game {
            presence.game_id.clone()
        } else {
            None
        },
    }
}

/// The ids on this account's friend list, cached for two minutes because the endpoint
/// allows twenty calls a minute and a page that re-mounts must not spend them.
fn friend_ids(
    budget: &Budget,
    cache: &Cache,
    cookie: &str,
    user: u64,
) -> Result<(Vec<u64>, bool), LaunchError> {
    let key = format!("friends:{user}");
    if let Some(body) = cache.get(&key) {
        if let Ok(parsed) = serde_json::from_str::<RawFriends>(&body) {
            return Ok((
                parsed.data.into_iter().map(|friend| friend.id).collect(),
                true,
            ));
        }
    }

    budget.spend(Class::Friends)?;
    let url = format!("{FRIENDS}{user}/friends?limit=200&sortOrder=Asc");
    let answer = http::agent(Duration::from_secs(20))
        .get(&url)
        .header("Cookie", &format!(".ROBLOSECURITY={cookie}"))
        .call()
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("The friend list could not be read: {e}."),
            )
            .with_action("Retry")
        })?;

    let status = answer.status().as_u16();
    let body = answer.into_body().read_to_string().unwrap_or_default();

    if status == 401 || status == 403 {
        return Err(LaunchError::new(
            FailureCode::SessionExpired,
            format!(
                "The friend list needs a signed-in session and Roblox answered {status}. \
                 That is a session problem, not a friend problem."
            ),
        )
        .with_action("Check the account"));
    }
    if status >= 400 {
        return Err(LaunchError::new(
            FailureCode::Network,
            format!("The friend list answered {status}: {body:.160}"),
        )
        .with_action("Retry"));
    }

    let parsed: RawFriends = serde_json::from_str(&body).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The friend list was not the documented shape: {e}."),
        )
    })?;

    cache.put(&key, &body, FRIENDS_TTL);
    Ok((
        parsed.data.into_iter().map(|friend| friend.id).collect(),
        false,
    ))
}

/// Names, in batches of a hundred. A name is cached for ten minutes: handles change
/// rarely and this is the second of the two calls that would otherwise repeat.
fn names(
    budget: &Budget,
    cache: &Cache,
    cookie: &str,
    ids: &[u64],
) -> Result<BTreeMap<u64, RawUser>, LaunchError> {
    let mut out = BTreeMap::new();
    let mut missing = Vec::new();

    for id in ids {
        match cache.get(&format!("uname:{id}")) {
            Some(body) => {
                if let Ok(user) = serde_json::from_str::<RawUser>(&body) {
                    out.insert(*id, user);
                }
            }
            None => missing.push(*id),
        }
    }
    if missing.is_empty() {
        return Ok(out);
    }

    // One batch first even when there are more: if the answer is refused the page
    // still draws, because presence and headshots do not depend on names.
    for chunk in missing.chunks(PER_CALL) {
        budget.spend(Class::Users)?;
        let payload = serde_json::json!({ "userIds": chunk, "excludeBannedUsers": false });
        let answer = http::post_json(
            USERS,
            &payload.to_string(),
            &[
                ("Cookie", &format!(".ROBLOSECURITY={cookie}")),
                ("Content-Type", "application/json"),
            ],
        )?;

        if !answer.ok() {
            return Err(LaunchError::new(
                FailureCode::Network,
                format!(
                    "Names answered {}: {}",
                    answer.status,
                    answer.body.chars().take(160).collect::<String>()
                ),
            )
            .with_action("Retry"));
        }

        let parsed: RawUsers = serde_json::from_str(&answer.body).map_err(|e| {
            LaunchError::new(
                FailureCode::Internal,
                format!("Names were not the documented shape: {e}."),
            )
        })?;

        for user in parsed.data {
            cache.put(
                &format!("uname:{}", user.id),
                &serde_json::to_string(&user).unwrap_or_default(),
                Duration::from_secs(600),
            );
            out.insert(user.id, user);
        }
    }

    Ok(out)
}

/// Faces. Cheap (500 a minute) and cached per id, so a second visit costs nothing.
fn headshots(budget: &Budget, cache: &Cache, ids: &[u64]) -> BTreeMap<u64, String> {
    let mut out = BTreeMap::new();
    let mut missing = Vec::new();

    for id in ids {
        match cache.get(&format!("uhead:{id}")) {
            Some(url) => {
                out.insert(*id, url);
            }
            None => missing.push(*id),
        }
    }
    if missing.is_empty() {
        return out;
    }

    for chunk in missing.chunks(PER_CALL) {
        if budget.spend(Class::Thumbs).is_err() {
            // A missing face is not a missing row: the list draws without it.
            return out;
        }
        let url = format!(
            "{HEADS}{}&size=100x100&format=Png&isCircular=true",
            chunk
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        let Ok(answer) = http::get(&url) else {
            return out;
        };
        if !answer.ok() {
            return out;
        }
        let Ok(parsed) = serde_json::from_str::<RawThumbs>(&answer.body) else {
            return out;
        };
        for thumb in parsed.data {
            if thumb.image_url.is_empty() {
                continue;
            }
            cache.put(
                &format!("uhead:{}", thumb.target_id),
                &thumb.image_url,
                Duration::from_secs(600),
            );
            out.insert(thumb.target_id, thumb.image_url);
        }
    }

    out
}

/// Presence, fresh every time. The cookie is sent because it is what raises the batch
/// ceiling from 50 to 200, measured rather than assumed.
fn presence(
    budget: &Budget,
    cookie: &str,
    ids: &[u64],
) -> Result<BTreeMap<u64, RawPresence>, LaunchError> {
    let mut out = BTreeMap::new();

    for chunk in ids.chunks(PRESENCE_PER_CALL) {
        budget.spend(Class::Presence)?;
        let payload = serde_json::json!({ "userIds": chunk });
        let answer = http::post_json(
            PRESENCE,
            &payload.to_string(),
            &[
                ("Content-Type", "application/json"),
                ("Cookie", &format!(".ROBLOSECURITY={cookie}")),
            ],
        )?;
        if !answer.ok() {
            return Err(LaunchError::new(
                FailureCode::Network,
                format!(
                    "Presence answered {}: {}",
                    answer.status,
                    answer.body.chars().take(160).collect::<String>()
                ),
            )
            .with_action("Retry"));
        }
        let parsed: RawPresences = serde_json::from_str(&answer.body).map_err(|e| {
            LaunchError::new(
                FailureCode::Internal,
                format!("Presence was not the documented shape: {e}."),
            )
        })?;
        for entry in parsed.user_presences {
            out.insert(entry.user_id, entry);
        }
    }

    Ok(out)
}

/// The roster: names, faces, and what everyone is doing, in that order of reliability.
pub fn roster(
    budget: &Budget,
    cache: &Cache,
    cookie: &str,
    user: u64,
) -> Result<Roster, LaunchError> {
    let (ids, cached) = friend_ids(budget, cache, cookie, user)?;
    let mut note = None;

    let users = match names(budget, cache, cookie, &ids) {
        Ok(users) => users,
        Err(failure) => {
            note = Some(failure.message);
            BTreeMap::new()
        }
    };
    let faces = headshots(budget, cache, &ids);
    let statuses = presence(budget, cookie, &ids)?;

    // A batch that answers for fewer ids than were sent is a fault worth saying out
    // loud: the alternative is a page where everyone reads as offline, which looks
    // like a fact about the friends rather than a failure of the parser.
    if statuses.len() < ids.len() {
        note = Some(match note {
            Some(existing) => format!(
                "{existing} Presence answered for {} of {} friends.",
                statuses.len(),
                ids.len()
            ),
            None => format!(
                "Presence answered for {} of {} friends.",
                statuses.len(),
                ids.len()
            ),
        });
    }

    let mut friends: Vec<Friend> = ids
        .iter()
        .map(|id| {
            let user = users.get(id);
            let presence = status(statuses.get(id));
            Friend {
                user: *id,
                name: user.map(|user| user.name.clone()).unwrap_or_default(),
                display: user
                    .map(|user| user.display_name.clone())
                    .filter(|display| !display.is_empty())
                    .unwrap_or_else(|| format!("User {id}")),
                verified: user.map(|user| user.has_verified_badge).unwrap_or(false),
                headshot: faces.get(id).cloned(),
                presence,
            }
        })
        .collect();

    // In a game first, then online, then the rest; alphabetical inside each group so
    // the list does not reshuffle every time somebody's presence changes.
    friends.sort_by(|a, b| {
        let rank = |friend: &Friend| match friend.presence.kind {
            2 => 0,
            3 => 1,
            1 => 2,
            _ => 3,
        };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.display.to_lowercase().cmp(&b.display.to_lowercase()))
    });

    let in_game = friends
        .iter()
        .filter(|friend| friend.presence.kind == 2)
        .count();
    let online = friends
        .iter()
        .filter(|friend| friend.presence.kind == 1)
        .count();
    let offline = friends.len() - in_game - online;

    Ok(Roster {
        friends,
        in_game,
        online,
        offline,
        fetched: now(),
        cached,
        note,
    })
}

// ---------------------------------------------------------------------------
// The stage
// ---------------------------------------------------------------------------

/// The full figure rather than the face, for a pad.
///
/// Measured 2026-11-23: `users/avatar` takes 100 ids a call at the same 500 a minute as
/// the headshot, and answers a **720 square PNG with a real alpha channel**: the figure
/// fills 66% of the width and 85% of the height and every corner is transparent. That
/// transparency is what lets a pad carry a person on a lit pool instead of a photograph
/// in a box.
const BODIES: &str = "https://thumbnails.roblox.com/v1/users/avatar?userIds=";

/// An outfit change moves the avatar, and the stage's one claim is that this is the
/// figure Roblox renders now, so a signed URL is trusted for an hour rather than for the
/// thirty days it stays valid.
const BODY_TTL: Duration = Duration::from_secs(3_600);

/// One account this machine can start, as the stage needs it.
#[derive(Debug, Clone, Default)]
pub struct Seat {
    pub user: u64,
    pub name: String,
    pub display: String,
    /// `ok`, `expired` or `unknown`, from the last health check.
    pub health: String,
    pub default: bool,
}

/// A figure standing on a pad.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pad {
    pub user: u64,
    pub name: String,
    pub display: String,
    /// The full figure, transparent. Absent when the thumbnail did not answer, which is
    /// drawn as a pad with no one on it rather than as a broken row.
    pub body: Option<String>,
    pub headshot: Option<String>,
    pub presence: Presence,
    pub health: String,
    pub default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageView {
    pub pads: Vec<Pad>,
    pub fetched: u64,
    /// True when the presence batch answered at all. `false` is not "everybody is
    /// offline": it means nobody knows, and the page says so instead of printing a
    /// status it does not have.
    pub presence: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn bodies(budget: &Budget, cache: &Cache, ids: &[u64]) -> BTreeMap<u64, String> {
    let mut out = BTreeMap::new();
    let mut missing = Vec::new();

    for id in ids {
        match cache.get(&format!("ubody:{id}")) {
            Some(url) => {
                out.insert(*id, url);
            }
            None => missing.push(*id),
        }
    }
    if missing.is_empty() {
        return out;
    }

    for chunk in missing.chunks(PER_CALL) {
        if budget.spend(Class::Thumbs).is_err() {
            // A missing figure is a lit pad with a name on it, not a missing pad.
            return out;
        }
        let url = format!(
            "{BODIES}{}&size=720x720&format=Png&isCircular=false",
            chunk
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        let Ok(answer) = http::get(&url) else {
            return out;
        };
        if !answer.ok() {
            return out;
        }
        let Ok(parsed) = serde_json::from_str::<RawThumbs>(&answer.body) else {
            return out;
        };
        for thumb in parsed.data {
            if thumb.image_url.is_empty() {
                continue;
            }
            cache.put(
                &format!("ubody:{}", thumb.target_id),
                &thumb.image_url,
                BODY_TTL,
            );
            out.insert(thumb.target_id, thumb.image_url);
        }
    }

    out
}

/// The pads: every account on this machine, its figure, and what it is doing.
///
/// Presence is one call for the whole batch and needs a session, so the session is
/// optional here. Without it the pads still draw, from the cached figures and the health
/// the last check wrote, and the note says why no pad knows whether it is free. A stage
/// that refused to draw because something else failed would be the wrong trade for the
/// page whose whole job is showing who is standing there.
pub fn stage(budget: &Budget, cache: &Cache, cookie: Option<&str>, seats: &[Seat]) -> StageView {
    if seats.is_empty() {
        return StageView {
            pads: Vec::new(),
            fetched: now(),
            presence: false,
            note: Some("There is no account on this machine to stand on a pad.".to_string()),
        };
    }

    let ids: Vec<u64> = seats.iter().map(|seat| seat.user).collect();
    let figures = bodies(budget, cache, &ids);
    let faces = headshots(budget, cache, &ids);

    let (statuses, mut note, answered) = match cookie {
        Some(cookie) => match presence(budget, cookie, &ids) {
            Ok(statuses) => (statuses, None, true),
            // The pads draw anyway; the sentence says which half is missing.
            Err(failure) => (BTreeMap::new(), Some(failure.message), false),
        },
        None => (
            BTreeMap::new(),
            Some("No account answered a session, so the pads cannot say who is free.".to_string()),
            false,
        ),
    };

    let pads: Vec<Pad> = seats
        .iter()
        .map(|seat| Pad {
            user: seat.user,
            name: seat.name.clone(),
            display: seat.display.clone(),
            body: figures.get(&seat.user).cloned(),
            headshot: faces.get(&seat.user).cloned(),
            presence: status(statuses.get(&seat.user)),
            health: seat.health.clone(),
            default: seat.default,
        })
        .collect();

    // A batch that answers for fewer ids than were sent is a fault worth naming. The
    // alternative is a page where a live account reads as offline, which looks like a
    // fact about the account rather than a failure of the parser.
    if note.is_none() && statuses.len() < ids.len() {
        note = Some(format!(
            "Presence answered for {} of {} pads, so some of them cannot say whether they are free.",
            statuses.len(),
            ids.len()
        ));
    }

    StageView {
        pads,
        fetched: now(),
        presence: answered,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(user: u64) -> Seat {
        Seat {
            user,
            name: format!("dummy{user}"),
            display: format!("Pad {user}"),
            health: "ok".to_string(),
            default: user == 1,
        }
    }

    /// The stage must draw with no network at all: the local account store is the only
    /// thing it is allowed to need. Every figure here is pre-seeded into the cache, which
    /// is exactly the state a second visit to the page is in.
    #[test]
    fn a_pad_draws_without_a_session() {
        let cache = Cache::default();
        cache.put("ubody:1", "https://cdn/one.png", Duration::from_secs(60));
        cache.put(
            "uhead:1",
            "https://cdn/one-face.png",
            Duration::from_secs(60),
        );

        let view = stage(&Budget::default(), &cache, None, &[seat(1)]);

        assert_eq!(view.pads.len(), 1);
        let pad = &view.pads[0];
        assert_eq!(pad.display, "Pad 1");
        assert_eq!(pad.body.as_deref(), Some("https://cdn/one.png"));
        assert_eq!(pad.headshot.as_deref(), Some("https://cdn/one-face.png"));
        // No session means no presence, and the page is told why rather than shown a
        // pad that reads as offline.
        assert_eq!(pad.presence.kind, 0);
        assert!(view.note.is_some());
    }

    /// A machine with no account gets a sentence, not a failed command: the page that
    /// shows this also offers the button that fixes it.
    #[test]
    fn an_empty_stage_says_so() {
        let view = stage(&Budget::default(), &Cache::default(), None, &[]);
        assert!(view.pads.is_empty());
        assert!(!view.presence);
        assert!(view.note.unwrap().contains("no account"));
    }

    /// Health travels through untouched, because the page decides whether a pad starts
    /// on the light or off it from this value alone.
    #[test]
    fn health_reaches_the_page() {
        let cache = Cache::default();
        cache.put("ubody:7", "https://cdn/seven.png", Duration::from_secs(60));
        let mut expired = seat(7);
        expired.health = "expired".to_string();

        let view = stage(&Budget::default(), &cache, None, &[expired]);
        assert_eq!(view.pads[0].health, "expired");
    }
}
