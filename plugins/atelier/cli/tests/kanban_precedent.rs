use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;

mod kanban_support;
use kanban_support::{
    chose, color, color_and_size, errored, home_deref, inbox_answer, inbox_verdict, js_len, now_ms,
    permission_facts, place_at, place_json, question, question_facts, ran, refused, typed_decision,
    with, Home, DAY, HOUR, REPO, SEC,
};

struct World {
    home: Home,
    now: u64,
}

home_deref!(World);

impl World {
    fn new() -> World {
        World {
            home: Home::default(),
            now: now_ms(),
        }
    }

    fn key_of(&self, id: &str) -> String {
        self.entry(id)["key"].as_str().expect("key").to_string()
    }

    fn approve(&self, id: &str) {
        self.rewrite_lines(|mut line| {
            if line["id"] == id {
                line["approved"] = json!(true);
            }
            line
        });
    }

    /// What `/kanban delegate act|shadow <scope>` leaves.
    fn set(&self, scope: &str, value: &str, origin: &str, at: u64) {
        let id = format!("S{at}-{scope}-{value}");
        self.add_ledger_line(json!({
            "v": 1,
            "type": "setting",
            "id": id,
            "at": at,
            "scope": scope,
            "value": value,
            "origin": origin,
        }));
    }

    fn act(&self, scope: &str) {
        self.set(scope, "act", "composer", self.now - 10 * DAY);
    }

    fn matched(&self, key: &str) -> Value {
        self.matched_at(key, self.now)
    }

    fn matched_at(&self, key: &str, now: u64) -> Value {
        self.ledger_match(key, now)
    }

    fn revoke(&self, entry: &str, at: u64) -> Value {
        self.kanban(&[
            "ledger",
            "revoke",
            "--entry",
            entry,
            "--reason",
            "사용자가 다르게 하라고 했어요",
            "--now",
            &at.to_string(),
        ])
    }

    fn reply(&self, ask: &str, answers: &[&str], precedents: &[(usize, &str)]) -> Value {
        let mut args: Vec<String> = ["reply", "--session", "w", "--ask", ask]
            .map(String::from)
            .to_vec();
        for answer in answers {
            args.extend(["--answer".to_string(), answer.to_string()]);
        }
        args.extend(proof(precedents, self.now));
        self.kanban(&args.iter().map(String::as_str).collect::<Vec<_>>())
    }

    fn permit(&self, request: &str, decision: &str, precedents: &[(usize, &str)]) -> Value {
        let mut args: Vec<String> = [
            "permit",
            "--session",
            "w",
            "--request",
            request,
            "--decision",
            decision,
        ]
        .map(String::from)
        .to_vec();
        args.extend(proof(precedents, self.now));
        self.kanban(&args.iter().map(String::as_str).collect::<Vec<_>>())
    }

    fn take(&self, open: Value) -> Value {
        self.home.take("w", self.now, "60s", None, Some(&open))
    }

    fn wait_permit(&self, request: &str, flag: &str, stdin: Value) -> Value {
        self.home
            .wait_permit("w", request, self.now, "60s", Some((flag, &stdin)))
    }
}

fn proof(precedents: &[(usize, &str)], now: u64) -> Vec<String> {
    let mut args = vec!["--from".to_string(), "agent".to_string()];
    for (q, id) in precedents {
        args.extend(["--precedent".to_string(), format!("{q}:{id}")]);
    }
    args.extend([
        "--reason".to_string(),
        "같은 질문에 두 번 Red 를 골랐어요".to_string(),
        "--now".to_string(),
        now.to_string(),
    ]);
    args
}

fn size() -> Value {
    json!([question("Which size?", &["S", "M"], false)])
}

fn picked(at: u64, label: &str) -> Value {
    question_facts(at, color(), chose(json!([label])))
}

fn by_inbox(at: u64, via: &str, label: &str) -> Value {
    question_facts(at, color(), inbox_answer(via, label))
}

fn two_reds(w: &World) -> (String, String) {
    (
        w.record(picked(w.now - 2 * DAY, "Red")),
        w.record(picked(w.now - DAY, "Red")),
    )
}

fn asked_color() -> Value {
    json!({
        "question": "Which color?",
        "header": "Pick",
        "multiSelect": false,
        "options": [{ "label": "Red", "description": "warm" }, { "label": "Blue" }],
    })
}

