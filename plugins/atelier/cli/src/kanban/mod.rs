//! Kanban subsystem — turns the session facts the hooks module keeps in
//! `$.store` into the board its pane draws, and carries requests between
//! sessions through their inboxes.
//!
//! Every subcommand prints one line of JSON and exits 0; a refused or failed
//! request prints `{"error": ...}`.

pub mod commands;
pub mod core;

use crate::kanban::commands::inbox::{ClockIds, FsInbox, ThreadPause};
use crate::kanban::commands::ledger::{KanbanLedger, ListArgs};
use crate::kanban::commands::ledger_store::FsLedger;
use crate::kanban::commands::paths::FsPaths;
use crate::kanban::commands::payload::{
    parse_answer_request, parse_bash_command, parse_pane_nonces, parse_sessions, parse_waits,
};
use crate::kanban::commands::records::{FsRecords, KanbanRecords, StatusQuery};
use crate::kanban::commands::requests::{Answerer, KanbanRequests};
use crate::kanban::commands::spawn::{ClaudeLauncher, GitProbe};
use crate::kanban::core::answer::compose;
use crate::kanban::core::bash_guard::refusal;
use crate::kanban::core::board::{build, Action, Board, BoardParams, Card, Column, Waiting};
use crate::kanban::core::detail::Detail;
use crate::kanban::core::duration::parse_millis;
use crate::kanban::core::layout::{fit, Layout};
use crate::kanban::core::question::{Ask, Question};
use crate::kanban::core::repos::{group, Repo};
use crate::kanban::core::request::Sender;
use crate::kanban::core::status::{render_text, select, Format};
use crate::kanban::core::text::defuse_marker;
use crate::kanban::core::wait::{derive, Blocked, Place, Subject};
use crate::shared::process::read_stdin_raw;
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

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
        /// Keeps only this column (repeatable): needs_input, working, idle, stale
        #[arg(long = "column")]
        columns: Vec<String>,
    },
    /// The board of the sessions' record copies, read without the pane
    Status {
        /// Keeps only this column (repeatable): needs_input, working, idle, stale
        #[arg(long = "column")]
        columns: Vec<String>,
        /// Lists only this session
        #[arg(long)]
        session: Option<String>,
        /// Current time, unix milliseconds; the system clock when absent
        #[arg(long)]
        now: Option<u64>,
        /// `json` or `text`
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Where a session's record copy is
    RecordPath {
        #[arg(long)]
        session: String,
    },
    /// Removes a session's record copy and inbox
    Forget {
        #[arg(long)]
        session: String,
    },
    /// An open question and the picks made on the board → one answer per question
    Answer,
    /// The Bash tool's input on stdin → why a model may not run it, if it may not
    GuardBash,
    /// The decisions taken for this machine's waits
    Ledger {
        #[command(subcommand)]
        command: LedgerCommand,
    },
    #[command(flatten)]
    Request(RequestCommand),
}

#[derive(Subcommand)]
pub enum LedgerCommand {
    /// What a session saw of one answer it took, on stdin → its entries
    Record,
    /// A permission request's tool and the calls in flight, on stdin → the call it belongs to
    Bind,
    /// Entries, newest first
    List(Box<ListArgs>),
    /// Drops entries older than 180 days, then the oldest past 50 MB
    Prune {
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
    },
}

#[derive(Subcommand)]
pub enum RequestCommand {
    /// Leaves a prompt in a session's inbox
    Send {
        #[arg(long)]
        session: String,
        /// `pane` for a person on the board, `agent` for a model
        #[arg(long, value_parser = parse_sender)]
        from: Sender,
        /// Names the sending session in an agent prompt's header
        #[arg(long)]
        from_label: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        text: String,
    },
    /// Leaves the answer to one permission request for the session that asked
    Permit {
        #[arg(long)]
        session: String,
        /// The permission request's id, as the board's card names it
        #[arg(long)]
        request: String,
        /// `allow` or `deny`
        #[arg(long)]
        decision: String,
        /// `pane` for a person on the board or a terminal, `agent` for a model
        #[arg(long, value_parser = parse_sender)]
        from: Sender,
        /// The pane's one-time token, carried to the answering session as given
        #[arg(long)]
        nonce: Option<String>,
    },
    /// Leaves the answers to one open question for the session that asked
    Reply {
        #[arg(long)]
        session: String,
        /// The question's tool_use_id, as the board's wait names it
        #[arg(long)]
        ask: String,
        /// One per question, in the questions' order
        #[arg(long = "answer", allow_hyphen_values = true)]
        answers: Vec<String>,
        /// `pane` for a person on the board or a terminal, `agent` for a model
        #[arg(long, value_parser = parse_sender)]
        from: Sender,
        /// The pane's one-time token, carried to the answering session as given
        #[arg(long)]
        nonce: Option<String>,
    },
    /// Leaves the reply to the decision a session's last turn ended on
    Decide {
        #[arg(long)]
        session: String,
        /// The decision's id, as the board's wait names it
        #[arg(long)]
        at: String,
        #[arg(long, allow_hyphen_values = true)]
        text: String,
        /// `pane` for a person on the board or a terminal, `agent` for a model
        #[arg(long, value_parser = parse_sender)]
        from: Sender,
        /// The pane's one-time token, carried to the answering session as given
        #[arg(long)]
        nonce: Option<String>,
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
        /// Reads the open questions, the decision and the nonces the pane
        /// left on stdin; without it stdin is left unread, the session waits
        /// on none and every answer comes through the terminal
        #[arg(long)]
        waits_stdin: bool,
    },
    /// Waits a little for the answer to one permission request
    WaitPermit {
        #[arg(long)]
        session: String,
        #[arg(long)]
        request: String,
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
        /// Age after which an answer is dropped (e.g. 60s)
        #[arg(long, value_parser = parse_millis)]
        ttl: u64,
        /// How long to wait for it (e.g. 2s)
        #[arg(long = "for", value_parser = parse_millis)]
        wait: u64,
        /// Reads the nonces the pane left on stdin; without it stdin is left
        /// unread and the answer comes through the terminal
        #[arg(long)]
        nonces_stdin: bool,
    },
    /// Removes the inbox and what is left in it
    Clear {
        #[arg(long)]
        session: String,
    },
}

