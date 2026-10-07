//! Session subsystem — helpers behind the plugin's session-boundary hooks.
//!
//! ```text
//! atelier session push-check      --project-dir <dir>   # Stop
//! atelier session ensure-env      --settings <file> --key <K> --value <V>  # SessionStart
//! ```
//!
//! Output contract: every command reads the hook payload from stdin, writes to
//! stdout only, and **always exits 0** — the exit code never carries a signal,
//! because a Stop hook's exit 2 means "block on stderr" and a failing binary
//! would then wedge every session end. The guarantee is this boundary's, not
//! each command's: `run_from` swallows clap's own parse failures too, so a
//! typo or a stale flag cannot block a Stop either. The shims still force
//! exit 0 themselves — a binary predating a subcommand fails inside clap,
//! before any of this code runs.
//!
//! What stdout carries differs by command: `ensure-env` prints one line only
//! when it added the key or declined to, and `push-check` may print a Stop
//! `{"decision":"block","reason":…}` document — still on exit 0, which is how
//! Claude Code reads a structured block.

pub mod commands;
pub mod core;

use crate::git::core::git::create_git_service;
use crate::git::core::github::create_github_service;
use crate::session::commands::ensure_env::EnsureEnvCommand;
use crate::session::commands::payload::SessionPayload;
use crate::session::commands::push_check::{render_block_json, PushCheckDecision, PushCheckDeps};
use crate::session::core::settings_env::FsSettingsFile;
use crate::shared::process::{default_project_dir, read_stdin_raw};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "session",
    version,
    about = "Session-scoped hook helpers (push-check / ensure-env)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Stop: block when a branch with an open PR has unpushed commits
    #[command(name = "push-check")]
    PushCheck {
        /// Project the git reads are anchored to (hook cwd may differ)
        #[arg(long = "project-dir")]
        project_dir: Option<String>,
    },
    /// SessionStart: add `env.<key>` to a settings file when it is absent
    #[command(name = "ensure-env")]
    EnsureEnv {
        /// Settings file to edit (e.g. `~/.claude/settings.json`)
        #[arg(long)]
        settings: String,
        /// Environment variable name under `env`
        #[arg(long)]
        key: String,
        /// Value written when the key is absent; an existing value is kept
        #[arg(long)]
        value: String,
    },
}

/// Resolves the project anchor: the explicit flag first, then the payload cwd,
/// then the process cwd. Never guesses beyond those documented fallbacks.
fn resolve_project_dir(flag: Option<String>, payload: &SessionPayload) -> String {
    default_project_dir(
        flag.filter(|d| !d.is_empty())
            .or_else(|| payload.cwd.clone().filter(|d| !d.is_empty())),
    )
}

/// The push check's only stdout write: the Stop hook's block document, or
/// nothing at all. Exit stays 0 either way — the JSON *is* the block signal.
fn emit_push_check(decision: &PushCheckDecision) {
    if let Some(json) = render_block_json(decision) {
        println!("{json}");
    }
}

/// Parses `argv` (including the leading program name) with the session clap
/// surface and runs the selected command. Always returns 0 — a parse failure
/// prints clap's own message and still exits 0, so no argv this binary does
/// understand can turn a Stop into a block.
pub fn run_from<I, T>(argv: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match Cli::try_parse_from(argv) {
        Ok(cli) => run(cli),
        // Covers `--help` and `--version`, which clap also reports as errors;
        // `print` routes each to the stream clap picked for it.
        Err(e) => {
            let _ = e.print();
            0
        }
    }
}

/// Runs a parsed session CLI. Always returns 0: a Stop hook's exit 2 means
/// "block on stderr", so any non-zero return here would turn a crash into a
/// session that cannot end. `push-check`'s block travels in the stdout
/// document instead.
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
        Commands::PushCheck { project_dir } => {
            // Both services pin their reads to the project — a Stop
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
        Commands::EnsureEnv {
            settings,
            key,
            value,
        } => {
            let file = FsSettingsFile::new(&settings);
            let outcome = EnsureEnvCommand::new(&file).run(&key, &value);
            if let Some(line) = commands::ensure_env::render(&outcome, &settings, &key, &value) {
                println!("{line}");
            }
        }
    }
    0
}
