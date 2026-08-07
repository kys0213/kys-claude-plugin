//! Session subsystem — hooks that need to know what *this* Claude Code session
//! did, as opposed to what was already in the tree.
//!
//! ```text
//! atelier session baseline        --project-dir <dir>   # SessionStart
//! atelier session simplify-check  --project-dir <dir>   # Stop
//! ```
//!
//! Output contract: advisory only. Both commands read the hook payload from
//! stdin, print at most a banner on stdout, and **always exit 0** — a Stop hook
//! that fails must never interrupt a session.

pub mod commands;
pub mod core;

use crate::session::commands::payload::SessionPayload;
use crate::session::commands::simplify::{render_banner, SimplifyDecision};
use crate::session::commands::SessionDeps;
use crate::session::core::baseline::{FsBaselineStore, DEFAULT_TTL};
use crate::session::core::repo::create_repo_reader;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "session",
    version,
    about = "Session-scoped hook helpers (baseline / simplify-check)"
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
}

/// Directory holding one JSON file per session.
fn state_dir() -> std::path::PathBuf {
    // `temp_dir()` is `$TMPDIR` (falling back to `/tmp`) on unix.
    std::env::temp_dir().join("atelier-sessions")
}

/// Reads stdin to a string (empty on read failure); parsing is command logic.
fn read_stdin_raw() -> String {
    use std::io::Read as _;
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
    buf
}

/// Resolves the project anchor: the explicit flag first, then the payload cwd,
/// then the process cwd. Never guesses beyond those documented fallbacks.
fn resolve_project_dir(flag: Option<String>, payload: &SessionPayload) -> String {
    flag.filter(|d| !d.is_empty())
        .or_else(|| payload.cwd.clone().filter(|d| !d.is_empty()))
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        })
}

/// The subsystem's only stdout write. #725 (moving hook output to
/// `hookSpecificOutput.additionalContext`) has exactly this one site to change.
fn emit(decision: &SimplifyDecision) {
    if let SimplifyDecision::Notify { files, total } = decision {
        print!("{}", render_banner(files, *total));
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

/// Runs a parsed session CLI. Always returns 0 — these hooks are non-blocking.
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
    }
    0
}
