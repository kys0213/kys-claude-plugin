mod kanban_support;

use kanban_support::{
    actions, board, event, now_ms, put_record, session_with, status_in, with, Home, MIN, NOW, SEC,
};
use serde_json::{json, Value};

fn card_ids(board: &Value) -> Vec<String> {
    board["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .flat_map(|c| c["cards"].as_array().expect("cards").clone())
        .map(|card| card["session_id"].as_str().expect("id").to_string())
        .collect()
}

fn actions_of(board: &Value, id: &str) -> Vec<String> {
    let card = board["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .flat_map(|c| c["cards"].as_array().expect("cards").clone())
        .find(|card| card["session_id"] == id)
        .unwrap_or_else(|| panic!("card {id} on the board"));
    actions(&card)
}

fn idle(id: &str, extra: Value) -> Value {
    session_with(id, json!([event("turn.complete", None, 5 * MIN)]), extra)
}

fn working(id: &str, extra: Value) -> Value {
    with(
        session_with(
            id,
            json!([event("turn.start", None, 5 * MIN)]),
            json!({ "turn_id": "t-1" }),
        ),
        extra,
    )
}

fn prompt(text: &str, origin: &str, ago: u64) -> Value {
    json!({ "text": text, "origin": origin, "at": NOW - ago })
}

#[test]
fn idle_and_working_cards_offer_hide_beside_their_own_actions() {
    let b = board(json!([idle("i", json!({})), working("w", json!({}))]));

    assert_eq!(actions_of(&b, "i"), ["send", "hide"]);
    assert_eq!(actions_of(&b, "w"), ["stop", "hide"]);
}

#[test]
fn hide_is_not_offered_on_the_calling_session_or_the_delegate() {
    let b = board(json!([
        idle("self-id", json!({})),
        idle("d", json!({ "delegate_since": NOW - MIN })),
    ]));

    assert_eq!(actions_of(&b, "self-id"), ["send"]);
    assert_eq!(actions_of(&b, "d"), ["send"]);
}

#[test]
fn a_hidden_session_is_left_off_the_board_but_not_dropped() {
    let b = board(json!([
        idle("i", json!({ "hidden_at": NOW - MIN })),
        working("w", json!({ "hidden_at": NOW - MIN })),
        idle("shown", json!({})),
    ]));

    assert_eq!(card_ids(&b), ["shown"]);
    assert_eq!(b["expired"], json!([]));
}

#[test]
fn a_person_prompt_after_the_hide_brings_the_card_back() {
    let b = board(json!([
        idle(
            "again",
            json!({ "hidden_at": NOW - 2 * MIN, "prompts": [prompt("Fix it", "composer", MIN)] })
        ),
        idle(
            "before",
            json!({ "hidden_at": NOW - MIN, "prompts": [prompt("Fix it", "composer", 2 * MIN)] })
        ),
        idle(
            "event",
            json!({
                "hidden_at": NOW - 2 * MIN,
                "prompts": [prompt("<task-notification>done</task-notification>", "composer", MIN)]
            })
        ),
    ]));

    assert_eq!(card_ids(&b), ["again"]);
}

#[test]
fn a_hidden_session_waiting_for_input_is_shown() {
    let asking = session_with(
        "q",
        json!([event("classic.PermissionRequest", Some("Bash"), 10 * SEC)]),
        json!({ "hidden_at": NOW - MIN, "turn_id": "t-1" }),
    );

    let b = board(json!([asking]));

    assert_eq!(card_ids(&b), ["q"]);
}

#[test]
fn a_hidden_session_gone_silent_is_shown_as_stale() {
    let mut silent = idle("x", json!({ "hidden_at": NOW - 20 * MIN }));
    silent["heartbeat_at"] = json!(NOW - 10 * MIN);

    let b = board(json!([silent]));

    assert_eq!(card_ids(&b), ["x"]);
    assert_eq!(actions_of(&b, "x"), ["forget"]);
}

#[test]
fn the_delegate_stays_on_the_board_though_hidden() {
    let b = board(json!([idle(
        "d",
        json!({ "delegate_since": NOW - MIN, "hidden_at": NOW - MIN })
    )]));

    assert_eq!(card_ids(&b), ["d"]);
}

#[test]
fn the_terminal_board_leaves_hidden_sessions_out_too() {
    let home = Home::default();
    put_record(home.config(), &idle("i", json!({ "hidden_at": NOW - MIN })));
    put_record(home.config(), &idle("shown", json!({})));

    let out = status_in(home.config(), &["--now", &NOW.to_string()]);

    let ids: Vec<&str> = out["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .map(|s| s["session_id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, ["shown"]);
}

#[test]
fn a_hide_is_taken_whether_or_not_a_turn_runs() {
    let home = Home::default();

    let queued = home.kanban(&["hide", "--session", "s-1"]);
    assert!(queued.get("error").is_none(), "{queued}");
    let out = home.take("s-1", now_ms(), "60s", Some("t-1"), None);
    assert_eq!(out["actions"], json!([{ "kind": "hide" }]));
    assert_eq!(out["results"][0]["status"], "delivered");

    home.kanban(&["hide", "--session", "s-1"]);
    let out = home.take("s-1", now_ms(), "60s", None, None);
    assert_eq!(out["actions"], json!([{ "kind": "hide" }]));
}

#[test]
fn hide_refuses_a_session_id_that_leaves_the_directory() {
    let home = Home::default();

    let out = home.kanban(&["hide", "--session", "../x"]);

    assert!(out["error"].is_string(), "{out}");
}
