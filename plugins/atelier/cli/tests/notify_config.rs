//! Black-box tests for channel config resolution: env wins as a whole over
//! the project config file, and missing/malformed config resolves to no
//! channels (no-op), never an error.

use atelier::notify::config::{
    resolve_channels, ConfigEnv, ConfigFs, ENV_DESKTOP, ENV_FILE, ENV_SLACK_WEBHOOK_URL,
    ENV_WEBHOOK_URL,
};
use atelier::notify::types::Channel;
use std::collections::HashMap;

struct MapEnv(HashMap<String, String>);

impl ConfigEnv for MapEnv {
    fn var(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

struct MapFs(HashMap<String, String>);

impl ConfigFs for MapFs {
    fn read_file(&self, path: &str) -> Option<String> {
        self.0.get(path).cloned()
    }
}

fn env(pairs: &[(&str, &str)]) -> MapEnv {
    MapEnv(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

fn fs(pairs: &[(&str, &str)]) -> MapFs {
    MapFs(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

#[test]
fn env_slack_var_resolves_slack_channel() {
    let channels = resolve_channels(
        &env(&[(ENV_SLACK_WEBHOOK_URL, "https://hooks.slack.com/x")]),
        &fs(&[]),
        "/proj",
    );
    assert_eq!(
        channels,
        vec![Channel::Slack {
            webhook_url: "https://hooks.slack.com/x".to_string()
        }]
    );
}

#[test]
fn env_wins_over_config_file_as_a_whole() {
    let file = r#"{"channels":[{"type":"webhook","url":"https://file.example/hook"}]}"#;
    let channels = resolve_channels(
        &env(&[(ENV_SLACK_WEBHOOK_URL, "https://hooks.slack.com/env")]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
    );
    // Env is set → the file channel must NOT be mixed in.
    assert_eq!(
        channels,
        vec![Channel::Slack {
            webhook_url: "https://hooks.slack.com/env".to_string()
        }]
    );
}

#[test]
fn falls_back_to_config_file_when_no_env() {
    let file = r#"{"channels":[
        {"type":"slack","webhookUrl":"https://hooks.slack.com/f"},
        {"type":"webhook","url":"https://file.example/hook"},
        {"type":"email","address":"x@y.z"}
    ]}"#;
    let channels = resolve_channels(
        &env(&[]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
    );
    // Unknown "email" type is skipped; known ones are kept in order.
    assert_eq!(
        channels,
        vec![
            Channel::Slack {
                webhook_url: "https://hooks.slack.com/f".to_string()
            },
            Channel::Webhook {
                url: "https://file.example/hook".to_string()
            },
        ]
    );
}

#[test]
fn env_file_var_resolves_file_channel_with_home_expansion() {
    let channels = resolve_channels(
        &env(&[
            (ENV_FILE, "~/.claude/atelier-notify/events.jsonl"),
            ("HOME", "/home/u"),
        ]),
        &fs(&[]),
        "/proj",
    );
    assert_eq!(
        channels,
        vec![Channel::File {
            path: "/home/u/.claude/atelier-notify/events.jsonl".to_string()
        }]
    );

    // Absolute path passes through untouched; no HOME leaves `~/` as-is.
    let abs = resolve_channels(&env(&[(ENV_FILE, "/var/log/e.jsonl")]), &fs(&[]), "/proj");
    assert_eq!(
        abs,
        vec![Channel::File {
            path: "/var/log/e.jsonl".to_string()
        }]
    );
    let no_home = resolve_channels(&env(&[(ENV_FILE, "~/e.jsonl")]), &fs(&[]), "/proj");
    assert_eq!(
        no_home,
        vec![Channel::File {
            path: "~/e.jsonl".to_string()
        }]
    );
}

#[test]
fn config_file_resolves_file_channel() {
    let file = r#"{"channels":[{"type":"file","path":"~/.claude/atelier-notify/events.jsonl"}]}"#;
    let channels = resolve_channels(
        &env(&[("HOME", "/home/u")]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
    );
    assert_eq!(
        channels,
        vec![Channel::File {
            path: "/home/u/.claude/atelier-notify/events.jsonl".to_string()
        }]
    );
}

#[test]
fn env_desktop_var_resolves_desktop_channel_when_truthy() {
    let on = resolve_channels(&env(&[(ENV_DESKTOP, "1")]), &fs(&[]), "/proj");
    assert_eq!(on, vec![Channel::Desktop]);

    for falsy in ["0", "false", "FALSE", ""] {
        let off = resolve_channels(&env(&[(ENV_DESKTOP, falsy)]), &fs(&[]), "/proj");
        assert!(off.is_empty(), "expected no channels for {falsy:?}");
    }
}

#[test]
fn config_file_resolves_desktop_channel() {
    let file = r#"{"channels":[{"type":"desktop"}]}"#;
    let channels = resolve_channels(
        &env(&[]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
    );
    assert_eq!(channels, vec![Channel::Desktop]);
}

#[test]
fn falls_back_to_global_home_config_when_project_config_missing() {
    let global = r#"{"channels":[{"type":"desktop"}]}"#;
    let channels = resolve_channels(
        &env(&[("HOME", "/home/u")]),
        &fs(&[("/home/u/.claude/atelier-notify.json", global)]),
        "/proj",
    );
    assert_eq!(channels, vec![Channel::Desktop]);

    // Project config wins over the global one when both exist.
    let project = r#"{"channels":[{"type":"webhook","url":"https://p.example/hook"}]}"#;
    let channels = resolve_channels(
        &env(&[("HOME", "/home/u")]),
        &fs(&[
            ("/proj/.claude/atelier-notify.json", project),
            ("/home/u/.claude/atelier-notify.json", global),
        ]),
        "/proj",
    );
    assert_eq!(
        channels,
        vec![Channel::Webhook {
            url: "https://p.example/hook".to_string()
        }]
    );
}

#[test]
fn missing_and_malformed_config_resolve_to_no_channels() {
    assert!(resolve_channels(&env(&[]), &fs(&[]), "/proj").is_empty());
    assert!(resolve_channels(
        &env(&[]),
        &fs(&[("/proj/.claude/atelier-notify.json", "{ broken")]),
        "/proj"
    )
    .is_empty());
}

#[test]
fn empty_env_values_are_ignored() {
    let channels = resolve_channels(
        &env(&[(ENV_SLACK_WEBHOOK_URL, ""), (ENV_WEBHOOK_URL, "")]),
        &fs(&[]),
        "/proj",
    );
    assert!(channels.is_empty());
}
