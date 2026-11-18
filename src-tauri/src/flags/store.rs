//! Named profiles, and the scope chain that decides which one wins.
//!
//! The brief's order is account, then game, then global, then the client's own
//! default, resolved per flag rather than per profile: a game profile that sets
//! one flag must not silently drop the other nine an account profile sets.
//!
//! Values are `serde_json::Value` on purpose. A flag is a boolean, an int, a
//! string or a log level, and writing `"true"` where the client wants `true`
//! is the kind of difference that only shows up as "my flag does nothing".

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::catalog::Kind;
use crate::error::{FailureCode, LaunchError};

pub const SCHEMA: u32 = 1;
const STORE: &str = "profiles.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub name: String,
    pub notes: String,
    pub values: BTreeMap<String, Value>,
}

/// Which profile applies where. Keys are stringified ids because they arrive
/// from the page as strings and a place id is a number larger than a JS safe
/// integer can be trusted with.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Scopes {
    pub global: Option<String>,
    pub accounts: BTreeMap<String, String>,
    pub games: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Store {
    pub schema: u32,
    pub profiles: Vec<Profile>,
    pub scopes: Scopes,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            profiles: Vec::new(),
            scopes: Scopes::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Account,
    Game,
    Global,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Account => "account",
            Source::Game => "game",
            Source::Global => "global",
        }
    }
}

/// One flag after the chain has been walked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub key: String,
    pub value: Value,
    pub source: Source,
    pub profile: String,
    pub kind: Kind,
}

fn path(root: &Path) -> PathBuf {
    root.join("flags").join(STORE)
}

pub fn load(root: &Path) -> Result<Store, LaunchError> {
    let file = path(root);
    match std::fs::read(&file) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            LaunchError::new(
                FailureCode::ConfigUnreadable,
                format!("Your flag profiles could not be read: {e}"),
            )
            .with_action("Open the data folder")
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(e) => Err(LaunchError::new(
            FailureCode::ConfigUnreadable,
            format!("Your flag profiles could not be opened: {e}"),
        )),
    }
}

pub fn save(root: &Path, store: &Store) -> Result<(), LaunchError> {
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
    let body = serde_json::to_vec_pretty(store).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("Your flag profiles could not be encoded: {e}"),
        )
    })?;
    std::fs::write(&tmp, body).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("Your flag profiles could not be written: {e}"),
        )
    })?;
    std::fs::rename(&tmp, &file).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("Your flag profiles could not be replaced: {e}"),
        )
    })
}

pub fn profile_named<'a>(store: &'a Store, name: &str) -> Option<&'a Profile> {
    store.profiles.iter().find(|profile| profile.name == name)
}

/// Puts a profile in place, creating it when the name is new.
pub fn upsert_profile(
    store: &mut Store,
    name: &str,
    values: BTreeMap<String, Value>,
) -> Result<(), LaunchError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "A profile needs a name.",
        ));
    }
    if name.len() > 48 {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "That profile name is longer than any screen can show.",
        ));
    }

    match store
        .profiles
        .iter_mut()
        .find(|profile| profile.name == name)
    {
        Some(profile) => profile.values = values,
        None => store.profiles.push(Profile {
            name: name.to_string(),
            notes: String::new(),
            values,
        }),
    }
    Ok(())
}

pub fn drop_profile(store: &mut Store, name: &str) {
    store.profiles.retain(|profile| profile.name != name);
    if store.scopes.global.as_deref() == Some(name) {
        store.scopes.global = None;
    }
    store.scopes.accounts.retain(|_, target| target != name);
    store.scopes.games.retain(|_, target| target != name);
}

/// Rejects a value whose shape contradicts the key's own name, so a typo fails
/// where it is typed instead of doing nothing at boot.
pub fn check_value(key: &str, value: &Value) -> Result<(), LaunchError> {
    let kind = Kind::of(key);
    let good = match kind {
        Kind::Flag => value.is_boolean() || value.as_str().is_some_and(is_bool_word),
        Kind::Int => {
            value.is_i64()
                || value.is_u64()
                || value
                    .as_str()
                    .is_some_and(|text| text.parse::<i64>().is_ok())
        }
        Kind::Number => {
            value.is_number()
                || value
                    .as_str()
                    .is_some_and(|text| text.parse::<f64>().is_ok())
        }
        Kind::String | Kind::Log => value.is_string(),
        Kind::Unknown => true,
    };

    if good {
        return Ok(());
    }

    Err(LaunchError::new(
        FailureCode::FlagRejected,
        format!(
            "{} does not hold that kind of value: this one wants {}.",
            key,
            match kind {
                Kind::Flag => "true or false",
                Kind::Int => "a whole number",
                Kind::Number => "a number",
                Kind::String => "text",
                Kind::Log => "a log level",
                Kind::Unknown => "anything",
            }
        ),
    )
    .with_action("Fix the value"))
}

