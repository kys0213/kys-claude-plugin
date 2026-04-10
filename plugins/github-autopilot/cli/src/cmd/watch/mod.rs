pub mod events;

use crate::github::GitHub;
use anyhow::Result;
use clap::Subcommand;
use events::{detect_events, BranchMode, EventFilter};
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

        let mut etag: Option<String> = None;
        let mut last_seen_id = String::from("0");
        let mut poll_interval = Duration::from_secs(poll_sec);

        loop {
            match self.github.fetch_events(etag.as_deref()) {
                Ok(Some(response)) => {
                    if response.poll_interval > 0 {
                        let server_interval = Duration::from_secs(response.poll_interval);
                        poll_interval = poll_interval.max(server_interval);
                    }

                    let events = detect_events(&response, &filter, &last_seen_id);

                    if let Some(max_id) = response
                        .events
                        .iter()
                        .filter_map(|e| e.id.parse::<u64>().ok())
                        .max()
                    {
                        last_seen_id = max_id.to_string();
                    }

                    etag = Some(response.etag);

                    for event in events {
                        println!("{event}");
                    }
                }
                Ok(None) => {}
                Err(_) => {}
            }

            thread::sleep(poll_interval);
        }
    }
}