fn asked(text: &str, labels: &[&str]) -> Value {
    json!({
        "question": text,
        "header": "Pick",
        "multiSelect": false,
        "options": labels.iter().map(|l| json!({ "label": l })).collect::<Vec<_>>(),
    })
}

fn open(questions: Value) -> Value {
    json!({
        "asks": [{ "id": "tu-9", "questions": questions }],
        "decision": null,
        "place": place_json(REPO),
    })
}

fn bash_wait(repo: &str, command: &str) -> Value {
    json!({
        "nonces": [],
        "place": place_json(repo),
        "permission": { "tool": "Bash", "input": { "command": command } },
    })
}

fn reasons(matched: &Value) -> Vec<String> {
    matched["reasons"]
        .as_array()
        .unwrap_or_else(|| panic!("reasons in {matched}"))
        .iter()
        .map(|r| r.as_str().expect("reason").to_string())
        .collect()
}

#[test]
fn two_direct_decisions_make_their_key_eligible() {
    let w = World::new();
    let (_, newest) = two_reds(&w);
    let key = w.key_of(&newest);

    let matched = w.matched(&key);

    assert_eq!(matched["key"], json!(key));
    assert_eq!(matched["eligible"], true, "{matched}");
    assert_eq!(matched["answer"], "Red");
    assert_eq!(matched["answer_fp"], w.entry(&newest)["answer_fp"]);
    assert_eq!(matched["support"], json!({ "direct": 2, "approved": 0 }));
    assert_eq!(matched["conflicts"], 0);
    assert_eq!(matched["disabled"], false);
    assert_eq!(matched["mode"], "shadow");
    assert_eq!(reasons(&matched), Vec::<String>::new());
}

#[test]
fn one_direct_decision_is_not_support_enough() {
    let w = World::new();
    let id = w.record(picked(w.now - DAY, "Red"));

    let matched = w.matched(&w.key_of(&id));

    assert_eq!(matched["eligible"], false);
    assert_eq!(reasons(&matched), vec!["support"]);
}

#[test]
fn approved_recommendations_count_only_as_three_with_direct_ones() {
    let w = World::new();
    let (a, b) = two_reds(&w);
    let key = w.key_of(&a);
    w.approve(&a);
    w.approve(&b);

    let two_approved = w.matched(&key);
    assert_eq!(
        two_approved["support"],
        json!({ "direct": 0, "approved": 2 })
    );
    assert_eq!(two_approved["eligible"], false);

    let c = w.record(picked(w.now - HOUR, "Red"));
    w.approve(&c);
    let three_approved = w.matched(&key);
    assert_eq!(
        three_approved["support"],
        json!({ "direct": 0, "approved": 3 })
    );
    assert_eq!(three_approved["eligible"], true);

    let w = World::new();
    let (a, _) = two_reds(&w);
    w.approve(&a);
    w.record(picked(w.now - HOUR, "Red"));
    let mixed = w.matched(&w.key_of(&a));
    assert_eq!(mixed["support"], json!({ "direct": 2, "approved": 1 }));
    assert_eq!(mixed["eligible"], true);
}

#[test]
fn a_decision_older_than_ninety_days_is_not_counted() {
    let w = World::new();
    let old = w.record(picked(w.now - 91 * DAY, "Red"));
    w.record(picked(w.now - DAY, "Red"));
    let key = w.key_of(&old);

    let matched = w.matched(&key);
    assert_eq!(matched["support"]["direct"], 1);
    assert_eq!(matched["eligible"], false);

    let earlier = w.matched_at(&key, w.now - 2 * DAY);
    assert_eq!(
        earlier["support"]["direct"], 1,
        "later entries are not seen"
    );
}

#[test]
fn a_different_human_answer_in_the_window_is_a_conflict() {
    for other in ["Blue", "Green"] {
        let w = World::new();
        let (a, _) = two_reds(&w);
        w.record(picked(w.now - HOUR, other));

        let matched = w.matched(&w.key_of(&a));

        assert_eq!(matched["eligible"], false, "{matched}");
        assert_eq!(matched["conflicts"], 1);
        assert_eq!(reasons(&matched), vec!["conflict"]);
    }
}

