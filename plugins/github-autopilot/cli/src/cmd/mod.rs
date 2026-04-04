pub mod issue;
pub mod labels;
pub mod pipeline;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "autopilot",
    version,
    about = "github-autopilot deterministic CLI"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Issue management
    Issue {
        #[command(subcommand)]
        command: IssueCommands,
    },
    /// Pipeline status
    Pipeline {
        #[command(subcommand)]
        command: PipelineCommands,
    },
}

#[derive(Subcommand)]
pub enum IssueCommands {
    /// Check if a fingerprint already exists in open issues
    CheckDup {
        /// Fingerprint string to search for
        #[arg(long)]
        fingerprint: String,
    },
    /// Create an issue with fingerprint dedup
    Create(issue::CreateArgs),
    /// Close CI-failure issues whose branch PR has been merged
    CloseResolved {
        /// Label prefix (default: "autopilot:")
        #[arg(long, default_value = "autopilot:")]
        label_prefix: String,
    },
}

#[derive(Subcommand)]
pub enum PipelineCommands {
    /// Check if the autopilot pipeline is idle
    Idle {
        /// Label prefix (default: "autopilot:")
        #[arg(long, default_value = "autopilot:")]
        label_prefix: String,
    },
}
