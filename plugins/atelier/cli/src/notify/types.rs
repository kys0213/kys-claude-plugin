//! Notify subsystem data types: parsed hook payloads and the JSON report the
//! CLI prints. Channel behavior lives under `channel/`; the canonical event
//! wrapper lives in `event`.

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
