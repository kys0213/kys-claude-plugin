pub mod check;
pub mod issue;
pub mod labels;
pub mod pipeline;
pub mod preflight;
pub mod simhash;

use clap::{Args, Parser, Subcommand};

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
    /// Change detection and state management
    Check {
        #[command(subcommand)]
        command: CheckCommands,
    },
    /// Pre-flight environment verification
    Preflight(PreflightArgs),
}

#[derive(Subcommand)]
pub enum CheckCommands {
    /// Diff since last mark, categorize spec vs code changes
    Diff {
        /// Loop name identifier
        loop_name: String,
        /// Comma-separated spec path prefixes
        #[arg(long, value_delimiter = ',')]
        spec_paths: Vec<String>,
    },
    /// Record current HEAD as analyzed
    Mark {
        /// Loop name identifier
        loop_name: String,
        /// Optional simhash of analysis output (for stagnation tracking)
        #[arg(long)]
        output_hash: Option<String>,
    },
    /// Show state of all loops
    Status,
}

#[derive(Args)]
pub struct PreflightArgs {
    /// Path to config file
    #[arg(long, default_value = "github-autopilot.local.md")]
    pub config: String,
    /// Repository root directory
    #[arg(long, default_value = ".")]
    pub repo_root: String,
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
    /// Search for similar issues by fingerprint and rank by simhash distance
    SearchSimilar(issue::SearchSimilarArgs),
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
