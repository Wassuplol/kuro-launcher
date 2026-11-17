//! Stage 03, Quick Login: a session without a password and without a paste.
//!
//! Roblox's cross-device sign-in is three calls around one human step. A token
//! is created with no authentication at all, the person approves the six
//! characters on the website or scans the QR in the mobile app, and the approval
//! is exchanged for a real session cookie through the same login endpoint the
//! site itself uses.
//!
//! Measured against the live service from this machine:
//!
//! - `login/create` answers 200 with `code`, `privateKey`, `expirationTime` and
//!   `imagePath`, and is rate limited to ten a minute.
//! - The token lives **two minutes**, not the ten the older notes claim, so the
//!   countdown on screen comes from Roblox's own `expirationTime`.
//! - `login/status` and `login/cancel` both want the CSRF dance: 403 with
//!   `x-csrf-token`, then the same call again with that header.
//! - The QR image is served under `/auth-token-service/v1/login/qr-code-image`.
//!   The `imagePath` in the create answer omits that first segment, so opening
//!   it literally answers 404.
//!
//! Nothing here logs, returns or stores the private key beyond the page that is
//! showing the code. The cookie it produces is sealed by `vault` before it
//! touches the disk, exactly like a pasted one.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{FailureCode, LaunchError};
use crate::http;

const CREATE: &str = "https://apis.roblox.com/auth-token-service/v1/login/create";
const STATUS: &str = "https://apis.roblox.com/auth-token-service/v1/login/status";
const CANCEL: &str = "https://apis.roblox.com/auth-token-service/v1/login/cancel";
const QR: &str = "https://apis.roblox.com/auth-token-service/v1/login/qr-code-image";
const EXCHANGE: &str = "https://auth.roblox.com/v2/login";
const SITE: &str = "https://www.roblox.com";

/// Where a person approves the code by hand. Shown on the panel as the fallback
/// for anyone without the mobile app.
pub const CONFIRM: &str = "https://www.roblox.com/crossdevicelogin/ConfirmCode";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub code: String,
    pub private_key: String,
    /// Roblox's own expiry, so the countdown is theirs and not a guess.
    pub expires_at: String,
    pub expires_in: i64,
    /// The QR as a data URL, or empty when the image could not be fetched. The
    /// code alone is enough to sign in, so a missing picture is not a failure.
    pub qr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Waiting for the person to enter the code.
    Created,
    /// Their account is attached but they have not approved yet.
    UserLinked,
    /// Approved. The only state that can be exchanged.
    Validated,
    Cancelled,
    TimedOut,
    /// A status this build does not recognise. Treated as "keep waiting" by the
    /// panel, because dropping a live attempt is worse than polling once more.
    Unknown,
}

impl State {
    fn of(raw: &str) -> Self {
        match raw {
            "Created" => State::Created,
            "UserLinked" => State::UserLinked,
            "Validated" => State::Validated,
            "Cancelled" => State::Cancelled,
            "TimedOut" => State::TimedOut,
            _ => State::Unknown,
        }
    }