#[test]
fn a_revoked_key_stays_off_until_two_new_direct_decisions_agree() {
    let w = World::new();
    w.record(picked(w.now - 5 * DAY, "Red"));
    let first = w.record(picked(w.now - 4 * DAY, "Red"));
    let proxied = w.record(by_inbox(w.now - 3 * DAY, "agent", "Red"));
    let key = w.key_of(&first);

    let out = w.revoke(&proxied, w.now - 2 * DAY);
    assert_eq!(out["revoked"]["entry"], json!(proxied), "{out}");
    assert_eq!(out["revoked"]["key"], json!(key));

    let off = w.matched(&key);
    assert_eq!(off["disabled"], true);
    assert_eq!(off["eligible"], false);
    assert_eq!(reasons(&off), vec!["disabled"]);

    w.record(picked(w.now - DAY, "Red"));
    assert_eq!(w.matched(&key)["disabled"], true);

    w.record(picked(w.now - HOUR, "Red"));
    let back = w.matched(&key);
    assert_eq!(back["disabled"], false);
    assert_eq!(back["eligible"], true);

    assert_eq!(w.matched_at(&key, w.now - 3 * DAY)["disabled"], false);
}

#[test]
fn only_a_proxy_answer_in_the_ledger_can_be_revoked() {
    let w = World::new();
    let (human, _) = two_reds(&w);

    assert!(refused(&w.revoke(&human, w.now)));
    assert!(refused(&w.revoke("L1-00000000", w.now)));
    let proxied = w.record(by_inbox(w.now - HOUR, "agent", "Red"));
    let long = "가".repeat(201);
    let out = w.kanban(&[
        "ledger",
        "revoke",
        "--entry",
        &proxied,
        "--reason",
        &long,
        "--now",
        &w.now.to_string(),
    ]);
    assert!(refused(&out), "{out}");
    assert!(!w.matched(&w.key_of(&human))["disabled"].as_bool().unwrap());
}

type FactsAt = Box<dyn Fn(u64) -> Value>;

#[test]
fn answers_no_person_picked_from_the_options_do_not_support() {
    let masked_question = json!([question(
        "Which key?",
        &["sk-abcdefghijklmnopqrstuv", "none"],
        false
    )]);
    let cases: Vec<(&str, FactsAt)> = vec![
        ("typed", Box::new(|at| picked(at, "Green"))),
        ("terminal", Box::new(|at| by_inbox(at, "terminal", "Red"))),
        ("proxy", Box::new(|at| by_inbox(at, "agent", "Red"))),
        (
            "away",
            Box::new(|at| {
                question_facts(
                    at,
                    color(),
                    with(chose(json!(["Red"])), json!({ "afk_timeout_ms": 60_000 })),
                )
            }),
        ),
        (
            "masked",
            Box::new(move |at| {
                question_facts(
                    at,
                    masked_question.clone(),
                    chose(json!(["sk-abcdefghijklmnopqrstuv"])),
                )
            }),
        ),
        (
            "inferred",
            Box::new(|at| permission_facts(REPO, at, "git status", errored())),
        ),
    ];
    for (name, facts) in cases {
        let w = World::new();
        let a = w.record(facts(w.now - 2 * DAY));
        w.record(facts(w.now - DAY));

        let matched = w.matched(&w.key_of(&a));

        assert_eq!(
            matched["support"],
            json!({ "direct": 0, "approved": 0 }),
            "{name}: {matched}"
        );
        assert_eq!(matched["eligible"], false, "{name}");
    }
}

#[test]
fn any_human_deny_conflicts_with_allows_for_a_permission_key() {
    for deny in [errored(), inbox_verdict("pane", "deny")] {
        let w = World::new();
        let a = w.record(permission_facts(REPO, w.now - 3 * DAY, "git status", ran()));
        w.record(permission_facts(REPO, w.now - 2 * DAY, "git status", ran()));
        let key = w.key_of(&a);
        assert_eq!(w.matched(&key)["eligible"], true);

        w.record(permission_facts(
            REPO,
            w.now - DAY,
            "git status",
            deny.clone(),
        ));

        let matched = w.matched(&key);
        assert_eq!(matched["eligible"], false, "{deny}: {matched}");
        assert_eq!(matched["conflicts"], 1);
    }
}

