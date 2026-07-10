//! Channel configuration resolution — precedence policy only; what each
//! channel's env var / config entry looks like belongs to its module under
//! `channel/` (SRP).
//!
//! Precedence, deterministic given the injected env and filesystem: env vars
//! win as a whole — if any factory resolves from env, only env-derived
//! channels are used. Otherwise the project config file
//! `<project>/.claude/atelier-notify.json` is read, then the global
//! `~/.claude/atelier-notify.json` (set once, applies to every project's
//! sessions). Missing/malformed config resolves to no channels (the command
//! then no-ops).

use crate::notify::channel::{registry, Effects, NotifyChannel};
use serde_json::Value;

/// Environment lookup the resolver depends on (injectable for tests).
pub trait ConfigEnv {
    fn var(&self, key: &str) -> Option<String>;
}

/// Filesystem read the resolver depends on (injectable for tests). `None`
/// covers both "missing" and "unreadable" — the resolver treats them alike.
pub trait ConfigFs {
    fn read_file(&self, path: &str) -> Option<String>;
}

fn config_path(dir: &str) -> String {
    format!("{dir}/.claude/atelier-notify.json")
}

/// Resolves the delivery channels for a project. Empty result means "not
/// configured" — callers must treat that as a silent no-op, never an error.
pub fn resolve_channels<'a>(
    env: &dyn ConfigEnv,
    fs: &dyn ConfigFs,
    project_dir: &str,
    fx: &Effects<'a>,
) -> Vec<Box<dyn NotifyChannel + 'a>> {
    let factories = registry();

    let from_env: Vec<_> = factories
        .iter()
        .filter_map(|f| f.build_from_env(env, fx))
        .collect();
    if !from_env.is_empty() {
        return from_env;
    }

    if let Some(raw) = fs.read_file(&config_path(project_dir)) {
        return parse_config(&raw, env, fx);
    }
    env.var("HOME")
        .filter(|h| !h.is_empty())
        .and_then(|home| fs.read_file(&config_path(&home)))
        .map(|raw| parse_config(&raw, env, fx))
        .unwrap_or_default()
}

/// Parses a config file by dispatching each `channels[]` entry to the factory
/// whose kind matches its `type` tag. Unknown types and entries the factory
/// rejects are skipped, so a partially-valid file still delivers what it can.
fn parse_config<'a>(
    raw: &str,
    env: &dyn ConfigEnv,
    fx: &Effects<'a>,
) -> Vec<Box<dyn NotifyChannel + 'a>> {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let factories = registry();
    v["channels"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|entry| {
                    let kind = entry["type"].as_str()?;
                    factories
                        .iter()
                        .find(|f| f.kind() == kind)?
                        .build_from_config(entry, env, fx)
                })
                .collect()
        })
        .unwrap_or_default()
}
