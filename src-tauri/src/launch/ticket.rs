//! Minting a launch ticket.
//!
//! One call, one dance: POST with the session cookie, take `x-csrf-token` off the
//! 403, POST again, and read the ticket out of the `rbx-authentication-ticket`
//! response header rather than the body. The ticket is single use and short
//! lived, so nothing here caches it.

use crate::error::{FailureCode, LaunchError};
use crate::http;

const TICKET: &str = "https://auth.roblox.com/v1/authentication-ticket";
const SITE: &str = "https://www.roblox.com";

/// A ticket for one launch. Call it once per launch and throw it away after.
pub fn mint(cookie: &str) -> Result<String, LaunchError> {
    let header = format!(".ROBLOSECURITY={cookie}");
    let answer = http::post_json(
        TICKET,
        "{}",
        &[("Cookie", header.as_str()), ("Origin", SITE)],
    )?;

    if answer.ok() {
        return answer
            .header("rbx-authentication-ticket")
            .map(str::to_string)
            .ok_or_else(|| {
                LaunchError::new(
                    FailureCode::Internal,
                    "Roblox accepted the session but sent no launch ticket.",
                )
                .with_action("Try again")
            });
    }

    // Both of these are the same thing from here: this session cannot start a
    // game. Retrying in a loop would only hammer an endpoint that has already
    // said no, so it is named and handed to the person.
    Err(LaunchError::new(
        FailureCode::SessionExpired,
        format!(
            "This session cannot start a game: {}. The session may have expired, or the \
             account may need two-step verification cleared.",
            answer.reason()
        ),
    )
    .with_action("Check the account"))
}
