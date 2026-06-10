//! Git subsystem — Rust port of the `git-utils` TypeScript CLI. Provides the
//! clap surface (`Cli`/`Commands`), the real `HookFs` implementation, and the
//! `run`/`run_from` entry points the top-level atelier router dispatches to.
//!
//! Output contract (preserved from `git-utils/src/cli.ts`):
//! - success: pretty-printed JSON of the command `data` on stdout, exit 0.
//! - command error: `Error: <message>` on stderr, exit 1.
//! - guard/pr-guard block: reason on stderr, exit 2.

pub mod commands;
pub mod core;
pub mod types;

use crate::git::commands::hook::{create_hook_command, HookFs};
use crate::git::core::git::create_git_service;
use crate::git::core::github::create_github_service;
use crate::git::core::guard::create_guard_service;
use crate::git::core::jira::create_jira_service;
use crate::git::core::pr_guard::create_pr_guard_service;
use crate::git::types::{
    BranchInput, CmdResult, CommitInput, GuardCommandTarget, GuardDecision, GuardTarget,
    HookListInput, HookRegisterInput, HookUnregisterInput, PrInput, ReviewsInput,
};
use clap::{Parser, Subcommand};
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "git",
    version,
    about = "Git workflow automation CLI (ported from git-utils)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Smart commit with Jira ticket detection
    Commit {
        /// Commit type (feat, fix, docs, ...)
        commit_type: String,
        /// Commit description
        #[arg(default_value = "")]
        description: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long = "skip-add")]
        skip_add: bool,
    },
    /// Create a new branch from base branch
    Branch {
        #[arg(default_value = "")]
        branch_name: String,
        #[arg(long)]
        base: Option<String>,
    },
    /// Create a Pull Request
    Pr {
        #[arg(default_value = "")]
        title: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Query unresolved PR review threads
    Reviews { pr_number: Option<i64> },
    /// Tool guard (Claude hook): branch protection or PR duplicate check
    Guard {
        /// write | commit | pr
        target: Option<String>,
        #[arg(long = "project-dir")]
        project_dir: Option<String>,
        #[arg(long = "create-branch-script")]
        create_branch_script: Option<String>,
        #[arg(long = "default-branch")]
        default_branch: Option<String>,
        #[arg(long = "protected-branches")]
        protected_branches: Option<String>,
    },
    /// Deprecated alias of `guard pr`
    #[command(name = "pr-guard")]
    PrGuard,
    /// Manage Claude Code hooks in settings.json
    Hook {
        /// register | unregister | list
        sub: Option<String>,
        /// positional args for the subcommand
        args: Vec<String>,
        #[arg(long)]
        timeout: Option<i64>,
        #[arg(long = "project-dir")]
        project_dir: Option<String>,
    },
}

/// Real filesystem for the hook command.
struct RealHookFs;

impl HookFs for RealHookFs {
    fn read_file(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    }
    fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        std::fs::write(path, content).map_err(|e| e.to_string())
    }
    fn exists(&self, path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
    fn mkdir(&self, path: &str) -> Result<(), String> {
        std::fs::create_dir_all(path).map_err(|e| e.to_string())
    }
}

/// Reads the Claude hook JSON from stdin, extracting `tool_input.command` and
/// `tool_input.file_path`. Returns `(None, None)` on any parse error, matching
/// the TS `readHookStdin` swallow-all behavior.
fn read_hook_stdin() -> (Option<String>, Option<String>) {
    use std::io::Read as _;
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() {
        return (None, None);
    }
    match serde_json::from_str::<serde_json::Value>(&buf) {
        Ok(v) => {
            let cmd = v["tool_input"]["command"].as_str().map(|s| s.to_string());
            let fp = v["tool_input"]["file_path"].as_str().map(|s| s.to_string());
            (cmd, fp)
        }
        Err(_) => (None, None),
    }
}

/// Maps a guard decision to the hook exit contract: allow → 0, block → reason
/// on stderr + exit 2 (PreToolUse deny signal).
fn guard_exit(decision: GuardDecision) -> i32 {
    if decision.allowed {
        return 0;
    }
    if let Some(reason) = decision.reason {
        eprintln!("{reason}");
    }
    2
}

/// Prints a successful command result as pretty JSON (exit 0) or an error to
/// stderr (exit 1), mirroring the TS `output()` helper. Works for any
/// `Serialize` payload, including raw `serde_json::Value` (hook list).
fn output<T: Serialize>(result: CmdResult<T>) -> i32 {
    match result {
        CmdResult::Ok(data) => {
            let json = serde_json::to_string_pretty(&data).unwrap_or_else(|_| "null".to_string());
            println!("{json}");
            0
        }
        CmdResult::Err(e) => {
            eprintln!("Error: {e}");
            1
        }
    }
}

/// Parses `argv` (including the leading program name) with the git clap surface
/// and runs the selected command, returning a process exit code.
pub fn run_from<I, T>(argv: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(argv);
    run(cli)
}

