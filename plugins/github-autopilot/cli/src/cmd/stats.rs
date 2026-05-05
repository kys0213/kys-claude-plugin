use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::cmd::check::state::{state_dir, utc_timestamp, validate_loop_name};
use crate::fs::FsOps;
use crate::git::GitOps;

const STATS_FILE: &str = "session-stats.json";

/// Canonical autopilot loop / cron command names that emit stats.
///
/// `--command` is intentionally a free string so new loops can land without a
/// CLI release, but values *should* match this list. Unknown names are accepted
/// (a warning is emitted on stderr) so we never block a new command from
/// recording stats; rejecting only happens for structurally invalid values
/// (empty, path-traversal characters — see `validate_loop_name`).
pub const KNOWN_COMMANDS: &[&str] = &[
    "build-issues",
    "gap-watch",
    "qa-boost",
    "ci-watch",
    "pr-merger",
    "merge-prs",
    "work-ledger",
];

#[derive(Serialize, Deserialize, Default)]
pub struct SessionStats {
    pub started_at: String,
    pub commands: HashMap<String, CommandStats>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct CommandStats {
    pub total_cycles: u32,
    pub processed: u32,
    pub success: u32,
    pub failed: u32,
    pub false_positive: u32,
    pub idle_cycles: u32,
    pub consecutive_idle: u32,
    pub agent_calls: u32,
}

pub struct StatsService {
    git: Box<dyn GitOps>,
    fs: Box<dyn FsOps>,
}

impl StatsService {
    pub fn new(git: Box<dyn GitOps>, fs: Box<dyn FsOps>) -> Self {
        Self { git, fs }
    }

    /// Initialize (or reset) session statistics.
    pub fn init(&self) -> Result<i32> {
        let path = state_dir(self.git.as_ref())?.join(STATS_FILE);
        let stats = SessionStats {
            started_at: utc_timestamp(),
            commands: HashMap::new(),
        };
        self.fs.write_file(&path, &serde_json::to_string(&stats)?)?;
        println!("session stats initialized at {}", stats.started_at);
        Ok(0)
    }

    /// Update statistics for a command.
    pub fn update(
        &self,
        command: &str,
        processed: u32,
        success: u32,
        failed: u32,
        false_positive: u32,
    ) -> Result<i32> {
        validate_loop_name(command)?;
        if !KNOWN_COMMANDS.contains(&command) {
            eprintln!(
                "warning: --command {command:?} is not in the canonical list ({}). Recording anyway.",
                KNOWN_COMMANDS.join(", ")
            );
        }
        let path = state_dir(self.git.as_ref())?.join(STATS_FILE);
        let mut stats = self.read_or_init(&path)?;

        {
            let entry = stats.commands.entry(command.to_string()).or_default();
            entry.total_cycles += 1;
            entry.processed += processed;
            entry.success += success;
            entry.failed += failed;
            entry.false_positive += false_positive;

            if processed == 0 {
                entry.idle_cycles += 1;
                entry.consecutive_idle += 1;
            } else {
                entry.consecutive_idle = 0;
                entry.agent_calls += processed;
            }
        }

        self.fs.write_file(&path, &serde_json::to_string(&stats)?)?;
        let entry = &stats.commands[command];
        println!("{}", serde_json::to_string(entry)?);
        Ok(0)
    }

    /// Show session statistics.
    pub fn show(&self, command: Option<&str>) -> Result<i32> {
        if let Some(name) = command {
            validate_loop_name(name)?;
        }
        let path = state_dir(self.git.as_ref())?.join(STATS_FILE);
        let stats = match self.fs.read_file(&path) {
            Ok(content) => serde_json::from_str::<SessionStats>(&content)?,
            Err(_) => {
                println!("(no session stats found)");
                return Ok(0);
            }
        };

        match command {
            Some(cmd) => match stats.commands.get(cmd) {
                Some(entry) => println!("{}", serde_json::to_string_pretty(entry)?),
                None => println!("(no stats for command: {cmd})"),
            },
            None => println!("{}", serde_json::to_string_pretty(&stats)?),
        }
        Ok(0)
    }

    fn read_or_init(&self, path: &std::path::Path) -> Result<SessionStats> {
        match self.fs.read_file(path) {
            Ok(content) => Ok(serde_json::from_str(&content)?),
            Err(_) => Ok(SessionStats {
                started_at: utc_timestamp(),
                commands: HashMap::new(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_commands_includes_work_ledger() {
        assert!(KNOWN_COMMANDS.contains(&"work-ledger"));
    }

    #[test]
    fn all_canonical_names_pass_validation() {
        for name in KNOWN_COMMANDS {
            assert!(validate_loop_name(name).is_ok(), "rejected: {name}");
        }
    }
}
