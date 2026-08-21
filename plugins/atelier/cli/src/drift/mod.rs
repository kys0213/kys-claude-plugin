//! Drift subsystem — Rust port of the `check-drift.sh` / `sync-artifact.sh`
//! scripts: judges whether the artifacts `/atelier:setup` copied (the
//! CLAUDE.md coding-style block, the project rules copy) still match the
//! plugin sources, and deterministically re-syncs one of them on request.
//!
//! ```text
//! atelier drift check --plugin-root <dir> [--claude-md <path>] [--project-dir <dir>]
//! atelier drift sync  --target <claude-md|rules> --plugin-root <dir> [...]
//! ```
//!
//! Exit-code contract (preserved from the shell scripts):
//! - `check`: 0 no drift (OK / NOT_INSTALLED only), 1 drift found, 2 usage or
//!   plugin-source error. The 0/1 split lives on `CheckReport::exit_code`.
//! - `sync`: 0 synced, 2 usage error / target not installed or corrupted /
//!   plugin source missing.
//!
//! Deliberate decision: exit 2 is reused from the shell contract even though
//! `git guard` reserves 2 for its hook-deny signal — drift commands are never
//! registered as Claude Code hooks (they are invoked from the `/atelier:update`
//! and `/atelier:setup` command specs), so 2 cannot be misread as "block" here
//! (see `git/mod.rs` on why the guard installer avoids it).
//!
//! Output is human text, not JSON: the consumer spec (`commands/update.md`)
//! branches on the `<check>=<STATUS>` line format and relays the `synced:`
//! line (backup path included) verbatim to the user.

pub mod commands;
pub mod core;

use crate::drift::commands::DriftDeps;
use crate::drift::core::artifact::{create_artifact_fs, create_backup_clock};
use crate::drift::core::types::{DriftPaths, SyncTarget};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "drift",
    version,
    about = "Judge and re-sync setup-copied artifacts against the plugin sources"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Report per-artifact drift status (read-only)
    Check {
        /// Plugin root holding the source template and rules files
        #[arg(long = "plugin-root")]
        plugin_root: String,
        /// User CLAUDE.md path (default: $HOME/.claude/CLAUDE.md)
        #[arg(long = "claude-md")]
        claude_md: Option<String>,
        /// Project root the rules copy lives under (default: ".")
        #[arg(long = "project-dir")]
        project_dir: Option<String>,
    },
    /// Update one installed copy from its plugin source (never installs)
    Sync {
        /// Which installed copy to update
        #[arg(long = "target", value_enum)]
        target: SyncTarget,
        /// Plugin root holding the source template and rules files
        #[arg(long = "plugin-root")]
        plugin_root: String,
        /// User CLAUDE.md path (default: $HOME/.claude/CLAUDE.md)
        #[arg(long = "claude-md")]
        claude_md: Option<String>,
        /// Project root the rules copy lives under (default: ".")
        #[arg(long = "project-dir")]
        project_dir: Option<String>,
    },
}

/// Resolves the flag trio into concrete paths — the only place defaults (and
/// therefore the environment) are consulted; the commands take resolved paths.
fn resolve_paths(
    plugin_root: String,
    claude_md: Option<String>,
    project_dir: Option<String>,
) -> Result<DriftPaths, String> {
    let claude_md = match claude_md {
        Some(path) => path,
        None => {
            let home = std::env::var("HOME")
                .map_err(|_| "HOME is not set — pass --claude-md explicitly".to_string())?;
            format!("{home}/.claude/CLAUDE.md")
        }
    };
    Ok(DriftPaths {
        plugin_root,
        claude_md,
        project_dir: project_dir.unwrap_or_else(|| ".".to_string()),
    })
}

/// The error edge: every failure is `Error: <message>` on stderr with exit 2,
/// the shell scripts' usage/refusal contract.
fn fail(message: &str) -> i32 {
    eprintln!("Error: {message}");
    2
}

/// Parses `argv` (including the leading program name) with the drift clap
/// surface and runs the selected command, returning a process exit code.
pub fn run_from<I, T>(argv: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    run(Cli::parse_from(argv))
}

/// Binds the real filesystem and clock, then hands the command its deps.
fn with_deps(command: impl FnOnce(&DriftDeps) -> i32) -> i32 {
    let fs = create_artifact_fs();
    let clock = create_backup_clock();
    let deps = DriftDeps {
        fs: &fs,
        clock: &clock,
    };
    command(&deps)
}

/// Runs a parsed drift CLI, returning a process exit code. The subsystem's
/// only stdout render site — commands return values, never print.
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

    match command {
        Commands::Check {
            plugin_root,
            claude_md,
            project_dir,
        } => {
            let paths = match resolve_paths(plugin_root, claude_md, project_dir) {
                Ok(paths) => paths,
                Err(e) => return fail(&e),
            };
            with_deps(|deps| match commands::check::run(deps, &paths) {
                Ok(report) => {
                    print!("{}", report.render());
                    report.exit_code()
                }
                Err(e) => fail(&e),
            })
        }
        Commands::Sync {
            target,
            plugin_root,
            claude_md,
            project_dir,
        } => {
            let paths = match resolve_paths(plugin_root, claude_md, project_dir) {
                Ok(paths) => paths,
                Err(e) => return fail(&e),
            };
            with_deps(|deps| match commands::sync::run(deps, &paths, target) {
                Ok(report) => {
                    println!("{}", report.render());
                    0
                }
                Err(e) => fail(&e),
            })
        }
    }
}
