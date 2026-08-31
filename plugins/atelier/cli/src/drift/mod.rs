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
use crate::shared::process::default_project_dir;
use clap::{Args, Parser, Subcommand};

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
        #[command(flatten)]
        args: PathArgs,
    },
    /// Update one installed copy from its plugin source (never installs)
    Sync {
        /// Which installed copy to update
        #[arg(long = "target", value_enum)]
        target: SyncTarget,
        #[command(flatten)]
        args: PathArgs,
    },
}

/// The flag set every drift command shares.
#[derive(Args)]
pub struct PathArgs {
    /// Plugin root holding the source template and rules files
    #[arg(long = "plugin-root")]
    plugin_root: String,
    /// User CLAUDE.md path (default: $HOME/.claude/CLAUDE.md)
    #[arg(long = "claude-md")]
    claude_md: Option<String>,
    /// Project root the rules copy lives under (default: the process cwd)
    #[arg(long = "project-dir")]
    project_dir: Option<String>,
    /// Directory the user-scope rules copies live in
    /// (default: $HOME/.claude/rules/atelier)
    #[arg(long = "user-rules-dir")]
    user_rules_dir: Option<String>,
}

impl PathArgs {
    /// Resolves the flags into concrete paths — the only place defaults (and
    /// therefore the environment) are consulted; the commands take resolved
    /// paths. HOME is only required for the paths actually left to default.
    fn resolve(self) -> Result<DriftPaths, String> {
        fn home_based(explicit: Option<String>, flag: &str, rel: &str) -> Result<String, String> {
            match explicit {
                Some(path) => Ok(path),
                None => {
                    let home = std::env::var("HOME")
                        .map_err(|_| format!("HOME is not set — pass {flag} explicitly"))?;
                    Ok(format!("{home}/{rel}"))
                }
            }
        }
        Ok(DriftPaths {
            plugin_root: self.plugin_root,
            claude_md: home_based(self.claude_md, "--claude-md", ".claude/CLAUDE.md")?,
            project_dir: default_project_dir(self.project_dir),
            user_rules_dir: home_based(
                self.user_rules_dir,
                "--user-rules-dir",
                ".claude/rules/atelier",
            )?,
        })
    }
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

    let fs = create_artifact_fs();
    let clock = create_backup_clock();
    let deps = DriftDeps {
        fs: &fs,
        clock: &clock,
    };
    // Both arms share the resolve → run → render pipeline; only the rendered
    // text and the exit code differ (check's 0/1 split lives on the report).
    let (rendered, code) = match command {
        Commands::Check { args } => {
            match args
                .resolve()
                .and_then(|paths| commands::check::run(&deps, &paths))
            {
                Ok(report) => (report.render(), report.exit_code()),
                Err(e) => return fail(&e),
            }
        }
        Commands::Sync { target, args } => {
            match args
                .resolve()
                .and_then(|paths| commands::sync::run(&deps, &paths, target))
            {
                Ok(reports) => (reports.iter().map(|r| r.render()).collect(), 0),
                Err(e) => return fail(&e),
            }
        }
    };
    print!("{rendered}");
    code
}
