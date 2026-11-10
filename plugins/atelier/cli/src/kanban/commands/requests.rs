//! The request commands: each validates its arguments, does its one I/O step
//! and answers one JSON value.

use crate::kanban::commands::inbox;
use crate::kanban::commands::spawn::{self, SpawnArgs};
use crate::kanban::core::request::{self, Request, Sender, Taken};
use serde_json::{json, Value};

fn error(message: String) -> Value {
    json!({ "error": message })
}

fn queue(session: &str, request: Result<Request, String>) -> Value {
    let queued = request.and_then(|r| inbox::put(session, &r).map(|id| (id, r.kind())));
    match queued {
        Ok((id, kind)) => json!({ "queued": { "session_id": session, "id": id, "kind": kind } }),
        Err(e) => error(e),
    }
}

pub fn send(session: &str, from: &str, from_label: Option<&str>, text: &str) -> Value {
    let request = Sender::parse(from)
        .ok_or_else(|| format!("--from 은 pane 또는 agent 예요 (받은 값: {from})"))
        .and_then(|from| Request::prompt(text, from, from_label));
    queue(session, request)
}

pub fn stop(session: &str, turn: &str) -> Value {
    queue(session, Request::stop(turn))
}

pub fn dir(session: &str) -> Value {
    match inbox::inbox_dir(session) {
        Ok(dir) => json!({ "dir": dir.display().to_string() }),
        Err(e) => error(e),
    }
}

fn taken_json(taken: &Taken) -> Value {
    match taken {
        Taken::Prompt { text, body, from } => {
            json!({ "kind": "prompt", "text": text, "body": body, "from": from.id() })
        }
        Taken::Stop { turn_id } => json!({ "kind": "stop", "turn_id": turn_id }),
    }
}

pub fn take(session: &str, now: u64, ttl_ms: u64, turn: Option<&str>) -> Value {
    match inbox::claim_all(session, now) {
        Ok(found) => {
            let (taken, outcomes) = request::take(found, turn, ttl_ms);
            json!({
                "actions": taken.iter().map(taken_json).collect::<Vec<_>>(),
                "results": outcomes
                    .iter()
                    .map(|o| json!({ "id": o.id, "kind": o.kind, "status": o.status.id() }))
                    .collect::<Vec<_>>(),
            })
        }
        Err(e) => error(e),
    }
}

pub fn clear(session: &str) -> Value {
    match inbox::clear(session) {
        Ok(()) => json!({ "cleared": session }),
        Err(e) => error(e),
    }
}

pub fn spawn(cwd: &str, text: &str, worktree: Option<&str>, from_label: Option<&str>) -> Value {
    let args = SpawnArgs {
        cwd,
        text,
        worktree,
        from_label,
    };
    match spawn::spawn(&args) {
        Ok(s) => json!({
            "spawned": { "short_id": s.short_id, "cwd": s.cwd, "worktree": s.worktree }
        }),
        Err(e) => error(e),
    }
}
