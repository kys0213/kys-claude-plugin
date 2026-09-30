use crate::session::core::settings_env::{ensure_env_key, EnvEdit, SettingsFile};

#[derive(Debug, PartialEq)]
pub enum EnsureEnvOutcome {
    Added { backup: Option<String> },
    AlreadySet,
    Skipped(String),
}

pub struct EnsureEnvCommand<'a> {
    file: &'a dyn SettingsFile,
}

impl<'a> EnsureEnvCommand<'a> {
    pub fn new(file: &'a dyn SettingsFile) -> Self {
        Self { file }
    }

    pub fn run(&self, key: &str, value: &str) -> EnsureEnvOutcome {
        let existing = match self.file.read() {
            Ok(existing) => existing,
            Err(e) => return EnsureEnvOutcome::Skipped(format!("cannot read settings: {e}")),
        };
        match ensure_env_key(existing.as_deref(), key, value) {
            EnvEdit::Present(_) => EnsureEnvOutcome::AlreadySet,
            EnvEdit::Unusable(reason) => EnsureEnvOutcome::Skipped(reason),
            EnvEdit::Add(content) => match self.file.replace(&content) {
                Ok(backup) => EnsureEnvOutcome::Added { backup },
                Err(e) => EnsureEnvOutcome::Skipped(format!("cannot write settings: {e}")),
            },
        }
    }
}

/// SessionStart stdout reaches the model as context, not the user directly, so
/// the line is phrased for the model to relay.
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
