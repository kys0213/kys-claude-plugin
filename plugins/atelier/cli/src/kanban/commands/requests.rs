//! The request commands: validate the arguments, do one step through the
//! injected edges, answer one JSON value.

use crate::kanban::commands::inbox::{IdSource, InboxStore, Pause};
use crate::kanban::commands::ledger_store::{read_book, LedgerStore};
use crate::kanban::commands::spawn::{Launcher, RepoProbe};
use crate::kanban::core::allowlist::RealPaths;
use crate::kanban::core::precedent::{
    vouch, whole_permission_wait, whole_question_wait, Auth, Precedent, Proxied, SessionPlace,
};
use crate::kanban::core::request::{
    self, check_id, Found, LeftNonce, Nonce, OpenAsk, PaneNonces, Request, RequestKind, Sender,
    Taken, Taking, Verdict, Waits,
};
use crate::kanban::core::spawn::{is_main_root, launch_args, short_id};
use serde_json::{json, Value};
use std::cell::OnceCell;

const PERMIT_POLL_MS: u64 = 200;

/// What a model's answer is sent on, as given.
#[derive(Debug, Clone, Copy, Default)]
pub struct Proof<'a> {
    /// `<question>:<ledger entry id>` each.
    pub precedents: &'a [String],
    pub reason: Option<&'a str>,
    pub relay: Option<&'a str>,
    /// When the precedents are judged, unix milliseconds.
    pub now: u64,
}

impl Proof<'_> {
    fn is_empty(&self) -> bool {
        self.precedents.is_empty() && self.reason.is_none() && self.relay.is_none()
    }
}

/// Who sends an answer, the nonce the pane left with it, and what a model
/// sends it on.
#[derive(Debug, Clone, Copy)]
pub struct Answerer<'a> {
    pub from: Sender,
    pub nonce: Option<&'a str>,
    pub proof: Proof<'a>,
}

impl From<Sender> for Answerer<'_> {
    fn from(from: Sender) -> Self {
        Answerer {
            from,
            nonce: None,
            proof: Proof::default(),
        }
    }
}

/// The permission request a session waits on, as the request holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermitWait {
    pub place: SessionPlace,
    pub tool: String,
    pub input: Vec<(String, String)>,
}

/// When a permission wait runs, how old an answer may be, and how long it waits.
#[derive(Debug, Clone, Copy)]
pub struct PermitTiming {
    pub now: u64,
    pub ttl_ms: u64,
    pub for_ms: u64,
}

pub struct KanbanRequests<'a> {
    pub inbox: &'a dyn InboxStore,
    pub pause: &'a dyn Pause,
    pub ids: &'a dyn IdSource,
    pub probe: &'a dyn RepoProbe,
    pub launcher: &'a dyn Launcher,
    pub ledger: &'a dyn LedgerStore,
    pub paths: &'a dyn RealPaths,
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
            auth,
        } => with_auth(
            json!({
                "kind": "answer",
                "tool_use_id": tool_use_id,
                "answers": answers,
                "from": from.id(),
                "via": via.id(),
            }),
            auth.as_ref(),
        ),
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
    if let (Some(reason), Some(fields)) = (&outcome.reason, value.as_object_mut()) {
        fields.insert("reason".to_string(), json!(reason));
    }
    value
}

