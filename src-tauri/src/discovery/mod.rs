//! Stage 06, discovery: what Roblox is serving, and the budget it takes to ask.
//!
//! Four facts shape this module, all measured on 2026-11-18 rather than assumed:
//!
//! - The home feed is `POST discovery-api/omni-recommendation` with the session
//!   cookie. It answers six sorts and about 250 KB, and **each item is only an
//!   id**: no name, no thumbnail, no player count. A grid therefore costs one
//!   feed call plus one hydration call per fifty ids.
//! - `games?universeIds=` refuses 100 ids with `400 Too many universe IDs were
//!   requested.` and accepts 50, so the cap is a constant here, not a guess.
//! - Search is the tightest budget on the whole surface: `search-api/omni-search`
//!   answers `x-ratelimit-limit: 4, 4;w=1`, four calls a minute. A search box
//!   that queries per keystroke spends that in one word.
//! - Thumbnails are effectively free by comparison (1500 a minute for game icons,
//!   500 for headshots), so the grid may be as visual as it likes.
//!
//! So there are two budgets and one cache, and nothing here issues a request that
//! the budget has not paid for. A refused call is a named error with the reason,
//! never a silent empty grid that reads as "Roblox has no games".

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{FailureCode, LaunchError};
use crate::http;

const FEED: &str = "https://apis.roblox.com/discovery-api/omni-recommendation";
const SEARCH: &str = "https://apis.roblox.com/search-api/omni-search";
const GAMES: &str = "https://games.roblox.com/v1/games?universeIds=";
const ICONS: &str = "https://thumbnails.roblox.com/v1/games/icons?universeIds=";
const HEADS: &str = "https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds=";
const SITE: &str = "https://www.roblox.com";

/// Measured: 50 is accepted, 100 answers `Too many universe IDs were requested`.
const GAMES_PER_CALL: usize = 50;
/// How many ids one section of the feed is allowed to show.
const PER_SECTION: usize = 20;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|seconds| seconds.as_secs())
        .unwrap_or(0)
}

// ---- the budget ----

/// The endpoint classes, each with its own published ceiling per minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    Feed,
    Search,
    Games,
    Servers,
    Thumbs,
    Library,
    Friends,
    Presence,
    Users,
}

impl Class {
    /// `x-ratelimit-limit`, measured. The window on all four is 60 seconds.
    fn per_minute(self) -> usize {
        match self {
            Class::Feed => 30,
            Class::Search => 4,
            Class::Games => 60,
            // Measured: the public server list is the most expensive call on the
            // surface, three a minute. A page that fetches it on mount spends the
            // minute before anyone asks for it.
            Class::Servers => 3,
            Class::Thumbs => 500,
            // Measured on `favorite/games`: `x-ratelimit-limit: 200, 200;w=60`.
            Class::Library => 200,
            // Measured on `friends`: `x-ratelimit-limit: 20, 20;w=60, 20;w=60`. The
            // tightest budget in the app, tighter than search, which is why the friend
            // list is cached for two minutes and only presence is asked again.
            Class::Friends => 20,
            // Measured on `presence/users`: `60, 60;w=60, 100;w=60`.
            Class::Presence => 60,
            // Measured on `users` (POST, the name hydrator): `100, 1000;w=1`.
            Class::Users => 600,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Class::Feed => "the home feed",
            Class::Search => "search",
            Class::Games => "game details",
            Class::Servers => "the server list",
            Class::Thumbs => "thumbnails",
            Class::Library => "your favourites",
            Class::Friends => "your friend list",
            Class::Presence => "presence",
            Class::Users => "names",
        }
    }
}

/// A sliding-minute allowance per class.
///
/// Keyed per class rather than per host on purpose: `games.roblox.com` serves both
/// a 60 a minute call and a 3 a minute one, and one host-wide bucket would throttle
/// the cheap call to the expensive one's ceiling.
#[derive(Default)]
pub struct Budget {
    calls: Mutex<HashMap<Class, Vec<Instant>>>,
}

impl Budget {
    /// Spends one allowance, or explains why not. The refusal is local and costs
    /// nothing: it is what keeps a rate-limited endpoint from being hammered into
    /// a longer ban.
    pub(crate) fn spend(&self, class: Class) -> Result<(), LaunchError> {
        let window = Duration::from_secs(60);
        let mut guard = self.calls.lock().map_err(|_| {
            LaunchError::new(FailureCode::Internal, "The request budget was poisoned.")
        })?;
        let list = guard.entry(class).or_default();
        list.retain(|at| at.elapsed() < window);

        if list.len() >= class.per_minute() {
            let oldest = list.first().copied().unwrap_or_else(Instant::now);
            let wait = window.saturating_sub(oldest.elapsed()).as_secs().max(1);
            return Err(LaunchError::new(
                FailureCode::TicketThrottled,
                format!(
                    "{} allows {} requests a minute and that is spent. About {} seconds to go.",
                    class.name(),
                    class.per_minute(),
                    wait
                ),
            )
            .with_action("Wait a moment"));
        }

        list.push(Instant::now());
        Ok(())
    }

