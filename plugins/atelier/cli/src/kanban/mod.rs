//! Kanban subsystem — turns the session facts the hooks module keeps in
//! `$.store` into the board its pane draws.
//!
//! ```text
//! atelier kanban board --now <unix-ms> --stale-after <dur> --expire-after <dur> [--self <id>]
//! ```
//!
//! Output contract: stdin JSON in, one line of stdout JSON out, and **always
//! exit 0**. Unreadable stdin prints `{"error": ...}`; an argument error
//! prints clap's message to stderr and nothing to stdout, so the caller's
//! JSON parse fails and it shows its own fallback.

pub mod commands;
pub mod core;

use crate::kanban::commands::payload::parse_sessions;
use crate::kanban::core::board::{build, Board, BoardParams, Card};
use crate::kanban::core::duration::parse_millis;
use crate::shared::process::read_stdin_raw;
use clap::{Parser, Subcommand};
use serde_json::{json, Value};

#[derive(Parser)]
#[command(
    name = "kanban",
    version,
    about = "Session kanban board for the hooks module's pane"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Session facts on stdin → columns of cards and the ids to drop
    Board {
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
        /// Heartbeat silence after which a session is shown as stale (e.g. 2m)
        #[arg(long, value_parser = parse_millis)]
        stale_after: u64,
        /// Heartbeat silence after which a session is dropped (e.g. 24h)
        #[arg(long, value_parser = parse_millis)]
        expire_after: u64,
        /// The calling session's id, marked `is_self`
        #[arg(long = "self")]
        self_id: Option<String>,
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
            0
        }
    }
}

fn card_json(card: &Card) -> Value {
    json!({
        "session_id": card.session_id,
        "label": card.label,
        "subtitle": card.subtitle,
        "reason": card.reason,
        "age": card.age,
        "is_self": card.is_self,
    })
}

fn board_json(board: &Board) -> Value {
    let columns: Vec<Value> = board
        .columns
        .iter()
        .map(|(column, cards)| {
            json!({
                "id": column.id(),
                "title": column.title(),
                "cards": cards.iter().map(card_json).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "columns": columns, "expired": board.expired })
}

pub fn run(cli: Cli) -> i32 {
    let Some(command) = cli.command else {
        use clap::CommandFactory;
        let _ = Cli::command().print_help();
        println!();
        return 0;
    };

    match command {
        Commands::Board {
            now,
            stale_after,
            expire_after,
            self_id,
        } => {
            let params = BoardParams {
                now,
                stale_after,
                expire_after,
                self_id,
            };
            let out = match parse_sessions(&read_stdin_raw()) {
                Ok(sessions) => board_json(&build(&sessions, &params)),
                Err(error) => json!({ "error": error }),
            };
            println!("{out}");
        }
    }
    0
}
