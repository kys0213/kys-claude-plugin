use serde_json::{json, Value};

mod kanban_support;
use kanban_support::{
    chose, decision_facts, home_deref, inbox_answer, now_ms, question, question_facts,
    typed_decision, with, Home, DAY, HOUR,
};

struct World {
    home: Home,
    now: u64,
    /// Hours before `now` of the next decision, so every decision has a time of its own.
    next: std::cell::Cell<u64>,
}

home_deref!(World);

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

    fn picks(&self, text: &str, labels: &[&str]) -> Vec<String> {
        labels
            .iter()
            .map(|label| {
                let at = self.tick();
                self.record(question_facts(at, which(text), chose(json!([label]))))
            })
            .collect()
    }

    fn answer_by(&self, text: &str, answer: Value) -> String {
        let at = self.tick();
        self.record(question_facts(at, which(text), answer))
    }

    fn rewrite(&self, id: &str, fields: Value) {
        self.rewrite_lines(|line| {
            if line["id"] == id {
                with(line, fields.clone())
            } else {
                line
            }
        });
    }

    fn stats(&self) -> Value {
        let now = self.now.to_string();
        self.kanban(&["ledger", "stats", "--now", &now])
    }

    fn question_stats(&self) -> Value {
        self.stats()["kinds"]["question"].clone()
    }
}

fn which(text: &str) -> Value {
    json!([question(text, &["Red", "Blue"], false)])
}

const RED: &str = "Red";
const BLUE: &str = "Blue";

#[test]
fn every_persons_decision_after_two_agreeing_ones_is_compared_whatever_way_it_came() {
    let w = World::new();
    w.picks("Which color?", &[RED, RED]);
    w.answer_by("Which color?", inbox_answer("pane", RED));
    w.picks("Which color?", &[RED]);
    let relayed = w.answer_by("Which color?", inbox_answer("agent", RED));
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
    w.answer_by("Which color?", inbox_answer("agent", RED));
    w.answer_by("Which color?", inbox_answer("terminal", BLUE));

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
        w.record(decision_facts(at, typed_decision("초안대로 진행")));
    }

    let stats = w.stats()["kinds"]["decision"].clone();

    assert_eq!(stats["compared"], json!(1), "{stats}");
    assert_eq!(stats["agreed"], json!(1));
    assert_eq!(stats["mode"], json!("shadow"));
}

#[test]
fn a_kind_two_revokes_stepped_back_names_why() {
    let w = World::new();
    w.add_ledger_line(
        json!({ "v": 1, "type": "setting", "id": "S1", "at": w.now - 10 * DAY, "scope": "question", "value": "act", "origin": "composer" }),
    );
    let proxy = w.answer_by("Which color?", inbox_answer("agent", RED));
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
