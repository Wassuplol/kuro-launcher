//! One blocking HTTP surface for the whole app.
//!
//! Two things live here because everything that talks to Roblox needs them and
//! nothing should write its own copy: the agent (Windows' own TLS, explicit user
//! agent, status codes returned rather than turned into errors), and the CSRF
//! dance that every write at Roblox performs before it answers.
//!
//! The dance is: POST without a token, read `x-csrf-token` off the 403, POST
//! again with it. The token stays valid for minutes and, measured, the same
//! value is served by `apis.roblox.com` and `auth.roblox.com`, so one cache for
//! the process is correct rather than a per-host map.

use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;

use crate::error::{FailureCode, LaunchError};

static CSRF: Mutex<Option<String>> = Mutex::new(None);

/// The blocking agent, with a timeout and a user agent that names this app.
pub fn agent(timeout: Duration) -> ureq::Agent {
    crate::supply::agent(timeout)
}

/// A response, held as plain data so callers do not carry ureq's types around.
pub struct Answer {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Answer {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Every value for a repeated header, which is how `set-cookie` arrives.
    pub fn headers_named(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .collect()
    }

    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Roblox's own sentence, which is always more useful than anything written
    /// here. Falls back to the status code when the body is not the envelope.
    pub fn reason(&self) -> String {
        error_message(&self.body).unwrap_or_else(|| format!("Roblox answered {}", self.status))
    }
}

pub fn error_message(body: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Envelope {
        errors: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        message: String,
    }

    serde_json::from_str::<Envelope>(body)
        .ok()
        .and_then(|envelope| envelope.errors.into_iter().next())
        .map(|item| item.message)
}

fn finish(
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<Answer, LaunchError> {
    let response = response.map_err(|e| {
        LaunchError::new(
            FailureCode::Network,
            format!("Roblox could not be reached: {e}"),
        )
        .with_action("Retry")
    })?;

    let status = response.status().as_u16();
    let mut headers = Vec::new();
    for (name, value) in response.headers() {
        if let Ok(text) = value.to_str() {
            headers.push((name.as_str().to_string(), text.to_string()));
        }
    }
    let body = response.into_body().read_to_string().unwrap_or_default();

    Ok(Answer {
        status,
        headers,
        body,
    })
}

/// A GET, with no token handling: nothing Roblox serves over GET needs one.
pub fn get(url: &str) -> Result<Answer, LaunchError> {
    finish(agent(Duration::from_secs(15)).get(url).call())
}

/// A GET that keeps the bytes, for images and packages rather than JSON.
pub fn get_bytes(url: &str) -> Result<(u16, Vec<u8>), LaunchError> {
    let response = agent(Duration::from_secs(20))
        .get(url)
        .call()
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("Roblox could not be reached: {e}"),
            )
        })?;
    let status = response.status().as_u16();
    let bytes = response.into_body().read_to_vec().unwrap_or_default();
    Ok((status, bytes))
}

pub fn post_raw(
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<Answer, LaunchError> {
    let mut request = agent(Duration::from_secs(15))
        .post(url)
        .header("Content-Type", "application/json");
    for (name, value) in headers {
        request = request.header(name.as_str(), value.as_str());
    }
    finish(request.send(body))
}

/// A JSON POST that performs the CSRF dance when it has to.
pub fn post_json(url: &str, body: &str, extra: &[(&str, &str)]) -> Result<Answer, LaunchError> {
    let mut headers: Vec<(String, String)> = extra
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect();

    if let Some(token) = CSRF.lock().ok().and_then(|slot| slot.clone()) {
        headers.push(("x-csrf-token".to_string(), token));
        let answer = post_raw(url, body, &headers)?;
        if answer.status != 403 {
            return Ok(answer);
        }
        headers.pop();
    }

    let first = post_raw(url, body, &headers)?;
    let Some(token) = first.header("x-csrf-token").map(str::to_string) else {
        // A 403 without a token is not the dance, so a retry would only repeat
        // the same refusal.
        return Ok(first);
    };

    if let Ok(mut slot) = CSRF.lock() {
        *slot = Some(token.clone());
    }
    headers.push(("x-csrf-token".to_string(), token));
    post_raw(url, body, &headers)
}
