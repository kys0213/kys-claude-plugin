//! Notify command cores — event gates + channel fan-out only. A channel is a
//! declared command (`ChannelSpec`); delivery = render the event's template
//! variables, substitute them into argv/stdin, and hand the result to the
//! runner. Delivery failures are data in the report, not errors — the CLI
//! edge always exits 0 because these run as advisory hooks (PreToolUse on
//! `AskUserQuestion`, Notification) and must never block.

use crate::notify::config::ChannelSpec;
use crate::notify::event::Event;
use crate::notify::exec::CommandRunner;
use crate::notify::render::{substitute, vars};
use crate::notify::types::{AskQuestionPayload, NotificationPayload, NotifyOutput, SendReport};

/// Delivers an `AskUserQuestion` payload. No channels (not configured) or no
/// questions (foreign/malformed payload) → silent no-op with an empty report.
pub fn run_ask_question(
    runner: &dyn CommandRunner,
    channels: &[ChannelSpec],
    payload: &AskQuestionPayload,
) -> NotifyOutput {
    if payload.questions.is_empty() {
        return NotifyOutput {
            notified: false,
            reports: Vec::new(),
        };
    }
    deliver(runner, channels, &Event::AskQuestion(payload))
}

/// Delivers a Notification payload (permission request, idle wait). No
/// channels or no message → silent no-op with an empty report.
pub fn run_notification(
    runner: &dyn CommandRunner,
    channels: &[ChannelSpec],
    payload: &NotificationPayload,
) -> NotifyOutput {
    if payload.message.is_none() {
        return NotifyOutput {
            notified: false,
            reports: Vec::new(),
        };
    }
    deliver(runner, channels, &Event::Notification(payload))
}

/// Fans the event out to every accepting channel, collecting per-channel
/// reports. Channels filtered out by their `events` list produce no report.
fn deliver(runner: &dyn CommandRunner, channels: &[ChannelSpec], event: &Event) -> NotifyOutput {
    let vars = vars(event);
    let reports: Vec<SendReport> = channels
        .iter()
        .filter(|spec| spec.accepts(event.kind()))
        .map(|spec| {
            let argv: Vec<String> = spec.exec.iter().map(|a| substitute(a, &vars)).collect();
            let stdin = spec.stdin.as_deref().map(|t| substitute(t, &vars));
            match runner.run(&argv, stdin.as_deref(), spec.timeout_secs) {
                Ok(()) => SendReport {
                    channel: spec.name.clone(),
                    ok: true,
                    error: None,
                },
                Err(e) => SendReport {
                    channel: spec.name.clone(),
                    ok: false,
                    error: Some(e),
                },
            }
        })
        .collect();

    NotifyOutput {
        notified: reports.iter().any(|r| r.ok),
        reports,
    }
}
