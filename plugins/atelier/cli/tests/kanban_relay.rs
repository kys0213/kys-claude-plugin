use serde_json::{json, Value};

mod kanban_support;
use kanban_support::{
    asking, asking_bash, asking_permission, chose, color_and_size, decision_waiting, event,
    home_deref, inbox_answer, js_len, place_json, put_record, question, question_lengths, refused,
    run_record, session_with, Home, MIN, NOW, REPO, SEC,
};

/// The delegate session that asks the person on the main conversation.
const MAIN: &str = "m";
const MAIN_ASK: &str = "tu-m";
const PERMIT: &str = "t-1-7";
/// Long enough that no request left in a test's inbox is taken as expired,
/// whatever the file's time against `NOW`.
const TTL: &str = "100000d";

struct World {
    home: Home,
    worker: Value,
}

home_deref!(World);

impl World {
    fn new(worker: Value) -> World {
        let w = World {
            home: Home::default(),
            worker,
        };
        put_record(w.config(), &w.worker);
        put_record(
            w.config(),
            &session_with(
                MAIN,
                json!([event("session.start", None, MIN)]),
                json!({ "cwd": "/work/main", "delegate_since": NOW - 10 * MIN }),
            ),
        );
        w
    }

    /// The worker's record copy as its mod keeps it alive until `at`.
    fn beat(&self, at: u64) {
        let mut worker = self.worker.clone();
        worker["heartbeat_at"] = json!(at - SEC);
        put_record(self.config(), &worker);
    }

    fn frame(&self, questions: Value) -> Value {
        let now = NOW.to_string();
        self.run(
            &["relay-frame", "--now", &now, "--session", MAIN],
            &json!({ "questions": questions }).to_string(),
        )
    }

