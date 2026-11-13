//! Every run exits 0. A subcommand prints one line of JSON, `{"error": ...}`
//! for a refused or failed request, except `status --format text`, which
//! prints the board as plain lines. Arguments clap cannot take print clap's
//! own text instead: a usage error on stderr, help on stdout. With no
//! subcommand, help is printed on stdout.

pub mod args;
pub mod delegate;
pub mod inbox;
pub mod ledger;
pub mod ledger_store;
pub mod paths;
pub mod payload;
pub mod records;
pub mod requests;
pub mod spawn;

use self::args::{Cli, Commands, DelegateCommandArgs, InboxCommand, LedgerCommand, RequestCommand};
use crate::kanban::commands::delegate::{
    attach_json, delegate_json, status_delegate_json, KanbanDelegate, Typed, WakeClock,
};
use crate::kanban::commands::inbox::{ClockIds, FsInbox, ThreadPause};
use crate::kanban::commands::ledger::KanbanLedger;
use crate::kanban::commands::ledger_store::{read_book, FsLedger, LedgerStore};
use crate::kanban::commands::paths::FsPaths;
use crate::kanban::commands::payload::{
    parse_answer_request, parse_bash_command, parse_nonce_keys, parse_pane_nonces,
    parse_permit_wait, parse_sessions, parse_tool_questions, parse_waits,
};
use crate::kanban::commands::records::{FsRecords, KanbanRecords, StatusQuery};
use crate::kanban::commands::requests::{Answerer, KanbanRequests, PermitTiming};
use crate::kanban::commands::spawn::{ClaudeLauncher, GitProbe};
use crate::kanban::core::allowlist::RealPaths;
use crate::kanban::core::answer::compose;
use crate::kanban::core::bash_guard::refusal;
use crate::kanban::core::board::{build, Action, Board, BoardParams, Card, Column, Waiting};
use crate::kanban::core::delegate::delegate_of;
use crate::kanban::core::detail::Detail;
use crate::kanban::core::layout::{fit, Layout};
use crate::kanban::core::nonce::past_cap;
use crate::kanban::core::precedent::Book;
use crate::kanban::core::question::{Ask, Question};
use crate::kanban::core::relay::unused;
use crate::kanban::core::repos::{group, Repo};
use crate::kanban::core::status::{render_text, select, Format};
use crate::kanban::core::text::defuse_marker;
use crate::kanban::core::wait::{derive, Blocked, Place, Subject};
use crate::kanban::core::wake::NextPrompt;
use crate::shared::process::read_stdin_raw;
use clap::Parser;
use serde_json::{json, Value};
use std::cell::OnceCell;
use std::time::{SystemTime, UNIX_EPOCH};

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

struct RelayBook<'a> {
    store: &'a dyn LedgerStore,
    book: OnceCell<Option<Book>>,
    now: u64,
}

impl<'a> RelayBook<'a> {
    fn new(store: &'a dyn LedgerStore, now: u64) -> RelayBook<'a> {
        RelayBook {
            store,
            book: OnceCell::new(),
            now,
        }
    }

    fn json(&self, session: &str, wait: &str) -> Value {
        let book = self.book.get_or_init(|| read_book(self.store).ok());
        let found = book
            .as_ref()
            .map(|book| unused(book, session, wait, self.now))
            .unwrap_or_default();
        found
            .iter()
            .map(|r| json!({ "id": r.id, "answer": r.answers, "at": r.at }))
            .collect()
    }
}

/// `relays` lists what a wait may be answered on.
struct WaitView<'a> {
    paths: &'a dyn RealPaths,
    relays: Option<&'a RelayBook<'a>>,
}

impl WaitView<'_> {
    fn repo_json(&self, repo: &Repo) -> Value {
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
                    "wait": self.wait_json(s.waiting.as_ref(), &place, &s.session_id),
                })
            })
            .collect();
        defused(json!({
            "label": repo.label,
            "root": repo.root,
            "sessions": sessions,
        }))
    }

    fn wait_json(&self, waiting: Option<&Waiting>, place: &Place, session: &str) -> Value {
        let Some(waiting) = waiting else {
            return Value::Null;
        };
        let wait = derive(waiting, place, self.paths);
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
        if let Some(book) = self.relays {
            out["relays"] = match &wait.id {
                Some(id) => book.json(session, id),
                None => json!([]),
            };
        }
        out
    }

    fn board_json(&self, board: &Board, layout: &Layout) -> Value {
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
        let repos: Vec<Value> = group(board)
            .iter()
            .map(|repo| self.repo_json(repo))
            .collect();
        json!({
            "columns": columns,
            "repos": repos,
            "delegate": delegate_json(&delegate_of(board)),
            "expired": board.expired,
            "layout": layout_json(layout),
        })
    }

    fn status_entry_json(&self, column: Column, card: &Card) -> Value {
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
            "wait": self.wait_json(card.waiting.as_ref(), &place, &card.session_id),
        }))
    }
}

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

fn with_records<T>(run: impl FnOnce(&KanbanRecords) -> T) -> T {
    let store = FsRecords::from_env();
    let inbox = FsInbox::from_env();
    run(&KanbanRecords {
        store: &store,
        inbox: &inbox,
        paths: &FsPaths,
    })
}

fn with_board_view<T>(run: impl FnOnce(&WaitView) -> T) -> T {
    run(&WaitView {
        paths: &FsPaths,
        relays: None,
    })
}