    /// What the budget looks like right now, for a screen that wants to say so.
    pub fn head_room(&self, classes: &[Class]) -> Vec<(String, usize, usize)> {
        let window = Duration::from_secs(60);
        let guard = self.calls.lock().ok();
        classes
            .iter()
            .map(|class| {
                let used = guard
                    .as_ref()
                    .and_then(|map| map.get(class))
                    .map(|list| list.iter().filter(|at| at.elapsed() < window).count())
                    .unwrap_or(0);
                (class.name().to_string(), used, class.per_minute())
            })
            .collect()
    }
}

// ---- the cache ----

/// Answers kept for a few minutes, keyed by the request that produced them.
///
/// The feed is 250 KB and changes slowly; search at four calls a minute cannot be
/// asked twice for the same word. Caching is what makes the budget survivable
/// rather than a decoration.
#[derive(Default)]
pub struct Cache {
    entries: Mutex<HashMap<String, (Instant, String)>>,
}

impl Cache {
    pub(crate) fn get(&self, key: &str) -> Option<String> {
        let guard = self.entries.lock().ok()?;
        let (expires, value) = guard.get(key)?;
        if expires.elapsed() > Duration::ZERO {
            return None;
        }
        Some(value.clone())
    }

    pub(crate) fn put(&self, key: &str, value: &str, ttl: Duration) {
        if let Ok(mut guard) = self.entries.lock() {
            guard.insert(key.to_string(), (Instant::now() + ttl, value.to_string()));
        }
    }
}

// ---- the shapes on the wire ----

