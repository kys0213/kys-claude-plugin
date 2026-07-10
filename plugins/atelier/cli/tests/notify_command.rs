//! Black-box tests for the notify command cores (`ask-question`,
//! `notification`): template substitution into argv/stdin, event filtering,
//! no-op gates, and the failure-is-data (never an error) contract — all
//! observed through a recording stub runner.

mod notify_support;

use atelier::notify::command::{run_ask_question, run_notification};
use atelier::notify::config::{resolve_channels, ChannelSpec, DEFAULT_TIMEOUT_SECS};
use atelier::notify::types::{AskQuestionPayload, NotificationPayload};
use notify_support::{ask_payload, env, fs, StubRunner};

fn notification_payload() -> NotificationPayload {
    NotificationPayload {
        session_id: Some("s1".to_string()),
        cwd: Some("/work/repo".to_string()),
        message: Some("Claude needs your permission to use Bash".to_string()),
    }
}

fn spec(name: &str, exec: &[&str], stdin: Option<&str>) -> ChannelSpec {
    ChannelSpec {
        name: name.to_string(),
        exec: exec.iter().map(|s| s.to_string()).collect(),
        stdin: stdin.map(|s| s.to_string()),
        timeout_secs: DEFAULT_TIMEOUT_SECS,
        events: None,
    }
}

#[test]
fn substitutes_event_variables_into_argv_and_stdin() {
    let runner = StubRunner::new(&[]);
    let channels = vec![
        spec(
            "slack",
            &["curl", "--data", "@-", "https://hooks.slack.com/x"],
            Some(r#"{"text": {text#json}}"#),
        ),
        spec("desktop", &["notify-send", "{title}", "{body}"], None),
        spec(
            "sink",
            &["sh", "-c", "cat >> /tmp/events.jsonl"],
            Some("{json}\n"),
        ),
    ];

    let out = run_ask_question(&runner, &channels, &ask_payload());

    assert!(out.notified);
    assert_eq!(out.reports.len(), 3);
    assert!(out.reports.iter().all(|r| r.ok));

    let calls = runner.calls.borrow();

    // Slack: stdin template renders the human-readable text as a JSON body.
    let (argv, stdin, timeout) = &calls[0];
    assert_eq!(argv[3], "https://hooks.slack.com/x");
    assert_eq!(*timeout, DEFAULT_TIMEOUT_SECS);
    let body: serde_json::Value = serde_json::from_str(stdin.as_deref().unwrap()).unwrap();
    let text = body["text"].as_str().unwrap();
    assert!(text.contains("Which auth method?"));
    assert!(text.contains("Auth"));
    assert!(text.contains("OAuth"));
    assert!(text.contains("/work/repo"));

    // Desktop: title/body substituted as argv elements (no shell involved).
    let (argv, stdin, _) = &calls[1];
    assert_eq!(argv[1], "Claude 질문 대기");
    assert!(argv[2].contains("Which auth method?"));
    assert!(argv[2].contains("/work/repo"));
    assert!(stdin.is_none());

    // File sink: the canonical structured event, one line.
    let (_, stdin, _) = &calls[2];
    let line = stdin.as_deref().unwrap();
    assert!(line.ends_with('\n'));
    let event: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(event["event"], "ask_user_question");
    assert_eq!(event["cwd"], "/work/repo");
    assert_eq!(event["questions"][0]["question"], "Which auth method?");
    assert_eq!(event["questions"][0]["options"][1], "API key");
}

#[test]
fn notification_event_renders_message_variables() {
    let runner = StubRunner::new(&[]);
    let channels = vec![
        spec("desktop", &["notify-send", "{title}", "{body}"], None),
        spec("hook", &["post-anywhere"], Some("{json}")),
    ];

    let out = run_notification(&runner, &channels, &notification_payload());

    assert!(out.notified);
    let calls = runner.calls.borrow();

    let (argv, _, _) = &calls[0];
    assert_eq!(argv[1], "Claude 입력 대기");
    assert!(argv[2].contains("Claude needs your permission to use Bash"));

    let (_, stdin, _) = &calls[1];
    let event: serde_json::Value = serde_json::from_str(stdin.as_deref().unwrap()).unwrap();
    assert_eq!(event["event"], "notification");
    assert_eq!(event["message"], "Claude needs your permission to use Bash");
}

#[test]
fn events_filter_selects_channels_per_event() {
    let runner = StubRunner::new(&[]);
    let mut only_notification = spec("perm-only", &["notify-send", "{body}"], None);
    only_notification.events = Some(vec!["notification".to_string()]);
    let channels = vec![only_notification, spec("all", &["true"], None)];

    let out = run_ask_question(&runner, &channels, &ask_payload());

    // The notification-only channel is skipped without a report.
    assert_eq!(out.reports.len(), 1);
    assert_eq!(out.reports[0].channel, "all");
    assert_eq!(runner.calls.borrow().len(), 1);

    let out = run_notification(&runner, &channels, &notification_payload());
    assert_eq!(out.reports.len(), 2);
}

#[test]
fn no_channels_is_a_silent_noop() {
    let runner = StubRunner::new(&[]);
    let out = run_ask_question(&runner, &[], &ask_payload());
    assert!(!out.notified);
    assert!(out.reports.is_empty());
    assert!(runner.calls.borrow().is_empty());
}

#[test]
fn gated_payloads_are_a_silent_noop() {
    let runner = StubRunner::new(&[]);
    let channels = vec![spec("any", &["true"], None)];

    let out = run_ask_question(&runner, &channels, &AskQuestionPayload::default());
    assert!(!out.notified);

    let out = run_notification(&runner, &channels, &NotificationPayload::default());
    assert!(!out.notified);

    assert!(runner.calls.borrow().is_empty());
}

#[test]
fn command_failure_is_reported_not_raised() {
    let runner = StubRunner::new(&["curl"]);
    let channels = vec![
        spec("slack", &["curl", "https://hooks.slack.com/x"], None),
        spec("desktop", &["notify-send", "{title}", "{body}"], None),
    ];

    let out = run_ask_question(&runner, &channels, &ask_payload());

    // One channel failed but the other succeeded → still notified, and the
    // failure is data in the report.
    assert!(out.notified);
    let slack = out.reports.iter().find(|r| r.channel == "slack").unwrap();
    assert!(!slack.ok);
    assert_eq!(slack.error.as_deref(), Some("spawn failed"));
    let desk = out.reports.iter().find(|r| r.channel == "desktop").unwrap();
    assert!(desk.ok);
}

#[test]
fn config_to_delivery_roundtrip() {
    // End-to-end through the public surface: config JSON → resolve → run.
    let runner = StubRunner::new(&[]);
    let file = r#"{"channels":[
        {"name":"banner","exec":["notify-send","{title}","{body}"],"timeoutSeconds":3}
    ]}"#;
    let channels = resolve_channels(
        &env(&[]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
    );

    let out = run_ask_question(&runner, &channels, &ask_payload());

    assert!(out.notified);
    assert_eq!(out.reports[0].channel, "banner");
    let calls = runner.calls.borrow();
    assert_eq!(calls[0].0[0], "notify-send");
    assert!(calls[0].0[2].contains("Which auth method?"));
    assert_eq!(calls[0].2, 3);
}
