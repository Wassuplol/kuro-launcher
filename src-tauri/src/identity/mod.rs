//! Stage 03, identity: the accounts this machine can launch as.
//!
//! An account is a sealed cookie and the public facts about who it belongs to.
//! The store is one JSON file under the data root; the only secret in it is a
//! DPAPI blob, and that blob is unsealed in exactly two places: a health check
//! and a launch. It never reaches the page.
//!
//! Everything is written through a temporary file and a rename, so a crash in
//! the middle of adding an account cannot leave a half-written store behind.

pub mod quicklogin;
pub mod vault;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{FailureCode, LaunchError};
use crate::supply::agent;

/// `users/authenticated` with no cookie answers
/// `{"errors":[{"code":9002,"message":"Authentication token is missing"}]}` and
/// HTTP 401, which is what the health check reads.
const WHOAMI: &str = "https://users.roblox.com/v1/users/authenticated";

const STORE: &str = "accounts.json";
const SCHEMA: u32 = 1;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What the store knows about one account. The cookie is not in here: it is
/// sealed separately so that listing accounts cannot leak one by accident.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Account {
    pub user_id: u64,
    pub name: String,
    pub display: String,
    pub added: u64,
    /// ok, expired or unknown. Written by the last health check.
    pub health: String,
    pub checked_at: u64,
    /// Hex of the DPAPI blob. Never a plaintext cookie.
    sealed: String,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            user_id: 0,
            name: String::new(),
            display: String::new(),
            added: 0,
            health: "unknown".to_string(),
            checked_at: 0,
            sealed: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Store {
    pub schema: u32,
    /// Which account a launch uses when nothing else is chosen.
    pub default: Option<u64>,
    /// The last account that was actually used, for "continue as".
    pub last_used: Option<u64>,
    pub accounts: Vec<Account>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            default: None,
            last_used: None,
            accounts: Vec::new(),
        }
    }
}

/// The account as the screen sees it. Note the absence of any cookie.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub user_id: u64,
    pub name: String,
    pub display: String,
    pub added: u64,
    pub health: String,
    pub checked_at: u64,
    pub is_default: bool,
    pub is_last_used: bool,
}

fn path(root: &Path) -> PathBuf {
    root.join(STORE)
}

/// Reads the store. A missing file is an empty store, not a failure: the first
/// run has no accounts and that is correct.
pub fn load(root: &Path) -> Result<Store, LaunchError> {
    let file = path(root);
    match std::fs::read(&file) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            LaunchError::new(
                FailureCode::ConfigUnreadable,
                format!("Your saved accounts could not be read: {e}"),
            )
            .with_action("Open the data folder")
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(e) => Err(LaunchError::new(
            FailureCode::ConfigUnreadable,
            format!("Your saved accounts could not be opened: {e}"),
        )),
    }
}

pub fn save(root: &Path, store: &Store) -> Result<(), LaunchError> {
    std::fs::create_dir_all(root).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("The data folder could not be created: {e}"),
        )
    })?;

    let file = path(root);
    let tmp = file.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(store).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("Your accounts could not be encoded: {e}"),
        )
    })?;

    std::fs::write(&tmp, body).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("Your accounts could not be written: {e}"),
        )
    })?;
    std::fs::rename(&tmp, &file).map_err(|e| {
        LaunchError::new(
            FailureCode::ConfigUnwritable,
            format!("Your accounts could not be replaced: {e}"),
        )
    })?;
    Ok(())
}

pub fn views(store: &Store) -> Vec<AccountView> {
    store
        .accounts
        .iter()
        .map(|account| AccountView {
            user_id: account.user_id,
            name: account.name.clone(),
            display: account.display.clone(),
            added: account.added,
            health: account.health.clone(),
            checked_at: account.checked_at,
            is_default: store.default == Some(account.user_id),
            is_last_used: store.last_used == Some(account.user_id),
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).ok())
        .collect()
}

