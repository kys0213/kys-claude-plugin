//! `atelier git <...>` clap routing. Replaces git-utils `cli.ts`'s custom
//! `parseArgs` (the unified atelier CLI uses clap, per architecture §4.3), so
//! the custom parser and its tests are obsoleted; routing behaviour is covered
//! by the `git_cli` e2e tests instead.
//!
//! Guard/hook subcommands take every input from args / env / stdin — nothing
//! is hardcoded. Hook *registration* is therefore done at setup time via
//! `atelier git hook register` (which writes the right `atelier git guard ...`
//! command into settings.json) rather than baking shell-script paths in.

use std::io::Read;
use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use serde_json::{json, Value};

use crate::git::commands::branch::BranchCommand;
use crate::git::commands::commit::CommitCommand;
use crate::git::commands::hook;
use crate::git::commands::pr::PrCommand;
use crate::git::commands::reviews::ReviewsCommand;
use crate::git::core::git::RealGitService;
use crate::git::core::github::RealGitHubService;
use crate::git::core::guard::{GuardService, RealGuardService};
use crate::git::types::{
    BranchInput, CommitInput, GuardInput, GuardTarget, HookListInput, HookRegisterInput,
    HookUnregisterInput, PrInput, ReviewsInput,
};

/// Args for the `atelier git <...>` subcommand group.
#[derive(Args)]
pub struct GitCli {
    #[command(subcommand)]
    pub command: GitCommands,
}

/// CLI-facing form of [`GuardTarget`].
#[derive(Clone, Copy, ValueEnum)]
pub enum GuardTargetArg {
    Write,
    Commit,
}

impl From<GuardTargetArg> for GuardTarget {
    fn from(v: GuardTargetArg) -> Self {
        match v {
            GuardTargetArg::Write => GuardTarget::Write,
            GuardTargetArg::Commit => GuardTarget::Commit,
        }
    }
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
    /// Default-branch guard for PreToolUse hooks. Reads the hook payload
    /// (`tool_input.command` / `tool_input.file_path`) on stdin. Exit `0`
    /// allows, `2` blocks.
    Guard {
        /// Which guard to run.
        #[arg(long, value_enum)]
        target: GuardTargetArg,
        /// Project root (defaults to $CLAUDE_PROJECT_DIR).
        #[arg(long, env = "CLAUDE_PROJECT_DIR", default_value = ".")]
        project_dir: String,
        /// Command suggested in the block message for creating a branch.
        #[arg(long, default_value = "atelier git branch")]
        create_branch_cmd: String,
        /// Extra protected branches beyond default + develop (repeatable).
        #[arg(long = "protected")]
        protected: Vec<String>,
        /// Override the detected default branch.
        #[arg(long)]
        default_branch: Option<String>,
    },
    /// Manage Claude Code hooks in `<project>/.claude/settings.json`
    Hook {
        #[command(subcommand)]
        command: HookCommand,
    },
}

#[derive(Subcommand)]
pub enum HookCommand {
    /// Register (or replace) a hook
    Register {
        #[arg(long = "type")]
        hook_type: String,
        #[arg(long)]
        matcher: String,
        #[arg(long)]
        command: String,
        #[arg(long)]
        timeout: Option<i64>,
        #[arg(long, env = "CLAUDE_PROJECT_DIR")]
        project_dir: Option<String>,
    },
    /// Remove hooks that invoke a command
    Unregister {
        #[arg(long = "type")]
        hook_type: String,
        #[arg(long)]
        command: String,
        #[arg(long, env = "CLAUDE_PROJECT_DIR")]
        project_dir: Option<String>,
    },
    /// List registered hooks (optionally filtered by type)
    List {
        #[arg(long = "type")]
        hook_type: Option<String>,
        #[arg(long, env = "CLAUDE_PROJECT_DIR")]
        project_dir: Option<String>,
    },
}

/// Runs the `atelier git` subcommand and exits with the appropriate code.
pub fn run(cli: GitCli) {
    std::process::exit(dispatch(cli.command));
}