#[derive(Deserialize)]
struct RawFeed {
    #[serde(default)]
    sorts: Vec<RawSort>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSort {
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    treatment_type: Option<String>,
    #[serde(default)]
    recommendation_list: Option<Vec<RawItem>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawItem {
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    content_id: Option<u64>,
}

#[derive(Deserialize)]
struct RawGames {
    #[serde(default)]
    data: Vec<RawGame>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawGame {
    id: u64,
    #[serde(default)]
    root_place_id: Option<u64>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    playing: u64,
    #[serde(default)]
    creator: Option<RawCreator>,
    /// Present and non-zero only for paid access. `games?universeIds=` carries it.
    #[serde(default)]
    price: Option<u64>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawCreator {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct RawThumbs {
    #[serde(default)]
    data: Vec<RawThumb>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawThumb {
    target_id: u64,
    #[serde(default)]
    image_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSearch {
    #[serde(default)]
    search_results: Vec<RawGroup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGroup {
    #[serde(default)]
    contents: Vec<RawResult>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawResult {
    #[serde(default)]
    universe_id: Option<u64>,
    #[serde(default)]
    root_place_id: Option<u64>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    player_count: Option<u64>,
    #[serde(default)]
    creator_name: Option<String>,
    #[serde(default)]
    content_id: Option<u64>,
    #[serde(default)]
    has_verified_badge: bool,
}

#[derive(Deserialize)]
struct RawOne {
    #[serde(default)]
    data: Vec<RawDetail>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDetail {
    id: u64,
    #[serde(default)]
    price: Option<u64>,
    #[serde(default)]
    root_place_id: Option<u64>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    creator: Option<RawCreator>,
    #[serde(default)]
    playing: u64,
    #[serde(default)]
    visits: u64,
    #[serde(default)]
    max_players: u64,
    #[serde(default)]
    genre: String,
    #[serde(default)]
    created: String,
    #[serde(default)]
    updated: String,
}

#[derive(Deserialize)]
struct RawVotes {
    #[serde(default)]
    data: Vec<RawVote>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVote {
    #[serde(default)]
    up_votes: u64,
    #[serde(default)]
    down_votes: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawServers {
    #[serde(default)]
    data: Vec<RawServer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawServer {
    #[serde(default)]
    id: String,
    #[serde(default)]
    playing: u32,
    #[serde(default)]
    max_players: u32,
    #[serde(default)]
    ping: u32,
    #[serde(default)]
    fps: f32,
}

// ---- the shapes on screen ----

/// What hydration adds to an id: the words and numbers the tile needs.
#[derive(Debug, Clone)]
struct Detail {
    name: String,
    creator: String,
    place: u64,
    players: u64,
    price: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCard {
    pub universe: u64,
    pub place: u64,
    pub name: String,
    pub players: u64,
    pub creator: String,
    pub icon: Option<String>,
    /// Paid access in Robux. `None` or zero is free, which most of them are.
    pub price: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub topic: String,
    pub treatment: String,
    pub games: Vec<GameCard>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feed {
    pub sections: Vec<Section>,
    pub fetched: u64,
    /// True when the answer came from the cache, which the screen says rather
    /// than pretending it is live.
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub user: u64,
    pub name: String,
    pub display: String,
    pub verified: bool,
    pub headshot: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDetail {
    pub universe: u64,
    pub place: u64,
    pub name: String,
    pub creator: String,
    pub description: String,
    pub players: u64,
    pub visits: u64,
    pub max_players: u64,
    pub genre: String,
    pub created: String,
    pub updated: String,
    pub up_votes: u64,
    pub down_votes: u64,
    pub icon: Option<String>,
    /// Paid access in Robux, when the game has one.
    pub price: Option<u64>,
    pub fetched: u64,
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    /// The instance id a launch needs, handed to `roblox://…&gameInstanceId=`.
    pub id: String,
    pub playing: u32,
    pub max_players: u32,
    pub ping: u32,
    pub fps: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Servers {
    pub place: u64,
    pub list: Vec<Server>,
    pub fetched: u64,
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    pub games: Vec<GameCard>,
    pub people: Vec<Person>,
    pub fetched: u64,
    pub cached: bool,
}

// ---- calls ----

fn session_error(answer: &http::Answer) -> LaunchError {
    LaunchError::new(
        FailureCode::SessionExpired,
        format!(
            "Roblox will not answer without a session: {}. The home feed is the one page that needs one.",
            answer.reason()
        ),
    )
    .with_action("Check the account")
}

fn network_error(e: LaunchError) -> LaunchError {
    LaunchError::new(
        FailureCode::Network,
        format!("Roblox could not be reached for discovery: {}.", e.message),
    )
    .with_action("Retry")
}

/// The feed, hydrated: sections of game cards with names and icons.
pub fn home(budget: &Budget, cache: &Cache, cookie: Option<&str>) -> Result<Feed, LaunchError> {
    let Some(cookie) = cookie else {
        return Err(LaunchError::new(
            FailureCode::SessionExpired,
            "The home feed is built for the signed-in account, and this machine has none.",
        )
        .with_action("Add an account"));
    };

    let cached = cache.get("feed").is_some();
    let body = match cache.get("feed") {
        Some(body) => body,
        None => {
            budget.spend(Class::Feed)?;
            let header = format!(".ROBLOSECURITY={cookie}");
            let request = serde_json::json!({
                "pageToken": "",
                "sessionId": uuid_like(),
                "device": "computer",
            });
            let answer = http::post_json(
                FEED,
                &request.to_string(),
                &[("Cookie", header.as_str()), ("Referer", SITE)],
            )
            .map_err(network_error)?;

            if !answer.ok() {
                return Err(session_error(&answer));
            }
            cache.put("feed", &answer.body, Duration::from_secs(120));
            answer.body
        }
    };

    let raw: RawFeed = serde_json::from_str(&body).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The home feed was not the shape Roblox documents: {e}."),
        )
    })?;

    // Collect the ids per section, in order, without asking for more than the
    // screen will show.
    let mut wanted: Vec<u64> = Vec::new();
    let mut sections: Vec<Section> = Vec::new();
    for sort in raw.sorts {
        let Some(topic) = sort.topic.filter(|topic| !topic.trim().is_empty()) else {
            continue;
        };
        let ids: Vec<u64> = sort
            .recommendation_list
            .unwrap_or_default()
            .into_iter()
            .filter(|item| item.content_type.as_deref() == Some("Game"))
            .filter_map(|item| item.content_id)
            .take(PER_SECTION)
            .collect();
        if ids.is_empty() {
            continue;
        }
        for id in &ids {
            if !wanted.contains(id) {
                wanted.push(*id);
            }
        }
        sections.push(Section {
            topic,
            treatment: sort.treatment_type.unwrap_or_default(),
            games: ids
                .into_iter()
                .map(|universe| GameCard {
                    universe,
                    place: universe,
                    name: String::new(),
                    players: 0,
                    creator: String::new(),
                    icon: None,
                    price: None,
                })
                .collect(),
        });
    }

    // One hydration call per fifty ids, and the icons in parallel with it.
    let details = hydrate(budget, cache, &wanted, Some(cookie))?;
    let icons = icons(budget, cache, &wanted)?;

    for section in &mut sections {
        for card in &mut section.games {
            if let Some(found) = details.get(&card.universe) {
                card.name = found.name.clone();
                card.creator = found.creator.clone();
                card.place = found.place;
                card.players = found.players;
                card.price = found.price;
            }
            card.icon = icons.get(&card.universe).cloned();
        }
    }

    Ok(Feed {
        sections,
        fetched: now(),
        cached,
    })
}

/// Names, player counts and creators for a list of universe ids.
fn hydrate(
    budget: &Budget,
    cache: &Cache,
    ids: &[u64],
    cookie: Option<&str>,
) -> Result<BTreeMap<u64, Detail>, LaunchError> {
    let mut out = BTreeMap::new();
    let mut missing: Vec<u64> = Vec::new();

    for id in ids {
        match cache.get(&format!("game:{id}")) {
            Some(body) => {
                if let Ok(game) = serde_json::from_str::<RawGame>(&body) {
                    out.insert(*id, detail_of(&game));
                }
            }
            None => missing.push(*id),
        }
    }

    for chunk in missing.chunks(GAMES_PER_CALL) {
        budget.spend(Class::Games)?;
        let url = format!(
            "{GAMES}{}",
            chunk
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut request = http::agent(Duration::from_secs(15)).get(&url);
        if let Some(cookie) = cookie {
            request = request.header("Cookie", &format!(".ROBLOSECURITY={cookie}"));
        }
        let answer = request.call().map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("Game details could not be read: {e}."),
            )
        })?;
        let status = answer.status().as_u16();
        let body = answer.into_body().read_to_string().unwrap_or_default();
        if status >= 400 {
            return Err(LaunchError::new(
                FailureCode::Network,
                format!("Game details answered {status}: {body:.160}"),
            )
            .with_action("Retry"));
        }
        let parsed: RawGames = serde_json::from_str(&body).map_err(|e| {
            LaunchError::new(
                FailureCode::Internal,
                format!("Game details were not the documented shape: {e}."),
            )
        })?;
        for game in parsed.data {
            cache.put(
                &format!("game:{}", game.id),
                &serde_json::to_string(&game).unwrap_or_default(),
                Duration::from_secs(300),
            );
            out.insert(game.id, detail_of(&game));
        }
    }

    Ok(out)
}

fn detail_of(game: &RawGame) -> Detail {
    Detail {
        name: game.name.clone(),
        creator: game
            .creator
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default(),
        place: game.root_place_id.unwrap_or(game.id),
        players: game.playing,
        price: game.price.filter(|price| *price > 0),
    }
}

/// Icons are cheap (1500 a minute), so they are not cached per section, only per id.
fn icons(
    budget: &Budget,
    cache: &Cache,
    ids: &[u64],
) -> Result<BTreeMap<u64, String>, LaunchError> {
    let mut out = BTreeMap::new();
    let mut missing = Vec::new();
    for id in ids {
        match cache.get(&format!("icon:{id}")) {
            Some(url) => {
                out.insert(*id, url);
            }
            None => missing.push(*id),
        }
    }
    if missing.is_empty() {
        return Ok(out);
    }

    budget.spend(Class::Thumbs)?;
    let url = format!(
        "{ICONS}{}&size=150x150&format=Png&isCircular=false",
        missing
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    let answer = http::get(&url)?;
    if !answer.ok() {
        // A missing icon is not a failed page: the tile draws without it and the
        // grid keeps its shape.
        return Ok(out);
    }
    let parsed: RawThumbs = serde_json::from_str(&answer.body).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The icon answer was not the documented shape: {e}."),
        )
    })?;
    for thumb in parsed.data {
        if thumb.image_url.is_empty() {
            continue;
        }
        cache.put(
            &format!("icon:{}", thumb.target_id),
            &thumb.image_url,
            Duration::from_secs(600),
        );
        out.insert(thumb.target_id, thumb.image_url);
    }
    Ok(out)
}

/// Games and people for one query. One call, because the budget is four a minute.
pub fn search(
    budget: &Budget,
    cache: &Cache,
    cookie: Option<&str>,
    term: &str,
    people: bool,
) -> Result<Search, LaunchError> {
    let term = term.trim();
    if term.len() < 2 {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "Type at least two characters, and search spends one of four calls a minute.",
        ));
    }

    let key = format!("search:{people}:{}", term.to_lowercase());
    let cached = cache.get(&key).is_some();

    let body = match cache.get(&key) {
        Some(body) => body,
        None => {
            budget.spend(Class::Search)?;
            let url = format!(
                "{SEARCH}?searchQuery={}&pageToken=&sessionId={}&verticalType={}",
                urlencode(term),
                uuid_like(),
                if people { "user" } else { "game" }
            );
            let mut request = http::agent(Duration::from_secs(15)).get(&url);
            if let Some(cookie) = cookie {
                request = request.header("Cookie", &format!(".ROBLOSECURITY={cookie}"));
            }
            request = request.header("Referer", SITE);
            let answer = request.call().map_err(|e| {
                LaunchError::new(
                    FailureCode::Network,
                    format!("Search could not be reached: {e}."),
                )
            })?;
            let status = answer.status().as_u16();
            let body = answer.into_body().read_to_string().unwrap_or_default();
            if status == 429 {
                return Err(LaunchError::new(
                    FailureCode::TicketThrottled,
                    "Roblox is rate limiting search for this machine. It allows four calls a minute.",
                )
                .with_action("Wait a moment"));
            }
            if status >= 400 {
                return Err(LaunchError::new(
                    FailureCode::Network,
                    format!("Search answered {status}: {body:.160}"),
                ));
            }
            cache.put(&key, &body, Duration::from_secs(180));
            body
        }
    };

    let raw: RawSearch = serde_json::from_str(&body).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("Search was not the documented shape: {e}."),
        )
    })?;

    let results: Vec<RawResult> = raw
        .search_results
        .into_iter()
        .flat_map(|group| group.contents)
        .collect();

    if people {
        let ids: Vec<u64> = results.iter().filter_map(|r| r.content_id).collect();
        let heads = headshots(budget, cache, &ids).unwrap_or_default();
        let people: Vec<Person> = results
            .into_iter()
            .filter_map(|r| {
                let user = r.content_id?;
                Some(Person {
                    user,
                    name: r.username.unwrap_or_default(),
                    display: r.display_name.unwrap_or_default(),
                    verified: r.has_verified_badge,
                    headshot: heads.get(&user).cloned(),
                })
            })
            .collect();
        return Ok(Search {
            games: Vec::new(),
            people,
            fetched: now(),
            cached,
        });
    }

    let mut games: Vec<GameCard> = results
        .into_iter()
        .filter_map(|r| {
            let universe = r.universe_id?;
            Some(GameCard {
                universe,
                place: r.root_place_id.unwrap_or(universe),
                name: r.name,
                players: r.player_count.unwrap_or(0),
                creator: r.creator_name.unwrap_or_default(),
                icon: None,
                price: None,
            })
        })
        .collect();

    let ids: Vec<u64> = games.iter().map(|game| game.universe).collect();
    let icons = icons(budget, cache, &ids).unwrap_or_default();

    // A search result is a card too, and a paid game has to say so here as well: this is
    // where a person looks for a game they heard about by name, and finding out about the
    // price after the client opens is the exact thing this build is stopping.
    let details = hydrate(budget, cache, &ids, cookie).unwrap_or_default();

    for game in &mut games {
        game.icon = icons.get(&game.universe).cloned();
        if let Some(found) = details.get(&game.universe) {
            game.price = found.price;
        }
    }

    Ok(Search {
        games,
        people: Vec::new(),
        fetched: now(),
        cached,
    })
}

/// One game, with the numbers a detail page needs.
///
/// Two calls: the game payload (60 a minute) and votes (200). Both are cached for
/// five minutes, because a detail page is a place people go back to.
pub fn game(
    budget: &Budget,
    cache: &Cache,
    cookie: Option<&str>,
    universe: u64,
) -> Result<GameDetail, LaunchError> {
    let key = format!("detail:{universe}");
    let cached = cache.get(&key).is_some();

    if let Some(body) = cache.get(&key) {
        if let Ok(detail) = serde_json::from_str::<GameDetail>(&body) {
            return Ok(GameDetail { cached, ..detail });
        }
    }

    budget.spend(Class::Games)?;
    let mut request = http::agent(Duration::from_secs(15)).get(&format!("{GAMES}{universe}"));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", &format!(".ROBLOSECURITY={cookie}"));
    }
    let answer = request.call().map_err(|e| {
        LaunchError::new(
            FailureCode::Network,
            format!("That game could not be read: {e}."),
        )
    })?;
    let status = answer.status().as_u16();
    let body = answer.into_body().read_to_string().unwrap_or_default();
    if status >= 400 {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            format!("Roblox answered {status} for that game."),
        )
        .with_action("Back to Home"));
    }

    let parsed: RawOne = serde_json::from_str(&body).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The game payload was not the documented shape: {e}."),
        )
    })?;
    let Some(raw) = parsed.data.into_iter().next() else {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "That game id is not one Roblox serves.",
        )
        .with_action("Back to Home"));
    };

    let (up_votes, down_votes) = votes(budget, cache, universe);
    let icons = icons(budget, cache, &[universe]).unwrap_or_default();

    let detail = GameDetail {
        universe,
        place: raw.root_place_id.unwrap_or(raw.id),
        name: raw.name,
        creator: raw.creator.map(|c| c.name).unwrap_or_default(),
        description: raw.description,
        players: raw.playing,
        visits: raw.visits,
        max_players: raw.max_players,
        genre: raw.genre,
        created: raw.created,
        updated: raw.updated,
        up_votes,
        down_votes,
        icon: icons.get(&universe).cloned(),
        price: raw.price.filter(|price| *price > 0),
        fetched: now(),
        cached,
    };
    cache.put(
        &key,
        &serde_json::to_string(&detail).unwrap_or_default(),
        Duration::from_secs(300),
    );
    Ok(detail)
}