/// Cleans a pasted cookie into the value Roblox actually expects.
///
/// People paste `.ROBLOSECURITY=...`, sometimes with quotes, sometimes with the
/// rest of the browser's cookie header attached. Rejecting that as "invalid"
/// would be technically true and useless, so the obvious forms are unwrapped and
/// anything that still looks wrong is refused with the reason.
pub fn normalize_cookie(raw: &str) -> Result<String, LaunchError> {
    let mut value = raw.trim().trim_matches('"').trim().to_string();

    if let Some(rest) = value.strip_prefix(".ROBLOSECURITY=") {
        value = rest.trim().to_string();
    }

    // A whole cookie header: keep the one that matters and drop the rest.
    if value.contains("; ") && value.contains(".ROBLOSECURITY=") {
        if let Some(found) = value.split("; ").find_map(|part| {
            part.trim()
                .strip_prefix(".ROBLOSECURITY=")
                .map(|cookie| cookie.to_string())
        }) {
            value = found;
        }
    }

    let invalid = |detail: &str| {
        LaunchError::new(
            FailureCode::SessionExpired,
            format!("That does not look like a session cookie: {detail}"),
        )
        .with_action("Paste the .ROBLOSECURITY value again")
    };

    if value.is_empty() {
        return Err(invalid("it is empty"));
    }
    if value.len() < 100 {
        return Err(invalid("it is too short to be a Roblox session"));
    }
    if value.len() > 4000 {
        return Err(invalid("it is longer than any session Roblox issues"));
    }
    if value.contains(char::is_whitespace) || value.contains(';') {
        return Err(invalid("it has spaces or semicolons in it"));
    }

    Ok(value)
}

/// The three facts Roblox will tell us about a session's owner.
#[derive(Debug, Clone, Deserialize)]
pub struct WhoAmI {
    pub id: u64,
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    errors: Vec<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: String,
}

fn cookie_header(cookie: &str) -> String {
    format!(".ROBLOSECURITY={cookie}")
}

/// Asks Roblox who this cookie belongs to.
///
/// This is the health check and the import check at once: one endpoint answers
/// both "is this a real session" and "whose". A refusal is reported as an
/// expired session because that is what it is from the person's side, whatever
/// the underlying reason.
pub fn whoami(cookie: &str) -> Result<WhoAmI, LaunchError> {
    let response = agent(Duration::from_secs(15))
        .get(WHOAMI)
        .header("Cookie", &cookie_header(cookie))
        .call()
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("Roblox could not be asked about this session: {e}"),
            )
            .with_action("Retry")
        })?;

    let status = response.status().as_u16();
    let body = response.into_body().read_to_string().unwrap_or_default();

    if (200..300).contains(&status) {
        return serde_json::from_str(&body).map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("Roblox answered in a shape this build does not know: {e}"),
            )
        });
    }

    let reason = serde_json::from_str::<ErrorEnvelope>(&body)
        .ok()
        .and_then(|envelope| envelope.errors.into_iter().next())
        .map(|error| error.message)
        .unwrap_or_else(|| format!("Roblox answered {status}"));

    Err(LaunchError::new(
        FailureCode::SessionExpired,
        format!("This session is no longer valid: {reason}"),
    )
    .with_action("Add the account again"))
}

/// Adds or refreshes one account from a pasted cookie.
pub fn import(root: &Path, raw_cookie: &str) -> Result<Store, LaunchError> {
    let cookie = normalize_cookie(raw_cookie)?;
    store_session(root, &cookie)
}

/// Adds or refreshes one account from a cookie that came from Roblox itself,
/// which is the Quick Login path. The value is already exact, so it is verified
/// and stored without the paste normaliser.
pub fn store_session(root: &Path, cookie: &str) -> Result<Store, LaunchError> {
    let who = whoami(cookie)?;
    let sealed = vault::seal(cookie)?;

    let mut store = load(root)?;
    store.accounts.retain(|account| account.user_id != who.id);
    store.accounts.push(Account {
        user_id: who.id,
        name: who.name,
        display: who.display_name,
        added: now(),
        health: "ok".to_string(),
        checked_at: now(),
        sealed: hex(&sealed),
    });

    // The first account becomes the default, so a fresh install has an obvious
    // answer to "which one do I launch".
    if store.default.is_none() {
        store.default = Some(who.id);
    }
    store.schema = SCHEMA;
    save(root, &store)?;
    Ok(store)
}

