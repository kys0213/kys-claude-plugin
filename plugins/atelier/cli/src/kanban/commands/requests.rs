//! The request commands: validate the arguments, do one step through the
//! injected edges, answer one JSON value.

use crate::kanban::commands::inbox::{IdSource, InboxStore, Pause};
use crate::kanban::commands::spawn::{Launcher, RepoProbe};
use crate::kanban::core::request::{
    self, check_answerer, check_id, Found, LeftNonce, Nonce, PaneNonces, Request, RequestKind,
    Sender, Taken, Waits,
};
use crate::kanban::core::spawn::{is_main_root, launch_args, short_id};
use serde_json::{json, Value};

const PERMIT_POLL_MS: u64 = 200;

/// Who sends an answer, and the nonce the pane left with it.
#[derive(Debug, Clone, Copy)]
pub struct Answerer<'a> {
    pub from: Sender,
    pub nonce: Option<&'a str>,
}

impl From<Sender> for Answerer<'_> {
    fn from(from: Sender) -> Self {
        Answerer { from, nonce: None }
    }
}

pub struct KanbanRequests<'a> {
    pub inbox: &'a dyn InboxStore,
    pub pause: &'a dyn Pause,
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
        Taken::Answer {
            tool_use_id,
            answers,
            from,
            via,
        } => json!({
            "kind": "answer",
            "tool_use_id": tool_use_id,
            "answers": answers,
            "from": from.id(),
            "via": via.id(),
        }),
        Taken::Decide {
            decision_at,
            text,
            from,
            via,
        } => json!({
            "kind": "decide",
            "decision_at": decision_at,
            "text": text,
            "from": from.id(),
            "via": via.id(),
        }),
    }
}

fn outcome_json(outcome: &request::Outcome) -> Value {
    let mut value = json!({
        "id": outcome.id,
        "kind": outcome.kind.map_or("unknown", |k| k.id()),
        "status": outcome.status.id(),
    });
    if let (Some(via), Some(fields)) = (outcome.via, value.as_object_mut()) {
        fields.insert("via".to_string(), json!(via.id()));
    }
    value
}

fn used_json(used: &[LeftNonce]) -> Value {
    used.iter().map(LeftNonce::to_json).collect()
}

impl KanbanRequests<'_> {
    fn queue(&self, session: &str, request: Result<Request, String>) -> Value {
        answer((|| {
            check_id("session", session)?;
            let request = request?;
            let contents = request.to_json().to_string();
            let id = match &request {
                Request::Permit { request_id, .. } => {
                    self.inbox.put_permit(session, request_id, &contents)?;
                    request_id.clone()
                }
                _ => {
                    let id = self.ids.next_id();
                    self.inbox.put(session, &id, &contents)?;
                    id
                }
            };
            Ok(
                json!({ "queued": { "session_id": session, "id": id, "kind": request.kind().id() } }),
            )
        })())
    }

    fn queue_answer(
        &self,
        session: &str,
        answerer: Answerer,
        request: impl FnOnce(Sender, Option<Nonce>) -> Result<Request, String>,
    ) -> Value {
        let request = check_answerer(answerer.from)
            .and_then(|()| answerer.nonce.map(Nonce::parse).transpose())
            .and_then(|nonce| request(answerer.from, nonce));
        self.queue(session, request)
    }

    pub fn send(&self, session: &str, from: Sender, from_label: Option<&str>, text: &str) -> Value {
        self.queue(session, Request::prompt(text, from, from_label))
    }

    pub fn stop(&self, session: &str, turn: &str) -> Value {
        self.queue(session, Request::stop(turn))
    }

    /// Leaves one answer per question for the open question `ask` names.
    pub fn reply(&self, session: &str, ask: &str, answers: &[String], answerer: Answerer) -> Value {
        self.queue_answer(session, answerer, |from, nonce| {
            Request::answer(ask, answers, from, nonce)
        })
    }

    /// Leaves the reply to the decision that `at` names.
    pub fn decide(&self, session: &str, at: &str, text: &str, answerer: Answerer) -> Value {
        self.queue_answer(session, answerer, |from, nonce| {
            Request::decide(at, text, from, nonce)
        })
    }

    pub fn permit(
        &self,
        session: &str,
        request: &str,
        decision: &str,
        answerer: Answerer,
    ) -> Value {
        self.queue_answer(session, answerer, |from, nonce| {
            Request::permit(request, decision, from, nonce)
        })
    }

    /// Waits up to `for_ms` for the answer to `request`, claiming it.
    pub fn wait_permit(
        &self,
        session: &str,
        request: &str,
        now: u64,
        ttl_ms: u64,
        for_ms: u64,
        mut nonces: PaneNonces,
    ) -> Value {
        answer((|| {
            check_id("session", session)?;
            check_id("permission", request)?;
            let mut waited = 0;
            loop {
                if let Some(claimed) = self.inbox.take_permit(session, request, now + waited)? {
                    if claimed.age_ms > ttl_ms {
                        return Ok(json!({ "decision": null, "status": "expired" }));
                    }
                    return Ok(match claimed.contents.as_deref().and_then(Request::parse) {
                        Some(Request::Permit {
                            verdict,
                            from,
                            nonce,
                            ..
                        }) => {
                            let via = nonces.spend(RequestKind::Permit, from, nonce.as_ref());
                            match check_answerer(from) {
                                Err(_) => json!({
                                    "decision": null,
                                    "status": "refused",
                                    "via": via.id(),
                                }),
                                Ok(()) => json!({
                                    "decision": verdict.id(),
                                    "from": from.id(),
                                    "via": via.id(),
                                    "used_nonces": used_json(nonces.used()),
                                }),
                            }
                        }
                        _ => json!({ "decision": null, "status": "invalid" }),
                    });
                }
                if waited >= for_ms {
                    return Ok(json!({ "decision": null }));
                }
                let step = PERMIT_POLL_MS.min(for_ms - waited);
                self.pause.pause(step);
                waited += step;
            }
        })())
    }

    pub fn dir(&self, session: &str) -> Value {
        answer(check_id("session", session).and_then(|()| {
            let dir = self.inbox.dir(session)?;
            Ok(json!({ "dir": dir.display().to_string() }))
        }))
    }

    /// Claims every request and says which to act on, judged against what
    /// the session waits on now.
    pub fn take(
        &self,
        session: &str,
        now: u64,
        ttl_ms: u64,
        turn: Option<&str>,
        waits: &Waits,
        nonces: PaneNonces,
    ) -> Value {
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
            let took = request::take(found, turn, waits, nonces, ttl_ms);
            Ok(json!({
                "actions": took.taken.iter().map(taken_json).collect::<Vec<_>>(),
                "results": took.outcomes.iter().map(outcome_json).collect::<Vec<_>>(),
                "used_nonces": used_json(&took.used),
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