fn parse_sender(raw: &str) -> Result<Sender, String> {
    Sender::parse(raw).ok_or_else(|| format!("pane 또는 agent 예요 (받은 값: {raw})"))
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
        "permission_id": card.permission_id,
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
            "id": d.at.to_string(),
            "ask": d.ask,
            "recap": d.recap,
            "options": d.options,
        })),
        "delivery": detail.delivery,
    })
}

fn repo_json(repo: &Repo) -> Value {
    let sessions: Vec<Value> = repo
        .sessions
        .iter()
        .map(|s| {
            let place = Place {
                repo_root: repo.root.as_deref(),
                cwd: &s.cwd,
                permission_mode: s.permission_mode.as_deref(),
            };
            json!({
                "session_id": s.session_id,
                "column": s.column.id(),
                "cwd": s.cwd,
                "permission_mode": s.permission_mode,
                "actions": action_ids(&s.actions),
                "subtitle": s.subtitle,
                "last_line": s.last_line,
                "delivery": s.delivery,
                "wait": wait_json(s.waiting.as_ref(), &place),
            })
        })
        .collect();
    defused(json!({
        "label": repo.label,
        "root": repo.root,
        "sessions": sessions,
    }))
}

fn wait_json(waiting: Option<&Waiting>, place: &Place) -> Value {
    let Some(waiting) = waiting else {
        return Value::Null;
    };
    let wait = derive(waiting, place, &FsPaths);
    let mut out = json!({
        "kind": wait.kind(),
        "id": wait.id,
        "since": wait.since,
        "request": wait.request,
        "context": wait.context,
    });
    match &wait.subject {
        Subject::Question(questions) => {
            out["questions"] = questions
                .iter()
                .map(|q| {
                    let mut shown = question_json(&q.question);
                    shown["key"] = json!(q.key);
                    shown["key_label"] = json!(q.key_label);
                    shown
                })
                .collect();
        }
        Subject::Permission(p) => {
            out["tool"] = json!(p.tool);
            out["target"] = json!(p.target);
            out["truncated"] = json!(p.truncated);
            out["key"] = json!(p.key);
            out["key_label"] = json!(p.key_label);
            out["read_only"] = json!(p.read_only);
        }
        Subject::Decision(d) => {
            out["ask"] = json!(d.ask);
            out["recap"] = json!(d.recap);
            out["options"] = json!(d.options);
            out["key"] = json!(d.key);
        }
        Subject::Input => {}
    }
    out["proxyable"] = json!(wait.proxyable());
    out["blocked"] = json!(wait.blocked.iter().map(Blocked::id).collect::<Vec<_>>());
    out
}

/// Every string with its wait marker defused; what the delegate reads of a
/// worker's words cannot pass for a marker.
fn defused(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(defuse_marker(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(defused).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(name, value)| (name, defused(value)))
                .collect(),
        ),
        other => other,
    }
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
                "heading": column.heading(cards.len()),
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

