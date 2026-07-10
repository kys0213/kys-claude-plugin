//! Black-box tests for template substitution — the contract config authors
//! rely on: `{var}` substitution, `{var#json}` JSON-literal escaping, and
//! unknown tokens left verbatim so typos stay visible.

use atelier::notify::event::Event;
use atelier::notify::render::{substitute, vars};
use atelier::notify::types::{AskQuestionPayload, Question};

fn payload_with_quotes() -> AskQuestionPayload {
    AskQuestionPayload {
        session_id: None,
        cwd: Some("/w".to_string()),
        questions: vec![Question {
            header: None,
            question: r#"Use "quotes" & \backslash?"#.to_string(),
            options: vec![],
            multi_select: false,
        }],
    }
}

#[test]
fn substitutes_known_variables() {
    let p = payload_with_quotes();
    let vars = vars(&Event::AskQuestion(&p));
    assert_eq!(
        substitute("{event} in {cwd}", &vars),
        "ask_user_question in /w"
    );
    assert!(substitute("{title}", &vars).contains("질문"));
    assert!(substitute("{body}", &vars).contains("quotes"));
}

#[test]
fn json_modifier_produces_a_valid_json_string_literal() {
    let p = payload_with_quotes();
    let vars = vars(&Event::AskQuestion(&p));

    // {text#json} must be embeddable in a JSON body even when the event data
    // contains quotes/backslashes — this is the injection-safety contract.
    let body = substitute(r#"{"text": {text#json}}"#, &vars);
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(parsed["text"].as_str().unwrap().contains(r#"Use "quotes""#));
}

#[test]
fn json_variable_is_the_structured_event() {
    let p = payload_with_quotes();
    let event = Event::AskQuestion(&p);
    let vars = vars(&event);
    let rendered = substitute("{json}", &vars);
    assert_eq!(rendered, event.structured_json());
    assert!(!rendered.contains('\n'));
}

#[test]
fn unknown_tokens_are_left_verbatim() {
    let p = payload_with_quotes();
    let vars = vars(&Event::AskQuestion(&p));
    assert_eq!(
        substitute("{nope} {cwd#gzip} {", &vars),
        "{nope} {cwd#gzip} {"
    );
    assert_eq!(substitute("no tokens", &vars), "no tokens");
}
