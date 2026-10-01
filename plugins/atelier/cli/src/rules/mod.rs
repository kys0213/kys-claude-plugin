//! Rules subsystem — which `.claude/rules` documents apply to a change set.
//!
//! ```text
//! atelier rules match --base <ref> [--head <ref>] --rules-dir <dir>...
//! ```
//!
//! Output contract: one line of JSON on stdout and exit 0, or the reason on
//! stderr and exit 2. Unlike the hook subsystems, a caller here wants a
//! failure to stop it — a review scoped from a half-read rules dir would
//! silently skip conventions.

pub mod commands;
pub mod core;

use crate::rules::commands::matching::MatchCommand;
use crate::rules::core::source::{FsRuleFiles, GitChangedFiles};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "rules",
    version,
    about = "Which .claude/rules documents apply to a change set"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List the files changed since the merge base of <base> and <head>
    /// (deletions excluded) with the rules whose `paths:` match each one
    Match {
        /// Ref the change set is measured from, e.g. the epic branch
        #[arg(long)]
        base: String,
        /// Ref holding the changes
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// Directory scanned recursively for `*.md` rules; repeatable
        #[arg(long = "rules-dir", required = true)]
        rules_dir: Vec<String>,
    },
}

pub fn run_from<I, T>(argv: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match Cli::try_parse_from(argv) {
        Ok(cli) => run(cli),
        Err(e) => {
            let _ = e.print();
            e.exit_code()
        }
    }
}

pub fn run(cli: Cli) -> i32 {
    match cli.command {
        Commands::Match {
            base,
            head,
            rules_dir,
        } => {
            match MatchCommand::new(&GitChangedFiles, &FsRuleFiles).run(&base, &head, &rules_dir) {
                Ok(report) => {
                    println!("{}", serde_json::to_string(&report).unwrap_or_default());
                    0
                }
                Err(e) => {
                    eprintln!("atelier rules match: {e}");
                    2
                }
            }
        }
    }
}