/// Up and down votes. A failure here leaves the game visible with zeros, because
/// a rating is not worth hiding a page over.
fn votes(budget: &Budget, cache: &Cache, universe: u64) -> (u64, u64) {
    let key = format!("votes:{universe}");
    if let Some(body) = cache.get(&key) {
        if let Ok(parsed) = serde_json::from_str::<RawVotes>(&body) {
            if let Some(vote) = parsed.data.into_iter().next() {
                return (vote.up_votes, vote.down_votes);
            }
        }
        return (0, 0);
    }

    if budget.spend(Class::Games).is_err() {
        return (0, 0);
    }
    let Ok(answer) = http::get(&format!(
        "https://games.roblox.com/v1/games/votes?universeIds={universe}"
    )) else {
        return (0, 0);
    };
    if !answer.ok() {
        return (0, 0);
    }
    cache.put(&key, &answer.body, Duration::from_secs(300));
    serde_json::from_str::<RawVotes>(&answer.body)
        .ok()
        .and_then(|parsed| parsed.data.into_iter().next())
        .map(|vote| (vote.up_votes, vote.down_votes))
        .unwrap_or((0, 0))
}

/// The public server list for one place.
///
/// Three calls a minute, so this is never fetched on mount: the page asks when the
/// Servers tab is opened, and the answer is kept for two minutes.
pub fn servers(budget: &Budget, cache: &Cache, place: u64) -> Result<Servers, LaunchError> {
    let key = format!("servers:{place}");
    let cached = cache.get(&key).is_some();

    if let Some(body) = cache.get(&key) {
        if let Ok(list) = serde_json::from_str::<Vec<Server>>(&body) {
            return Ok(Servers {
                place,
                list,
                fetched: now(),
                cached,
            });
        }
    }

    budget.spend(Class::Servers)?;
    let url =
        format!("https://games.roblox.com/v1/games/{place}/servers/Public?limit=10&sortOrder=Desc");
    let answer = http::get(&url)?;
    if !answer.ok() {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            format!(
                "The server list answered {}: {}",
                answer.status,
                answer.reason()
            ),
        )
        .with_action("Try again"));
    }

    let parsed: RawServers = serde_json::from_str(&answer.body).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The server list was not the documented shape: {e}."),
        )
    })?;

    let list: Vec<Server> = parsed
        .data
        .into_iter()
        .map(|server| Server {
            id: server.id,
            playing: server.playing,
            max_players: server.max_players,
            ping: server.ping,
            fps: server.fps,
        })
        .collect();

    cache.put(
        &key,
        &serde_json::to_string(&list).unwrap_or_default(),
        Duration::from_secs(120),
    );

    Ok(Servers {
        place,
        list,
        fetched: now(),
        cached,
    })
}

