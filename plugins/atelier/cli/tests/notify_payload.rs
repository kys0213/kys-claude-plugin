//! Black-box tests for `AskUserQuestion` PreToolUse payload parsing: the
//! schema is locked here so hook input drift surfaces as a test failure, and
//! malformed input must degrade to an empty (no-op) payload, never an error.

use atelier::notify::types::{AskQuestionPayload, NotificationPayload};

#[test]
fn parses_full_ask_user_question_payload() {
    let raw = r#"{
        "session_id": "abc-123",
        "cwd": "/work/repo",
        "tool_name": "AskUserQuestion",
        "tool_input": {
            "questions": [
                {
                    "question": "Which auth method?",
                    "header": "Auth",
                    "multiSelect": false,
                    "options": [
                        { "label": "OAuth", "description": "standard flow" },
                        { "label": "API key", "description": "simple" }
                    ]
                },
                {
                    "question": "Pick features",
                    "multiSelect": true,
                    "options": ["a", "b"]
                }
            ]
        }
    }"#;

    let p = AskQuestionPayload::parse(raw);
    assert_eq!(p.session_id.as_deref(), Some("abc-123"));
    assert_eq!(p.cwd.as_deref(), Some("/work/repo"));
    assert_eq!(p.questions.len(), 2);

    let q0 = &p.questions[0];
    assert_eq!(q0.header.as_deref(), Some("Auth"));
    assert_eq!(q0.question, "Which auth method?");
    assert_eq!(q0.options, vec!["OAuth", "API key"]);
    assert!(!q0.multi_select);

    let q1 = &p.questions[1];
    assert_eq!(q1.header, None);
    assert_eq!(q1.options, vec!["a", "b"]);
    assert!(q1.multi_select);
}

#[test]
fn malformed_json_yields_empty_payload() {
    let p = AskQuestionPayload::parse("not json at all {");
    assert_eq!(p, AskQuestionPayload::default());
    assert!(p.questions.is_empty());
}

#[test]
fn foreign_tool_payload_yields_no_questions() {
    // A payload from some other tool (no tool_input.questions) must parse to
    // zero questions so the command no-ops.
    let raw = r#"{"session_id":"s","cwd":"/w","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
    let p = AskQuestionPayload::parse(raw);
    assert!(p.questions.is_empty());
}

#[test]
fn parses_notification_payload() {
    let raw = r#"{
        "session_id": "abc-123",
        "cwd": "/work/repo",
        "hook_event_name": "Notification",
        "message": "Claude needs your permission to use Bash"
    }"#;
    let p = NotificationPayload::parse(raw);
    assert_eq!(p.session_id.as_deref(), Some("abc-123"));
    assert_eq!(p.cwd.as_deref(), Some("/work/repo"));
    assert_eq!(
        p.message.as_deref(),
        Some("Claude needs your permission to use Bash")
    );
}

#[test]
fn malformed_or_empty_notification_yields_no_message() {
    assert_eq!(
        NotificationPayload::parse("nope {"),
        NotificationPayload::default()
    );
    let p = NotificationPayload::parse(r#"{"cwd":"/w","message":""}"#);
    assert_eq!(p.message, None);
}

#[test]
fn entries_without_question_text_are_dropped() {
    let raw = r#"{"tool_input":{"questions":[{"header":"only header"},{"question":"kept"}]}}"#;
    let p = AskQuestionPayload::parse(raw);
    assert_eq!(p.questions.len(), 1);
    assert_eq!(p.questions[0].question, "kept");
}
