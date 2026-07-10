//! Black-box tests for the notify command cores (`ask-question`,
//! `notification`) with a stub poster: channel fan-out, per-channel body
//! selection, no-op gates, and the failure-is-data (never an error) contract.

use atelier::notify::command::{run_ask_question, run_notification, NotifyDeps};
use atelier::notify::types::{AskQuestionPayload, Channel, NotificationPayload, Question};
use std::cell::RefCell;

/// Poster stub recording every post; URLs listed in `fail` return Err.
struct StubPoster {
    posts: RefCell<Vec<(String, String)>>,
    fail: Vec<String>,
}

impl StubPoster {
    fn new(fail: &[&str]) -> Self {
        StubPoster {
            posts: RefCell::new(Vec::new()),
            fail: fail.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl atelier::notify::transport::HttpPoster for StubPoster {
    fn post_json(&self, url: &str, body: &str) -> Result<(), String> {
        self.posts
            .borrow_mut()
            .push((url.to_string(), body.to_string()));
        if self.fail.iter().any(|f| f == url) {
            return Err("boom".to_string());
        }
        Ok(())
    }
}

/// Appender stub recording every append; paths listed in `fail` return Err.
struct StubAppender {
    appends: RefCell<Vec<(String, String)>>,
    fail: Vec<String>,
}

impl StubAppender {
    fn new(fail: &[&str]) -> Self {
        StubAppender {
            appends: RefCell::new(Vec::new()),
            fail: fail.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl atelier::notify::transport::FileAppender for StubAppender {
    fn append_line(&self, path: &str, line: &str) -> Result<(), String> {
        self.appends
            .borrow_mut()
            .push((path.to_string(), line.to_string()));
        if self.fail.iter().any(|f| f == path) {
            return Err("disk full".to_string());
        }
        Ok(())
    }
}

/// Desktop stub recording every banner; `fail` makes it return Err.
struct StubDesktop {
    banners: RefCell<Vec<(String, String)>>,
    fail: bool,
}

impl StubDesktop {
    fn new(fail: bool) -> Self {
        StubDesktop {
            banners: RefCell::new(Vec::new()),
            fail,
        }
    }
}

impl atelier::notify::transport::DesktopNotifier for StubDesktop {
    fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        self.banners
            .borrow_mut()
            .push((title.to_string(), body.to_string()));
        if self.fail {
            return Err("no notifier".to_string());
        }
        Ok(())
    }
}

fn deps<'a>(
    poster: &'a StubPoster,
    appender: &'a StubAppender,
    desktop: &'a StubDesktop,
) -> NotifyDeps<'a> {
    NotifyDeps {
        poster,
        appender,
        desktop,
    }
}

fn payload() -> AskQuestionPayload {
    AskQuestionPayload {
        session_id: Some("s1".to_string()),
        cwd: Some("/work/repo".to_string()),
        questions: vec![Question {
            header: Some("Auth".to_string()),
            question: "Which auth method?".to_string(),
            options: vec!["OAuth".to_string(), "API key".to_string()],
            multi_select: false,
        }],
    }
}

#[test]
fn fans_out_to_all_channels_with_channel_specific_bodies() {
    let poster = StubPoster::new(&[]);
    let appender = StubAppender::new(&[]);
    let desktop = StubDesktop::new(false);
    let channels = vec![
        Channel::Slack {
            webhook_url: "https://hooks.slack.com/x".to_string(),
        },
        Channel::Webhook {
            url: "https://generic.example/hook".to_string(),
        },
    ];

    let out = run_ask_question(&deps(&poster, &appender, &desktop), &channels, &payload());

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
    let poster = StubPoster::new(&[]);
    let appender = StubAppender::new(&[]);
    let desktop = StubDesktop::new(false);
    let out = run_ask_question(&deps(&poster, &appender, &desktop), &[], &payload());
    assert!(!out.notified);
    assert!(out.reports.is_empty());
    assert!(poster.posts.borrow().is_empty());
}

#[test]
fn payload_without_questions_is_a_silent_noop() {
    let poster = StubPoster::new(&[]);
    let appender = StubAppender::new(&[]);
    let desktop = StubDesktop::new(false);
    let channels = vec![Channel::Slack {
        webhook_url: "https://hooks.slack.com/x".to_string(),
    }];
    let out = run_ask_question(
        &deps(&poster, &appender, &desktop),
        &channels,
        &AskQuestionPayload::default(),
    );
    assert!(!out.notified);
    assert!(poster.posts.borrow().is_empty());
}

#[test]
fn notification_fans_out_with_message_bodies() {
    let poster = StubPoster::new(&[]);
    let appender = StubAppender::new(&[]);
    let desktop = StubDesktop::new(false);
    let channels = vec![
        Channel::Slack {
            webhook_url: "https://hooks.slack.com/x".to_string(),
        },
        Channel::Webhook {
            url: "https://generic.example/hook".to_string(),
        },
    ];
    let payload = NotificationPayload {
        session_id: Some("s1".to_string()),
        cwd: Some("/work/repo".to_string()),
        message: Some("Claude needs your permission to use Bash".to_string()),
    };

    let out = run_notification(&deps(&poster, &appender, &desktop), &channels, &payload);

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
    let poster = StubPoster::new(&[]);
    let appender = StubAppender::new(&[]);
    let desktop = StubDesktop::new(false);
    let channels = vec![Channel::Slack {
        webhook_url: "https://hooks.slack.com/x".to_string(),
    }];
    let out = run_notification(
        &deps(&poster, &appender, &desktop),
        &channels,
        &NotificationPayload::default(),
    );
    assert!(!out.notified);
    assert!(poster.posts.borrow().is_empty());
}

#[test]
fn file_channel_appends_structured_event_line() {
    let poster = StubPoster::new(&[]);
    let appender = StubAppender::new(&[]);
    let desktop = StubDesktop::new(false);
    let channels = vec![Channel::File {
        path: "/home/u/.claude/atelier-notify/events.jsonl".to_string(),
    }];

    let out = run_ask_question(&deps(&poster, &appender, &desktop), &channels, &payload());

    assert!(out.notified);
    assert!(poster.posts.borrow().is_empty());
    let appends = appender.appends.borrow();
    assert_eq!(appends.len(), 1);
    assert_eq!(appends[0].0, "/home/u/.claude/atelier-notify/events.jsonl");
    // The appended line is the structured (webhook) body — one event per
    // line, so a Monitor tailing the file gets exactly one event per line.
    assert!(!appends[0].1.contains('\n'));
    let event: serde_json::Value = serde_json::from_str(&appends[0].1).unwrap();
    assert_eq!(event["event"], "ask_user_question");
    assert_eq!(event["questions"][0]["question"], "Which auth method?");
}

#[test]
fn file_append_failure_is_reported_not_raised() {
    let poster = StubPoster::new(&[]);
    let appender = StubAppender::new(&["/full/events.jsonl"]);
    let desktop = StubDesktop::new(false);
    let channels = vec![Channel::File {
        path: "/full/events.jsonl".to_string(),
    }];

    let out = run_ask_question(&deps(&poster, &appender, &desktop), &channels, &payload());

    assert!(!out.notified);
    let report = out.reports.iter().find(|r| r.channel == "file").unwrap();
    assert!(!report.ok);
    assert_eq!(report.error.as_deref(), Some("disk full"));
}

#[test]
fn desktop_channel_gets_banner_with_first_question() {
    let poster = StubPoster::new(&[]);
    let appender = StubAppender::new(&[]);
    let desktop = StubDesktop::new(false);
    let channels = vec![Channel::Desktop];

    let mut p = payload();
    p.questions.push(Question {
        header: None,
        question: "Second?".to_string(),
        options: vec![],
        multi_select: false,
    });
    let out = run_ask_question(&deps(&poster, &appender, &desktop), &channels, &p);

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
    let np = NotificationPayload {
        session_id: None,
        cwd: Some("/work/repo".to_string()),
        message: Some("Claude needs your permission to use Bash".to_string()),
    };
    run_notification(&deps(&poster, &appender, &desktop), &channels, &np);
    let banners = desktop.banners.borrow();
    let (title, body) = &banners[1];
    assert_eq!(title, "Claude 입력 대기");
    assert!(body.contains("Claude needs your permission to use Bash"));
}

#[test]
fn desktop_failure_is_reported_not_raised() {
    let poster = StubPoster::new(&[]);
    let appender = StubAppender::new(&[]);
    let desktop = StubDesktop::new(true);
    let out = run_ask_question(
        &deps(&poster, &appender, &desktop),
        &[Channel::Desktop],
        &payload(),
    );
    assert!(!out.notified);
    let report = out.reports.iter().find(|r| r.channel == "desktop").unwrap();
    assert!(!report.ok);
    assert_eq!(report.error.as_deref(), Some("no notifier"));
}

#[test]
fn delivery_failure_is_reported_not_raised() {
    let poster = StubPoster::new(&["https://hooks.slack.com/x"]);
    let appender = StubAppender::new(&[]);
    let desktop = StubDesktop::new(false);
    let channels = vec![
        Channel::Slack {
            webhook_url: "https://hooks.slack.com/x".to_string(),
        },
        Channel::Webhook {
            url: "https://generic.example/hook".to_string(),
        },
    ];

    let out = run_ask_question(&deps(&poster, &appender, &desktop), &channels, &payload());

    // One channel failed but the other succeeded → still notified, and the
    // failure is data in the report.
    assert!(out.notified);
    let slack = out.reports.iter().find(|r| r.channel == "slack").unwrap();
    assert!(!slack.ok);
    assert_eq!(slack.error.as_deref(), Some("boom"));
    let hook = out.reports.iter().find(|r| r.channel == "webhook").unwrap();
    assert!(hook.ok);
}
