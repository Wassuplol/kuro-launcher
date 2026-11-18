//! The assistant: one forwarded conversation, streamed back as it arrives.
//!
//! Nothing here is a second product. The app already holds the account, the client
//! versions, the library and the failures, so the assistant is a pane that can be
//! asked about them, with the same honesty rules as the rest of the UI: it names
//! what it knows, refuses to invent ids, and never acts on its own.
//!
//! Three decisions worth stating:
//!
//! - **The key is sealed, not stored.** It is written through DPAPI by the same
//!   vault that holds session cookies, and it is never returned to the page: the
//!   settings screen learns `keySet: true` and nothing else.
//! - **Configuration resolves saved, then environment, then Hermes.** A build
//!   handed to somebody else starts blank and says so, instead of shipping mine.
//! - **Streaming is a `Read` on the response body.** The endpoint answers
//!   `text/event-stream` with `data: {...}` chunks and `data: [DONE]`; each delta is
//!   forwarded as it lands, because a pane that waits for the whole answer looks
//!   frozen for the ten seconds that matter.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{FailureCode, LaunchError};
use crate::settings::Assistant;
use crate::supply::agent;

/// The model this app opens with. Cheap, fast, good enough at instructions, and
/// probed against the endpoint before it was written here.
const DEFAULT_MODEL: &str = "z-ai/glm-5.3-flash";
const DEFAULT_BASE: &str = "https://nano-gpt.com/api/v1";

/// A streamed answer is allowed to breathe, but not forever.
const STREAM_TIMEOUT: Duration = Duration::from_secs(180);

/// One turn on the wire, exactly as the OpenAI shape wants it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub role: String,
    pub content: String,
}

/// What the screen is allowed to know about the connection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub base_url: String,
    pub model: String,
    /// Whether a key is set. The key itself never crosses this boundary.
    pub key_set: bool,
    /// "saved", "environment", "hermes" or "none", so a screen can say where the
    /// key came from instead of implying the person typed it.
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub base_url: String,
    pub model: String,
    pub key: Option<String>,
    pub source: &'static str,
}

/// `%LOCALAPPDATA%\hermes\.env`, the file this machine already keeps provider keys
/// in. Reading it is a convenience for one person's build, and it is last in the
/// chain so a saved setting always wins.
fn hermes_env() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let path = PathBuf::from(local).join("hermes").join(".env");
    path.exists().then_some(path)
}

/// Pulls one key out of a dotenv file.
///
/// A blank line or a comment has to be *skipped*, not treated as the end of the
/// file: the first version used `?` here, so the very first empty line returned
/// None and the whole fallback silently reported "no key".
fn parse_env(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != name {
            continue;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn from_hermes_env(name: &str) -> Option<String> {
    let text = std::fs::read_to_string(hermes_env()?).ok()?;
    parse_env(&text, name)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Saved setting, then process environment, then the Hermes `.env` on this machine.
pub fn resolve(saved: &Assistant) -> Resolved {
    let base_url = if !saved.base_url.trim().is_empty() {
        saved.base_url.trim().trim_end_matches('/').to_string()
    } else {
        env("KURO_ASSISTANT_BASE")
            .or_else(|| env("OPENAI_BASE_URL"))
            .unwrap_or_else(|| DEFAULT_BASE.to_string())
            .trim_end_matches('/')
            .to_string()
    };

    let model = if !saved.model.trim().is_empty() {
        saved.model.trim().to_string()
    } else {
        env("KURO_ASSISTANT_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_string())
    };

    let sealed = crate::identity::unhex(&saved.key_sealed)
        .and_then(|bytes| crate::identity::vault::unseal(&bytes).ok())
        .filter(|key| !key.trim().is_empty());

    let (key, source) = match sealed {
        Some(key) => (Some(key), "saved"),
        None => match env("KURO_ASSISTANT_KEY").or_else(|| env("OPENAI_API_KEY")) {
            Some(key) => (Some(key), "environment"),
            None => match from_hermes_env("OPENAI_API_KEY") {
                Some(key) => (Some(key), "hermes"),
                None => (None, "none"),
            },
        },
    };

    Resolved {
        base_url,
        model,
        key,
        source,
    }
}

impl Resolved {
    pub fn connection(&self) -> Connection {
        Connection {
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            key_set: self.key.is_some(),
            source: self.source.to_string(),
        }
    }

    fn key(&self) -> Result<&str, LaunchError> {
        self.key.as_deref().ok_or_else(|| {
            LaunchError::new(
                FailureCode::ConfigUnreadable,
                "The assistant has no key. One can be set in Settings, and it is stored sealed.",
            )
            .with_action("Set the key in Settings")
        })
    }
}

#[derive(Debug, Deserialize)]
struct ModelList {
    #[serde(default)]
    data: Vec<ModelRow>,
}

#[derive(Debug, Deserialize)]
struct ModelRow {
    id: String,
}

/// The models the endpoint serves, for the picker. Sorted and de-duplicated because
/// the same model can appear under a couple of routes.
pub fn models(resolved: &Resolved) -> Result<Vec<String>, LaunchError> {
    let key = resolved.key()?;
    let url = format!("{}/models", resolved.base_url);
    let answer = agent(Duration::from_secs(20))
        .get(&url)
        .header("Authorization", &format!("Bearer {key}"))
        .call()
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("The model list could not be read: {e}."),
            )
            .with_action("Retry")
        })?;

    let status = answer.status().as_u16();
    let body = answer.into_body().read_to_string().unwrap_or_default();
    if status == 401 || status == 403 {
        return Err(LaunchError::new(
            FailureCode::ConfigUnreadable,
            "The endpoint refused the key while listing models.",
        )
        .with_action("Check the key in Settings"));
    }
    if status >= 400 {
        return Err(LaunchError::new(
            FailureCode::Network,
            format!(
                "The model list answered {status}: {}",
                body.chars().take(160).collect::<String>()
            ),
        )
        .with_action("Retry"));
    }

    let parsed: ModelList = serde_json::from_str(&body).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The model list was not the documented shape: {e}."),
        )
    })?;

    let mut ids: Vec<String> = parsed.data.into_iter().map(|row| row.id).collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

