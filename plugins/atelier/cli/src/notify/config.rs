//! Channel configuration resolution. Deterministic given the injected env and
//! filesystem: env vars win as a whole — if any `ATELIER_NOTIFY_*` channel var
//! is set, only env-derived channels are used; otherwise the project config
//! file `<project>/.claude/atelier-notify.json` is read, then the global
//! `~/.claude/atelier-notify.json` (set once, applies to every project's
//! sessions). Missing/malformed config resolves to no channels (the command
//! then no-ops).

use crate::notify::types::Channel;
use serde_json::Value;

pub const ENV_SLACK_WEBHOOK_URL: &str = "ATELIER_NOTIFY_SLACK_WEBHOOK_URL";
pub const ENV_WEBHOOK_URL: &str = "ATELIER_NOTIFY_WEBHOOK_URL";
pub const ENV_FILE: &str = "ATELIER_NOTIFY_FILE";
pub const ENV_DESKTOP: &str = "ATELIER_NOTIFY_DESKTOP";

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
    if let Some(path) = env.var(ENV_FILE).filter(|s| !s.is_empty()) {
        from_env.push(Channel::File {
            path: expand_home(&path, env),
        });
    }
    if env.var(ENV_DESKTOP).map(|v| truthy(&v)).unwrap_or(false) {
        from_env.push(Channel::Desktop);
    }
    if !from_env.is_empty() {
        return from_env;
    }

    if let Some(raw) = fs.read_file(&config_path(project_dir)) {
        return parse_config(&raw, env);
    }
    env.var("HOME")
        .filter(|h| !h.is_empty())
        .and_then(|home| fs.read_file(&config_path(&home)))
        .map(|raw| parse_config(&raw, env))
        .unwrap_or_default()
}

fn truthy(v: &str) -> bool {
    !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
}

/// Expands a leading `~/` with `$HOME` so a global sink path (shared across
/// sessions/projects) can be written portably. No HOME → path kept as-is.
fn expand_home(path: &str, env: &dyn ConfigEnv) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => match env.var("HOME").filter(|h| !h.is_empty()) {
            Some(home) => format!("{home}/{rest}"),
            None => path.to_string(),
        },
        None => path.to_string(),
    }
}

/// Parses the config file. Unknown channel types and entries missing their
/// URL field are skipped, so a partially-valid file still delivers what it can.
fn parse_config(raw: &str, env: &dyn ConfigEnv) -> Vec<Channel> {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    v["channels"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|c| parse_channel(c, env)).collect())
        .unwrap_or_default()
}

fn parse_channel(v: &Value, env: &dyn ConfigEnv) -> Option<Channel> {
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
        "file" => Some(Channel::File {
            path: expand_home(v["path"].as_str().filter(|s| !s.is_empty())?, env),
        }),
        "desktop" => Some(Channel::Desktop),
        _ => None,
    }
}
