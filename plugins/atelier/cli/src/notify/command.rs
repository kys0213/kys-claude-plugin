//! Notify command cores — event gates + channel fan-out only; the commands
//! are channel-agnostic (what a channel does with an event lives under
//! `channel/`). Delivery failures are data in the report, not errors — the
//! CLI edge always exits 0 because these run as advisory hooks (PreToolUse on
//! `AskUserQuestion`, Notification) and must never block.

use crate::notify::channel::NotifyChannel;
use crate::notify::event::Event;
use crate::notify::types::{AskQuestionPayload, NotificationPayload, NotifyOutput, SendReport};

/// Delivers an `AskUserQuestion` payload. No channels (not configured) or no
/// questions (foreign/malformed payload) → silent no-op with an empty report.
pub fn run_ask_question(
    channels: &[Box<dyn NotifyChannel + '_>],
    payload: &AskQuestionPayload,
) -> NotifyOutput {
    if payload.questions.is_empty() {
        return NotifyOutput {
            notified: false,
            reports: Vec::new(),
        };
    }
    deliver(channels, &Event::AskQuestion(payload))
}

/// Delivers a Notification payload (permission request, idle wait). No
/// channels or no message → silent no-op with an empty report.
pub fn run_notification(
    channels: &[Box<dyn NotifyChannel + '_>],
    payload: &NotificationPayload,
) -> NotifyOutput {
    if payload.message.is_none() {
        return NotifyOutput {
            notified: false,
            reports: Vec::new(),
        };
    }
    deliver(channels, &Event::Notification(payload))
}

/// Fans the event out to every channel, collecting per-channel reports.
fn deliver(channels: &[Box<dyn NotifyChannel + '_>], event: &Event) -> NotifyOutput {
    let reports: Vec<SendReport> = channels
        .iter()
        .map(|channel| match channel.send(event) {
            Ok(()) => SendReport {
                channel: channel.kind().to_string(),
                ok: true,
                error: None,
            },
            Err(e) => SendReport {
                channel: channel.kind().to_string(),
                ok: false,
                error: Some(e),
            },
        })
        .collect();

    NotifyOutput {
        notified: reports.iter().any(|r| r.ok),
        reports,
    }
}