    /// What the delegate session's mod records once the person answered its
    /// framed question: `answer` how it came in.
    fn record_relay(&self, framed: &Value, at: u64, answer: Value) -> Vec<String> {
        let questions: Vec<Value> = framed["questions"]
            .as_array()
            .expect("framed questions")
            .iter()
            .map(|q| {
                json!({
                    "question": q["question"],
                    "header": q["header"],
                    "multi_select": q["multiSelect"],
                    "options": q["options"]
                        .as_array()
                        .expect("options")
                        .iter()
                        .map(|o| json!({ "label": o["label"], "description": o["description"] }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        let questions = Value::Array(questions);
        let facts = json!({
            "at": at,
            "session": MAIN,
            "repo_root": "/work/main",
            "cwd": "/work/main",
            "permission_mode": "default",
            "question": {
                "tool_use_id": MAIN_ASK,
                "questions": questions,
                "at": at - 5 * SEC,
                "orig_len": question_lengths(&questions),
            },
            "relay_frame": framed["target"],
            "answer": answer,
        });
        self.record_all(facts)
    }

    fn relayed(&self, questions: Value, labels: &[&str], at: u64) -> String {
        let framed = self.frame(questions);
        assert!(framed["target"].is_object(), "framed: {framed}");
        let ids = self.record_relay(&framed, at, chose(json!(labels)));
        ids.first()
            .unwrap_or_else(|| panic!("a relay entry, got {ids:?}"))
            .clone()
    }

    fn relays(&self, at: u64) -> Value {
        self.beat(at);
        let out = self.status_at(at, &["--session", "w"]);
        out["sessions"][0]["wait"]["relays"].clone()
    }

    fn send(&self, args: &[&str], relay: &str, at: u64) -> Value {
        let now = at.to_string();
        let mut full = args.to_vec();
        full.extend(["--from", "agent", "--relay", relay, "--now", &now]);
        self.kanban(&full)
    }

    fn reply(&self, session: &str, ask: &str, answers: &[&str], relay: &str, at: u64) -> Value {
        let mut args = vec!["reply", "--session", session, "--ask", ask];
        for answer in answers {
            args.extend(["--answer", answer]);
        }
        self.send(&args, relay, at)
    }

    fn permit(&self, decision: &str, relay: &str, at: u64) -> Value {
        self.send(
            &[
                "permit",
                "--session",
                "w",
                "--request",
                PERMIT,
                "--decision",
                decision,
            ],
            relay,
            at,
        )
    }

    fn take(&self, open: Value, at: u64) -> Value {
        self.home.take("w", at, TTL, None, Some(&open))
    }

    fn wait_permit(&self, at: u64) -> Value {
        let facts = json!({
            "nonces": [],
            "place": place_json(REPO),
            "permission": { "tool": "Bash", "input": { "command": "git status" } },
        });
        self.home
            .wait_permit("w", PERMIT, at, TTL, Some(("--wait-stdin", &facts)))
    }
}

/// A question the main agent writes in the tool's own spelling.
fn ask(text: &str, labels: &[&str]) -> Value {
    json!({
        "question": text,
        "header": "칸반",
        "multiSelect": false,
        "options": labels
            .iter()
            .map(|l| json!({ "label": l, "description": format!("{l} 고르기") }))
            .collect::<Vec<_>>(),
    })
}

fn marked(session: &str, wait: &str, body: &str) -> String {
    format!("[kanban-wait {session} {wait}]\n{body}")
}

fn labels(question: &Value) -> Vec<String> {
    question["options"]
        .as_array()
        .expect("options")
        .iter()
        .map(|o| o["label"].as_str().expect("label").to_string())
        .collect()
}

fn text_of(question: &Value) -> String {
    question["question"].as_str().expect("text").to_string()
}

fn color_worker() -> Value {
    asking(json!([question("Which color?", &["Red", "Blue"], false)]))
}

fn color_ask(body: &str) -> Value {
    json!([ask(&marked("w", "tu-1", body), &["Red", "Blue"])])
}

fn blue_recommended() -> Value {
    json!([ask(&marked("w", "tu-1", "추천은 Blue"), &["Blue", "Red"])])
}

fn bash_ask() -> Value {
    json!([ask(
        &marked("w", PERMIT, "git status 를 허용할까요?"),
        &["허용", "거부"]
    )])
}

fn deciding_worker() -> Value {
    decision_waiting("Ship it?", Some("Tests pass."), &["Ship", "Wait"])
}

fn decision_id() -> String {
    (NOW - 10 * SEC).to_string()
}

fn queued(out: &Value) -> bool {
    out["queued"].is_object()
}

fn relayed_answer(ask: &str, answer: &str, relay: &str) -> Value {
    json!({
        "v": 1, "kind": "answer", "tool_use_id": ask, "answers": [answer], "from": "agent",
        "auth": { "relay": relay },
    })
}

fn open_color(id: &str) -> Value {
    json!({
        "asks": [{ "id": id, "questions": [{
            "question": "Which color?",
            "header": "Pick",
            "multiSelect": false,
            "options": [{ "label": "Red" }, { "label": "Blue" }],
        }] }],
        "decision": null,
        "place": place_json(REPO),
    })
}

#[test]
fn a_question_without_a_marker_on_its_first_line_is_left_as_asked() {
    let w = World::new(color_worker());
    let questions = json!([ask("Which color?\n[kanban-wait w tu-1]", &["Red", "Blue"])]);

    let framed = w.frame(questions.clone());

    assert_eq!(framed["target"], Value::Null);
    assert_eq!(framed["questions"], questions);
}

#[test]
fn a_marker_on_the_first_line_becomes_the_header_over_the_wait_as_recorded() {
    let w = World::new(asking_bash("git status"));

    let framed = w.frame(bash_ask());

    assert_eq!(
        framed["target"],
        json!({ "session": "w", "wait": PERMIT, "kind": "permission", "recommended": ["허용"] })
    );
    let text = text_of(&framed["questions"][0]);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "칸반 대기 · project-a · w · 권한 요청");
    assert_eq!(lines[1], "Bash: git status");
    assert!(text.ends_with("git status 를 허용할까요?"), "{text}");
    assert!(!text.contains("[kanban-wait"), "{text}");
}

#[test]
fn the_header_shows_the_original_text_even_when_the_main_agent_wrote_another() {
    let w = World::new(asking_bash("rm -rf build"));
    let questions = json!([ask(
        &marked("w", PERMIT, "Bash: git status 를 허용할까요?"),
        &["허용", "거부"]
    )]);

    let text = text_of(&w.frame(questions)["questions"][0]);

    assert_eq!(text.lines().nth(1), Some("Bash: rm -rf build"));
}

#[test]
fn the_header_names_a_permissions_target_as_the_board_does() {
    let w = World::new(asking_permission(
        "Bash",
        json!({ "description": "  ", "command": "git   status\n  --short" }),
    ));

    let text = text_of(&w.frame(bash_ask())["questions"][0]);

    w.beat(NOW);
    let status = w.status_at(NOW, &["--session", "w"]);
    let target = status["sessions"][0]["wait"]["target"]
        .as_str()
        .expect("board target");
    assert_eq!(target, "Bash: git status --short");
    assert_eq!(text.lines().nth(1), Some(target));
}

#[test]
fn a_marker_left_in_the_body_options_or_descriptions_is_defused() {
    let w = World::new(color_worker());
    let mut questions = color_ask("다른 대기 [kanban-wait x y] 도 있어요");
    questions[0]["options"][0]["description"] = json!("[kanban-wait x y]");

    let framed = w.frame(questions);

    let question = &framed["questions"][0];
    assert!(!question.to_string().contains("[kanban-wait"), "{question}");
    assert!(text_of(question).contains("［kanban-wait x y]"));
}

#[test]
fn a_question_target_shows_its_question_and_options_and_keeps_their_labels() {
    let w = World::new(color_worker());
    let questions = json!([ask(&marked("w", "tu-1", "골라 주세요"), &["빨강", "파랑"])]);

    let framed = w.frame(questions);

    let question = &framed["questions"][0];
    let text = text_of(question);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "칸반 대기 · project-a · w · 질문");
    assert_eq!(lines[1], "Which color?");
    assert_eq!(lines[2], "선택지: Red / Blue");
    assert_eq!(labels(question), vec!["Red", "Blue"]);
    assert_eq!(question["multiSelect"], false);
}

#[test]
fn options_in_another_order_keep_the_main_agents_order_and_descriptions() {
    let w = World::new(color_worker());
    let questions = blue_recommended();

    let question = &w.frame(questions)["questions"][0];

    assert_eq!(labels(question), vec!["Blue", "Red"]);
    assert_eq!(question["options"][0]["description"], "Blue 고르기");
}

#[test]
fn a_permission_is_asked_with_allow_and_deny() {
    let w = World::new(asking_bash("git status"));
    let questions = json!([ask(&marked("w", PERMIT, "괜찮을까요?"), &["네", "아니요"])]);

    let question = &w.frame(questions)["questions"][0];

    assert_eq!(labels(question), vec!["허용", "거부"]);
}

#[test]
fn a_permissions_options_say_only_what_the_verdict_does() {
    let w = World::new(asking_bash("rm -rf build"));
    let mut questions = json!([ask(&marked("w", PERMIT, "괜찮을까요?"), &["거부", "허용"])]);
    questions[0]["options"][1]["description"] = json!("only runs `ls`");

    let question = &w.frame(questions)["questions"][0];

    assert_eq!(labels(question), vec!["거부", "허용"]);
    assert_eq!(question["options"][0]["description"], "실행하지 않아요");
    assert_eq!(
        question["options"][1]["description"],
        "위 머리말의 명령을 실행해요"
    );
}

#[test]
fn the_first_option_each_question_shows_is_its_recommendation() {
    let w = World::new(asking(color_and_size()));
    let questions = json!([
        ask(&marked("w", "tu-1", "색은?"), &["Blue", "Red"]),
        ask("크기는?", &["X", "Y"]),
    ]);

    let framed = w.frame(questions);

    assert_eq!(framed["target"]["recommended"], json!(["Blue", "S"]));
}

#[test]
fn a_cut_target_is_marked_in_the_header_with_its_lengths() {
    let command = format!("cat {}", "a".repeat(300));
    let mut worker = asking_bash(&command[..200]);
    worker["permission"]["input_len"]["command"] = json!(js_len(&command));
    let w = World::new(worker);

    let text = text_of(&w.frame(bash_ask())["questions"][0]);

    assert_eq!(
        text.lines().next(),
        Some("칸반 대기 · project-a · w · 권한 요청 (잘림 — 원래 304자 중 앞 200자)")
    );
}

#[test]
fn a_marker_on_the_second_question_binds_nothing() {
    let w = World::new(color_worker());
    let questions = json!([
        ask("Which size?", &["S", "M"]),
        ask(&marked("w", "tu-1", "색은?"), &["Red", "Blue"]),
    ]);

    assert_eq!(w.frame(questions)["target"], Value::Null);
}

#[test]
fn a_question_count_other_than_the_waits_binds_nothing() {
    let w = World::new(asking(color_and_size()));

    let framed = w.frame(color_ask("색은?"));

    assert_eq!(framed["target"], Value::Null);
}

#[test]
fn a_marker_naming_no_wait_is_an_error() {
    let w = World::new(color_worker());

    for questions in [
        json!([ask(&marked("w", "tu-404", "?"), &["A", "B"])]),
        json!([ask(&marked("nobody", "tu-1", "?"), &["A", "B"])]),
    ] {
        assert!(refused(&w.frame(questions.clone())), "{questions}");
    }
}

#[test]
fn a_person_answering_a_framed_question_leaves_a_relay_bound_to_the_wait() {
    let w = World::new(color_worker());

    let id = w.relayed(color_ask("색은?"), &["Blue"], NOW);

    let entry = w.entry(&id);
    assert_eq!(entry["session"], MAIN);
    assert_eq!(entry["wait_id"], MAIN_ASK);
    assert_eq!(entry["target_session"], "w");
    assert_eq!(entry["target_wait"], "tu-1");
    assert_eq!(entry["decided_by"], "human");
    assert_eq!(entry["via"], "session");
    assert_eq!(entry["key"], Value::Null);
    assert_eq!(
        w.relays(NOW + MIN),
        json!([{ "id": id, "answer": ["Blue"], "at": NOW }])
    );
}

#[test]
fn a_framed_question_answered_on_the_pane_leaves_a_relay() {
    let w = World::new(color_worker());
    let framed = w.frame(color_ask("색은?"));

    let ids = w.record_relay(&framed, NOW, inbox_answer("pane", "Red"));

    assert_eq!(ids.len(), 1);
    assert_eq!(w.entry(&ids[0])["via"], "pane");
}

#[test]
fn a_framed_question_no_person_answered_leaves_no_relay() {
    let w = World::new(color_worker());
    let framed = w.frame(color_ask("색은?"));

    for answer in [
        inbox_answer("agent", "Red"),
        inbox_answer("terminal", "Red"),
        json!({ "path": "session", "answers": ["Red"], "afk_timeout_ms": 60000, "is_error": false, "denied": false }),
    ] {
        assert_eq!(
            w.record_relay(&framed, NOW, answer.clone()),
            Vec::<String>::new(),
            "{answer}"
        );
    }
    assert_eq!(w.relays(NOW + MIN), json!([]));
}

#[test]
fn a_permission_relay_keeps_the_verdict_the_label_stands_for() {
    let w = World::new(asking_bash("git status"));

    let id = w.relayed(bash_ask(), &["허용"], NOW);

    assert_eq!(w.entry(&id)["answer"], "allow");
}

#[test]
fn a_relay_carries_the_answer_the_person_gave_to_the_wait_it_names() {
    let w = World::new(color_worker());
    let id = w.relayed(color_ask("색은?"), &["Blue"], NOW);

    assert!(refused(&w.reply("w", "tu-1", &["Red"], &id, NOW + MIN)));
    assert!(refused(&w.reply("w", "tu-9", &["Blue"], &id, NOW + MIN)));
    assert!(refused(&w.reply("x", "tu-1", &["Blue"], &id, NOW + MIN)));
    assert!(refused(&w.reply(
        "w",
        "tu-1",
        &["Blue"],
        "L1-00000000",
        NOW + MIN
    )));
    assert!(queued(&w.reply("w", "tu-1", &["Blue"], &id, NOW + MIN)));
}

#[test]
fn a_relay_is_good_for_30_minutes() {
    let w = World::new(color_worker());
    let id = w.relayed(color_ask("색은?"), &["Blue"], NOW);

    assert!(queued(&w.reply(
        "w",
        "tu-1",
        &["Blue"],
        &id,
        NOW + 30 * MIN
    )));
    assert!(refused(&w.reply(
        "w",
        "tu-1",
        &["Blue"],
        &id,
        NOW + 31 * MIN
    )));
    assert_eq!(w.relays(NOW + 31 * MIN), json!([]));
}

#[test]
fn an_entry_that_is_no_relay_cannot_be_sent_on() {
    let w = World::new(color_worker());
    let facts = json!({
        "at": NOW, "session": "w", "repo_root": REPO, "cwd": REPO, "permission_mode": null,
        "question": {
            "tool_use_id": "tu-1",
            "questions": [question("Which color?", &["Red", "Blue"], false)],
            "at": NOW - 10 * SEC,
            "orig_len": { "question": [12], "labels": [[3, 4]] },
        },
        "answer": chose(json!(["Blue"])),
    });
    let plain = w.record(facts);

    assert!(refused(&w.reply("w", "tu-1", &["Blue"], &plain, NOW + MIN)));
}

#[test]
fn an_agent_answer_names_no_recommendation_of_its_own() {
    let w = World::new(color_worker());
    let id = w.relayed(color_ask("색은?"), &["Blue"], NOW);
    let now = (NOW + MIN).to_string();

    w.command()
        .args([
            "reply",
            "--session",
            "w",
            "--ask",
            "tu-1",
            "--answer",
            "Blue",
            "--from",
            "agent",
            "--relay",
            &id,
            "--recommended",
            "Blue",
            "--now",
            &now,
        ])
        .assert()
        .stdout("")
        .stderr(predicates::str::contains(
            "unexpected argument '--recommended'",
        ));
    assert!(!w.inbox("w").exists());
}

#[test]
fn a_permission_relay_sends_the_verdict_its_label_stands_for() {
    let w = World::new(asking_bash("git status"));
    let id = w.relayed(bash_ask(), &["허용"], NOW);

    assert!(refused(&w.permit("deny", &id, NOW + MIN)));
    assert!(queued(&w.permit("allow", &id, NOW + MIN)));
}

#[test]
fn a_decision_is_answered_with_the_text_the_person_chose() {
    let w = World::new(deciding_worker());
    let at = decision_id();
    let questions = json!([ask(&marked("w", &at, "배포할까요?"), &["Ship", "Wait"])]);
    let id = w.relayed(questions, &["Ship"], NOW);
    let decide = |text: &str| {
        w.send(
            &["decide", "--session", "w", "--at", &at, "--text", text],
            &id,
            NOW + MIN,
        )
    };

    assert!(refused(&decide("Wait")));
    assert!(queued(&decide("Ship")));
    let taken = w.take(
        json!({ "asks": [], "decision": { "at": NOW - 10 * SEC }, "place": place_json(REPO) }),
        NOW + MIN,
    );
    assert_eq!(taken["actions"][0]["kind"], "decide");
    assert_eq!(taken["actions"][0]["text"], "Ship");
    assert_eq!(taken["actions"][0]["auth"], json!({ "relay": id }));
}

#[test]
fn a_decision_is_still_never_answered_on_precedents() {
    let w = World::new(deciding_worker());
    w.leave(
        "w",
        "1-1",
        &json!({
            "v": 1, "kind": "decide", "decision_at": decision_id(), "text": "Ship", "from": "agent",
            "auth": { "precedents": [{ "q": 1, "id": "L1-aaaa" }], "reason": "r" },
        }),
    );

    let taken = w.take(
        json!({ "asks": [], "decision": { "at": NOW - 10 * SEC }, "place": place_json(REPO) }),
        NOW + MIN,
    );

    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["status"], "refused");
}

#[test]
fn the_waiting_session_takes_a_relayed_answer_once() {
    let w = World::new(color_worker());
    let id = w.relayed(color_ask("색은?"), &["Blue"], NOW);
    assert!(queued(&w.reply("w", "tu-1", &["Blue"], &id, NOW + MIN)));
    w.leave("w", "9-9", &relayed_answer("tu-1", "Blue", &id));

    let taken = w.take(open_color("tu-1"), NOW + MIN);

    assert_eq!(taken["actions"].as_array().map(Vec::len), Some(1));
    assert_eq!(taken["actions"][0]["answers"], json!(["Blue"]));
    assert_eq!(taken["actions"][0]["via"], "agent");
    assert_eq!(taken["actions"][0]["auth"], json!({ "relay": id }));
    let statuses: Vec<&Value> = taken["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| &r["status"])
        .collect();
    assert!(statuses.contains(&&json!("delivered")));
    assert!(statuses.contains(&&json!("stale")) || statuses.contains(&&json!("refused")));
    assert_eq!(w.relays(NOW + MIN), json!([]));
    assert!(refused(&w.reply(
        "w",
        "tu-1",
        &["Blue"],
        &id,
        NOW + 2 * MIN
    )));
}

#[test]
fn a_used_relay_left_again_in_the_inbox_is_refused() {
    let w = World::new(color_worker());
    let id = w.relayed(color_ask("색은?"), &["Blue"], NOW);
    w.leave("w", "1-1", &relayed_answer("tu-1", "Blue", &id));
    w.take(open_color("tu-1"), NOW + MIN);
    w.leave("w", "2-1", &relayed_answer("tu-1", "Blue", &id));

    let taken = w.take(open_color("tu-1"), NOW + 2 * MIN);

    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["status"], "refused");
}

#[test]
fn a_relay_left_for_another_wait_is_refused_by_the_session_that_takes_it() {
    let w = World::new(color_worker());
    let id = w.relayed(color_ask("색은?"), &["Blue"], NOW);
    w.leave("w", "1-1", &relayed_answer("tu-2", "Blue", &id));

    let taken = w.take(open_color("tu-2"), NOW + MIN);

    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["status"], "refused");
    assert_eq!(w.relays(NOW + MIN).as_array().map(Vec::len), Some(1));
}

#[test]
fn a_relay_older_than_30_minutes_is_refused_by_the_session_that_takes_it() {
    let w = World::new(color_worker());
    let id = w.relayed(color_ask("색은?"), &["Blue"], NOW);
    w.leave("w", "1-1", &relayed_answer("tu-1", "Blue", &id));

    let taken = w.take(open_color("tu-1"), NOW + 31 * MIN);

    assert_eq!(taken["results"][0]["status"], "refused");
}

#[test]
fn a_permission_wait_takes_a_relayed_allow_and_spends_the_relay() {
    let w = World::new(asking_bash("git status"));
    let id = w.relayed(bash_ask(), &["허용"], NOW);
    assert!(queued(&w.permit("allow", &id, NOW + MIN)));

    let out = w.wait_permit(NOW + MIN);

    assert_eq!(out["decision"], "allow");
    assert_eq!(out["via"], "agent");
    assert_eq!(out["auth"], json!({ "relay": id }));
    assert!(refused(&w.permit("allow", &id, NOW + 2 * MIN)));
}

#[test]
fn a_relayed_deny_is_taken_as_well() {
    let w = World::new(asking_permission(
        "Write",
        json!({ "file_path": "/etc/hosts", "content": "x" }),
    ));
    let id = w.relayed(bash_ask(), &["거부"], NOW);
    assert!(queued(&w.permit("deny", &id, NOW + MIN)));

    let out = w.wait_permit(NOW + MIN);

    assert_eq!(out["decision"], "deny");
}

fn took_facts(relay: &str, answer: &str, at: u64) -> Value {
    json!({
        "at": at, "session": "w", "repo_root": REPO, "cwd": REPO, "permission_mode": null,
        "question": {
            "tool_use_id": "tu-1",
            "questions": [question("Which color?", &["Red", "Blue"], false)],
            "at": NOW - 10 * SEC,
            "orig_len": { "question": [12], "labels": [[3, 4]] },
        },
        "answer": {
            "path": "inbox", "via": "agent", "answers": [answer],
            "auth": { "relay": relay },
        },
    })
}

impl World {
    fn took(&self, relay: &str, answer: &str, at: u64) -> Value {
        let id = self.record(took_facts(relay, answer, at));
        self.entry(&id)
    }
}

#[test]
fn a_person_picking_the_first_option_shown_approves_the_recommendation() {
    let w = World::new(color_worker());
    let questions = blue_recommended();

    let id = w.relayed(questions, &["Blue"], NOW);

    let relay = w.entry(&id);
    assert_eq!(relay["recommended"], "Blue");
    assert_eq!(relay["approved"], true);
    let entry = w.took(&id, "Blue", NOW + MIN);
    assert_eq!(entry["decided_by"], "human");
    assert_eq!(entry["via"], "agent");
    assert_eq!(entry["relay"], id);
    assert_eq!(entry["recommended"], "Blue");
    assert_eq!(entry["approved"], true);
    assert_eq!(entry["precedents"], json!([]));
}

#[test]
fn a_person_picking_another_option_decides_directly() {
    let w = World::new(color_worker());
    let questions = blue_recommended();

    let id = w.relayed(questions, &["Red"], NOW);

    assert_eq!(w.entry(&id)["approved"], false);
    let entry = w.took(&id, "Red", NOW + MIN);
    assert_eq!(entry["recommended"], "Blue");
    assert_eq!(entry["approved"], false);
}

#[test]
fn a_question_asked_with_no_recommendation_still_counts_its_first_option_as_one() {
    let w = World::new(color_worker());

    let id = w.relayed(color_ask("색은?"), &["Red"], NOW);

    assert_eq!(w.took(&id, "Red", NOW + MIN)["approved"], true);
}

#[test]
fn an_approved_relay_counts_as_an_approved_recommendation_for_its_key() {
    let w = World::new(color_worker());
    let questions = blue_recommended();
    let id = w.relayed(questions, &["Blue"], NOW);
    let key = w.took(&id, "Blue", NOW + MIN)["key"]
        .as_str()
        .expect("key")
        .to_string();

    let matched = w.ledger_match(&key, NOW + 2 * MIN);

    assert_eq!(matched["support"], json!({ "direct": 0, "approved": 1 }));
}

#[test]
fn an_answer_taken_on_a_relay_the_ledger_lacks_is_not_recorded() {
    let w = World::new(color_worker());

    let out = run_record(w.config(), &took_facts("L1-abcd", "Blue", NOW + MIN));

    assert!(refused(&out), "{out}");
    let listed = w.kanban(&["ledger", "list"]);
    assert_eq!(listed["entries"], json!([]));
}
