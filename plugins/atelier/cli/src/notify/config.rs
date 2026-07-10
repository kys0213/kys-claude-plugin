//! Channel configuration resolution. A channel is **pure data** — an argv
//! command plus optional stdin template — so adding a channel is a config
//! edit, never a code change (OCP taken to the config level). This module
//! owns only the resolution precedence and the spec schema:
//!
//! 1. `ATELIER_NOTIFY_CONFIG` env var — explicit config file path
//! 2. `<project>/.claude/atelier-notify.json`
//! 3. `~/.claude/atelier-notify.json` (set once, applies to every project)
//!
//! Missing/malformed config resolves to no channels (the command then
//! no-ops). Schema:
//!
//! ```json
//! {
//!   "channels": [
//!     {
//!       "name": "slack",
//!       "exec": ["curl", "-sS", "--data", "@-", "https://hooks.slack..."],
//!       "stdin": "{\"text\": {text#json}}",
//!       "timeoutSeconds": 5,
//!       "events": ["ask_user_question"]
//!     }
//!   ]
//! }
//! ```
//!
//! `exec` elements and `stdin` support the `render` template variables. The
//! command is spawned without a shell, so substituted event data is never
//! shell-interpreted.

use serde_json::Value;

pub const ENV_CONFIG: &str = "ATELIER_NOTIFY_CONFIG";

/// Seconds a channel command may run before the runner kills it.
pub const DEFAULT_TIMEOUT_SECS: u64 = 5;

/// Environment lookup the resolver depends on (injectable for tests).
pub trait ConfigEnv {
    fn var(&self, key: &str) -> Option<String>;
}

/// Filesystem read the resolver depends on (injectable for tests). `None`
/// covers both "missing" and "unreadable" — the resolver treats them alike.
pub trait ConfigFs {
    fn read_file(&self, path: &str) -> Option<String>;
}

/// One declared channel: which command to run and what to feed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSpec {
    /// Report label. Defaults to the command name (`exec[0]`).
    pub name: String,
    /// Command argv; elements support template variables.
    pub exec: Vec<String>,
    /// Optional stdin template. Absent → stdin closed.
    pub stdin: Option<String>,
    pub timeout_secs: u64,
    /// Event kinds this channel accepts. `None` → all events.
    pub events: Option<Vec<String>>,
}

impl ChannelSpec {
    pub fn accepts(&self, event_kind: &str) -> bool {
        match &self.events {
            Some(kinds) => kinds.iter().any(|k| k == event_kind),
            None => true,
        }
    }
}

fn config_path(dir: &str) -> String {
    format!("{dir}/.claude/atelier-notify.json")
}

/// Expands a leading `~/` with `$HOME` so the env var can point at a global
/// path portably. No HOME → path kept as-is.
fn expand_home(path: &str, env: &dyn ConfigEnv) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => match env.var("HOME").filter(|h| !h.is_empty()) {
            Some(home) => format!("{home}/{rest}"),
            None => path.to_string(),
        },
        None => path.to_string(),
    }
}

/// Resolves the channel specs for a project. Empty result means "not
/// configured" — callers must treat that as a silent no-op, never an error.
pub fn resolve_channels(
    env: &dyn ConfigEnv,
    fs: &dyn ConfigFs,
    project_dir: &str,
) -> Vec<ChannelSpec> {
    if let Some(path) = env.var(ENV_CONFIG).filter(|s| !s.is_empty()) {
        // Explicit override: use exactly this file, resolving to no channels
        // when it is missing/unreadable (advisory no-op, never an error).
        return fs
            .read_file(&expand_home(&path, env))
            .map(|raw| parse_config(&raw))
            .unwrap_or_default();
    }

    if let Some(raw) = fs.read_file(&config_path(project_dir)) {
        return parse_config(&raw);
    }
    env.var("HOME")
        .filter(|h| !h.is_empty())
        .and_then(|home| fs.read_file(&config_path(&home)))
        .map(|raw| parse_config(&raw))
        .unwrap_or_default()
}

/// Parses the config file. Entries without a non-empty `exec` array are
/// skipped, so a partially-valid file still delivers what it can.
fn parse_config(raw: &str) -> Vec<ChannelSpec> {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    v["channels"]
        .as_array()
        .map(|arr| arr.iter().filter_map(parse_channel).collect())
        .unwrap_or_default()
}

fn parse_channel(v: &Value) -> Option<ChannelSpec> {
    let exec: Vec<String> = v["exec"]
        .as_array()?
        .iter()
        .filter_map(|a| a.as_str())
        .map(|s| s.to_string())
        .collect();
    if exec.is_empty() {
        return None;
    }
    let name = v["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(&exec[0])
        .to_string();
    let timeout_secs = v["timeoutSeconds"]
        .as_u64()
        .filter(|t| *t > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    let events = v["events"].as_array().map(|arr| {
        arr.iter()
            .filter_map(|e| e.as_str())
            .map(|s| s.to_string())
            .collect()
    });
    Some(ChannelSpec {
        name,
        exec,
        stdin: v["stdin"].as_str().map(|s| s.to_string()),
        timeout_secs,
        events,
    })
}