fn headshots(
    budget: &Budget,
    cache: &Cache,
    ids: &[u64],
) -> Result<BTreeMap<u64, String>, LaunchError> {
    let mut out = BTreeMap::new();
    let mut missing = Vec::new();
    for id in ids {
        match cache.get(&format!("head:{id}")) {
            Some(url) => {
                out.insert(*id, url);
            }
            None => missing.push(*id),
        }
    }
    if missing.is_empty() {
        return Ok(out);
    }

    for chunk in missing.chunks(GAMES_PER_CALL) {
        budget.spend(Class::Thumbs)?;
        let url = format!(
            "{HEADS}{}&size=150x150&format=Png&isCircular=false",
            chunk
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        let answer = http::get(&url)?;
        if !answer.ok() {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<RawThumbs>(&answer.body) else {
            continue;
        };
        for thumb in parsed.data {
            if thumb.image_url.is_empty() {
                continue;
            }
            cache.put(
                &format!("head:{}", thumb.target_id),
                &thumb.image_url,
                Duration::from_secs(600),
            );
            out.insert(thumb.target_id, thumb.image_url);
        }
    }
    Ok(out)
}

/// Percent-encodes a query term. Small on purpose: a dependency for this would be
/// a dependency in the launcher's most sensitive crate.
fn urlencode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// A session id for the discovery endpoints. They want one and do not check it.
fn uuid_like() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        nanos as u32,
        (nanos >> 32) as u16,
        (nanos >> 48) as u16,
        pid as u16,
        (nanos >> 16) as u64 & 0xffff_ffff_ffff
    )
}