    pub fn is_final(self) -> bool {
        matches!(self, State::Validated | State::Cancelled | State::TimedOut)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Poll {
    pub state: State,
    pub account_name: Option<String>,
    pub account_picture: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Created {
    code: String,
    private_key: String,
    #[serde(default)]
    expiration_time: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusAnswer {
    status: String,
    #[serde(default)]
    account_name: Option<String>,
    #[serde(default)]
    account_picture: Option<String>,
    #[serde(default)]
    expiration_time: Option<String>,
}

/// Starts an attempt. No authentication goes out with this call, because the
/// service has none to ask for.
pub fn create() -> Result<Attempt, LaunchError> {
    let answer = http::post_raw(CREATE, "{}", &[])?;

    if answer.status == 429 {
        let reset = answer
            .header("x-ratelimit-reset")
            .and_then(|value| value.parse::<u64>().ok())
            .map(|seconds| format!(" Try again in {seconds} seconds."))
            .unwrap_or_else(|| " Try again in a minute.".to_string());
        return Err(LaunchError::new(
            FailureCode::TicketThrottled,
            format!("Roblox allows ten codes a minute and this one hit the limit.{reset}"),
        )
        .with_action("Retry"));
    }

    if !(200..300).contains(&answer.status) {
        return Err(LaunchError::new(
            FailureCode::Network,
            format!(
                "Roblox would not create a sign-in code: {}",
                answer.reason()
            ),
        )
        .with_action("Retry"));
    }

    let created: Created = serde_json::from_str(&answer.body).map_err(|e| {
        LaunchError::new(
            FailureCode::Network,
            format!("Roblox answered in a shape this build does not know: {e}"),
        )
    })?;

    let expires_in = seconds_until(&created.expiration_time).unwrap_or(120);

    Ok(Attempt {
        code: created.code.clone(),
        private_key: created.private_key.clone(),
        expires_at: created.expiration_time,
        expires_in,
        qr: qr_data_url(&created.private_key, &created.code).unwrap_or_default(),
    })
}

fn qr_data_url(private_key: &str, code: &str) -> Option<String> {
    let url = format!("{QR}?key={private_key}&code={code}");
    let answer = http::get_bytes(&url).ok()?;
    if answer.0 != 200 {
        return None;
    }
    Some(format!("data:image/png;base64,{}", base64(&answer.1)))
}

/// One poll. `Validated` is the only state worth exchanging.
pub fn poll(code: &str, private_key: &str) -> Result<Poll, LaunchError> {
    let body = serde_json::json!({ "code": code, "privateKey": private_key }).to_string();
    let answer = http::post_json(STATUS, &body, &[])?;

    // A 400 here is the service saying it has never heard of this token: it was
    // cancelled elsewhere or it expired. Both end the attempt, so neither is
    // reported as a network fault the person should retry.
    if answer.status == 400 {
        return Ok(Poll {
            state: State::TimedOut,
            account_name: None,
            account_picture: None,
            expires_at: None,
        });
    }

    if !(200..300).contains(&answer.status) {
        return Err(LaunchError::new(
            FailureCode::Network,
            format!("The sign-in code could not be checked: {}", answer.reason()),
        )
        .with_action("Retry"));
    }

    let status: StatusAnswer = serde_json::from_str(&answer.body).map_err(|e| {
        LaunchError::new(
            FailureCode::Network,
            format!("Roblox answered in a shape this build does not know: {e}"),
        )
    })?;

    Ok(Poll {
        state: State::of(&status.status),
        account_name: status.account_name.filter(|name| !name.is_empty()),
        account_picture: status.account_picture.filter(|url| !url.is_empty()),
        expires_at: status.expiration_time,
    })
}

/// Frees the pending approval. Roblox keeps a created token live until it
/// expires, so backing out has to say so rather than leave it hanging on the
/// person's account.
pub fn cancel(code: &str, private_key: &str) -> Result<(), LaunchError> {
    let body = serde_json::json!({ "code": code, "privateKey": private_key }).to_string();
    let answer = http::post_json(CANCEL, &body, &[])?;

    if (200..300).contains(&answer.status) {
        return Ok(());
    }

    Err(LaunchError::new(
        FailureCode::Network,
        format!("The sign-in code was not cancelled: {}", answer.reason()),
    ))
}

/// Trades an approved code for a session cookie.
///
/// This is the same endpoint the website signs in with, so the answer carries
/// `.ROBLOSECURITY` in `set-cookie` and the account in the body. The cookie is
/// returned, never logged, and the caller seals it immediately.
pub fn exchange(code: &str, private_key: &str) -> Result<String, LaunchError> {
    let body = serde_json::json!({
        "ctype": "AuthToken",
        "cvalue": code,
        "password": private_key,
    })
    .to_string();

    let answer = http::post_json(EXCHANGE, &body, &[("Origin", SITE)])?;

    if !(200..300).contains(&answer.status) {
        let reason = http::error_message(&answer.body)
            .unwrap_or_else(|| format!("Roblox answered {}", answer.status));
        return Err(LaunchError::new(
            FailureCode::SessionExpired,
            format!("Roblox did not accept that code: {reason}"),
        )
        .with_action("Start again"));
    }

    for header in answer.headers_named("set-cookie") {
        if let Some(value) = cookie_value(header, ".ROBLOSECURITY") {
            return Ok(value);
        }
    }

    Err(LaunchError::new(
        FailureCode::SessionExpired,
        "Roblox accepted the approval but sent no session with it.",
    )
    .with_action("Start again"))
}

/// Pulls one cookie out of a `set-cookie` header, whatever else is in it.
fn cookie_value(header: &str, name: &str) -> Option<String> {
    header.split(';').find_map(|part| {
        let part = part.trim();
        let rest = part.strip_prefix(name)?;
        let value = rest.strip_prefix('=')?.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

// ---- the small pieces, kept here so the module has no extra dependencies ----

/// Seconds from now until an ISO timestamp without a zone, treated as UTC.
fn seconds_until(iso: &str) -> Option<i64> {
    let target = unix_from_iso(iso)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some((target - now).max(0))
}

fn unix_from_iso(iso: &str) -> Option<i64> {
    let (date, time) = iso.split_once('T')?;
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;

    let mut clock = time.split(':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    let second: i64 = clock.next()?.split('.').next()?.parse().ok()?;

    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_status_names_roblox_actually_sends() {
        assert_eq!(State::of("Created"), State::Created);
        assert_eq!(State::of("UserLinked"), State::UserLinked);
        assert_eq!(State::of("Validated"), State::Validated);
        assert_eq!(State::of("Cancelled"), State::Cancelled);
        assert_eq!(State::of("TimedOut"), State::TimedOut);
        // An unrecognised name must not end a live attempt.
        assert_eq!(State::of("SomethingNew"), State::Unknown);
        assert!(!State::Unknown.is_final());
        assert!(State::Validated.is_final());
    }

    #[test]
    fn pulls_the_session_out_of_a_set_cookie_header() {
        let header = ".ROBLOSECURITY=ABC123; Domain=.roblox.com; Path=/; Secure; HttpOnly";
        assert_eq!(
            cookie_value(header, ".ROBLOSECURITY").as_deref(),
            Some("ABC123")
        );
        assert_eq!(cookie_value("other=1; Path=/", ".ROBLOSECURITY"), None);
        assert_eq!(
            cookie_value(".ROBLOSECURITY=; Path=/", ".ROBLOSECURITY"),
            None
        );
    }

    #[test]
    fn turns_roblox_timestamps_into_seconds() {
        assert_eq!(unix_from_iso("1970-01-01T00:00:00").as_ref(), Some(&0));
        assert_eq!(unix_from_iso("1970-01-02T00:00:00").as_ref(), Some(&86_400));
        // A real answer from the service, cross-checked against `date -u -d`.
        assert_eq!(
            unix_from_iso("2026-11-17T21:15:41.4212883").as_ref(),
            Some(&1_794_950_141)
        );
    }

    #[test]
    fn encodes_a_png_header_the_way_a_browser_expects() {
        let png = [0x89u8, 0x50, 0x4e, 0x47];
        assert_eq!(base64(&png), "iVBORw==");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn reads_the_message_out_of_an_error_envelope() {
        let body = r#"{"errors":[{"code":0,"message":"Token has expired"}]}"#;
        assert_eq!(
            http::error_message(body).as_deref(),
            Some("Token has expired")
        );
        assert_eq!(http::error_message("not json"), None);
    }
}
