//! Message body formatting per channel kind. Pure functions: same payload →
//! same body, so the formats are locked by unit tests.

use crate::notify::types::{AskQuestionPayload, NotificationPayload};
use serde_json::json;

/// Slack Incoming Webhook body: a single human-readable `text` message.
pub fn slack_body(payload: &AskQuestionPayload) -> String {
    let mut text = String::from(":question: *Claude 세션이 응답을 기다리고 있습니다*\n");
    if let Some(cwd) = &payload.cwd {
        text.push_str(&format!("프로젝트: `{cwd}`\n"));
    }
    for q in &payload.questions {
        match &q.header {
            Some(h) => text.push_str(&format!("\n*[{h}]* {}\n", q.question)),
            None => text.push_str(&format!("\n*{}*\n", q.question)),
        }
        for opt in &q.options {
            text.push_str(&format!("• {opt}\n"));
        }
    }
    json!({ "text": text }).to_string()
}

/// Generic webhook body: the structured payload itself, for receivers that
/// format on their side (Discord relays, mail bridges, custom servers).
pub fn webhook_body(payload: &AskQuestionPayload) -> String {
    json!({
        "event": "ask_user_question",
        "session_id": payload.session_id,
        "cwd": payload.cwd,
        "questions": payload.questions,
    })
    .to_string()
}

/// Desktop banner (title, body) for `AskUserQuestion`. Banners truncate, so
/// only the first question + a count of the rest.
pub fn ask_question_desktop(payload: &AskQuestionPayload) -> (String, String) {
    let mut body = match payload.questions.first() {
        Some(q) => q.question.clone(),
        None => String::new(),
    };
    if payload.questions.len() > 1 {
        body.push_str(&format!(" (외 {}개)", payload.questions.len() - 1));
    }
    if let Some(cwd) = &payload.cwd {
        body.push_str(&format!("\n{cwd}"));
    }
    ("Claude 질문 대기".to_string(), body)
}

/// Desktop banner (title, body) for Notification hook events.
pub fn notification_desktop(payload: &NotificationPayload) -> (String, String) {
    let mut body = payload.message.clone().unwrap_or_default();
    if let Some(cwd) = &payload.cwd {
        body.push_str(&format!("\n{cwd}"));
    }
    ("Claude 입력 대기".to_string(), body)
}

/// Slack body for Notification hook events (permission requests, idle waits).
pub fn notification_slack_body(payload: &NotificationPayload) -> String {
    let mut text = String::from(":bell: *Claude 세션이 입력을 기다리고 있습니다*\n");
    if let Some(cwd) = &payload.cwd {
        text.push_str(&format!("프로젝트: `{cwd}`\n"));
    }
    if let Some(message) = &payload.message {
        text.push_str(&format!("\n{message}\n"));
    }
    json!({ "text": text }).to_string()
}

/// Generic webhook body for Notification hook events.
pub fn notification_webhook_body(payload: &NotificationPayload) -> String {
    json!({
        "event": "notification",
        "session_id": payload.session_id,
        "cwd": payload.cwd,
        "message": payload.message,
    })
    .to_string()
}
