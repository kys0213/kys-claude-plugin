//! Drift subsystem: judges whether the setup-copied artifacts still match the
//! plugin sources, and re-syncs them on request.
//!
//! Errors exit 2, the same code `git guard` uses to deny a hook. Drift commands
//! are never registered as Claude Code hooks, so 2 cannot be misread as
//! "block" here.
//!
//! Output is human text, not JSON, and callers parse it line by line — every
//! rendered line format is a contract.

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
    /// Update one installed copy from its plugin source (installs only manifest
    /// files missing from an already installed policy scope)
    Sync {
        /// Which installed copy to update
        #[arg(long = "target", value_enum)]
        target: SyncTarget,
        /// user-rules / project-rules only: sync this one manifest file
        /// instead of all of them
        #[arg(long = "name")]
        name: Option<String>,
        #[command(flatten)]
        args: PathArgs,
    },
}

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

fn fail(message: &str) -> i32 {
    eprintln!("Error: {message}");
    2
}

/// `argv` includes the leading program name.
pub fn run_from<I, T>(argv: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    run(Cli::parse_from(argv))
}

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
        Commands::Sync { target, name, args } => {
            match args
                .resolve()
                .and_then(|paths| commands::sync::run(&deps, &paths, target, name.as_deref()))
            {
                Ok(reports) => (reports.iter().map(|r| r.render()).collect(), 0),
                Err(e) => return fail(&e),
            }
        }
    };
    print!("{rendered}");
    code
}