#[test]
fn a_kind_acts_only_on_a_persons_latest_setting_and_steps_back_after_two_revokes() {
    let w = World::new();
    let (a, _) = two_reds(&w);
    let question_key = w.key_of(&a);
    let p = w.record(permission_facts(REPO, w.now - DAY, "git status", ran()));
    let permission_key = w.key_of(&p);

    w.set("question", "act", "sdk", w.now - 9 * DAY);
    assert_eq!(w.matched(&question_key)["mode"], "shadow");

    w.act("question");
    assert_eq!(w.matched(&question_key)["mode"], "act");
    assert_eq!(w.matched(&permission_key)["mode"], "shadow");

    w.set("question", "shadow", "bridge", w.now - 8 * DAY);
    assert_eq!(w.matched(&question_key)["mode"], "shadow");
    w.set("question", "act", "bridge", w.now - 7 * DAY);
    assert_eq!(w.matched(&question_key)["mode"], "act");

    let x = w.record(by_inbox(w.now - 5 * DAY, "agent", "Red"));
    let y = w.record(by_inbox(w.now - 4 * DAY, "agent", "Red"));
    w.revoke(&x, w.now - 3 * DAY);
    assert_eq!(w.matched(&question_key)["mode"], "act");
    w.revoke(&y, w.now - 2 * DAY);
    assert_eq!(w.matched(&question_key)["mode"], "shadow");
    assert_eq!(
        w.matched_at(&question_key, w.now + 6 * DAY)["mode"],
        "shadow"
    );
    assert_eq!(
        w.matched_at(&question_key, w.now + 60 * DAY)["mode"],
        "shadow"
    );
}

#[test]
fn a_demoted_kind_acts_again_only_on_a_persons_newer_act() {
    let w = World::new();
    let (a, _) = two_reds(&w);
    let question_key = w.key_of(&a);
    w.act("question");
    let x = w.record(by_inbox(w.now - 6 * DAY, "agent", "Red"));
    let y = w.record(by_inbox(w.now - 5 * DAY, "agent", "Red"));
    w.revoke(&x, w.now - 4 * DAY);
    w.revoke(&y, w.now - 3 * DAY);
    assert_eq!(w.matched(&question_key)["mode"], "shadow");

    w.set("question", "act", "sdk", w.now - 2 * DAY);
    assert_eq!(w.matched(&question_key)["mode"], "shadow");

    w.set("question", "act", "composer", w.now - DAY);
    assert_eq!(w.matched(&question_key)["mode"], "act");
}

#[test]
fn two_revokes_more_than_a_week_apart_do_not_demote_a_kind() {
    let w = World::new();
    let (a, _) = two_reds(&w);
    let question_key = w.key_of(&a);
    w.act("question");
    let x = w.record(by_inbox(w.now - 9 * DAY - HOUR, "agent", "Red"));
    let y = w.record(by_inbox(w.now - 2 * DAY, "agent", "Red"));
    w.revoke(&x, w.now - 9 * DAY);
    w.revoke(&y, w.now - DAY);

    assert_eq!(w.matched(&question_key)["mode"], "act");
}

#[test]
fn a_decision_key_never_acts() {
    let w = World::new();
    w.set("decision", "act", "composer", w.now - DAY);
    let decision = json!({
        "at": w.now - DAY - 10 * SEC,
        "ask": "진행할까요?",
        "options": ["네", "아니요"],
        "orig_len": { "ask": js_len("진행할까요?"), "options": [1, 3] },
    });
    let prompt = |at: u64| {
        let mut facts = with(
            place_at(at, REPO),
            json!({ "decision": decision, "answer": typed_decision("네") }),
        );
        facts["decision"]["at"] = json!(at - 10 * SEC);
        facts
    };
    let a = w.record(prompt(w.now - 2 * DAY));
    w.record(prompt(w.now - DAY));

    assert_eq!(w.matched(&w.key_of(&a))["mode"], "shadow");
}

fn precedent_ids(matched: &Value) -> Vec<String> {
    matched["precedent_ids"]
        .as_array()
        .unwrap_or_else(|| panic!("precedent_ids in {matched}"))
        .iter()
        .map(|id| id.as_str().expect("id").to_string())
        .collect()
}

#[test]
fn match_names_only_a_persons_own_picks_of_its_answer_in_the_window_newest_first() {
    let w = World::new();
    for days in [93, 92, 91] {
        w.record(picked(w.now - days * DAY, "Red"));
    }
    w.record(picked(w.now - 94 * DAY, "Blue"));
    let older = w.record(picked(w.now - 3 * DAY, "Red"));
    let newer = w.record(picked(w.now - 2 * DAY, "Red"));
    w.record(by_inbox(w.now - DAY, "terminal", "Red"));
    w.record(by_inbox(w.now - DAY, "agent", "Red"));
    for (hours, flag) in [(3, "inferred"), (2, "answer_masked"), (1, "free_text")] {
        let id = w.record(picked(w.now - hours * HOUR, "Red"));
        w.rewrite_lines(|mut line| {
            if line["id"] == id {
                line[flag] = json!(true);
            }
            line
        });
    }
    let key = w.key_of(&newer);

    let matched = w.matched(&key);

    assert_eq!(matched["eligible"], true, "{matched}");
    assert_eq!(precedent_ids(&matched), vec![newer.clone(), older.clone()]);

    w.act("question");
    let named = precedent_ids(&matched);
    let ids: Vec<(usize, &str)> = named.iter().map(|id| (1, id.as_str())).collect();
    let out = w.reply("tu-9", &["Red"], &ids);
    assert_eq!(out["queued"]["kind"], "answer", "{out}");
}

