//! Notify subsystem types. `AskQuestionPayload` is the parsed PreToolUse
//! payload for the `AskUserQuestion` tool; `Channel` is a resolved delivery
//! target; `NotifyOutput` is the JSON report the CLI prints.

use serde::Serialize;

/// One question extracted from the `AskUserQuestion` tool input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Question {
    pub header: Option<String>,
    pub question: String,
    pub options: Vec<String>,
    #[serde(rename = "multiSelect")]
    pub multi_select: bool,
}

/// Parsed PreToolUse payload for `AskUserQuestion`. Parsing is swallow-all:
/// malformed input yields an empty payload so the hook stays a no-op.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AskQuestionPayload {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub questions: Vec<Question>,
}

/// Parsed Notification hook payload (permission requests, idle waiting).
/// Parsing is swallow-all: malformed input yields an empty payload so the
/// hook stays a no-op.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct NotificationPayload {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub message: Option<String>,
}

/// A resolved delivery channel. New channel kinds (e.g. email) extend this
/// enum plus `command`'s body dispatch — nothing else changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Channel {
    Slack { webhook_url: String },
    Webhook { url: String },
}

impl Channel {
    /// Stable kind name used in reports and the config file `type` field.
    pub fn kind(&self) -> &'static str {
        match self {
            Channel::Slack { .. } => "slack",
            Channel::Webhook { .. } => "webhook",
        }
    }
}

/// Per-channel delivery result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SendReport {
    pub channel: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Overall command result. `notified` is true when at least one channel
/// accepted the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotifyOutput {
    pub notified: bool,
    pub reports: Vec<SendReport>,
}