/// Re-checks every account and records the result. Returns the store so the
/// screen shows what Roblox said rather than what it remembered.
pub fn check_all(root: &Path) -> Result<Store, LaunchError> {
    let mut store = load(root)?;

    for account in &mut store.accounts {
        let Some(bytes) = unhex(&account.sealed) else {
            account.health = "expired".to_string();
            account.checked_at = now();
            continue;
        };

        match vault::unseal(&bytes).and_then(|cookie| whoami(&cookie)) {
            Ok(who) => {
                account.health = "ok".to_string();
                // The display name is the one thing Roblox lets people change
                // without changing the account, so it is refreshed here.
                account.name = who.name;
                account.display = who.display_name;
            }
            Err(error) if error.code == FailureCode::Network => {
                // A network failure says nothing about the session, so it must
                // not be recorded as one.
                account.health = "unknown".to_string();
            }
            Err(_) => account.health = "expired".to_string(),
        }
        account.checked_at = now();
    }

    save(root, &store)?;
    Ok(store)
}

/// Forgets an account on this machine.
///
/// Deliberately local only: the session stays valid on Roblox's side until it
/// expires or is revoked from the website, and the screen says so. Pretending
/// otherwise would be the more comfortable lie.
pub fn remove(root: &Path, user_id: u64) -> Result<Store, LaunchError> {
    let mut store = load(root)?;
    let before = store.accounts.len();
    store.accounts.retain(|account| account.user_id != user_id);

    if store.accounts.len() == before {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "That account is not saved on this machine.",
        ));
    }

    if store.default == Some(user_id) {
        store.default = store.accounts.first().map(|account| account.user_id);
    }
    if store.last_used == Some(user_id) {
        store.last_used = None;
    }

    save(root, &store)?;
    Ok(store)
}

pub fn set_default(root: &Path, user_id: u64) -> Result<Store, LaunchError> {
    let mut store = load(root)?;
    if !store
        .accounts
        .iter()
        .any(|account| account.user_id == user_id)
    {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "That account is not saved on this machine.",
        ));
    }
    store.default = Some(user_id);
    save(root, &store)?;
    Ok(store)
}

/// Records that an account was the one a launch used, so "continue as" has an
/// answer. Best effort: a launch that worked is not undone by a failed write.
pub fn mark_used(root: &Path, user_id: u64) -> Result<Store, LaunchError> {
    let mut store = load(root)?;
    if !store
        .accounts
        .iter()
        .any(|account| account.user_id == user_id)
    {
        return Ok(store);
    }
    store.last_used = Some(user_id);
    save(root, &store)?;
    Ok(store)
}

/// Reads one account's cookie, for the two places allowed to: a launch, and the
/// health check above.
pub fn cookie_for(root: &Path, user_id: u64) -> Result<String, LaunchError> {
    let store = load(root)?;
    let account = store
        .accounts
        .iter()
        .find(|account| account.user_id == user_id)
        .ok_or_else(|| {
            LaunchError::new(
                FailureCode::NotFound,
                "That account is not saved on this machine.",
            )
        })?;

    let bytes = unhex(&account.sealed).ok_or_else(|| {
        LaunchError::new(
            FailureCode::SessionExpired,
            "The saved session is unreadable.",
        )
        .with_action("Add the account again")
    })?;

    vault::unseal(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_forms_people_actually_paste() {
        let bare = "A".repeat(200);
        assert_eq!(normalize_cookie(&bare).unwrap(), bare);
        assert_eq!(
            normalize_cookie(&format!(".ROBLOSECURITY={bare}")).unwrap(),
            bare
        );
        assert_eq!(normalize_cookie(&format!("  {bare}  ")).unwrap(), bare);
        assert_eq!(normalize_cookie(&format!("\"{bare}\"")).unwrap(), bare);
        assert_eq!(
            normalize_cookie(&format!("other=1; .ROBLOSECURITY={bare}; more=2")).unwrap(),
            bare
        );
    }

    #[test]
    fn refuses_what_cannot_be_a_cookie() {
        assert!(normalize_cookie("").is_err());
        assert!(normalize_cookie("short").is_err());
        assert!(normalize_cookie(&format!("{} with spaces", "A".repeat(200))).is_err());
        assert!(normalize_cookie(&format!("{};extra", "A".repeat(200))).is_err());
    }

    #[test]
    fn hex_round_trips() {
        let bytes = vec![0u8, 1, 15, 16, 255];
        assert_eq!(unhex(&hex(&bytes)).unwrap(), bytes);
        assert!(unhex("abc").is_none());
    }

    #[test]
    fn an_empty_store_is_not_an_error() {
        let root = std::env::temp_dir().join("kuro-accounts-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let store = load(&root).expect("a missing store is an empty store");
        assert!(store.accounts.is_empty());
        assert!(store.default.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
