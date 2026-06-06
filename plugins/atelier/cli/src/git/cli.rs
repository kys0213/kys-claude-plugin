//! `atelier git <...>` clap routing. Replaces git-utils `cli.ts`'s custom
//! `parseArgs` (the unified atelier CLI uses clap, per architecture §4.3), so
//! the custom parser and its tests are obsoleted; routing behaviour is covered
//! by the `git_cli` e2e tests instead.

use clap::{Args, Subcommand};

use crate::git::commands::branch::BranchCommand;
use crate::git::commands::commit::CommitCommand;
use crate::git::commands::pr::PrCommand;
use crate::git::commands::reviews::ReviewsCommand;
use crate::git::core::git::RealGitService;
use crate::git::core::github::RealGitHubService;
use crate::git::types::{BranchInput, CommitInput, PrInput, ReviewsInput};

/// Args for the `atelier git <...>` subcommand group.
#[derive(Args)]
pub struct GitCli {
    #[command(subcommand)]
    pub command: GitCommands,
}

#[derive(Subcommand)]
pub enum GitCommands {
    /// Create a new branch from a base branch
    Branch {
        branch_name: String,
        /// Base branch (defaults to the detected default branch)
        #[arg(long)]
        base: Option<String>,
    },
    /// Smart commit with Jira ticket detection
    Commit {
        /// Conventional-commit type (feat, fix, docs, …)
        commit_type: String,
        description: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long = "skip-add")]
        skip_add: bool,
    },
    /// Create a Pull Request
    Pr {
        title: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Query unresolved PR review threads
    Reviews {
        /// PR number (defaults to the current branch's PR)
        #[arg(long = "pr")]
        pr_number: Option<i64>,
    },
}

/// Runs the `atelier git` subcommand, printing the result and exiting with
/// `0` on success or `1` on a command error (stderr).
pub fn run(cli: GitCli) {
    let outcome = match cli.command {
        GitCommands::Branch { branch_name, base } => {
            let git = RealGitService::new(None);
            BranchCommand::new(&git)
                .run(&BranchInput {
                    branch_name,
                    base_branch: base,
                })
                .map(|o| {
                    format!(
                        "Created branch '{}' from '{}'",
                        o.branch_name, o.base_branch
                    )
                })
        }
        GitCommands::Commit {
            commit_type,
            description,
            scope,
            body,
            skip_add,
        } => {
            let git = RealGitService::new(None);
            CommitCommand::new(&git)
                .run(&CommitInput {
                    commit_type,
                    description,
                    scope,
                    body,
                    skip_add,
                })
                .map(|o| format!("Committed: {}", o.subject))
        }
        GitCommands::Pr { title, description } => {
            let git = RealGitService::new(None);
            let github = RealGitHubService::new(None);
            PrCommand::new(&git, &github)
                .run(&PrInput { title, description })
                .map(|o| o.url)
        }
        GitCommands::Reviews { pr_number } => {
            let github = RealGitHubService::new(None);
            ReviewsCommand::new(&github)
                .run(&ReviewsInput { pr_number })
                .map(|o| format!("{} — {} review thread(s)", o.pr_title, o.threads.len()))
        }
    };

    match outcome {
        Ok(message) => {
            println!("{message}");
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
