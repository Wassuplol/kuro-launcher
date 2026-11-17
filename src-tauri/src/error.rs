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
    /// A channel the version service will not answer without a signed-in
    /// session. Measured, not assumed: zcanary and zintegration refuse.
    ChannelUnavailable,
    /// The person asked for the work to stop, and it stopped.
    Cancelled,
    /// A fault with no better name. The message says what happened.
    Internal,
}

impl FailureCode {
    /// The wire name, for places that need the code as text without going
    /// through serialisation, such as a progress line.
    pub fn as_str(self) -> &'static str {
        match self {
            FailureCode::ClientMissing => "client_missing",
            FailureCode::ClientCorrupt => "client_corrupt",
            FailureCode::SessionExpired => "session_expired",
            FailureCode::SessionFlagged => "session_flagged",
            FailureCode::TicketThrottled => "ticket_throttled",
            FailureCode::MutexHeld => "mutex_held",
            FailureCode::AlreadyRunning => "already_running",
            FailureCode::QueueFull => "queue_full",
            FailureCode::FlagRejected => "flag_rejected",
            FailureCode::EarlyExit => "early_exit",
            FailureCode::Network => "network",
            FailureCode::UpdateInFlight => "update_in_flight",
            FailureCode::ConfigUnreadable => "config_unreadable",
            FailureCode::ConfigUnwritable => "config_unwritable",
            FailureCode::NotFound => "not_found",
            FailureCode::ChannelUnavailable => "channel_unavailable",
            FailureCode::Cancelled => "cancelled",
            FailureCode::Internal => "internal",
        }
    }
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
