//! Black-box tests for channel config resolution: env wins as a whole over
//! the project config file, and missing/malformed config resolves to no
//! channels (no-op), never an error.

use atelier::notify::config::{
    resolve_channels, ConfigEnv, ConfigFs, ENV_SLACK_WEBHOOK_URL, ENV_WEBHOOK_URL,
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