fn is_bool_word(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "true" | "false" | "1" | "0"
    )
}

/// Walks account, then game, then global, and returns one row per key.
///
/// The rows are the union, not the winning profile: a person needs to see the
/// nine flags their account profile sets even when a game profile sets a tenth.
pub fn resolve(store: &Store, account: Option<u64>, place: Option<u64>) -> Vec<Row> {
    let mut chain: Vec<(Source, String)> = Vec::new();

    if let Some(id) = account {
        if let Some(name) = store.scopes.accounts.get(&id.to_string()) {
            chain.push((Source::Account, name.clone()));
        }
    }
    if let Some(id) = place {
        if let Some(name) = store.scopes.games.get(&id.to_string()) {
            chain.push((Source::Game, name.clone()));
        }
    }
    if let Some(name) = &store.scopes.global {
        chain.push((Source::Global, name.clone()));
    }

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut rows = Vec::new();

    for (source, name) in chain {
        let Some(profile) = profile_named(store, &name) else {
            continue;
        };
        for (key, value) in &profile.values {
            if !seen.insert(key.clone()) {
                continue;
            }
            rows.push(Row {
                key: key.clone(),
                value: value.clone(),
                source,
                profile: profile.name.clone(),
                kind: Kind::of(key),
            });
        }
    }

    rows.sort_by(|a, b| a.key.cmp(&b.key));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> Store {
        let mut store = Store::default();
        upsert_profile(
            &mut store,
            "global",
            BTreeMap::from([
                ("FFlagOne".to_string(), json!(true)),
                ("DFIntTwo".to_string(), json!(60)),
            ]),
        )
        .unwrap();
        upsert_profile(
            &mut store,
            "game",
            BTreeMap::from([
                ("FFlagOne".to_string(), json!(false)),
                ("FFlagGameOnly".to_string(), json!(true)),
            ]),
        )
        .unwrap();
        upsert_profile(
            &mut store,
            "account",
            BTreeMap::from([("FFlagOne".to_string(), json!(true))]),
        )
        .unwrap();
        store.scopes.global = Some("global".to_string());
        store
            .scopes
            .games
            .insert("1818".to_string(), "game".to_string());
        store
            .scopes
            .accounts
            .insert("156".to_string(), "account".to_string());
        store
    }

    #[test]
    fn account_beats_game_beats_global_per_flag() {
        let rows = resolve(&store(), Some(156), Some(1818));
        let one = rows.iter().find(|row| row.key == "FFlagOne").unwrap();
        assert_eq!(one.source, Source::Account);
        assert_eq!(one.value, json!(true));

        // The game profile still contributes the key the account does not set.
        let game_only = rows.iter().find(|row| row.key == "FFlagGameOnly").unwrap();
        assert_eq!(game_only.source, Source::Game);

        // And the global one still contributes its own.
        let two = rows.iter().find(|row| row.key == "DFIntTwo").unwrap();
        assert_eq!(two.source, Source::Global);
        assert_eq!(two.value, json!(60));
    }

    #[test]
    fn a_game_profile_does_not_drop_the_account_profile() {
        let rows = resolve(&store(), Some(156), Some(1818));
        // Union, not the winning profile: three distinct keys across the chain.
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn with_no_scopes_nothing_resolves() {
        assert!(resolve(&Store::default(), Some(1), Some(2)).is_empty());
    }

    #[test]
    fn a_scope_naming_a_missing_profile_is_skipped_not_fatal() {
        let mut store = store();
        store.scopes.global = Some("deleted".to_string());
        let rows = resolve(&store, Some(156), Some(1818));
        assert!(rows.iter().all(|row| row.source != Source::Global));
    }

    #[test]
    fn refuses_a_value_that_contradicts_the_key() {
        assert!(check_value("FFlagThing", &json!(true)).is_ok());
        assert!(check_value("FFlagThing", &json!("True")).is_ok());
        assert!(check_value("FFlagThing", &json!(3)).is_err());
        assert!(check_value("DFIntFps", &json!(120)).is_ok());
        assert!(check_value("DFIntFps", &json!("fast")).is_err());
        assert!(check_value("FStringName", &json!("text")).is_ok());
        assert!(check_value("FStringName", &json!(true)).is_err());
        // An unknown shape is allowed through: the name says nothing, so there
        // is nothing to contradict.
        assert!(check_value("Sawdust", &json!(true)).is_ok());
    }

    #[test]
    fn deleting_a_profile_clears_every_scope_that_named_it() {
        let mut store = store();
        drop_profile(&mut store, "account");
        assert!(store.scopes.accounts.is_empty());
        assert!(profile_named(&store, "account").is_none());
    }
}
