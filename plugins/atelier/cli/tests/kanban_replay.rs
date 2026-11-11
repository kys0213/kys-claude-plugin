use serde_json::{json, Value};
use std::fs;
use std::ops::Deref;

mod kanban_support;
use kanban_support::{js_len, now_ms, question, question_lengths, Home, REPO, SEC};

const HOUR: u64 = 60 * 60 * SEC;
const DAY: u64 = 24 * HOUR;

struct World {
    home: Home,
    now: u64,
    /// Hours before `now` of the next decision, so every decision has a time of its own.
    next: std::cell::Cell<u64>,
}

impl Deref for World {
    type Target = Home;

    fn deref(&self) -> &Home {
        &self.home
    }
}

impl World {
    fn new() -> World {
        World {
            home: Home::default(),
            now: now_ms(),
            next: std::cell::Cell::new(1_000),
        }
    }

    fn tick(&self) -> u64 {
        let hours = self.next.get();
        self.next.set(hours - 1);
        self.now - hours * HOUR
    }

    fn record(&self, facts: Value) -> String {
        let out = self.run(&["ledger", "record"], &facts.to_string());
        let ids = out["recorded"]
            .as_array()
            .unwrap_or_else(|| panic!("recorded ids, got {out}"));
        assert_eq!(ids.len(), 1, "one entry: {out}");
        ids[0].as_str().expect("id").to_string()
    }

    /// A person's picks of `labels` for the question `text`, one after another.
    fn picks(&self, text: &str, labels: &[&str]) -> Vec<String> {
        labels
            .iter()
            .map(|label| {
                let at = self.tick();
                self.record(question_facts(at, text, chose(label)))
            })
            .collect()
    }

    fn answer_by(&self, text: &str, answer: Value) -> String {
        let at = self.tick();
        self.record(question_facts(at, text, answer))
    }

    fn rewrite(&self, id: &str, fields: Value) {
        let dir = self.config().join("atelier/kanban/ledger");
        for file in fs::read_dir(dir).expect("ledger dir") {
            let path = file.expect("file").path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let lines: Vec<String> = fs::read_to_string(&path)
                .expect("read")
                .lines()
                .map(|line| {
                    let mut value: Value = serde_json::from_str(line).expect("line");
                    if value["id"] == id {
                        for (key, field) in fields.as_object().expect("fields") {
                            value[key] = field.clone();
                        }
                    }
                    value.to_string()
                })
                .collect();
            fs::write(&path, lines.join("\n") + "\n").expect("write");
        }
    }

    fn stats(&self) -> Value {
        let now = self.now.to_string();
        self.kanban(&["ledger", "stats", "--now", &now])
    }

    fn question_stats(&self) -> Value {
        self.stats()["kinds"]["question"].clone()
    }
}

fn question_facts(at: u64, text: &str, answer: Value) -> Value {
    let questions = json!([question(text, &["Red", "Blue"], false)]);
    json!({
        "at": at,
        "session": "w",
        "repo_root": REPO,
        "cwd": REPO,
        "permission_mode": "default",
        "question": {
            "tool_use_id": format!("tu-{at}"),
            "questions": questions,
            "at": at - 10 * SEC,
            "orig_len": question_lengths(&questions),
        },
        "answer": answer,
    })
}

fn chose(label: &str) -> Value {
    json!({ "path": "session", "answers": [label], "afk_timeout_ms": null, "is_error": false, "denied": false })
}

fn inbox(via: &str, label: &str) -> Value {
    json!({ "path": "inbox", "via": via, "answers": [label] })
}

const RED: &str = "Red";
const BLUE: &str = "Blue";

#[test]
fn every_persons_decision_after_two_agreeing_ones_is_compared_whatever_way_it_came() {
    let w = World::new();
    w.picks("Which color?", &[RED, RED]);
    w.answer_by("Which color?", inbox("pane", RED));
    w.picks("Which color?", &[RED]);
    let relayed = w.answer_by("Which color?", inbox("agent", RED));
    w.rewrite(
        &relayed,
        json!({ "decided_by": "human", "relay": "L1-00000000" }),
    );

    let stats = w.question_stats();

    assert_eq!(stats["compared"], json!(3), "{stats}");
    assert_eq!(stats["agreed"], json!(3));
    assert_eq!(stats["progress"], json!("3/10"));
    assert_eq!(stats["mode"], json!("shadow"));
}

#[test]
fn proxy_and_terminal_answers_are_not_compared() {
    let w = World::new();
    w.picks("Which color?", &[RED, RED]);
    w.answer_by("Which color?", inbox("agent", RED));
    w.answer_by("Which color?", inbox("terminal", BLUE));

    let stats = w.question_stats();

    assert_eq!(stats["compared"], json!(0), "{stats}");
}

#[test]
fn a_decision_the_precedents_would_have_answered_otherwise_is_a_disagreement() {
    let w = World::new();
    w.picks("Which color?", &[RED, RED, BLUE]);

    let stats = w.question_stats();

    assert_eq!(stats["compared"], json!(1), "{stats}");
    assert_eq!(stats["agreed"], json!(0));
    assert_eq!(stats["disagreed"], json!(1));
}