fn with_records<T>(run: impl FnOnce(&KanbanRecords) -> T) -> T {
    let store = FsRecords::from_env();
    let inbox = FsInbox::from_env();
    run(&KanbanRecords {
        store: &store,
        inbox: &inbox,
    })
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn status_entry_json(column: Column, card: &Card) -> Value {
    let place = Place {
        repo_root: card.repo_root.as_deref(),
        cwd: &card.cwd,
        permission_mode: card.permission_mode.as_deref(),
    };
    defused(json!({
        "session_id": card.session_id,
        "label": card.label,
        "repo_root": card.repo_root,
        "cwd": card.cwd,
        "column": column.id(),
        "age": card.age,
        "subtitle": card.subtitle,
        "permission_mode": card.permission_mode,
        "actions": action_ids(&card.actions),
        "last_line": card.last_line,
        "delivery": card.detail.delivery,
        "wait": wait_json(card.waiting.as_ref(), &place),
    }))
}

fn run_status(
    columns: Vec<String>,
    session: Option<String>,
    now: Option<u64>,
    format: &str,
) -> String {
    let now = now.unwrap_or_else(now_millis);
    let query = StatusQuery {
        columns,
        session,
        now,
    };
    let shown = Format::parse(format)
        .and_then(|format| with_records(|r| r.status(&query)).map(|board| (format, board)));
    match shown {
        Ok((Format::Json, board)) => {
            let sessions: Vec<Value> = select(&board, query.session.as_deref())
                .into_iter()
                .flat_map(|(column, cards)| cards.into_iter().map(move |card| (column, card)))
                .map(|(column, card)| status_entry_json(column, card))
                .collect();
            json!({ "now": now, "sessions": sessions }).to_string()
        }
        Ok((Format::Text, board)) => defuse_marker(&render_text(&board, query.session.as_deref())),
        Err(error) => json!({ "error": error }).to_string(),
    }
}

fn run_ledger(command: LedgerCommand) -> Value {
    let store = FsLedger::from_env();
    let ledger = KanbanLedger {
        store: &store,
        paths: &FsPaths,
    };
    match command {
        LedgerCommand::Record => ledger.record(&read_stdin_raw()),
        LedgerCommand::Bind => KanbanLedger::bind(&read_stdin_raw()),
        LedgerCommand::List(args) => ledger.list(*args),
        LedgerCommand::Prune { now } => ledger.prune(now),
    }
}

fn run_request(command: RequestCommand) -> Value {
    let inbox = FsInbox::from_env();
    let requests = KanbanRequests {
        inbox: &inbox,
        pause: &ThreadPause,
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
        } => requests.send(&session, from, from_label.as_deref(), &text),
        RequestCommand::Stop { session, turn } => requests.stop(&session, &turn),
        RequestCommand::Permit {
            session,
            request,
            decision,
            from,
            nonce,
        } => requests.permit(
            &session,
            &request,
            &decision,
            Answerer {
                from,
                nonce: nonce.as_deref(),
            },
        ),
        RequestCommand::Reply {
            session,
            ask,
            answers,
            from,
            nonce,
        } => requests.reply(
            &session,
            &ask,
            &answers,
            Answerer {
                from,
                nonce: nonce.as_deref(),
            },
        ),
        RequestCommand::Decide {
            session,
            at,
            text,
            from,
            nonce,
        } => requests.decide(
            &session,
            &at,
            &text,
            Answerer {
                from,
                nonce: nonce.as_deref(),
            },
        ),
        RequestCommand::Inbox { command } => match command {
            InboxCommand::Dir { session } => requests.dir(&session),
            InboxCommand::Take {
                session,
                now,
                ttl,
                turn,
                waits_stdin,
            } => {
                let raw = waits_stdin.then(read_stdin_raw).unwrap_or_default();
                match parse_waits(&raw).and_then(|waits| Ok((waits, parse_pane_nonces(&raw)?))) {
                    Ok((waits, nonces)) => {
                        requests.take(&session, now, ttl, turn.as_deref(), &waits, nonces)
                    }
                    Err(error) => json!({ "error": error }),
                }
            }
            InboxCommand::WaitPermit {
                session,
                request,
                now,
                ttl,
                wait,
                nonces_stdin,
            } => match parse_pane_nonces(&nonces_stdin.then(read_stdin_raw).unwrap_or_default()) {
                Ok(nonces) => requests.wait_permit(&session, &request, now, ttl, wait, nonces),
                Err(error) => json!({ "error": error }),
            },
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
            columns,
        } => {
            let params = BoardParams {
                now,
                stale_after,
                expire_after,
                self_id,
            };
            let out = match Column::parse_all(&columns)
                .and_then(|only| Ok((only, parse_sessions(&read_stdin_raw())?)))
            {
                Ok((only, sessions)) => {
                    let board = build(&sessions, &params).only(&only);
                    let layout = fit(width, board.columns.len() as u32);
                    board_json(&board, &layout)
                }
                Err(error) => json!({ "error": error }),
            };
            println!("{out}");
        }
        Commands::Status {
            columns,
            session,
            now,
            format,
        } => println!("{}", run_status(columns, session, now, &format)),
        Commands::RecordPath { session } => println!("{}", with_records(|r| r.path(&session))),
        Commands::Forget { session } => println!("{}", with_records(|r| r.forget(&session))),
        Commands::Answer => {
            let out = match parse_answer_request(&read_stdin_raw())
                .and_then(|(questions, picks)| compose(&questions, &picks))
            {
                Ok(answers) => json!({ "answers": answers }),
                Err(error) => json!({ "error": error }),
            };
            println!("{out}");
        }
        Commands::GuardBash => {
            let out = match parse_bash_command(&read_stdin_raw()) {
                Ok(command) => json!({ "deny": refusal(&command) }),
                Err(error) => json!({ "error": error }),
            };
            println!("{out}");
        }
        Commands::Ledger { command } => println!("{}", run_ledger(command)),
        Commands::Request(command) => println!("{}", run_request(command)),
    }
    0
}
