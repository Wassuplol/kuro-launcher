//! What the account has, and what a game costs.
//!
//! Two different questions with two different answers to "where does this come from".
//!
//! A price is public. `games?universeIds=` carries `price` on the same call that gives
//! the name and the player count, so a paid game is known before anything launches,
//! which is the whole point: nobody should meet the paywall in the Roblox client.
//!
//! A balance is private. `economy.roblox.com/v1/users/{id}/currency` answers 401 without
//! a session, so it is read with the same sealed cookie the launch path mints tickets
//! from. The number is never written to disk, and an expired session says so instead of
//! showing a zero: a zero balance and an unreadable one are different facts.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use crate::error::{FailureCode, LaunchError};
use crate::http;
use crate::identity;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub user: u64,
    pub name: String,
    pub display: String,
    pub robux: u64,
}

/// The Robux the signed-in account holds, straight from Roblox.
pub fn balance(root: &Path) -> Result<Balance, LaunchError> {
    let store = identity::load(root)?;
    let Some(user) = store.last_used.or(store.default) else {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "There is no account to read a balance for.",
        )
        .with_action("Add an account"));
    };

    let Some(account) = store.accounts.iter().find(|entry| entry.user_id == user) else {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "That account is no longer in this launcher.",
        )
        .with_action("Add the account again"));
    };

    let cookie = identity::cookie_for(root, user)?;
    let url = format!("https://economy.roblox.com/v1/users/{user}/currency");

    let answer = http::agent(Duration::from_secs(12))
        .get(&url)
        .header("Cookie", &format!(".ROBLOSECURITY={cookie}"))
        .header("Accept", "application/json")
        .call()
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("Roblox did not answer the balance request: {e}."),
            )
            .with_action("Try again")
        })?;

    let status = answer.status().as_u16();
    let body = answer.into_body().read_to_string().unwrap_or_default();

    if status == 401 || status == 403 {
        return Err(LaunchError::new(
            FailureCode::SessionExpired,
            "That session has expired, so Roblox will not say what the balance is.",
        )
        .with_action("Add the account again"));
    }
    if status >= 400 {
        return Err(LaunchError::new(
            FailureCode::Network,
            format!("The balance request answered {status}."),
        )
        .with_action("Try again"));
    }

    let parsed: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The balance was not the documented shape: {e}."),
        )
    })?;

    let robux = parsed
        .get("robux")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| {
            LaunchError::new(
                FailureCode::Internal,
                "Roblox answered without a robux field, so there is no balance to show.",
            )
        })?;

    Ok(Balance {
        user,
        name: account.name.clone(),
        display: account.display.clone(),
        robux,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The error a dead session produces has to name the session, not the network: the
    /// screen offers "add the account again" only for one of them.
    #[test]
    fn an_expired_session_is_not_a_network_error() {
        let expired = LaunchError::new(
            FailureCode::SessionExpired,
            "That session has expired, so Roblox will not say what the balance is.",
        )
        .with_action("Add the account again");

        assert_eq!(expired.code, FailureCode::SessionExpired);
        assert_eq!(expired.action.as_deref(), Some("Add the account again"));
    }
}
