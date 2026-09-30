//! `session ensure-env` command — makes sure a settings file's `env` carries a
//! key, adding it only when absent. Which key, which value and which file are
//! all arguments; whether to run at all is the calling shim's decision.

use crate::session::core::settings_env::{ensure_env_key, EnvEdit, SettingsFile};

#[derive(Debug, PartialEq)]
pub enum EnsureEnvOutcome {
    /// The key was written. `backup` is where the previous file was saved.
    Added { backup: Option<String> },
    /// The key was already present; nothing was written.
    AlreadySet,
    /// Nothing was written, for this reason (unreadable, unparseable or
    /// unwritable settings).
    Skipped(String),
}

pub fn run(file: &dyn SettingsFile, key: &str, value: &str) -> EnsureEnvOutcome {
    let existing = match file.read() {
        Ok(existing) => existing,
        Err(e) => return EnsureEnvOutcome::Skipped(format!("cannot read settings: {e}")),
    };
    match ensure_env_key(existing.as_deref(), key, value) {
        EnvEdit::Present(_) => EnsureEnvOutcome::AlreadySet,
        EnvEdit::Unusable(reason) => EnsureEnvOutcome::Skipped(reason),
        EnvEdit::Add(content) => match file.replace(&content) {
            Ok(backup) => EnsureEnvOutcome::Added { backup },
            Err(e) => EnsureEnvOutcome::Skipped(format!("cannot write settings: {e}")),
        },
    }
}

/// The line a SessionStart hook prints for the outcome, or `None` when there
/// is nothing to say. SessionStart stdout reaches the model as context, so the
/// text is written for it to relay to the user.
pub fn render(outcome: &EnsureEnvOutcome, path: &str, key: &str, value: &str) -> Option<String> {
    match outcome {
        EnsureEnvOutcome::AlreadySet => None,
        EnsureEnvOutcome::Added { backup } => {
            let backup = backup
                .as_deref()
                .map(|b| format!(" (backup: {b})"))
                .unwrap_or_default();
            Some(format!(
                "[atelier] Added env.{key}=\"{value}\" to {path}{backup}. \
                 It takes effect from the next Claude Code session. \
                 To opt out, set it to \"0\" there — an existing value is never overwritten."
            ))
        }
        EnsureEnvOutcome::Skipped(reason) => Some(format!(
            "[atelier] Did not add env.{key} to {path}: {reason}. The file was left unchanged."
        )),
    }
}
