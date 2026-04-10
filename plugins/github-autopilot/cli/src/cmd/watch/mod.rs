pub mod events;

use crate::github::GitHub;
use anyhow::Result;
use clap::Subcommand;
use events::{collect_ids, detect_events, BranchMode, EventFilter};
use std::collections::HashSet;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Subcommand)]
pub enum WatchCommands {
    /// Watch repository events (push, CI, issues) via GitHub Events API
    Events {
        /// Poll interval in seconds (respects X-Poll-Interval from server)
        #[arg(long, default_value = "60")]
        poll_sec: u64,
        /// Branch filter mode
        #[arg(long, value_enum, default_value = "autopilot")]
        branch_filter: BranchMode,
    },
}

/// Service for running watch loops.
pub struct WatchService {
    github: Arc<dyn GitHub>,
}

impl WatchService {
    pub fn new(github: Arc<dyn GitHub>) -> Self {
        Self { github }
    }

    /// Run the events watch loop. Emits WatchEvent lines to stdout.
    ///
    /// This function runs indefinitely until the process is killed.
    pub fn run_events(&self, branch_mode: &BranchMode, poll_sec: u64) -> Result<i32> {
        let default_branch = self.github.default_branch()?;

        let filter = EventFilter {
            default_branch,
            branch_mode: branch_mode.clone(),
        };

        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut poll_interval = Duration::from_secs(poll_sec);

        loop {
            // Always fetch without ETag — GitHub's 300s cache makes conditional
            // requests return 304 even when new events exist. seen_ids handles dedup.
            match self.github.fetch_events(None) {
                Ok(Some(response)) => {
                    if response.poll_interval > 0 {
                        let server_interval = Duration::from_secs(response.poll_interval);
                        poll_interval = poll_interval.max(server_interval);
                    }

                    let events = detect_events(&response, &filter, &seen_ids);
                    seen_ids.extend(collect_ids(&response));

                    for event in events {
                        println!("{event}");
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("fetch error: {e:#}");
                }
            }

            thread::sleep(poll_interval);
        }
    }
}
