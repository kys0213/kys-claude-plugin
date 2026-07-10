//! Black-box tests for the notify command cores (`ask-question`,
//! `notification`): channel fan-out, per-channel body selection, no-op gates,
//! and the failure-is-data (never an error) contract. Channels are resolved
//! through the public config surface so these tests survive internal
//! refactoring of the channel modules.

mod notify_support;

use atelier::notify::channel::desktop::ENV_DESKTOP;
use atelier::notify::channel::file::ENV_PATH as ENV_FILE;
use atelier::notify::channel::slack::ENV_WEBHOOK_URL as ENV_SLACK_WEBHOOK_URL;
use atelier::notify::channel::webhook::ENV_URL as ENV_WEBHOOK_URL;
use atelier::notify::command::{run_ask_question, run_notification};
use atelier::notify::config::resolve_channels;
use atelier::notify::types::{AskQuestionPayload, NotificationPayload, Question};
use notify_support::{ask_payload, env, fs, fx, StubAppender, StubDesktop, StubPoster};

fn notification_payload() -> NotificationPayload {
    NotificationPayload {
        session_id: Some("s1".to_string()),
        cwd: Some("/work/repo".to_string()),
        message: Some("Claude needs your permission to use Bash".to_string()),
    }
}

#[test]
fn fans_out_to_all_channels_with_channel_specific_bodies() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let channels = resolve_channels(
        &env(&[
            (ENV_SLACK_WEBHOOK_URL, "https://hooks.slack.com/x"),
            (ENV_WEBHOOK_URL, "https://generic.example/hook"),
        ]),
        &fs(&[]),
        "/proj",
        &fx,
    );

    let out = run_ask_question(&channels, &ask_payload());

    assert!(out.notified);
    assert_eq!(out.reports.len(), 2);
    assert!(out.reports.iter().all(|r| r.ok));

    let posts = poster.posts.borrow();
    assert_eq!(posts.len(), 2);

    // Slack gets a human-readable {"text": ...} body carrying the question.
    let (slack_url, slack_body) = &posts[0];
    assert_eq!(slack_url, "https://hooks.slack.com/x");
    let slack: serde_json::Value = serde_json::from_str(slack_body).unwrap();
    let text = slack["text"].as_str().unwrap();
    assert!(text.contains("Which auth method?"));
    assert!(text.contains("Auth"));
    assert!(text.contains("OAuth"));
    assert!(text.contains("/work/repo"));

    // Generic webhook gets the structured event.
    let (hook_url, hook_body) = &posts[1];
    assert_eq!(hook_url, "https://generic.example/hook");
    let hook: serde_json::Value = serde_json::from_str(hook_body).unwrap();
    assert_eq!(hook["event"], "ask_user_question");
    assert_eq!(hook["cwd"], "/work/repo");
    assert_eq!(hook["questions"][0]["question"], "Which auth method?");
    assert_eq!(hook["questions"][0]["options"][1], "API key");
}

#[test]
fn no_channels_is_a_silent_noop() {
    let out = run_ask_question(&[], &ask_payload());
    assert!(!out.notified);
    assert!(out.reports.is_empty());
}

#[test]
fn payload_without_questions_is_a_silent_noop() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let channels = resolve_channels(
        &env(&[(ENV_SLACK_WEBHOOK_URL, "https://hooks.slack.com/x")]),
        &fs(&[]),
        "/proj",
        &fx,
    );
    let out = run_ask_question(&channels, &AskQuestionPayload::default());
    assert!(!out.notified);
    assert!(poster.posts.borrow().is_empty());
}

#[test]
fn notification_fans_out_with_message_bodies() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let channels = resolve_channels(
        &env(&[
            (ENV_SLACK_WEBHOOK_URL, "https://hooks.slack.com/x"),
            (ENV_WEBHOOK_URL, "https://generic.example/hook"),
        ]),
        &fs(&[]),
        "/proj",
        &fx,
    );

    let out = run_notification(&channels, &notification_payload());

    assert!(out.notified);
    let posts = poster.posts.borrow();
    assert_eq!(posts.len(), 2);

    let slack: serde_json::Value = serde_json::from_str(&posts[0].1).unwrap();
    let text = slack["text"].as_str().unwrap();
    assert!(text.contains("Claude needs your permission to use Bash"));
    assert!(text.contains("/work/repo"));

    let hook: serde_json::Value = serde_json::from_str(&posts[1].1).unwrap();
    assert_eq!(hook["event"], "notification");
    assert_eq!(hook["message"], "Claude needs your permission to use Bash");
    assert_eq!(hook["cwd"], "/work/repo");
}