fn with_auth(mut value: Value, auth: Option<&Auth>) -> Value {
    if let (Some(auth), Some(fields)) = (auth, value.as_object_mut()) {
        fields.insert("auth".to_string(), auth.to_json());
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

    /// What a model's answer is sent on, when its precedents vouch for it; a
    /// person's answer carries nothing.
    fn auth(
        &self,
        answerer: &Answerer,
        proxied: Option<Proxied>,
        answers: &[String],
    ) -> Result<Option<Auth>, String> {
        let proof = answerer.proof;
        if answerer.from == Sender::Pane {
            if !proof.is_empty() {
                return Err(
                    "`--precedent`·`--reason`·`--relay` 는 `--from agent` 에만 붙여요".to_string(),
                );
            }
            return Ok(None);
        }
        let proxied = proxied.ok_or("결정 요청은 아직 대신 답하지 않아요".to_string())?;
        if proof.relay.is_some() {
            return Err("물어 받은 답(`--relay`)은 아직 받지 않아요".to_string());
        }
        let Some(reason) = proof.reason.filter(|_| !proof.precedents.is_empty()) else {
            return Err(
                "메인 에이전트의 답(`--from agent`)은 선례 `--precedent <질문 순번>:<장부 id>` 와 근거 `--reason` 이 있어야 해요"
                    .to_string(),
            );
        };
        let precedents = proof
            .precedents
            .iter()
            .map(|p| Precedent::parse(p))
            .collect::<Result<Vec<_>, _>>()?;
        let auth = Auth::new(precedents, reason)?;
        vouch(
            &read_book(self.ledger)?,
            proxied,
            &auth.claims(answers)?,
            None,
            proof.now,
        )?;
        Ok(Some(auth))
    }

    fn queue_answer(
        &self,
        session: &str,
        answerer: Answerer,
        sent: (Option<Proxied>, &[String]),
        request: impl FnOnce(Sender, Option<Nonce>) -> Result<Request, String>,
    ) -> Value {
        let (proxied, answers) = sent;
        let request = (|| {
            let nonce = answerer.nonce.map(Nonce::parse).transpose()?;
            let request = request(answerer.from, nonce)?;
            let auth = self.auth(&answerer, proxied, answers)?;
            Ok(request.with_auth(auth))
        })();
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
        self.queue_answer(
            session,
            answerer,
            (Some(Proxied::Question), answers),
            |from, nonce| Request::answer(ask, answers, from, nonce),
        )
    }

    /// Leaves the reply to the decision that `at` names.
    pub fn decide(&self, session: &str, at: &str, text: &str, answerer: Answerer) -> Value {
        self.queue_answer(session, answerer, (None, &[]), |from, nonce| {
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
        let answers = [decision.to_string()];
        self.queue_answer(
            session,
            answerer,
            (Some(Proxied::Permission), &answers),
            |from, nonce| Request::permit(request, decision, from, nonce),
        )
    }

    /// Whether the precedents vouch for a model's verdict on the request as
    /// the waiting session holds it.
    fn vet_permit(
        &self,
        request: &str,
        verdict: Verdict,
        auth: Option<&Auth>,
        wait: Option<&PermitWait>,
        now: u64,
    ) -> Result<(), String> {
        let auth = auth.ok_or("메인 에이전트의 답에 선례가 없어요".to_string())?;
        let wait = wait.ok_or("받는 세션이 원래 요청을 넘기지 않았어요".to_string())?;
        let whole =
            whole_permission_wait(request, &wait.tool, &wait.input, &wait.place, self.paths);
        let answers = [verdict.id().to_string()];
        vouch(
            &read_book(self.ledger)?,
            Proxied::Permission,
            &auth.claims(&answers)?,
            Some(&whole),
            now,
        )
    }

    /// Waits up to `for_ms` for the answer to `request`, claiming it.
    pub fn wait_permit(
        &self,
        session: &str,
        request: &str,
        timing: PermitTiming,
        mut nonces: PaneNonces,
        wait: Option<&PermitWait>,
    ) -> Value {
        let PermitTiming {
            now,
            ttl_ms,
            for_ms,
        } = timing;
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
                            auth,
                            ..
                        }) => {
                            let via = nonces.spend(RequestKind::Permit, from, nonce.as_ref());
                            let vouched = match from {
                                Sender::Pane => Ok(()),
                                Sender::Agent => {
                                    self.vet_permit(request, verdict, auth.as_ref(), wait, now)
                                }
                            };
                            match vouched {
                                Err(reason) => json!({
                                    "decision": null,
                                    "status": "refused",
                                    "via": via.id(),
                                    "reason": reason,
                                }),
                                Ok(()) => with_auth(
                                    json!({
                                        "decision": verdict.id(),
                                        "from": from.id(),
                                        "via": via.id(),
                                        "used_nonces": used_json(nonces.used()),
                                    }),
                                    auth.as_ref().filter(|_| from == Sender::Agent),
                                ),
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
            let book = OnceCell::new();
            let vet = |ask: &OpenAsk, answers: &[String], auth: &Auth| {
                let book = book
                    .get_or_init(|| read_book(self.ledger))
                    .as_ref()
                    .map_err(Clone::clone)?;
                let (Some(questions), Some(place)) = (&ask.whole, &waits.place) else {
                    return Err("받는 세션이 원래 질문을 넘기지 않았어요".to_string());
                };
                let wait = whole_question_wait(&ask.id, questions, place, self.paths);
                vouch(
                    book,
                    Proxied::Question,
                    &auth.claims(answers)?,
                    Some(&wait),
                    now,
                )
            };
            let took = request::take(
                found,
                Taking {
                    running_turn: turn,
                    waits,
                    nonces,
                    ttl_ms,
                    vet: &vet,
                },
            );
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