/// Prints a command result: `Ok` → stdout + exit 0, `Err` → stderr + exit 1.
fn print_result(result: Result<String, String>) -> i32 {
    match result {
        Ok(message) => {
            println!("{message}");
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn dispatch(command: GitCommands) -> i32 {
    match command {
        GitCommands::Branch { branch_name, base } => {
            let git = RealGitService::new(None);
            print_result(
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
                    }),
            )
        }
        GitCommands::Commit {
            commit_type,
            description,
            scope,
            body,
            skip_add,
        } => {
            let git = RealGitService::new(None);
            print_result(
                CommitCommand::new(&git)
                    .run(&CommitInput {
                        commit_type,
                        description,
                        scope,
                        body,
                        skip_add,
                    })
                    .map(|o| format!("Committed: {}", o.subject)),
            )
        }
        GitCommands::Pr { title, description } => {
            let git = RealGitService::new(None);
            let github = RealGitHubService::new(None);
            print_result(
                PrCommand::new(&git, &github)
                    .run(&PrInput { title, description })
                    .map(|o| o.url),
            )
        }
        GitCommands::Reviews { pr_number } => {
            let github = RealGitHubService::new(None);
            print_result(
                ReviewsCommand::new(&github)
                    .run(&ReviewsInput { pr_number })
                    .map(|o| format!("{} — {} review thread(s)", o.pr_title, o.threads.len())),
            )
        }
        GitCommands::Guard {
            target,
            project_dir,
            create_branch_cmd,
            protected,
            default_branch,
        } => run_guard(
            target,
            project_dir,
            create_branch_cmd,
            protected,
            default_branch,
        ),
        GitCommands::Hook { command } => dispatch_hook(command),
    }
}

/// PreToolUse guard: builds [`GuardInput`] entirely from args/env/stdin (no
/// hardcoded paths or branches) and maps the decision to a hook exit code.
fn run_guard(
    target: GuardTargetArg,
    project_dir: String,
    create_branch_cmd: String,
    protected: Vec<String>,
    default_branch: Option<String>,
) -> i32 {
    let payload = read_stdin_json();
    let tool_input = &payload["tool_input"];
    let tool_command = tool_input["command"].as_str().map(str::to_string);
    let tool_file_path = tool_input["file_path"].as_str().map(str::to_string);

    let git = RealGitService::new(Some(PathBuf::from(&project_dir)));
    let out = RealGuardService::new(&git).check(&GuardInput {
        target: target.into(),
        project_dir,
        create_branch_script: create_branch_cmd,
        default_branch,
        protected_branches: (!protected.is_empty()).then_some(protected),
        tool_command,
        tool_file_path,
    });

    if out.allowed {
        0
    } else {
        if let Some(reason) = out.reason {
            eprintln!("{reason}");
        }
        2 // PreToolUse: non-zero (2) blocks the tool call
    }
}

fn dispatch_hook(command: HookCommand) -> i32 {
    match command {
        HookCommand::Register {
            hook_type,
            matcher,
            command,
            timeout,
            project_dir,
        } => print_result(
            hook::register(&HookRegisterInput {
                hook_type,
                matcher,
                command,
                timeout,
                project_dir,
            })
            .map(|o| format!("{} hook: {}", o.action, o.command)),
        ),
        HookCommand::Unregister {
            hook_type,
            command,
            project_dir,
        } => print_result(
            hook::unregister(&HookUnregisterInput {
                hook_type,
                command,
                project_dir,
            })
            .map(|o| format!("removed hook: {}", o.command)),
        ),
        HookCommand::List {
            hook_type,
            project_dir,
        } => print_result(
            hook::list(&HookListInput {
                hook_type,
                project_dir,
            })
            .and_then(|v| serde_json::to_string_pretty(&v).map_err(|e| e.to_string())),
        ),
    }
}

/// Reads stdin as JSON (the PreToolUse hook payload); empty/invalid → `{}`.
fn read_stdin_json() -> Value {
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
    serde_json::from_str(&buf).unwrap_or_else(|_| json!({}))
}