fn with_status_view<T>(now: u64, run: impl FnOnce(&WaitView, &FsLedger) -> T) -> T {
    let ledger = FsLedger::from_env();
    let relays = RelayBook::new(&ledger, now);
    run(
        &WaitView {
            paths: &FsPaths,
            relays: Some(&relays),
        },
        &ledger,
    )
}

fn with_delegate(run: impl FnOnce(&KanbanDelegate) -> Value) -> Value {
    let records = FsRecords::from_env();
    let ledger = FsLedger::from_env();
    run(&KanbanDelegate {
        records: &records,
        ledger: &ledger,
    })
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
        Ok((Format::Json, board)) => with_status_view(now, |view, ledger| {
            let sessions: Vec<Value> = select(&board, query.session.as_deref())
                .into_iter()
                .flat_map(|(column, cards)| cards.into_iter().map(move |card| (column, card)))
                .map(|(column, card)| view.status_entry_json(column, card))
                .collect();
            let book = read_book(ledger).ok();
            let delegate = status_delegate_json(&delegate_of(&board), book.as_ref(), now);
            json!({ "now": now, "delegate": delegate, "sessions": sessions }).to_string()
        }),
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
        LedgerCommand::Summary { since, until } => ledger.summary(since, until),
        LedgerCommand::Match { key, now } => ledger.matched(&key, now),
        LedgerCommand::Revoke { entry, reason, now } => ledger.revoke(&entry, &reason, now),
        LedgerCommand::Stats { now } => ledger.stats(now),
        LedgerCommand::Prune { now } => ledger.prune(now),
    }
}

fn run_request(command: RequestCommand) -> Value {
    let inbox = FsInbox::from_env();
    let ledger = FsLedger::from_env();
    let requests = KanbanRequests {
        inbox: &inbox,
        pause: &ThreadPause,
        ids: &ClockIds,
        probe: &GitProbe,
        launcher: &ClaudeLauncher,
        ledger: &ledger,
        paths: &FsPaths,
    };
    match command {
        RequestCommand::Send {
            session,
            from,
            from_label,
            nonce,
            text,
        } => requests.send(
            &session,
            from,
            from_label.as_deref(),
            nonce.as_deref(),
            &text,
        ),
        RequestCommand::Stop { session, turn } => requests.stop(&session, &turn),
        RequestCommand::Permit {
            session,
            request,
            decision,
            from,
            nonce,
            proof,
        } => requests.permit(
            &session,
            &request,
            &decision,
            Answerer {
                from,
                nonce: nonce.as_deref(),
                proof: proof.proof(),
            },
        ),
        RequestCommand::Reply {
            session,
            ask,
            answers,
            from,
            nonce,
            proof,
        } => requests.reply(
            &session,
            &ask,
            &answers,
            Answerer {
                from,
                nonce: nonce.as_deref(),
                proof: proof.proof(),
            },
        ),
        RequestCommand::Decide {
            session,
            at,
            text,
            from,
            nonce,
            proof,
        } => requests.decide(
            &session,
            &at,
            &text,
            Answerer {
                from,
                nonce: nonce.as_deref(),
                proof: proof.proof(),
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
                wait_stdin,
            } => {
                let timing = PermitTiming {
                    now,
                    ttl_ms: ttl,
                    for_ms: wait,
                };
                let raw = (nonces_stdin || wait_stdin)
                    .then(read_stdin_raw)
                    .unwrap_or_default();
                let read = if wait_stdin {
                    parse_permit_wait(&raw)
                } else {
                    parse_pane_nonces(&raw).map(|nonces| (nonces, None))
                };
                match read {
                    Ok((nonces, held)) => {
                        requests.wait_permit(&session, &request, timing, nonces, held.as_ref())
                    }
                    Err(error) => json!({ "error": error }),
                }
            }
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
                    with_board_view(|view| view.board_json(&board, &layout))
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
        Commands::RelayFrame { now, session } => {
            let out = match parse_tool_questions(&read_stdin_raw()) {
                Ok(questions) => with_records(|r| r.relay_frame(&questions, &session, now)),
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
        Commands::GuardBash => {
            let out = match parse_bash_command(&read_stdin_raw()) {
                Ok(command) => json!({ "deny": refusal(&command) }),
                Err(error) => json!({ "error": error }),
            };
            println!("{out}");
        }
        Commands::PruneNonces => {
            let out = match parse_nonce_keys(&read_stdin_raw()) {
                Ok(keys) => json!({ "drop": past_cap(&keys) }),
                Err(error) => json!({ "error": error }),
            };
            println!("{out}");
        }
        Commands::Command {
            session,
            now,
            origin,
            args,
        } => println!(
            "{}",
            with_delegate(|d| d.command(&Typed {
                session: &session,
                now,
                origin: &origin,
                args: &args,
            }))
        ),
        Commands::Delegate { command } => {
            let out = match command {
                DelegateCommandArgs::Wake {
                    session,
                    now,
                    report_at,
                    utc_offset,
                } => {
                    let raw = read_stdin_raw();
                    with_delegate(|d| {
                        d.wake(
                            &raw,
                            &WakeClock {
                                session: &session,
                                now,
                                report_at: &report_at,
                                utc_offset,
                            },
                        )
                    })
                }
                DelegateCommandArgs::Report { now } => with_delegate(|d| d.report(now)),
                DelegateCommandArgs::Attach {
                    origin,
                    slash_command,
                    meta,
                } => attach_json(&NextPrompt {
                    origin: &origin,
                    slash_command,
                    meta,
                }),
            };
            println!("{out}");
        }
        Commands::Ledger { command } => println!("{}", run_ledger(command)),
        Commands::Request(command) => println!("{}", run_request(command)),
    }
    0
}
