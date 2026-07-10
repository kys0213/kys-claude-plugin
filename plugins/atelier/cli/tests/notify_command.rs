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
    let channels = vec![
        Channel::Slack {
            webhook_url: "https://hooks.slack.com/x".to_string(),
        },
        Channel::Webhook {
            url: "https://generic.example/hook".to_string(),
        },
    ];

    let out = run_ask_question(&NotifyDeps { poster: &poster }, &channels, &payload());

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
    let out = run_ask_question(&NotifyDeps { poster: &poster }, &[], &payload());
    assert!(!out.notified);
    assert!(out.reports.is_empty());
    assert!(poster.posts.borrow().is_empty());
}

#[test]
fn payload_without_questions_is_a_silent_noop() {
    let poster = StubPoster::new(&[]);
    let channels = vec![Channel::Slack {
        webhook_url: "https://hooks.slack.com/x".to_string(),
    }];
    let out = run_ask_question(
        &NotifyDeps { poster: &poster },
        &channels,
        &AskQuestionPayload::default(),
    );
    assert!(!out.notified);
    assert!(poster.posts.borrow().is_empty());
}

#[test]
fn notification_fans_out_with_message_bodies() {
    let poster = StubPoster::new(&[]);
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

    let out = run_notification(&NotifyDeps { poster: &poster }, &channels, &payload);

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
    let channels = vec![Channel::Slack {
        webhook_url: "https://hooks.slack.com/x".to_string(),
    }];
    let out = run_notification(
        &NotifyDeps { poster: &poster },
        &channels,
        &NotificationPayload::default(),
    );
    assert!(!out.notified);
    assert!(poster.posts.borrow().is_empty());
}

#[test]
fn delivery_failure_is_reported_not_raised() {
    let poster = StubPoster::new(&["https://hooks.slack.com/x"]);
    let channels = vec![
        Channel::Slack {
            webhook_url: "https://hooks.slack.com/x".to_string(),
        },
        Channel::Webhook {
            url: "https://generic.example/hook".to_string(),
        },
    ];

    let out = run_ask_question(&NotifyDeps { poster: &poster }, &channels, &payload());

    // One channel failed but the other succeeded → still notified, and the
    // failure is data in the report.
    assert!(out.notified);
    let slack = out.reports.iter().find(|r| r.channel == "slack").unwrap();
    assert!(!slack.ok);
    assert_eq!(slack.error.as_deref(), Some("boom"));
    let hook = out.reports.iter().find(|r| r.channel == "webhook").unwrap();
    assert!(hook.ok);
}
