//! Black-box tests for channel config resolution: env wins as a whole over
//! config files, project config wins over the global home config, and
//! missing/malformed config resolves to no channels (no-op), never an error.
//! Channel identity is asserted via `kind()` and delivery behavior (what the
//! stub ports record), not internal structure.

mod notify_support;

use atelier::notify::channel::slack::ENV_WEBHOOK_URL as ENV_SLACK_WEBHOOK_URL;
use atelier::notify::channel::webhook::ENV_URL as ENV_WEBHOOK_URL;
use atelier::notify::channel::{desktop::ENV_DESKTOP, file::ENV_PATH as ENV_FILE};
use atelier::notify::config::resolve_channels;
use atelier::notify::event::Event;
use notify_support::{ask_payload, env, fs, fx, StubAppender, StubDesktop, StubPoster};

fn kinds(channels: &[Box<dyn atelier::notify::channel::NotifyChannel + '_>]) -> Vec<&'static str> {
    channels.iter().map(|c| c.kind()).collect()
}

#[test]
fn env_slack_var_resolves_slack_channel_with_its_url() {
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
    assert_eq!(kinds(&channels), vec!["slack"]);

    // The resolved channel carries the env URL — verified via delivery.
    let payload = ask_payload();
    channels[0].send(&Event::AskQuestion(&payload)).unwrap();
    assert_eq!(poster.posts.borrow()[0].0, "https://hooks.slack.com/x");
}

#[test]
fn env_wins_over_config_file_as_a_whole() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let file = r#"{"channels":[{"type":"webhook","url":"https://file.example/hook"}]}"#;
    let channels = resolve_channels(
        &env(&[(ENV_SLACK_WEBHOOK_URL, "https://hooks.slack.com/env")]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
        &fx,
    );
    // Env is set → the file channel must NOT be mixed in.
    assert_eq!(kinds(&channels), vec!["slack"]);
}

#[test]
fn falls_back_to_config_file_when_no_env() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let file = r#"{"channels":[
        {"type":"slack","webhookUrl":"https://hooks.slack.com/f"},
        {"type":"webhook","url":"https://file.example/hook"},
        {"type":"email","address":"x@y.z"}
    ]}"#;
    let channels = resolve_channels(
        &env(&[]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
        &fx,
    );
    // Unknown "email" type is skipped; known ones are kept in file order.
    assert_eq!(kinds(&channels), vec!["slack", "webhook"]);
}

#[test]
fn env_file_var_resolves_file_channel_with_home_expansion() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let payload = ask_payload();

    let channels = resolve_channels(
        &env(&[
            (ENV_FILE, "~/.claude/atelier-notify/events.jsonl"),
            ("HOME", "/home/u"),
        ]),
        &fs(&[]),
        "/proj",
        &fx,
    );
    assert_eq!(kinds(&channels), vec!["file"]);
    channels[0].send(&Event::AskQuestion(&payload)).unwrap();
    assert_eq!(
        appender.appends.borrow()[0].0,
        "/home/u/.claude/atelier-notify/events.jsonl"
    );

    // Absolute path passes through untouched; no HOME leaves `~/` as-is.
    let abs = resolve_channels(&env(&[(ENV_FILE, "/var/log/e.jsonl")]), &fs(&[]), "/p", &fx);
    abs[0].send(&Event::AskQuestion(&payload)).unwrap();
    assert_eq!(appender.appends.borrow()[1].0, "/var/log/e.jsonl");

    let no_home = resolve_channels(&env(&[(ENV_FILE, "~/e.jsonl")]), &fs(&[]), "/p", &fx);
    no_home[0].send(&Event::AskQuestion(&payload)).unwrap();
    assert_eq!(appender.appends.borrow()[2].0, "~/e.jsonl");
}

#[test]
fn config_file_resolves_file_channel_with_home_expansion() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let file = r#"{"channels":[{"type":"file","path":"~/.claude/atelier-notify/events.jsonl"}]}"#;
    let channels = resolve_channels(
        &env(&[("HOME", "/home/u")]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
        &fx,
    );
    assert_eq!(kinds(&channels), vec!["file"]);
    let payload = ask_payload();
    channels[0].send(&Event::AskQuestion(&payload)).unwrap();
    assert_eq!(
        appender.appends.borrow()[0].0,
        "/home/u/.claude/atelier-notify/events.jsonl"
    );
}

#[test]
fn env_desktop_var_resolves_desktop_channel_when_truthy() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);

    let on = resolve_channels(&env(&[(ENV_DESKTOP, "1")]), &fs(&[]), "/proj", &fx);
    assert_eq!(kinds(&on), vec!["desktop"]);

    for falsy in ["0", "false", "FALSE", ""] {
        let off = resolve_channels(&env(&[(ENV_DESKTOP, falsy)]), &fs(&[]), "/proj", &fx);
        assert!(off.is_empty(), "expected no channels for {falsy:?}");
    }
}

#[test]
fn config_file_resolves_desktop_channel() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let file = r#"{"channels":[{"type":"desktop"}]}"#;
    let channels = resolve_channels(
        &env(&[]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
        &fx,
    );
    assert_eq!(kinds(&channels), vec!["desktop"]);
}

#[test]
fn falls_back_to_global_home_config_when_project_config_missing() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let global = r#"{"channels":[{"type":"desktop"}]}"#;
    let channels = resolve_channels(
        &env(&[("HOME", "/home/u")]),
        &fs(&[("/home/u/.claude/atelier-notify.json", global)]),
        "/proj",
        &fx,
    );
    assert_eq!(kinds(&channels), vec!["desktop"]);

    // Project config wins over the global one when both exist.
    let project = r#"{"channels":[{"type":"webhook","url":"https://p.example/hook"}]}"#;
    let channels = resolve_channels(
        &env(&[("HOME", "/home/u")]),
        &fs(&[
            ("/proj/.claude/atelier-notify.json", project),
            ("/home/u/.claude/atelier-notify.json", global),
        ]),
        "/proj",
        &fx,
    );
    assert_eq!(kinds(&channels), vec!["webhook"]);
}

#[test]
fn missing_and_malformed_config_resolve_to_no_channels() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    assert!(resolve_channels(&env(&[]), &fs(&[]), "/proj", &fx).is_empty());
    assert!(resolve_channels(
        &env(&[]),
        &fs(&[("/proj/.claude/atelier-notify.json", "{ broken")]),
        "/proj",
        &fx,
    )
    .is_empty());
}

#[test]
fn empty_env_values_are_ignored() {
    let (poster, appender, desktop) = (
        StubPoster::new(&[]),
        StubAppender::new(&[]),
        StubDesktop::new(false),
    );
    let fx = fx(&poster, &appender, &desktop);
    let channels = resolve_channels(
        &env(&[
            (ENV_SLACK_WEBHOOK_URL, ""),
            (ENV_WEBHOOK_URL, ""),
            (ENV_FILE, ""),
        ]),
        &fs(&[]),
        "/proj",
        &fx,
    );
    assert!(channels.is_empty());
}