/// Runs a parsed git CLI, returning a process exit code.
pub fn run(cli: Cli) -> i32 {
    // No subcommand: print usage and exit 0, matching the standalone
    // `git-utils` CLI (cli.ts prints usage + exit 0 on no args) rather than
    // clap's default missing-subcommand error (exit 2).
    let command = match cli.command {
        Some(c) => c,
        None => {
            use clap::CommandFactory;
            let _ = Cli::command().print_help();
            println!();
            return 0;
        }
    };

    match command {
        Commands::Commit {
            commit_type,
            description,
            scope,
            body,
            skip_add,
        } => {
            let git = create_git_service(None);
            let jira = create_jira_service();
            let deps = commands::commit::CommitDeps {
                git: &git,
                jira: &jira,
            };
            let input = CommitInput {
                commit_type,
                description,
                scope,
                body,
                skip_add,
            };
            match commands::commit::run(&deps, &input) {
                Ok(result) => output(result),
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        Commands::Branch { branch_name, base } => {
            let git = create_git_service(None);
            let deps = commands::branch::BranchDeps { git: &git };
            let input = BranchInput {
                branch_name,
                base_branch: base,
            };
            match commands::branch::run(&deps, &input) {
                Ok(result) => output(result),
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        Commands::Pr { title, description } => {
            let git = create_git_service(None);
            let jira = create_jira_service();
            let github = create_github_service(None);
            let deps = commands::pr::PrDeps {
                git: &git,
                jira: &jira,
                github: &github,
            };
            let input = PrInput { title, description };
            output(commands::pr::run(&deps, &input))
        }
        Commands::Reviews { pr_number } => {
            let github = create_github_service(None);
            let deps = commands::reviews::ReviewsDeps { github: &github };
            let input = ReviewsInput { pr_number };
            output(commands::reviews::run(&deps, &input))
        }
        Commands::Guard {
            target,
            project_dir,
            create_branch_script,
            default_branch,
            protected_branches,
        } => {
            let (tool_command, tool_file_path) = read_hook_stdin();
            let target = match target.as_deref() {
                Some("write") => GuardCommandTarget::Branch(GuardTarget::Write {
                    file_path: tool_file_path,
                }),
                Some("commit") => GuardCommandTarget::Branch(GuardTarget::Commit {
                    command: tool_command,
                }),
                Some("pr") => GuardCommandTarget::Pr {
                    command: tool_command,
                },
                _ => {
                    eprintln!("Usage: atelier git guard <write|commit|pr> --project-dir=<p> --create-branch-script=<s>");
                    return 1;
                }
            };
            let protected = protected_branches.map(|raw| {
                raw.split(',')
                    .map(|b| b.trim().to_string())
                    .filter(|b| !b.is_empty())
                    .collect::<Vec<_>>()
            });
            let project_dir = project_dir.unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
            let create_branch_script =
                create_branch_script.unwrap_or_else(|| "atelier git branch".to_string());
            // Pin the git service to project_dir so branch / special-state /
            // default-branch detection reflect the project, not the hook's
            // process cwd (worktree / subagent contexts) — see #780.
            let git = create_git_service(Some(project_dir.clone()));
            let branch_guard = create_guard_service(&git);
            let github = create_github_service(None);
            let pr_guard = create_pr_guard_service(&github);
            let deps = commands::guard::GuardCommandDeps {
                branch_guard: &branch_guard,
                pr_guard: &pr_guard,
            };
            let input = commands::guard::GuardCommandInput {
                target,
                project_dir,
                create_branch_script,
                default_branch,
                protected_branches: protected,
            };
            guard_exit(commands::guard::run(&deps, &input))
        }
        Commands::PrGuard => {
            // Legacy alias of `guard pr` — kept so hooks registered before
            // the unified `guard` surface (#777) keep working.
            let github = create_github_service(None);
            let pr_guard = create_pr_guard_service(&github);
            let (tool_command, _) = read_hook_stdin();
            guard_exit(commands::guard::check_pr(&pr_guard, tool_command))
        }
        Commands::Hook {
            sub,
            args,
            timeout,
            project_dir,
        } => {
            let fs = RealHookFs;
            let hook = create_hook_command(&fs);
            match sub.as_deref() {
                Some("register") => {
                    let input = HookRegisterInput {
                        hook_type: args.first().cloned().unwrap_or_default(),
                        matcher: args.get(1).cloned().unwrap_or_default(),
                        command: args.get(2).cloned().unwrap_or_default(),
                        timeout,
                        project_dir,
                    };
                    match hook.register(&input) {
                        Ok(result) => output(result),
                        Err(e) => {
                            eprintln!("Error: {e}");
                            1
                        }
                    }
                }
                Some("unregister") => {
                    let input = HookUnregisterInput {
                        hook_type: args.first().cloned().unwrap_or_default(),
                        command: args.get(1).cloned().unwrap_or_default(),
                        project_dir,
                    };
                    match hook.unregister(&input) {
                        Ok(result) => output(result),
                        Err(e) => {
                            eprintln!("Error: {e}");
                            1
                        }
                    }
                }
                Some("list") => {
                    let input = HookListInput {
                        hook_type: args.first().cloned().filter(|s| !s.is_empty()),
                        project_dir,
                    };
                    match hook.list(&input) {
                        Ok(result) => output(result),
                        Err(e) => {
                            eprintln!("Error: {e}");
                            1
                        }
                    }
                }
                _ => {
                    eprintln!("Usage: atelier git hook <register|unregister|list> [args...]");
                    1
                }
            }
        }
    }
}