#[test]
fn match_names_at_most_five_precedents() {
    let w = World::new();
    let ids: Vec<String> = (1..=7)
        .map(|days| w.record(picked(w.now - days * DAY, "Red")))
        .collect();

    let matched = w.matched(&w.key_of(&ids[0]));

    assert_eq!(precedent_ids(&matched), ids[..5].to_vec());
}

#[test]
fn match_names_no_precedents_for_a_key_that_is_not_eligible() {
    let w = World::new();
    let (a, _) = two_reds(&w);
    let key = w.key_of(&a);
    w.record(picked(w.now - HOUR, "Blue"));

    let conflicted = w.matched(&key);
    assert_eq!(conflicted["eligible"], false);
    assert_eq!(precedent_ids(&conflicted), Vec::<String>::new());

    let w = World::new();
    let one = w.record(picked(w.now - DAY, "Red"));
    assert_eq!(
        precedent_ids(&w.matched(&w.key_of(&one))),
        Vec::<String>::new()
    );
}

#[test]
fn match_refuses_a_key_that_is_not_a_decision_key() {
    let w = World::new();
    for key in ["", "x1:00", "q1:xyz", "q2:0123456789abcdef0123456789abcdef"] {
        assert!(refused(&w.matched(key)), "{key}");
    }
}

#[test]
fn an_agent_reply_on_eligible_precedents_in_act_is_queued_and_taken() {
    let w = World::new();
    let (a, b) = two_reds(&w);
    w.act("question");

    let out = w.reply("tu-9", &["Red"], &[(1, &a), (1, &b)]);
    assert_eq!(out["queued"]["kind"], "answer", "{out}");

    let taken = w.take(open(json!([asked_color()])));
    assert_eq!(taken["results"][0]["status"], "delivered", "{taken}");
    let action = &taken["actions"][0];
    assert_eq!(action["answers"], json!(["Red"]));
    assert_eq!(action["via"], "agent");
    assert_eq!(
        action["auth"]["precedents"],
        json!([{ "q": 1, "id": a }, { "q": 1, "id": b }])
    );
    assert_eq!(
        action["auth"]["reason"],
        "같은 질문에 두 번 Red 를 골랐어요"
    );
}

#[test]
fn an_agent_reply_while_the_kind_only_watches_is_refused() {
    let w = World::new();
    let (a, b) = two_reds(&w);

    let out = w.reply("tu-9", &["Red"], &[(1, &a), (1, &b)]);
    assert!(
        out["error"]
            .as_str()
            .unwrap_or_default()
            .contains("지켜보기"),
        "{out}"
    );
    assert!(!w.inbox("w").exists());
}

#[test]
fn an_agent_reply_whose_answer_differs_from_its_precedents_is_refused_before_queueing() {
    let w = World::new();
    let (a, b) = two_reds(&w);
    w.act("question");

    let out = w.reply("tu-9", &["Blue"], &[(1, &a), (1, &b)]);
    assert!(
        out["error"]
            .as_str()
            .unwrap_or_default()
            .contains("답과 달라요"),
        "{out}"
    );
    assert!(!w.inbox("w").exists());
}

#[test]
fn the_taker_refuses_an_answer_unlike_its_precedents_or_outside_the_options() {
    let w = World::new();
    let (a, b) = two_reds(&w);
    w.act("question");
    let auth =
        json!({ "precedents": [{ "q": 1, "id": a }, { "q": 1, "id": b }], "reason": "선례" });
    w.leave(
        "w",
        "1-1",
        &json!({ "v": 1, "kind": "answer", "tool_use_id": "tu-9", "answers": ["Blue"], "from": "agent", "auth": auth }),
    );

    let taken = w.take(open(json!([asked_color()])));
    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["status"], "refused", "{taken}");
    assert_reason(&taken["results"][0], "달라요");

    let out = w.reply("tu-9", &["red"], &[(1, &a), (1, &b)]);
    assert_eq!(
        out["queued"]["kind"], "answer",
        "same fingerprint passes the sender: {out}"
    );
    let taken = w.take(open(json!([asked_color()])));
    assert_eq!(taken["results"][0]["status"], "refused", "{taken}");
    assert_reason(&taken["results"][0], "선택지");
}

