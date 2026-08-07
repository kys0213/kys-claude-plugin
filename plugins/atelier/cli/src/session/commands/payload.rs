//! Hook stdin payload fields the session commands consume. Every Claude Code
//! hook receives `session_id`, `transcript_path`, `cwd`, `hook_event_name` and
//! `permission_mode` on stdin; only the two fields used here are parsed.
//!
//! Deliberately separate from `git::commands::guard::HookPayload`, which reads
//! `tool_input.*` for PreToolUse — the two schemas share no field.
//!
//! `parse` is swallow-all: unreadable or non-JSON stdin yields all-`None`, and
//! the caller then stays silent.

/// Session hook payload as far as this subsystem cares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionPayload {
    pub session_id: Option<String>,
    /// Session cwd — the project-dir fallback when the shim passes no flag.
    pub cwd: Option<String>,
}

impl SessionPayload {
    pub fn parse(raw: &str) -> SessionPayload {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
            return SessionPayload::default();
        };
        SessionPayload {
            session_id: value["session_id"].as_str().map(|s| s.to_string()),
            cwd: value["cwd"].as_str().map(|s| s.to_string()),
        }
    }
}
