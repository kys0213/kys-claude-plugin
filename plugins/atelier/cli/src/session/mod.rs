//! Session subsystem — hooks that need to know what *this* Claude Code session
//! did, as opposed to what was already in the tree.
//!
//! ```text
//! atelier session baseline        --project-dir <dir>   # SessionStart
//! atelier session simplify-check  --project-dir <dir>   # Stop
//! atelier session push-check      --project-dir <dir>   # Stop
//! ```
//!
//! Output contract: every command reads the hook payload from stdin, writes to
//! stdout only, and **always exits 0** — the exit code never carries a signal,
//! because a Stop hook's exit 2 means "block on stderr" and a failing binary
//! would then wedge every session end.
//!
//! What stdout carries differs by command: `baseline` prints nothing,
//! `simplify-check` prints at most an advisory banner, and `push-check` may
//! print a Stop `{"decision":"block","reason":…}` document — still on exit 0,
//! which is how Claude Code reads a structured block.

pub mod commands;
pub mod core;

use crate::git::core::git::create_git_service;
use crate::git::core::github::create_github_service;
use crate::session::commands::payload::SessionPayload;
use crate::session::commands::push_check::{render_block_json, PushCheckDecision, PushCheckDeps};
use crate::session::commands::simplify::{render_banner, SimplifyDecision};
use crate::session::commands::SessionDeps;
use crate::session::core::baseline::{FsBaselineStore, DEFAULT_TTL};
use crate::session::core::repo::create_repo_reader;
use crate::shared::process::{default_project_dir, read_stdin_raw};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "session",
    version,
    about = "Session-scoped hook helpers (baseline / simplify-check / push-check)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// SessionStart: record the repository state this session starts from
    Baseline {
        /// Project the git reads are anchored to (hook cwd may differ — #780)
        #[arg(long = "project-dir")]
        project_dir: Option<String>,
    },
    /// Stop: suggest `/simplify` when this session changed code
    #[command(name = "simplify-check")]
    SimplifyCheck {
        /// Project the git reads are anchored to (hook cwd may differ — #780)
        #[arg(long = "project-dir")]
        project_dir: Option<String>,
    },
    /// Stop: block when a branch with an open PR has unpushed commits
    #[command(name = "push-check")]
    PushCheck {
        /// Project the git reads are anchored to (hook cwd may differ — #780)
        #[arg(long = "project-dir")]
        project_dir: Option<String>,
    },
}

/// Directory holding one JSON file per session.
fn state_dir() -> std::path::PathBuf {
    // `temp_dir()` is `$TMPDIR` (falling back to `/tmp`) on unix.
    std::env::temp_dir().join("atelier-sessions")
}

/// Resolves the project anchor: the explicit flag first, then the payload cwd,
/// then the process cwd. Never guesses beyond those documented fallbacks.
fn resolve_project_dir(flag: Option<String>, payload: &SessionPayload) -> String {
    default_project_dir(
        flag.filter(|d| !d.is_empty())
            .or_else(|| payload.cwd.clone().filter(|d| !d.is_empty())),
    )
}

/// The simplify banner's only stdout write. #725 (moving hook output to
/// `hookSpecificOutput.additionalContext`) has exactly this one site to change.
fn emit(decision: &SimplifyDecision) {
    if let SimplifyDecision::Notify { files, total } = decision {
        print!("{}", render_banner(files, *total));
    }
}

/// The push check's only stdout write: the Stop hook's block document, or
/// nothing at all. Exit stays 0 either way — the JSON *is* the block signal.
fn emit_push_check(decision: &PushCheckDecision) {
    if let Some(json) = render_block_json(decision) {
        println!("{json}");
    }
}

/// Parses `argv` (including the leading program name) with the session clap
/// surface and runs the selected command. Always returns 0.
pub fn run_from<I, T>(argv: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    run(Cli::parse_from(argv))
}

/// Binds the real store and repository reader to the resolved project, then
/// hands the command its dependencies and the payload's session id.
fn with_deps(
    project_dir: Option<String>,
    payload: &SessionPayload,
    command: impl FnOnce(&SessionDeps, &str),
) {
    let repo = create_repo_reader(resolve_project_dir(project_dir, payload));
    let store = FsBaselineStore::new(state_dir(), DEFAULT_TTL);
    let deps = SessionDeps {
        store: &store,
        repo: &repo,
    };
    command(&deps, payload.session_id.as_deref().unwrap_or_default());
}

/// Runs a parsed session CLI. Always returns 0: a Stop hook's exit 2 means
/// "block on stderr", so any non-zero return here would turn a crash — or an
/// unknown subcommand from an older binary — into a session that cannot end.
/// `push-check`'s block travels in the stdout document instead.
pub fn run(cli: Cli) -> i32 {
    let command = match cli.command {
        Some(c) => c,
        None => {
            use clap::CommandFactory;
            let _ = Cli::command().print_help();
            println!();
            return 0;
        }
    };

    let payload = SessionPayload::parse(&read_stdin_raw());
    match command {
        Commands::Baseline { project_dir } => with_deps(project_dir, &payload, |deps, id| {
            commands::baseline::run(deps, id);
        }),
        Commands::SimplifyCheck { project_dir } => with_deps(project_dir, &payload, |deps, id| {
            emit(&commands::simplify::run(deps, id));
        }),
        Commands::PushCheck { project_dir } => {
            // Both services pin their reads to the project (#780) — a Stop
            // hook's process cwd can be a worktree or a subagent's directory.
            let dir = resolve_project_dir(project_dir, &payload);
            let git = create_git_service(Some(dir.clone()));
            let github = create_github_service(Some(dir));
            let deps = PushCheckDeps {
                git: &git,
                open_pr: &github,
            };
            emit_push_check(&commands::push_check::run(&deps, payload.stop_hook_active));
        }
    }
    0
}
