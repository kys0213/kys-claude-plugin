//! Orchestrator subsystem — deterministic dispatch-rule checks that the
//! Claude Code function hooks module (`plugins/atelier/hooks/register.ts`)
//! calls as an adapter: it moves `agent.spawn`/`tool.call`/`session.compact`
//! event facts into these commands' stdin and moves the JSON result back
//! into the event's `context`/`instructions`.
//!
//! ```text
//! atelier orchestrator spawn-check     # agent.spawn + tool.call{Agent}
//! atelier orchestrator compact-note    # session.compact
//! ```
//!
//! Output contract, same as `session` (`crate::session` module docs): stdin
//! JSON in, one line of stdout JSON out, and **always exit 0** — including
//! unparseable stdin and clap parse failures — because a hooks-module caller
//! treats a nonzero exit or malformed stdout as "drop this adapter's
//! contribution", never as a signal worth surfacing to the user.

pub mod commands;
pub mod core;

use crate::orchestrator::commands::compact_note;
use crate::orchestrator::commands::payload::parse_spawn_facts;
use crate::orchestrator::core::spawn_check::check;
use crate::shared::process::read_stdin_raw;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "orchestrator",
    version,
    about = "Orchestrator dispatch-rule checks for function hooks (spawn-check / compact-note)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// agent.spawn / tool.call{Agent}: warn on model-unspecified or
    /// tier-cap-exceeded dispatch (orchestrator 불변식 21)
    #[command(name = "spawn-check")]
    SpawnCheck,
    /// session.compact (main loop only): instruct the summarizer to preserve
    /// orchestrator run state
    #[command(name = "compact-note")]
    CompactNote,
}

/// Parses `argv` (including the leading program name) with the orchestrator
/// clap surface and runs the selected command. Always returns 0 — a parse
/// failure prints clap's own message and still exits 0, matching
/// `session::run_from`'s guarantee that no argv this binary receives can
/// signal failure through the exit code.
pub fn run_from<I, T>(argv: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match Cli::try_parse_from(argv) {
        Ok(cli) => run(cli),
        // Covers `--help` and `--version`, which clap also reports as
        // errors; `print` routes each to the stream clap picked for it.
        Err(e) => {
            let _ = e.print();
            0
        }
    }
}

/// The spawn-check command's only stdout write: one line of `{"warnings":
/// [...]}`, even when empty.
fn emit_spawn_check(warnings: &[String]) {
    println!("{}", serde_json::json!({ "warnings": warnings }));
}

/// The compact-note command's only stdout write: one line of
/// `{"instructions": "..."}`.
fn emit_compact_note(instructions: &str) {
    println!("{}", serde_json::json!({ "instructions": instructions }));
}

/// Runs a parsed orchestrator CLI. Always returns 0 — same reasoning as
/// `session::run`: this binary is called from a hooks-module adapter that
/// must never see this process fail the run it is instrumenting.
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
        Commands::SpawnCheck => {
            let warnings = parse_spawn_facts(&read_stdin_raw())
                .map(|facts| check(&facts))
                .unwrap_or_default();
            emit_spawn_check(&warnings);
        }
        Commands::CompactNote => {
            emit_compact_note(&compact_note::instructions());
        }
    }
    0
}
