//! The flag catalog.
//!
//! Roblox serves its own flag set, per application and per channel, from the
//! settings service. That is the only honest source for "official keys with
//! their defaults": nothing here is invented, and a key that the service does
//! not serve is marked as having no known default rather than given a guess.
//!
//! Measured: `application/PCDesktopClient` answers 200 with 25,584 keys, every
//! value a string (`"True"`, `"1000"`, `"Trace;7746948539"`). The desktop
//! client itself pulls the same catalogue at boot from
//! `clientsettingscdn.roblox.com/v2/settings-compressed/application/PCDesktopClient/bucket/<bucket>/<hash>.dcz`,
//! which the launcher does not use because the uncompressed endpoint is the
//! same data and needs no bucket discovery.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{FailureCode, LaunchError};
use crate::http;

const SERVICE: &str = "https://clientsettings.roblox.com/v2/settings/application/PCDesktopClient";
const APPLICATION: &str = "PCDesktopClient";

/// What a key's value is, which decides the control the screen draws. Roblox
/// encodes it in the name, so it is read rather than guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Flag,
    Int,
    String,
    Log,
    Number,
    Unknown,
}

impl Kind {
    pub fn of(key: &str) -> Self {
        let rest = key
            .strip_prefix("DF")
            .or_else(|| key.strip_prefix("SF"))
            .or_else(|| key.strip_prefix("F"))
            .or_else(|| key.strip_prefix("D"))
            .or_else(|| key.strip_prefix("S"))
            .or_else(|| key.strip_prefix("I"));

        match rest {
            Some(name) if name.starts_with("Flag") => Kind::Flag,
            Some(name) if name.starts_with("Int") => Kind::Int,
            Some(name) if name.starts_with("String") => Kind::String,
            Some(name) if name.starts_with("Log") => Kind::Log,
            Some(name) if name.starts_with("Float") || name.starts_with("Double") => Kind::Number,
            _ => Kind::Unknown,
        }
    }

    /// A key is only offered as editable when its name says what it holds.
    pub fn known(self) -> bool {
        self != Kind::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    /// Seconds since the epoch when the service answered.
    pub fetched: u64,
    pub application: String,
    /// Every served key with the value the service is handing out, which is the
    /// client's own default until someone overrides it.
    pub values: BTreeMap<String, String>,
}

fn path(root: &Path) -> PathBuf {
    root.join("flags").join("catalog.json")
}

pub fn load(root: &Path) -> Option<Catalog> {
    let bytes = std::fs::read(path(root)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn store(root: &Path, catalog: &Catalog) -> Result<(), LaunchError> {
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
    let body = serde_json::to_vec(catalog).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The flag catalog could not be encoded: {e}"),
        )
    })?;
    std::fs::write(&tmp, body).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The flag catalog could not be written: {e}"),
        )
    })?;
    std::fs::rename(&tmp, &file).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The flag catalog could not be replaced: {e}"),
        )
    })
}

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "applicationSettings")]
    settings: BTreeMap<String, String>,
}

/// Asks Roblox for the current catalogue and keeps it.
pub fn refresh(root: &Path) -> Result<Catalog, LaunchError> {
    let answer = http::get(SERVICE)?;

    if !answer.ok() {
        return Err(LaunchError::new(
            FailureCode::Network,
            format!("Roblox would not list its flags: {}", answer.reason()),
        )
        .with_action("Retry"));
    }

    let parsed: Answer = serde_json::from_str(&answer.body).map_err(|e| {
        LaunchError::new(
            FailureCode::Network,
            format!("Roblox answered in a shape this build does not know: {e}"),
        )
    })?;

    let catalog = Catalog {
        fetched: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        application: APPLICATION.to_string(),
        values: parsed.settings,
    };

    store(root, &catalog)?;
    Ok(catalog)
}

