//! Notify command cores: fan a hook event out to every resolved channel and
//! collect per-channel reports. Delivery failures are data in the report, not
//! errors — the CLI edge always exits 0 because these run as advisory hooks
//! (PreToolUse on `AskUserQuestion`, Notification) and must never block.

use crate::notify::message::{
    ask_question_desktop, notification_desktop, notification_slack_body, notification_webhook_body,
    slack_body, webhook_body,
};
use crate::notify::transport::{DesktopNotifier, FileAppender, HttpPoster};
use crate::notify::types::{
    AskQuestionPayload, Channel, NotificationPayload, NotifyOutput, SendReport,
};

pub struct NotifyDeps<'a> {
    pub poster: &'a dyn HttpPoster,
    pub appender: &'a dyn FileAppender,
    pub desktop: &'a dyn DesktopNotifier,
}

/// Delivers an `AskUserQuestion` payload. No channels (not configured) or no
/// questions (foreign/malformed payload) → silent no-op with an empty report.
pub fn run_ask_question(
    deps: &NotifyDeps,
    channels: &[Channel],
    payload: &AskQuestionPayload,
) -> NotifyOutput {
    if payload.questions.is_empty() {
        return NotifyOutput {
            notified: false,
            reports: Vec::new(),
        };
    }
    deliver(
        deps,
        channels,
        &slack_body(payload),
        &webhook_body(payload),
        &ask_question_desktop(payload),
    )
}

/// Delivers a Notification payload (permission request, idle wait). No
/// channels or no message → silent no-op with an empty report.
pub fn run_notification(
    deps: &NotifyDeps,
    channels: &[Channel],
    payload: &NotificationPayload,
) -> NotifyOutput {
    if payload.message.is_none() {
        return NotifyOutput {
            notified: false,
            reports: Vec::new(),
        };
    }
    deliver(
        deps,
        channels,
        &notification_slack_body(payload),
        &notification_webhook_body(payload),
        &notification_desktop(payload),
    )
}

/// Shared fan-out: delivers the channel-appropriate body to each channel —
/// push channels get an HTTP POST, the file channel gets a JSONL append of
/// the structured (webhook) body so pollers read one event per line, and the
/// desktop channel gets an OS banner (title, body).
fn deliver(
    deps: &NotifyDeps,
    channels: &[Channel],
    slack_body: &str,
    webhook_body: &str,
    desktop: &(String, String),
) -> NotifyOutput {
    let reports: Vec<SendReport> = channels
        .iter()
        .map(|channel| {
            let result = match channel {
                Channel::Slack { webhook_url } => deps.poster.post_json(webhook_url, slack_body),
                Channel::Webhook { url } => deps.poster.post_json(url, webhook_body),
                Channel::File { path } => deps.appender.append_line(path, webhook_body),
                Channel::Desktop => deps.desktop.notify(&desktop.0, &desktop.1),
            };
            match result {
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
            }
        })
        .collect();

    NotifyOutput {
        notified: reports.iter().any(|r| r.ok),
        reports,
    }
}