// ---- the library ----

/// Roblox's own favourite list for one account.
///
/// Measured on 2026-11-18: `v2/users/{id}/favorite/games` answers an anonymous GET
/// with `x-ratelimit-limit: 200, 200;w=60`, each entry's `id` **is** the universe id
/// (hydrating it through `games?universeIds=` returns the same name), and
/// `rootPlace.id` is the place a launch actually needs. So a page of fifty costs one
/// call plus the hydration and icon calls every grid already makes, and the names
/// arrive even when hydration does not.
const FAVORITES: &str = "https://games.roblox.com/v2/users/";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFavourite {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    root_place: Option<RootPlace>,
}

#[derive(Debug, Clone, Deserialize)]
struct RootPlace {
    id: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFavourites {
    #[serde(default)]
    data: Vec<RawFavourite>,
    #[serde(default, rename = "nextPageCursor")]
    next: Option<String>,
}

/// One page of favourites, in Roblox's own order.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shelf {
    pub games: Vec<GameCard>,
    /// The cursor Roblox gave for the following page, or none on the last one.
    pub next: Option<String>,
    pub fetched: u64,
    pub cached: bool,
    /// Set when the page arrived and something smaller inside it did not, so the
    /// screen can name what is missing instead of drawing a blank tile and letting
    /// it read as a game with no name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Percent-encodes a cursor. The value is opaque base64 that can carry `+`, `/` and
/// `=`, and an unescaped `+` arrives as a space, which Roblox answers with a 400.
fn percent(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Favourites for one account. The page is cached for a couple of minutes because it
/// changes when somebody clicks a star, not when somebody clicks Refresh.
pub fn favorites(
    budget: &Budget,
    cache: &Cache,
    user: u64,
    cursor: Option<&str>,
) -> Result<Shelf, LaunchError> {
    let key = format!("fav:{user}:{}", cursor.unwrap_or("-"));
    let held = cache.get(&key);
    let cached = held.is_some();

    let body = match held {
        Some(body) => body,
        None => {
            budget.spend(Class::Library)?;
            let mut url = format!(
                "{FAVORITES}{user}/favorite/games?accessFilter=Public&limit=50&sortOrder=Desc"
            );
            if let Some(cursor) = cursor {
                url.push_str("&cursor=");
                url.push_str(&percent(cursor));
            }

            let answer = http::agent(Duration::from_secs(15))
                .get(&url)
                .header("Accept", "application/json")
                .call()
                .map_err(|e| {
                    LaunchError::new(
                        FailureCode::Network,
                        format!("Your favourites could not be read: {e}."),
                    )
                    .with_action("Retry")
                })?;

            let status = answer.status().as_u16();
            let body = answer.into_body().read_to_string().unwrap_or_default();
            if status >= 400 {
                return Err(LaunchError::new(
                    FailureCode::Network,
                    format!("The favourite list answered {status}: {body:.160}"),
                )
                .with_action("Retry"));
            }

            cache.put(&key, &body, Duration::from_secs(120));
            body
        }
    };

    let parsed: RawFavourites = serde_json::from_str(&body).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The favourite list was not the documented shape: {e}."),
        )
    })?;

    let ids: Vec<u64> = parsed.data.iter().map(|favourite| favourite.id).collect();
    let mut note: Option<String> = None;

    let details = match hydrate(budget, cache, &ids, None) {
        Ok(details) => details,
        Err(failure) => {
            note = Some(format!("Player counts are missing: {}", failure.message));
            BTreeMap::new()
        }
    };
    let icons = match icons(budget, cache, &ids) {
        Ok(icons) => icons,
        Err(failure) => {
            note = Some(match note {
                Some(existing) => format!("{existing} Icons are missing too: {}", failure.message),
                None => format!("Icons are missing: {}", failure.message),
            });
            BTreeMap::new()
        }
    };

    let games = parsed
        .data
        .into_iter()
        .map(|favourite| {
            let detail = details.get(&favourite.id);
            GameCard {
                universe: favourite.id,
                place: favourite
                    .root_place
                    .map(|place| place.id)
                    .or_else(|| detail.map(|detail| detail.place))
                    .unwrap_or(favourite.id),
                name: detail
                    .map(|detail| detail.name.clone())
                    .filter(|name| !name.is_empty())
                    .unwrap_or(favourite.name),
                players: detail.map(|detail| detail.players).unwrap_or(0),
                creator: detail
                    .map(|detail| detail.creator.clone())
                    .unwrap_or_default(),
                icon: icons.get(&favourite.id).cloned(),
                price: detail.and_then(|detail| detail.price),
            }
        })
        .collect();

    Ok(Shelf {
        games,
        next: parsed.next,
        fetched: now(),
        cached,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_refuses_the_fifth_search_in_a_minute() {
        let budget = Budget::default();
        for _ in 0..4 {
            budget.spend(Class::Search).expect("four fit in a minute");
        }
        let refused = budget.spend(Class::Search).expect_err("the fifth does not");
        assert_eq!(refused.code, FailureCode::TicketThrottled);
        assert!(refused.message.contains("four") || refused.message.contains("4"));
    }

    #[test]
    fn a_class_that_was_never_used_reports_full_head_room() {
        let budget = Budget::default();
        let room = budget.head_room(&[Class::Search, Class::Feed]);
        assert_eq!(room[0].1, 0);
        assert_eq!(room[0].2, 4);
        assert_eq!(room[1].2, 30);
    }

    #[test]
    fn the_cache_forgets_what_it_should() {
        let cache = Cache::default();
        assert!(cache.get("k").is_none());
        cache.put("k", "value", Duration::from_secs(60));
        assert_eq!(cache.get("k").as_deref(), Some("value"));
        cache.put("z", "value", Duration::ZERO);
        std::thread::sleep(Duration::from_millis(2));
        assert!(
            cache.get("z").is_none(),
            "a zero ttl means it is already stale"
        );
    }

    #[test]
    fn a_query_is_percent_encoded() {
        assert_eq!(urlencode("tower defense"), "tower%20defense");
        assert_eq!(urlencode("a+b&c"), "a%2Bb%26c");
        assert_eq!(urlencode("Pokémon"), "Pok%C3%A9mon");
    }

    #[test]
    fn the_feed_is_read_for_ids_only() {
        let body = r#"{"sorts":[
            {"topic":"Continue","treatmentType":"Carousel","recommendationList":[
                {"contentType":"Game","contentId":65241},
                {"contentType":"Game","contentId":189707},
                {"contentType":"User","contentId":7}
            ]},
            {"topic":"","recommendationList":[]}
        ]}"#;
        let raw: RawFeed = serde_json::from_str(body).unwrap();
        let first = &raw.sorts[0];
        assert_eq!(first.topic.as_deref(), Some("Continue"));
        let ids: Vec<u64> = first
            .recommendation_list
            .as_ref()
            .unwrap()
            .iter()
            .filter(|item| item.content_type.as_deref() == Some("Game"))
            .filter_map(|item| item.content_id)
            .collect();
        assert_eq!(ids, vec![65241, 189707]);
    }

    #[test]
    fn the_server_list_is_what_a_launch_needs() {
        let body = r#"{"data":[{"id":"b1f3fe94-97fd-4d7a-8942-e09f62e62a91","playing":12,"maxPlayers":30,"ping":42,"fps":59.8}],"nextPageCursor":null}"#;
        let raw: RawServers = serde_json::from_str(body).unwrap();
        assert_eq!(raw.data.len(), 1);
        assert_eq!(raw.data[0].ping, 42);
        assert_eq!(raw.data[0].id, "b1f3fe94-97fd-4d7a-8942-e09f62e62a91");
    }

    #[test]
    fn a_search_group_shape_flattens() {
        let body = r#"{"searchResults":[
            {"contentGroupType":"Game","contents":[{"universeId":1176784616,"rootPlaceId":7,"name":"Tower Defense Simulator","playerCount":12722,"creatorName":"Paradox"}]},
            {"contentGroupType":"Game","contents":[{"universeId":2,"rootPlaceId":8,"name":"Other","playerCount":5,"creatorName":"Someone"}]}
        ]}"#;
        let raw: RawSearch = serde_json::from_str(body).unwrap();
        let flat: Vec<RawResult> = raw
            .search_results
            .into_iter()
            .flat_map(|g| g.contents)
            .collect();
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].name, "Tower Defense Simulator");
    }
}
