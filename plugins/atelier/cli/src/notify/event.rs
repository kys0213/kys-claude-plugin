//! Channel-agnostic notify event. Channels render from this — the event owns
//! only its identity and its canonical (structured JSON) serialization;
//! channel-specific renderings (Slack text, desktop banner) live in each
//! channel module so adding a rendering never modifies this type (OCP).

use crate::notify::types::{AskQuestionPayload, NotificationPayload};
use serde_json::json;

pub enum Event<'p> {
    AskQuestion(&'p AskQuestionPayload),
    Notification(&'p NotificationPayload),
}

impl Event<'_> {
    /// Stable event kind name — the `event` field of the structured JSON and
    /// the value channel `events` filters match against.
    pub fn kind(&self) -> &'static str {
        match self {
            Event::AskQuestion(_) => "ask_user_question",
            Event::Notification(_) => "notification",
        }
    }

    /// Project directory the event originated from, when the hook supplied it.
    pub fn cwd(&self) -> Option<&str> {
        match self {
            Event::AskQuestion(p) => p.cwd.as_deref(),
            Event::Notification(p) => p.cwd.as_deref(),
        }
    }

    /// Canonical structured serialization — the wire format shared by every
    /// machine-readable channel (webhook POST body, file JSONL line). Single
    /// line, no embedded newlines.
    pub fn structured_json(&self) -> String {
        match self {
            Event::AskQuestion(p) => json!({
                "event": "ask_user_question",
                "session_id": p.session_id,
                "cwd": p.cwd,
                "questions": p.questions,
            })
            .to_string(),
            Event::Notification(p) => json!({
                "event": "notification",
                "session_id": p.session_id,
                "cwd": p.cwd,
                "message": p.message,
            })
            .to_string(),
        }
    }
}