#[derive(Debug, Deserialize)]
struct Chunk {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    #[serde(default)]
    delta: Delta,
}

#[derive(Debug, Deserialize, Default)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
}

/// Streams one answer, calling `on_delta` for each piece of text as it lands.
///
/// `cancel` is checked between deltas, so Stop ends the read rather than leaving a
/// socket open until the model finishes talking to nobody.
pub fn stream(
    resolved: &Resolved,
    system: &str,
    turns: &[Turn],
    cancel: Arc<AtomicBool>,
    mut on_delta: impl FnMut(&str),
) -> Result<usize, LaunchError> {
    let key = resolved.key()?;

    let mut messages = Vec::with_capacity(turns.len() + 1);
    messages.push(serde_json::json!({ "role": "system", "content": system }));
    for turn in turns {
        messages.push(serde_json::json!({ "role": turn.role, "content": turn.content }));
    }

    let payload = serde_json::json!({
        "model": resolved.model,
        "messages": messages,
        "stream": true,
        "temperature": 0.4,
        "max_tokens": 900,
    });

    let url = format!("{}/chat/completions", resolved.base_url);
    let mut response = agent(STREAM_TIMEOUT)
        .post(&url)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .send(payload.to_string())
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("The assistant could not be reached: {e}."),
            )
            .with_action("Retry")
        })?;

    let status = response.status().as_u16();
    if status >= 400 {
        let body = response.into_body().read_to_string().unwrap_or_default();
        let detail: String = body.chars().take(200).collect();
        return Err(match status {
            401 | 403 => LaunchError::new(
                FailureCode::ConfigUnreadable,
                format!("The endpoint refused the key ({status}). {detail}"),
            )
            .with_action("Check the key in Settings"),
            429 => LaunchError::new(
                FailureCode::TicketThrottled,
                format!("The endpoint is rate limiting this key. {detail}"),
            )
            .with_action("Wait a minute"),
            _ => LaunchError::new(
                FailureCode::Network,
                format!("The assistant answered {status}: {detail}"),
            )
            .with_action("Retry"),
        });
    }

    let reader = response.body_mut().as_reader();
    let mut sent = 0usize;

    for line in BufReader::new(reader).lines() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let Ok(line) = line else { break };
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() {
            continue;
        }
        if payload == "[DONE]" {
            break;
        }
        // A malformed chunk is skipped rather than ending the answer: one bad line
        // should not cost the rest of the reply.
        let Ok(chunk) = serde_json::from_str::<Chunk>(payload) else {
            continue;
        };
        for choice in chunk.choices {
            if let Some(text) = choice.delta.content {
                if !text.is_empty() {
                    sent += text.len();
                    on_delta(&text);
                }
            }
        }
    }

    Ok(sent)
}

/// The prompt the app opens with. It is part of the product, so it is written to
/// the same rules as the rest of the copy: short, specific, no flourishes, and
/// honest about what it cannot see.
pub fn system_prompt(context: &str) -> String {
    format!(
        "You are the assistant inside KURO, a desktop launcher for Roblox. You answer \
questions about this app and about the person's own setup, in short plain sentences. \
Sentence case. No em dashes. No exclamation marks. Never invent a place id, a version \
number or an account name: if you do not know, say which screen holds the answer. \
You cannot start a game, add an account or write flags yourself, and you say so when \
asked. When a concrete action would help, end your reply with exactly one line of the form \
ACTION: {{\"open\":\"library\"}} where the page is one of home, search, library, friends, \
launch, accounts, flags, versions, or ACTION: {{\"launch\":<place id>}} only when the \
person gave you that id. The person clicks the button; you never act on your own.\n\n\
What the app knows right now:\n{context}"
    )
}

#[derive(Debug, Deserialize)]
pub struct SendRequest {
    #[serde(default)]
    pub turns: Vec<Turn>,
    /// What the page can see: the current screen, accounts, whether a client runs.
    #[serde(default)]
    pub context: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct StreamedDelta {
    pub text: String,
}

pub const DELTA: &str = "assistant:delta";
pub const DONE: &str = "assistant:done";
pub const FAILED: &str = "assistant:failed";

#[derive(Debug, Serialize, Clone)]
pub struct Finished {
    pub ms: u64,
    pub chars: usize,
}

pub fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_line_does_not_end_the_search() {
        let text = "# comment\n\nOPENAI_BASE_URL=https://example.test/v1\nOPENAI_API_KEY=k-123\n";
        assert_eq!(parse_env(text, "OPENAI_API_KEY").as_deref(), Some("k-123"));
    }

    #[test]
    fn quotes_and_stray_spaces_are_stripped() {
        let text = "  OPENAI_API_KEY = \"k-abcdef\"  \n";
        assert_eq!(
            parse_env(text, "OPENAI_API_KEY").as_deref(),
            Some("k-abcdef")
        );
    }

    #[test]
    fn a_missing_key_is_none_and_not_the_next_line() {
        let text = "OTHER=value\nOPENAI_API_KEY=\n";
        assert_eq!(parse_env(text, "OPENAI_API_KEY"), None);
    }

    #[test]
    fn the_prompt_carries_the_house_rules() {
        let prompt = system_prompt("Screen: home");
        assert!(prompt.contains("No em dashes"));
        assert!(prompt.contains("ACTION:"));
        assert!(prompt.contains("Screen: home"));
    }
}
