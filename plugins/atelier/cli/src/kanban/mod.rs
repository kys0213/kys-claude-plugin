//! Kanban subsystem — turns the session facts the hooks module keeps in
//! `$.store` into the board its pane draws, and carries requests between
//! sessions through their inboxes.
//!
//! Every subcommand prints one line of JSON and exits 0; a refused or failed
//! request prints `{"error": ...}`.

pub mod commands;
pub mod core;

use crate::kanban::commands::inbox::{ClockIds, FsInbox};
use crate::kanban::commands::payload::{parse_answer_request, parse_sessions};
use crate::kanban::commands::requests::KanbanRequests;
use crate::kanban::commands::spawn::{ClaudeLauncher, GitProbe};
use crate::kanban::core::answer::compose;
use crate::kanban::core::board::{build, Action, Board, BoardParams, Card};
use crate::kanban::core::detail::Detail;
use crate::kanban::core::duration::parse_millis;
use crate::kanban::core::layout::{fit, Layout};
use crate::kanban::core::question::{Ask, Question};
use crate::kanban::core::repos::{group, Repo};
use crate::kanban::core::request::Sender;
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
        /// The pane's width in columns, which decides the board's layout
        #[arg(long)]
        width: u32,
        /// The calling session's id, marked `is_self`
        #[arg(long = "self")]
        self_id: Option<String>,
    },
    /// An open question and the picks made on the board → one answer per question
    Answer,
    #[command(flatten)]
    Request(RequestCommand),
}

#[derive(Subcommand)]
pub enum RequestCommand {
    /// Leaves a prompt in a session's inbox
    Send {
        #[arg(long)]
        session: String,
        /// `pane` for a person on the board, `agent` for a model
        #[arg(long)]
        from: String,
        /// Names the sending session in an agent prompt's header
        #[arg(long)]
        from_label: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        text: String,
    },
    /// Leaves a stop for one turn in a session's inbox
    Stop {
        #[arg(long)]
        session: String,
        #[arg(long)]
        turn: String,
    },
    /// A session's own inbox
    Inbox {
        #[command(subcommand)]
        command: InboxCommand,
    },
    /// Starts a background session at a repository's main working tree
    Spawn {
        #[arg(long)]
        cwd: String,
        #[arg(long, allow_hyphen_values = true)]
        text: String,
        /// Runs the session in a new git worktree of this name
        #[arg(long, allow_hyphen_values = true)]
        worktree: Option<String>,
        #[arg(long)]
        from_label: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum InboxCommand {
    /// Where the inbox is
    Dir {
        #[arg(long)]
        session: String,
    },
    /// Claims every request and says which to act on
    Take {
        #[arg(long)]
        session: String,
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
        /// Age after which a request is dropped (e.g. 60s)
        #[arg(long, value_parser = parse_millis)]
        ttl: u64,
        /// The turn the session runs now; absent while it waits for a prompt
        #[arg(long)]
        turn: Option<String>,
    },
    /// Removes the inbox and what is left in it
    Clear {
        #[arg(long)]
        session: String,
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
        "headline": card.headline(),
        "progress": card.progress,
        "last_line": card.last_line,
        "cwd": card.cwd,
        "repo_root": card.repo_root,
        "permission_mode": card.permission_mode,
        "turn_id": card.turn_id,
        "actions": action_ids(&card.actions),
        "detail": detail_json(&card.detail),
        "asks": card.asks.iter().map(ask_json).collect::<Vec<_>>(),
    })
}

fn action_ids(actions: &[Action]) -> Vec<&'static str> {
    actions.iter().map(|a| a.id()).collect()
}

fn detail_json(detail: &Detail) -> Value {
    json!({
        "sections": detail
            .sections
            .iter()
            .map(|s| json!({ "title": s.title, "lines": s.lines }))
            .collect::<Vec<_>>(),
        "decision": detail.decision.as_ref().map(|d| json!({
            "ask": d.ask,
            "recap": d.recap,
            "options": d.options,
        })),
        "delivery": detail.delivery,
    })
}

