pub mod check;
pub mod epic;
pub mod events;
pub mod issue;
pub mod issue_list;
pub mod labels;
pub mod pipeline;
pub mod preflight;
pub mod simhash;
pub mod stats;
pub mod suppress;
pub mod task;
pub mod watch;
pub mod worktree;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "autopilot",
    version,
    about = "github-autopilot deterministic CLI"
)]
pub struct Cli {
    /// Path to autopilot.toml; defaults to `./autopilot.toml` if it exists
    #[arg(long, global = true)]
    pub config: Option<std::path::PathBuf>,

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
    /// Watch for push, CI, and issue events (event-driven autopilot)
    Watch(watch::WatchArgs),
    /// Worktree lifecycle management
    Worktree {
        #[command(subcommand)]
        command: WorktreeCommands,
    },
    /// Session statistics management
    Stats {
        #[command(subcommand)]
        command: StatsCommands,
    },
    /// Operator-facing task store diagnostics
    Task {
        #[command(subcommand)]
        command: TaskCommands,
    },
    /// Epic ledger operations
    Epic {
        #[command(subcommand)]
        command: epic::EpicCommands,
    },
    /// Fingerprint suppression for HITL alerts
    Suppress {
        #[command(subcommand)]
        command: suppress::SuppressCommands,
    },
    /// Event log queries
    Events {
        #[command(subcommand)]
        command: events::EventsCommands,
    },
}

#[derive(Subcommand)]
pub enum TaskCommands {
    /// List tasks in an epic
    List {
        /// Epic name
        #[arg(long)]
        epic: String,
        /// Filter by status
        #[arg(long)]
        status: Option<task::TaskStatusArg>,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Show details of a single task
    Show {
        /// Task id
        task_id: String,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Show details of a single task (alias of `show`, spec-canonical name)
    Get {
        /// Task id
        task_id: String,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Force a task into a specific status (operator override)
    ForceStatus {
        /// Task id
        task_id: String,
        /// Target status
        #[arg(long)]
        to: task::TaskStatusArg,
        /// Optional reason recorded with the override
        #[arg(long)]
        reason: Option<String>,
    },
    /// Insert (or detect duplicate of) a watch-style task
    Add {
        /// Epic name
        #[arg(long)]
        epic: String,
        /// Task id (deterministic 12-hex-char id)
        #[arg(long)]
        id: String,
        /// Title
        #[arg(long)]
        title: String,
        /// Optional body / description
        #[arg(long)]
        body: Option<String>,
        /// Override fingerprint (hex). When omitted, derived from title+body.
        #[arg(long)]
        fingerprint: Option<String>,
        /// Origin tag (defaults to `human`)
        #[arg(long, default_value = "human")]
        source: task::TaskSourceArg,
    },
    /// Insert tasks from a JSONL batch file
    AddBatch {
        /// Epic name
        #[arg(long)]
        epic: String,
        /// JSONL file. Each line: {"id":"...","title":"...","body?":"...","fingerprint?":"...","source?":"..."}
        #[arg(long)]
        from: std::path::PathBuf,
    },
    /// Look up a task by the PR number it owns
    FindByPr {
        /// PR number
        pr_number: u64,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Atomically claim the next ready task on an epic (Ready -> Wip)
    Claim {
        /// Epic name
        #[arg(long)]
        epic: String,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Release a Wip claim back to Ready (UC-11 push-reject path)
    Release {
        /// Task id
        task_id: String,
    },
    /// Mark a task as completed and unblock its dependents
    Complete {
        /// Task id
        task_id: String,
        /// Owning PR number
        #[arg(long)]
        pr: u64,
    },
    /// Record a failed attempt; outputs JSON {"outcome":"retried|escalated","attempts":N}
    Fail {
        /// Task id
        task_id: String,
    },
    /// Attach the HITL escalation issue number to a task
    Escalate {
        /// Task id
        task_id: String,
        /// HITL issue number
        #[arg(long)]
        issue: u64,
    },
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
        /// Loop status: "idle" increments idle counter, "active" resets it
        #[arg(long)]
        status: Option<LoopStatus>,
    },
    /// Show state of all loops
    Status,
    /// Pipeline health report across all loops
    Health,
    /// Reset (delete) loop state files
    Reset {
        /// Loop name to reset. If omitted, resets all loops.
        loop_name: Option<String>,
    },
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
    /// Detect overlapping issues by text similarity (stdin JSON)
    DetectOverlap(issue::DetectOverlapArgs),
    /// Filter issue comments for implementer agents (stdin JSON)
    FilterComments,
    /// List issues filtered by lifecycle stage
    List(ListArgs),
    /// Extract gap-fingerprint from issue body (stdin)
    ExtractFingerprint,
}

#[derive(Args)]
pub struct ListArgs {
    /// Lifecycle stage to filter by
    #[arg(long)]
    pub stage: issue_list::Stage,
    /// Label prefix (default: "autopilot:")
    #[arg(long, default_value = "autopilot:")]
    pub label_prefix: String,
    /// Only include issues with this exact label
    #[arg(long)]
    pub require_label: Option<String>,
    /// Maximum number of issues to fetch
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Subcommand)]
pub enum WorktreeCommands {
    /// Clean up worktree and local branch after PR merge
    Cleanup {
        /// Branch name to clean up
        #[arg(long)]
        branch: String,
    },
    /// Remove stale draft/* worktrees, preserving branches with partial commits
    CleanupStale,
}

#[derive(Clone, ValueEnum)]
pub enum LoopStatus {
    Idle,
    Active,
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

#[derive(Subcommand)]
pub enum StatsCommands {
    /// Initialize (or reset) session statistics
    Init,
    /// Update statistics for a command
    Update {
        /// Command name (e.g. "build-issues")
        #[arg(long)]
        command: String,
        /// Number of issues processed this cycle
        #[arg(long, default_value_t = 0)]
        processed: u32,
        /// Number of successful implementations
        #[arg(long, default_value_t = 0)]
        success: u32,
        /// Number of failed implementations
        #[arg(long, default_value_t = 0)]
        failed: u32,
        /// Number of false positives closed
        #[arg(long, default_value_t = 0)]
        false_positive: u32,
    },
    /// Show session statistics
    Show {
        /// Filter by command name (omit for all)
        #[arg(long)]
        command: Option<String>,
    },
}