#[test]
fn a_permission_deny_is_never_sent_or_taken_for_the_agent() {
    let w = World::new();
    let a = w.record(permission_facts(
        REPO,
        w.now - 2 * DAY,
        "git status",
        inbox_verdict("pane", "deny"),
    ));
    let b = w.record(permission_facts(
        REPO,
        w.now - DAY,
        "git status",
        inbox_verdict("pane", "deny"),
    ));
    w.act("permission");
    assert_eq!(w.matched(&w.key_of(&a))["eligible"], true);

    let out = w.permit("p-1", "deny", &[(1, &a), (1, &b)]);
    assert!(refused(&out), "{out}");

    w.leave_permit(
        "w",
        "p-1",
        &json!({ "v": 1, "kind": "permit", "request_id": "p-1", "decision": "deny", "from": "agent",
                "auth": { "precedents": [{ "q": 1, "id": a }, { "q": 1, "id": b }], "reason": "선례" } }),
    );
    let out = w.wait_permit("p-1", "--wait-stdin", bash_wait(REPO, "git status"));
    assert_eq!(out["decision"], Value::Null, "{out}");
    assert_eq!(out["status"], "refused");
    assert_reason(&out, "허용만");
}

#[test]
fn an_agent_allow_on_eligible_precedents_is_taken_against_the_whole_request() {
    let w = World::new();
    let a = w.record(permission_facts(REPO, w.now - 2 * DAY, "git status", ran()));
    let b = w.record(permission_facts(REPO, w.now - DAY, "git status", ran()));
    w.act("permission");

    let out = w.permit("p-1", "allow", &[(1, &a), (1, &b)]);
    assert_eq!(out["queued"]["kind"], "permit", "{out}");
    let out = w.wait_permit("p-1", "--wait-stdin", bash_wait(REPO, "git status"));
    assert_eq!(out["decision"], "allow", "{out}");
    assert_eq!(out["via"], "agent");
    assert_eq!(out["auth"]["precedents"][0]["id"], json!(a));

    let out = w.permit("p-2", "allow", &[(1, &a), (1, &b)]);
    assert_eq!(out["queued"]["kind"], "permit");
    let out = w.wait_permit("p-2", "--nonces-stdin", json!({ "nonces": [] }));
    assert_eq!(
        out["status"], "refused",
        "without the request the agent is refused: {out}"
    );
    assert_reason(&out, "넘기지 않았어요");

    w.permit("p-3", "allow", &[(1, &a), (1, &b)]);
    let out = w.wait_permit("p-3", "--wait-stdin", bash_wait(REPO, "git log"));
    assert_eq!(out["status"], "refused", "another key: {out}");
    assert_reason(&out, "대기와 달라요");
}

#[test]
fn every_question_needs_its_own_precedents_of_its_own_key() {
    let w = World::new();
    let two = color_and_size();
    let mut colors = Vec::new();
    let mut sizes = Vec::new();
    for at in [w.now - 2 * DAY, w.now - DAY] {
        let ids = w.record_all(question_facts(at, two.clone(), chose(json!(["Red", "M"]))));
        colors.push(ids[0].clone());
        sizes.push(ids[1].clone());
    }
    w.act("question");
    let asked_two = json!([asked_color(), asked("Which size?", &["S", "M"])]);

    let ok = w.reply(
        "tu-9",
        &["Red", "M"],
        &[
            (1, &colors[0]),
            (1, &colors[1]),
            (2, &sizes[0]),
            (2, &sizes[1]),
        ],
    );
    assert_eq!(ok["queued"]["kind"], "answer", "{ok}");
    let taken = w.take(open(asked_two.clone()));
    assert_eq!(taken["results"][0]["status"], "delivered", "{taken}");

    let mixed = w.reply(
        "tu-9",
        &["Red", "M"],
        &[(1, &colors[0]), (1, &sizes[0]), (2, &sizes[1])],
    );
    assert!(refused(&mixed), "{mixed}");
    let missing = w.reply("tu-9", &["Red", "M"], &[(1, &colors[0]), (1, &colors[1])]);
    assert!(refused(&missing), "{missing}");

    let swapped = w.reply(
        "tu-9",
        &["M", "Red"],
        &[
            (1, &sizes[0]),
            (1, &sizes[1]),
            (2, &colors[0]),
            (2, &colors[1]),
        ],
    );
    assert_eq!(swapped["queued"]["kind"], "answer", "{swapped}");
    let taken = w.take(open(asked_two));
    assert_eq!(taken["results"][0]["status"], "refused", "{taken}");
    assert_reason(&taken["results"][0], "대기와 달라요");
}