#[test]
fn notification_without_message_is_a_silent_noop() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let channels = resolve_channels(
        &env(&[(ENV_SLACK_WEBHOOK_URL, "https://hooks.slack.com/x")]),
        &fs(&[]),
        "/proj",
        &fx,
    );
    let out = run_notification(&channels, &NotificationPayload::default());
    assert!(!out.notified);
    assert!(poster.posts.borrow().is_empty());
}

#[test]
fn file_channel_appends_structured_event_line() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let channels = resolve_channels(
        &env(&[(ENV_FILE, "/home/u/.claude/atelier-notify/events.jsonl")]),
        &fs(&[]),
        "/proj",
        &fx,
    );

    let out = run_ask_question(&channels, &ask_payload());

    assert!(out.notified);
    assert!(poster.posts.borrow().is_empty());
    let appends = appender.appends.borrow();
    assert_eq!(appends.len(), 1);
    assert_eq!(appends[0].0, "/home/u/.claude/atelier-notify/events.jsonl");
    // The appended line is the structured event — one event per line, so a
    // Monitor tailing the file gets exactly one event per line.
    assert!(!appends[0].1.contains('\n'));
    let event: serde_json::Value = serde_json::from_str(&appends[0].1).unwrap();
    assert_eq!(event["event"], "ask_user_question");
    assert_eq!(event["questions"][0]["question"], "Which auth method?");
}

#[test]
fn file_append_failure_is_reported_not_raised() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&["/full/events.jsonl"]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let channels = resolve_channels(
        &env(&[(ENV_FILE, "/full/events.jsonl")]),
        &fs(&[]),
        "/p",
        &fx,
    );

    let out = run_ask_question(&channels, &ask_payload());

    assert!(!out.notified);
    let report = out.reports.iter().find(|r| r.channel == "file").unwrap();
    assert!(!report.ok);
    assert_eq!(report.error.as_deref(), Some("disk full"));
}

#[test]
fn desktop_channel_gets_banner_with_first_question() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let channels = resolve_channels(&env(&[(ENV_DESKTOP, "1")]), &fs(&[]), "/proj", &fx);

    let mut p = ask_payload();
    p.questions.push(Question {
        header: None,
        question: "Second?".to_string(),
        options: vec![],
        multi_select: false,
    });
    let out = run_ask_question(&channels, &p);

    assert!(out.notified);
    {
        let banners = desktop.banners.borrow();
        assert_eq!(banners.len(), 1);
        let (title, body) = &banners[0];
        assert_eq!(title, "Claude 질문 대기");
        // Banners truncate → first question + count of the rest + project dir.
        assert!(body.contains("Which auth method?"));
        assert!(body.contains("외 1개"));
        assert!(body.contains("/work/repo"));
    }

    // Notification event gets its own title and the raw message.
    run_notification(&channels, &notification_payload());
    let banners = desktop.banners.borrow();
    let (title, body) = &banners[1];
    assert_eq!(title, "Claude 입력 대기");
    assert!(body.contains("Claude needs your permission to use Bash"));
}

#[test]
fn desktop_failure_is_reported_not_raised() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(true),
    );
    let fx = fx(&poster, &appender, &desktop);
    let channels = resolve_channels(&env(&[(ENV_DESKTOP, "1")]), &fs(&[]), "/proj", &fx);

    let out = run_ask_question(&channels, &ask_payload());

    assert!(!out.notified);
    let report = out.reports.iter().find(|r| r.channel == "desktop").unwrap();
    assert!(!report.ok);
    assert_eq!(report.error.as_deref(), Some("no notifier"));
}

#[test]
fn delivery_failure_is_reported_not_raised() {
    let (poster, appender, desktop) = (
        StubPoster::new(&["https://hooks.slack.com/x"]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let channels = resolve_channels(
        &env(&[
            (ENV_SLACK_WEBHOOK_URL, "https://hooks.slack.com/x"),
            (ENV_WEBHOOK_URL, "https://generic.example/hook"),
        ]),
        &fs(&[]),
        "/proj",
        &fx,
    );

    let out = run_ask_question(&channels, &ask_payload());

    // One channel failed but the other succeeded → still notified, and the
    // failure is data in the report.
    assert!(out.notified);
    let slack = out.reports.iter().find(|r| r.channel == "slack").unwrap();
    assert!(!slack.ok);
    assert_eq!(slack.error.as_deref(), Some("boom"));
    let hook = out.reports.iter().find(|r| r.channel == "webhook").unwrap();
    assert!(hook.ok);
}