#[test]
fn ten_agreeing_comparisons_recommend_promotion_and_nine_do_not() {
    let nine = World::new();
    nine.picks("Which color?", &[RED; 11]);
    let ten = World::new();
    ten.picks("Which color?", &[RED; 12]);

    let nine = nine.question_stats();
    let ten = ten.question_stats();

    assert_eq!(nine["compared"], json!(9), "{nine}");
    assert_eq!(nine["promotable"], json!(false));
    assert_eq!(ten["compared"], json!(10), "{ten}");
    assert_eq!(ten["recent5_agreed"], json!(5));
    assert_eq!(ten["promotable"], json!(true));
}

#[test]
fn one_early_disagreement_still_recommends_promotion() {
    let w = World::new();
    w.picks("Which size?", &[RED, RED, BLUE]);
    w.picks("Which color?", &[RED; 11]);

    let stats = w.question_stats();

    assert_eq!(stats["compared"], json!(10), "{stats}");
    assert_eq!(stats["disagreed"], json!(1));
    assert_eq!(stats["promotable"], json!(true));
}

#[test]
fn two_disagreements_do_not_recommend_promotion() {
    let w = World::new();
    w.picks("Which size?", &[RED, RED, BLUE]);
    w.picks("Which shape?", &[RED, RED, BLUE]);
    w.picks("Which color?", &[RED; 10]);

    let stats = w.question_stats();

    assert_eq!(stats["compared"], json!(10), "{stats}");
    assert_eq!(stats["disagreed"], json!(2));
    assert_eq!(stats["promotable"], json!(false));
}

#[test]
fn a_disagreement_among_the_last_five_does_not_recommend_promotion() {
    let w = World::new();
    w.picks("Which color?", &[RED; 11]);
    w.picks("Which size?", &[RED, RED, BLUE]);

    let stats = w.question_stats();

    assert_eq!(stats["compared"], json!(10), "{stats}");
    assert_eq!(stats["disagreed"], json!(1));
    assert_eq!(stats["recent5_agreed"], json!(4));
    assert_eq!(stats["promotable"], json!(false));
}

#[test]
fn a_comparison_older_than_ninety_days_is_not_counted() {
    let w = World::new();
    w.next.set(95 * 24);
    w.picks("Which color?", &[RED, RED, RED]);

    let stats = w.question_stats();

    assert_eq!(stats["compared"], json!(0), "{stats}");
}

#[test]
fn decisions_are_compared_though_they_never_act() {
    let w = World::new();
    for _ in 0..3 {
        let at = w.tick();
        let ask = "초안대로 진행할까요?";
        let options = ["초안대로 진행", "다시 쓰기"];
        w.record(json!({
            "at": at,
            "session": "w",
            "repo_root": REPO,
            "cwd": REPO,
            "permission_mode": "default",
            "decision": {
                "at": at - 10 * SEC,
                "ask": ask,
                "recap": "초안을 정리했어요.",
                "options": options,
                "orig_len": { "ask": js_len(ask), "options": options.map(js_len) },
            },
            "answer": { "path": "session", "text": "초안대로 진행", "origin": "composer" },
        }));
    }

    let stats = w.stats()["kinds"]["decision"].clone();

    assert_eq!(stats["compared"], json!(1), "{stats}");
    assert_eq!(stats["agreed"], json!(1));
    assert_eq!(stats["mode"], json!("shadow"));
}

fn add_line(w: &World, line: Value) {
    let dir = w.config().join("atelier/kanban/ledger");
    fs::create_dir_all(&dir).expect("ledger dir");
    let path = dir.join("2026-01.jsonl");
    let mut contents = fs::read_to_string(&path).unwrap_or_default();
    contents.push_str(&format!("{line}\n"));
    fs::write(path, contents).expect("write");
}

#[test]
fn a_kind_two_revokes_stepped_back_names_why() {
    let w = World::new();
    add_line(
        &w,
        json!({ "v": 1, "type": "setting", "id": "S1", "at": w.now - 10 * DAY, "scope": "question", "value": "act", "origin": "composer" }),
    );
    let proxy = w.answer_by("Which color?", inbox("agent", RED));
    for (n, at) in [w.now - 3 * DAY, w.now - 2 * DAY].into_iter().enumerate() {
        let out = w.kanban(&[
            "ledger",
            "revoke",
            "--entry",
            &proxy,
            "--reason",
            &format!("틀렸어요 {n}"),
            "--now",
            &at.to_string(),
        ]);
        assert!(out["revoked"].is_object(), "{out}");
    }

    let stats = w.stats();

    assert_eq!(stats["kinds"]["question"]["mode"], json!("shadow"));
    assert!(
        stats["kinds"]["question"]["demoted_reason"].is_string(),
        "{stats}"
    );
    assert_eq!(stats["kinds"]["permission"]["demoted_reason"], Value::Null);
    let disabled = stats["disabled_keys"].as_array().expect("disabled keys");
    assert_eq!(disabled.len(), 1, "{stats}");
    assert_eq!(disabled[0]["entry"], json!(proxy));
}