#[test]
fn the_taker_judges_paths_where_they_really_lead() {
    let w = World::new();
    let place = TempDir::new().expect("place");
    let base = fs::canonicalize(place.path()).expect("real");
    let repo = base.join("repo");
    let outside = base.join("outside");
    fs::create_dir_all(repo.join("docs")).expect("docs");
    fs::create_dir_all(&outside).expect("outside");
    fs::write(repo.join("docs/a.md"), "a").expect("a");
    fs::write(outside.join("a.md"), "secret").expect("secret");
    let repo_str = repo.to_str().expect("utf-8");
    let a = w.record(permission_facts(
        repo_str,
        w.now - 2 * DAY,
        "cat docs/a.md",
        ran(),
    ));
    let b = w.record(permission_facts(
        repo_str,
        w.now - DAY,
        "cat docs/a.md",
        ran(),
    ));
    w.act("permission");

    w.permit("p-1", "allow", &[(1, &a), (1, &b)]);
    let out = w.wait_permit("p-1", "--wait-stdin", bash_wait(repo_str, "cat docs/a.md"));
    assert_eq!(out["decision"], "allow", "{out}");

    fs::remove_dir_all(repo.join("docs")).expect("drop docs");
    std::os::unix::fs::symlink(&outside, repo.join("docs")).expect("link");
    w.permit("p-2", "allow", &[(1, &a), (1, &b)]);
    let out = w.wait_permit("p-2", "--wait-stdin", bash_wait(repo_str, "cat docs/a.md"));
    assert_eq!(out["status"], "refused", "{out}");
    assert_reason(&out, "outside_repo");
}

#[test]
fn a_wait_no_precedent_may_answer_is_refused_by_the_taker() {
    let w = World::new();
    let questions = json!([question("Delete the old branch?", &["Yes", "No"], false)]);
    let a = w.record(question_facts(
        w.now - 2 * DAY,
        questions.clone(),
        chose(json!(["Yes"])),
    ));
    let b = w.record(question_facts(
        w.now - DAY,
        questions,
        chose(json!(["Yes"])),
    ));
    w.act("question");

    let out = w.reply("tu-9", &["Yes"], &[(1, &a), (1, &b)]);
    assert_eq!(out["queued"]["kind"], "answer", "{out}");
    let taken = w.take(open(json!([asked(
        "Delete the old branch?",
        &["Yes", "No"]
    )])));
    assert_eq!(taken["results"][0]["status"], "refused", "{taken}");
    assert_reason(&taken["results"][0], "guard:irreversible");
}

#[test]
fn a_taker_that_does_not_hand_over_the_whole_wait_refuses_the_agent() {
    let w = World::new();
    let (a, b) = two_reds(&w);
    w.act("question");

    for open in [
        json!({ "asks": [{ "id": "tu-9", "questions": [asked_color()] }], "decision": null }),
        json!({ "asks": [{ "id": "tu-9", "questions": [{ "question": "Which color?" }] }], "decision": null, "place": place_json(REPO) }),
    ] {
        w.reply("tu-9", &["Red"], &[(1, &a), (1, &b)]);
        let taken = w.take(open.clone());
        assert_eq!(taken["results"][0]["status"], "refused", "{open}: {taken}");
        assert_reason(&taken["results"][0], "넘기지 않았어요");
    }
}