fn repo_json(repo: &Repo) -> Value {
    json!({
        "label": repo.label,
        "root": repo.root,
        "sessions": repo
            .sessions
            .iter()
            .map(|s| json!({
                "session_id": s.session_id,
                "column": s.column.id(),
                "cwd": s.cwd,
                "permission_mode": s.permission_mode,
                "actions": action_ids(&s.actions),
                "subtitle": s.subtitle,
                "last_line": s.last_line,
                "delivery": s.delivery,
            }))
            .collect::<Vec<_>>(),
    })
}

fn ask_json(ask: &Ask) -> Value {
    json!({
        "tool_use_id": ask.tool_use_id,
        "questions": ask.questions.iter().map(question_json).collect::<Vec<_>>(),
    })
}

fn question_json(question: &Question) -> Value {
    json!({
        "question": question.question,
        "header": question.header,
        "multi_select": question.multi_select,
        "options": question
            .options
            .iter()
            .map(|o| json!({ "label": o.label, "description": o.description }))
            .collect::<Vec<_>>(),
    })
}

fn layout_json(layout: &Layout) -> Value {
    json!({
        "direction": layout.direction.id(),
        "column_width": layout.column_width,
        "column_gap": layout.column_gap,
    })
}

fn board_json(board: &Board, layout: &Layout) -> Value {
    let columns: Vec<Value> = board
        .columns
        .iter()
        .map(|(column, cards)| {
            json!({
                "id": column.id(),
                "title": column.title(),
                "heading": format!("{} {}", column.title(), cards.len()),
                "cards": cards.iter().map(card_json).collect::<Vec<_>>(),
            })
        })
        .collect();
    let repos: Vec<Value> = group(board).iter().map(repo_json).collect();
    json!({
        "columns": columns,
        "repos": repos,
        "expired": board.expired,
        "layout": layout_json(layout),
    })
}

fn run_request(command: RequestCommand) -> Value {
    let inbox = FsInbox::from_env();
    let requests = KanbanRequests {
        inbox: &inbox,
        ids: &ClockIds,
        probe: &GitProbe,
        launcher: &ClaudeLauncher,
    };
    match command {
        RequestCommand::Send {
            session,
            from,
            from_label,
            text,
        } => match Sender::parse(&from) {
            Some(from) => requests.send(&session, from, from_label.as_deref(), &text),
            None => json!({ "error": format!("--from 은 pane 또는 agent 예요 (받은 값: {from})") }),
        },
        RequestCommand::Stop { session, turn } => requests.stop(&session, &turn),
        RequestCommand::Inbox { command } => match command {
            InboxCommand::Dir { session } => requests.dir(&session),
            InboxCommand::Take {
                session,
                now,
                ttl,
                turn,
            } => requests.take(&session, now, ttl, turn.as_deref()),
            InboxCommand::Clear { session } => requests.clear(&session),
        },
        RequestCommand::Spawn {
            cwd,
            text,
            worktree,
            from_label,
        } => requests.spawn(&cwd, &text, worktree.as_deref(), from_label.as_deref()),
    }
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
            width,
            self_id,
        } => {
            let params = BoardParams {
                now,
                stale_after,
                expire_after,
                self_id,
            };
            let out = match parse_sessions(&read_stdin_raw()) {
                Ok(sessions) => {
                    let board = build(&sessions, &params);
                    let layout = fit(width, board.columns.len() as u32);
                    board_json(&board, &layout)
                }
                Err(error) => json!({ "error": error }),
            };
            println!("{out}");
        }
        Commands::Answer => {
            let out = match parse_answer_request(&read_stdin_raw())
                .and_then(|(questions, picks)| compose(&questions, &picks))
            {
                Ok(answers) => json!({ "answers": answers }),
                Err(error) => json!({ "error": error }),
            };
            println!("{out}");
        }
        Commands::Request(command) => println!("{}", run_request(command)),
    }
    0
}
