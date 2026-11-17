use serde::Serialize;
use std::fmt;

/// Every failure the app can name, from the taxonomy in the master prompt.
///
/// The frontend renders `code` plus the offered action. It never prints a raw
/// error string, so a new failure needs a code here before it can reach a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    ClientMissing,
    ClientCorrupt,
    SessionExpired,
    SessionFlagged,
    TicketThrottled,
    MutexHeld,
    AlreadyRunning,
    QueueFull,
    FlagRejected,
    EarlyExit,
    Network,
    UpdateInFlight,
    /// The settings file exists but could not be read or parsed.
    ConfigUnreadable,
    /// The settings file could not be written.
    ConfigUnwritable,
    /// A command asked for something that is not there.
    NotFound,
    /// A fault with no better name. The message says what happened.
    Internal,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchError {
    pub code: FailureCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

impl LaunchError {
    pub fn new(code: FailureCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            action: None,
        }
    }

    /// The action is a short, verb-first label the UI can offer as a button.
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LaunchError {}