#[test]
fn the_agent_answers_only_with_precedents_and_a_reason() {
    let w = World::new();
    let (a, b) = two_reds(&w);
    w.act("question");
    let now = w.now.to_string();
    let pa = format!("1:{a}");
    let pb = format!("1:{b}");
    let base = [
        "reply",
        "--session",
        "w",
        "--ask",
        "tu-9",
        "--answer",
        "Red",
    ];
    let cases: Vec<Vec<&str>> = vec![
        vec!["--from", "agent", "--reason", "r", "--now", &now],
        vec![
            "--from",
            "agent",
            "--precedent",
            &pa,
            "--precedent",
            &pb,
            "--now",
            &now,
        ],
        vec!["--from", "agent", "--relay", "L1-00000000", "--now", &now],
        vec![
            "--from",
            "agent",
            "--relay",
            "L1-00000000",
            "--precedent",
            &pa,
            "--reason",
            "r",
        ],
        vec!["--from", "pane", "--precedent", &pa, "--reason", "r"],
        vec![
            "--from",
            "agent",
            "--precedent",
            "x",
            "--reason",
            "r",
            "--now",
            &now,
        ],
        vec![
            "--from",
            "agent",
            "--precedent",
            "0:L1-0",
            "--reason",
            "r",
            "--now",
            &now,
        ],
    ];
    for extra in cases {
        let mut args = base.to_vec();
        args.extend(extra.iter());
        let out = w.kanban(&args);
        assert!(refused(&out), "{extra:?} → {out}");
    }
    let long = "가".repeat(201);
    let out = w.kanban(&[
        "reply",
        "--session",
        "w",
        "--ask",
        "tu-9",
        "--answer",
        "Red",
        "--from",
        "agent",
        "--precedent",
        &pa,
        "--precedent",
        &pb,
        "--reason",
        &long,
        "--now",
        &now,
    ]);
    assert!(refused(&out), "{out}");
    let six: Vec<String> = (0..6).map(|_| format!("1:{a}")).collect();
    let mut args = base.to_vec();
    args.extend(["--from", "agent", "--reason", "r", "--now", &now]);
    for p in &six {
        args.extend(["--precedent", p]);
    }
    assert!(refused(&w.kanban(&args)));
    assert!(!w.inbox("w").exists());
}

#[test]
fn the_agent_never_answers_a_decision() {
    let w = World::new();
    let out = w.kanban(&[
        "decide",
        "--session",
        "w",
        "--at",
        "1700",
        "--text",
        "네",
        "--from",
        "agent",
        "--precedent",
        "1:L1-00000000",
        "--reason",
        "r",
    ]);
    assert!(refused(&out), "{out}");
    assert!(!w.inbox("w").exists());
}

#[test]
fn a_proxy_answer_keeps_the_precedents_it_was_sent_on_and_why() {
    let w = World::new();
    let two = color_and_size();
    let ids = w.record_all(question_facts(
        w.now,
        two,
        json!({
            "path": "inbox",
            "via": "agent",
            "answers": ["Red", "M"],
            "auth": {
                "precedents": [{ "q": 1, "id": "L1-aaaa" }, { "q": 2, "id": "L2-bbbb" }, { "q": 2, "id": "L3-cccc" }],
                "reason": "token=abcdef 를 골랐던 선례",
            },
        }),
    ));

    let color = w.entry(&ids[0]);
    assert_eq!(color["decided_by"], "proxy");
    assert_eq!(color["precedents"], json!(["L1-aaaa"]));
    assert_eq!(color["reason"], "token=[masked] 를 골랐던 선례");
    assert_eq!(
        w.entry(&ids[1])["precedents"],
        json!(["L2-bbbb", "L3-cccc"])
    );
}

#[test]
fn only_an_agent_answer_keeps_precedents() {
    let w = World::new();
    let id = w.record(question_facts(
        w.now,
        size(),
        json!({
            "path": "inbox",
            "via": "pane",
            "answers": ["M"],
            "auth": { "precedents": [{ "q": 1, "id": "L1-aaaa" }], "reason": "r" },
        }),
    ));

    assert_eq!(w.entry(&id)["precedents"], json!([]));
    assert_eq!(w.entry(&id)["reason"], Value::Null);
}

#[test]
fn the_ledger_list_skips_revokes_and_settings() {
    let w = World::new();
    let (a, _) = two_reds(&w);
    let proxied = w.record(by_inbox(w.now - HOUR, "agent", "Red"));
    w.revoke(&proxied, w.now);
    w.act("question");

    let out = w.kanban(&["ledger", "list"]);
    let ids: Vec<&str> = out["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| e["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids.len(), 3, "{ids:?}");
    assert!(ids.contains(&a.as_str()));
}

fn assert_reason(refusal: &Value, why: &str) {
    let reason = refusal["reason"].as_str().unwrap_or_default();
    assert!(reason.contains(why), "expected `{why}` in {refusal}");
}