/// The catalogue, fetched on first use and reused afterwards.
pub fn ensure(root: &Path) -> Result<Catalog, LaunchError> {
    if let Some(catalog) = load(root) {
        return Ok(catalog);
    }
    refresh(root)
}

/// One search result. `default` is absent when Roblox does not serve the key,
/// which is a real state: a locally set flag can be honoured and still have no
/// published default.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub key: String,
    pub default: Option<String>,
    pub kind: Kind,
    /// Whether the client will take this key from a local file. False does not
    /// mean the key is wrong, only that a local override of it is discarded —
    /// the screen has to say which, because silently offering both is the lie
    /// the allowlist created.
    pub honoured: bool,
}

pub fn search(catalog: &Catalog, query: &str, limit: usize) -> Vec<Hit> {
    let needle = query.trim().to_ascii_lowercase();

    let mut hits: Vec<Hit> = catalog
        .values
        .iter()
        .filter(|(key, _)| needle.is_empty() || key.to_ascii_lowercase().contains(&needle))
        .take(limit)
        .map(|(key, value)| Hit {
            key: key.clone(),
            default: Some(value.clone()),
            kind: Kind::of(key),
            honoured: super::allow::honours(key),
        })
        .collect();

    // A query that names a key Roblox does not serve is still offered, with no
    // default, because that is exactly the case a person is asking about when
    // they type a name they read somewhere. `DFIntTaskSchedulerTargetFps` is the
    // proof that this branch matters: it is absent from the served list and
    // people arrive looking for it, and the row is also the proof that "offered"
    // must never mean "works" — the client drops it, because it is not on the
    // allowlist.
    let typed = query.trim();
    let typed_is_key = Kind::of(typed).known() && typed.len() > 6;
    let already = hits.iter().any(|hit| hit.key == typed);

    if typed_is_key && !already {
        hits.push(Hit {
            key: typed.to_string(),
            default: None,
            kind: Kind::of(typed),
            honoured: super::allow::honours(typed),
        });
    }

    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_type_out_of_the_name() {
        assert_eq!(Kind::of("FFlagDebugDisplayFPS"), Kind::Flag);
        assert_eq!(Kind::of("DFFlagSoundServiceMockOutputDevices"), Kind::Flag);
        assert_eq!(Kind::of("FIntFontSizePadding"), Kind::Int);
        assert_eq!(Kind::of("DFIntTaskSchedulerTargetFps"), Kind::Int);
        assert_eq!(Kind::of("FStringVoiceUserAgencyIXPLayerName"), Kind::String);
        assert_eq!(Kind::of("FLogNetworkTiming"), Kind::Log);
        assert_eq!(Kind::of("FFloatSomeThreshold"), Kind::Number);
        // Anything else is not offered as editable.
        assert_eq!(Kind::of("LayeredClothingMode"), Kind::Unknown);
        assert!(!Kind::of("Sawdust").known());
    }

    #[test]
    fn finds_keys_and_offers_an_unserved_one_rather_than_nothing() {
        let mut values = BTreeMap::new();
        values.insert("FFlagAlpha".to_string(), "True".to_string());
        values.insert("DFIntBeta".to_string(), "30".to_string());
        let catalog = Catalog {
            fetched: 0,
            application: "PCDesktopClient".to_string(),
            values,
        };

        let hits = search(&catalog, "alpha", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].default.as_deref(), Some("True"));
        assert_eq!(hits[0].kind, Kind::Flag);

        // A key Roblox does not serve comes back with no default rather than
        // being silently dropped, even when its name matches nothing served.
        let hits = search(&catalog, "DFIntTaskSchedulerTargetFps", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].default, None);
        assert_eq!(hits[0].kind, Kind::Int);

        // A partial query is a substring search and does not invent a key.
        let hits = search(&catalog, "alp", 10);
        assert_eq!(hits.len(), 1);
        assert!(hits.iter().all(|hit| hit.key == "FFlagAlpha"));
    }
}
