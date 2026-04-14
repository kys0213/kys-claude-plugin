use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::cmd::check::state::{state_dir, utc_timestamp};
use crate::fs::FsOps;
use crate::git::GitOps;

const STATS_FILE: &str = "session-stats.json";

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
