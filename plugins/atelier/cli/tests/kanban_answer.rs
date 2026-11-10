use assert_cmd::Command;
use serde_json::{json, Value};

fn answer(stdin: &str) -> Value {
    let output = Command::cargo_bin("atelier")
        .expect("locate `atelier` cargo binary")
        .args(["kanban", "answer"])
        .write_stdin(stdin)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid JSON on stdout")
}

fn question(header: &str, multi_select: bool, labels: &[&str]) -> Value {
    json!({
        "question": format!("{header}?"),
        "header": header,
        "multi_select": multi_select,
        "options": labels
            .iter()
            .map(|l| json!({ "label": l, "description": "" }))
            .collect::<Vec<_>>(),
    })
}

fn pick(options: &[&str], text: Option<&str>) -> Value {
    json!({ "options": options, "text": text })
}

fn answer_for(questions: Value, picks: Value) -> Value {
    answer(&json!({ "questions": questions, "picks": picks }).to_string())
}

#[test]
fn each_question_gets_its_picked_label_in_question_order() {
    let out = answer_for(
        json!([
            question("Color", false, &["Red", "Blue"]),
            question("Size", false, &["S", "M"]),
        ]),
        json!([pick(&["Blue"], None), pick(&["S"], None)]),
    );
    assert_eq!(out, json!({ "answers": ["Blue", "S"] }));
}

#[test]
fn a_multi_select_answer_joins_the_labels_in_option_order() {
    let out = answer_for(
        json!([question("Pets", true, &["Cat", "Dog", "Fish"])]),
        json!([pick(&["Fish", "Cat"], None)]),
    );
    assert_eq!(out, json!({ "answers": ["Cat, Fish"] }));
}

#[test]
fn typed_text_wins_over_picked_options() {
    let out = answer_for(
        json!([question("Color", false, &["Red", "Blue"])]),
        json!([pick(&["Red"], Some("  Green  "))]),
    );
    assert_eq!(out, json!({ "answers": ["Green"] }));
}

#[test]
fn an_unanswered_question_is_refused_by_its_header() {
    for unanswered in [pick(&[], None), pick(&[], Some("   "))] {
        let out = answer_for(
            json!([
                question("Color", false, &["Red", "Blue"]),
                question("Size", false, &["S", "M"]),
            ]),
            json!([pick(&["Red"], None), unanswered]),
        );
        assert_eq!(out, json!({ "error": "답하지 않은 질문이 있어요: Size" }));
    }
}

#[test]
fn a_label_the_question_does_not_offer_is_refused() {
    let out = answer_for(
        json!([question("Color", false, &["Red", "Blue"])]),
        json!([pick(&["Green"], None)]),
    );
    assert_eq!(out, json!({ "error": "없는 선택지예요: Color — Green" }));
}

#[test]
fn a_single_select_question_takes_one_label() {
    let out = answer_for(
        json!([question("Color", false, &["Red", "Blue"])]),
        json!([pick(&["Red", "Blue"], None)]),
    );
    assert_eq!(out, json!({ "error": "하나만 고를 수 있어요: Color" }));
}

#[test]
fn picks_must_match_the_questions_one_to_one() {
    let out = answer_for(
        json!([question("Color", false, &["Red", "Blue"])]),
        json!([pick(&["Red"], None), pick(&["S"], None)]),
    );
    assert_eq!(out, json!({ "error": "질문 1개에 답 2개가 왔어요" }));
}

#[test]
fn malformed_stdin_reports_an_error_and_still_exits_zero() {
    for stdin in ["nope", "{}", r#"{"questions":[],"picks":"x"}"#] {
        let out = answer(stdin);
        assert!(out["error"].is_string(), "{stdin}: {out}");
    }
}
