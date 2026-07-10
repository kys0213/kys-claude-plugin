//! Channel configuration resolution. Deterministic given the injected env and
//! filesystem: env vars win as a whole — if any `ATELIER_NOTIFY_*` channel var
//! is set, only env-derived channels are used; otherwise the project config
//! file `<project>/.claude/atelier-notify.json` is read. Missing/malformed
//! config resolves to no channels (the command then no-ops).

use crate::notify::types::Channel;
use serde_json::Value;

pub const ENV_SLACK_WEBHOOK_URL: &str = "ATELIER_NOTIFY_SLACK_WEBHOOK_URL";
pub const ENV_WEBHOOK_URL: &str = "ATELIER_NOTIFY_WEBHOOK_URL";

/// Environment lookup the resolver depends on (injectable for tests).
pub trait ConfigEnv {
    fn var(&self, key: &str) -> Option<String>;
}

/// Filesystem read the resolver depends on (injectable for tests). `None`
/// covers both "missing" and "unreadable" — the resolver treats them alike.
pub trait ConfigFs {
    fn read_file(&self, path: &str) -> Option<String>;
}

fn config_path(project_dir: &str) -> String {
    format!("{project_dir}/.claude/atelier-notify.json")
}

/// Resolves the delivery channels for a project. Empty result means "not
/// configured" — callers must treat that as a silent no-op, never an error.
pub fn resolve_channels(env: &dyn ConfigEnv, fs: &dyn ConfigFs, project_dir: &str) -> Vec<Channel> {
    let mut from_env = Vec::new();
    if let Some(url) = env.var(ENV_SLACK_WEBHOOK_URL).filter(|s| !s.is_empty()) {
        from_env.push(Channel::Slack { webhook_url: url });
    }
    if let Some(url) = env.var(ENV_WEBHOOK_URL).filter(|s| !s.is_empty()) {
        from_env.push(Channel::Webhook { url });
    }
    if !from_env.is_empty() {
        return from_env;
    }

    fs.read_file(&config_path(project_dir))
        .map(|raw| parse_config(&raw))
        .unwrap_or_default()
}

/// Parses the config file. Unknown channel types and entries missing their
/// URL field are skipped, so a partially-valid file still delivers what it can.
fn parse_config(raw: &str) -> Vec<Channel> {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    v["channels"]
        .as_array()
        .map(|arr| arr.iter().filter_map(parse_channel).collect())
        .unwrap_or_default()
}

fn parse_channel(v: &Value) -> Option<Channel> {
    match v["type"].as_str()? {
        "slack" => Some(Channel::Slack {
            webhook_url: v["webhookUrl"]
                .as_str()
                .filter(|s| !s.is_empty())?
                .to_string(),
        }),
        "webhook" => Some(Channel::Webhook {
            url: v["url"].as_str().filter(|s| !s.is_empty())?.to_string(),
        }),
        _ => None,
    }
}
