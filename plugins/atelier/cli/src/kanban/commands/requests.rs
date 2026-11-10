//! The request commands: validate the arguments, do one step through the
//! injected edges, answer one JSON value.

use crate::kanban::commands::inbox::{IdSource, InboxStore};
use crate::kanban::commands::spawn::{Launcher, RepoProbe};
use crate::kanban::core::request::{self, check_id, Found, Request, Sender, Taken};
use crate::kanban::core::spawn::{is_main_root, launch_args, short_id};
use serde_json::{json, Value};

pub struct KanbanRequests<'a> {
    pub inbox: &'a dyn InboxStore,
    pub ids: &'a dyn IdSource,
    pub probe: &'a dyn RepoProbe,
    pub launcher: &'a dyn Launcher,
}

fn answer(result: Result<Value, String>) -> Value {
    result.unwrap_or_else(|e| json!({ "error": e }))
}

fn taken_json(taken: &Taken) -> Value {
    match taken {
        Taken::Prompt { text, body, from } => {
            json!({ "kind": "prompt", "text": text, "body": body, "from": from.id() })
        }
        Taken::Stop { turn_id } => json!({ "kind": "stop", "turn_id": turn_id }),
    }
}

impl KanbanRequests<'_> {
    fn queue(&self, session: &str, request: Result<Request, String>) -> Value {
        answer((|| {
            check_id("session", session)?;
            let request = request?;
            let id = self.ids.next_id();
            self.inbox
                .put(session, &id, &request.to_json().to_string())?;
            Ok(
                json!({ "queued": { "session_id": session, "id": id, "kind": request.kind().id() } }),
            )
        })())
    }

    pub fn send(&self, session: &str, from: Sender, from_label: Option<&str>, text: &str) -> Value {
        self.queue(session, Request::prompt(text, from, from_label))
    }

    pub fn stop(&self, session: &str, turn: &str) -> Value {
        self.queue(session, Request::stop(turn))
    }

    pub fn dir(&self, session: &str) -> Value {
        answer(check_id("session", session).and_then(|()| {
            let dir = self.inbox.dir(session)?;
            Ok(json!({ "dir": dir.display().to_string() }))
        }))
    }

    pub fn take(&self, session: &str, now: u64, ttl_ms: u64, turn: Option<&str>) -> Value {
        answer(check_id("session", session).and_then(|()| {
            let found = self
                .inbox
                .claim_all(session, now)?
                .into_iter()
                .map(|c| Found {
                    id: c.id,
                    age_ms: c.age_ms,
                    request: c.contents.as_deref().and_then(Request::parse),
                })
                .collect();
            let (taken, outcomes) = request::take(found, turn, ttl_ms);
            Ok(json!({
                "actions": taken.iter().map(taken_json).collect::<Vec<_>>(),
                "results": outcomes
                    .iter()
                    .map(|o| json!({
                        "id": o.id,
                        "kind": o.kind.map_or("unknown", |k| k.id()),
                        "status": o.status.id(),
                    }))
                    .collect::<Vec<_>>(),
            }))
        }))
    }

    pub fn clear(&self, session: &str) -> Value {
        answer(check_id("session", session).and_then(|()| {
            self.inbox.clear(session)?;
            Ok(json!({ "cleared": session }))
        }))
    }

    pub fn spawn(
        &self,
        cwd: &str,
        text: &str,
        worktree: Option<&str>,
        from_label: Option<&str>,
    ) -> Value {
        answer((|| {
            if !self.probe.paths(cwd).is_some_and(|p| is_main_root(&p)) {
                return Err(format!(
                    "`{cwd}` 는 git 저장소의 메인 작업 트리 루트가 아니에요"
                ));
            }
            let args = launch_args(text, worktree, from_label)?;
            let out = self.launcher.launch(cwd, &args)?;
            match short_id(&out.stdout) {
                Some(short_id) if out.success => Ok(json!({
                    "spawned": { "short_id": short_id, "cwd": cwd, "worktree": worktree }
                })),
                _ => {
                    let said = [out.stdout.trim(), out.stderr.trim()]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" / ");
                    Err(format!("새 세션을 띄우지 못했어요: {said}"))
                }
            }
        })())
    }
}
