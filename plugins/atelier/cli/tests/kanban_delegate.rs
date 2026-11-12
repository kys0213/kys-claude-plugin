use serde_json::{json, Value};

mod kanban_support;
use kanban_support::{
    asking, board, color, event, put_record, session_with, Home, HOUR, MIN, NOW, SEC,
};

const ANY_QUESTION_KEY: &str = "q1:00000000000000000000000000000000";
const ANY_PERMISSION_KEY: &str = "p1:00000000000000000000000000000000";

fn command(home: &Home, origin: &str, args: &str) -> Value {
    let now = NOW.to_string();
    home.kanban(&[
        "command",
        "--session",
        "m",
        "--now",
        &now,
        "--origin",
        origin,
        "--args",
        args,
    ])
}

fn mode_of(home: &Home, key: &str) -> Value {
    home.ledger_match(key, NOW + SEC)["mode"].clone()
}

fn delegate_record(id: &str, since: u64) -> Value {
    session_with(
        id,
        json!([event("session.start", None, MIN)]),
        json!({ "delegate_since": since }),
    )
}

#[test]
fn delegate_on_typed_by_a_person_makes_this_session_the_delegate_from_now() {
    let home = Home::default();

    let on = command(&home, "composer", "delegate on");
    let off = command(&home, "bridge", "delegate off");

    assert_eq!(on["delegate_since"], json!(NOW), "{on}");
    assert!(
        on["text"].as_str().expect("text").contains("켰어요"),
        "{on}"
    );
    assert!(off.get("delegate_since").is_some(), "{off}");
    assert_eq!(off["delegate_since"], Value::Null);
    assert!(
        off["text"].as_str().expect("text").contains("껐어요"),
        "{off}"
    );
}

#[test]
fn a_delegate_command_no_person_typed_changes_nothing() {
    for origin in [
        "plugin",
        "sdk",
        "peer",
        "scheduled-trigger",
        "kanban",
        "unknown",
    ] {
        let home = Home::default();

        let on = command(&home, origin, "delegate on");
        let act = command(&home, origin, "delegate act question");

        assert!(
            on["error"].as_str().expect("error").contains(origin),
            "{on}"
        );
        assert!(on.get("delegate_since").is_none(), "{on}");
        assert!(act["error"].is_string(), "{act}");
        assert_eq!(mode_of(&home, ANY_QUESTION_KEY), json!("shadow"));
    }
}

#[test]
fn act_is_taken_below_the_promotion_bar_and_says_where_the_comparison_stands() {
    let home = Home::default();

    let act = command(&home, "composer", "delegate act question");

    let text = act["text"]
        .as_str()
        .unwrap_or_else(|| panic!("text: {act}"));
    assert!(text.contains("비교 0/10"), "{text}");
    assert!(text.contains("아직"), "{text}");
    assert_eq!(mode_of(&home, ANY_QUESTION_KEY), json!("act"));
    assert_eq!(mode_of(&home, ANY_PERMISSION_KEY), json!("shadow"));
}

#[test]
fn act_on_decisions_is_refused() {
    let home = Home::default();

    let act = command(&home, "composer", "delegate act decision");

    assert_eq!(act["error"], json!("결정 요청은 아직 대신 답하지 않아요"));
}

#[test]
fn shadow_steps_a_kind_back_from_acting() {
    let home = Home::default();
    command(&home, "composer", "delegate act permission");

    let later = (NOW + SEC).to_string();
    let shadow = home.kanban(&[
        "command",
        "--session",
        "m",
        "--now",
        &later,
        "--origin",
        "composer",
        "--args",
        "delegate shadow permission",
    ]);

    assert!(shadow["text"].is_string(), "{shadow}");
    let mode = home.ledger_match(ANY_PERMISSION_KEY, NOW + 2 * SEC);
    assert_eq!(mode["mode"], json!("shadow"));
}

#[test]
fn arguments_that_are_not_a_delegate_command_are_refused() {
    let home = Home::default();

    for args in [
        "delegate",
        "delegate act",
        "delegate act everything",
        "open",
        "delegate on now",
    ] {
        let out = command(&home, "composer", args);
        assert!(out["error"].is_string(), "{args}: {out}");
    }
}

#[test]
fn turning_on_a_second_delegate_names_the_one_that_stops() {
    let home = Home::default();
    put_record(home.config(), &delegate_record("old", NOW - HOUR));

    let on = command(&home, "composer", "delegate on");

    let text = on["text"].as_str().expect("text");
    assert!(text.contains("old"), "{text}");
}

#[test]
fn status_names_the_live_delegate_turned_on_last_and_the_others_as_conflicts() {
    let home = Home::default();
    put_record(home.config(), &delegate_record("old", NOW - 2 * HOUR));
    put_record(home.config(), &delegate_record("new", NOW - HOUR));
    let mut gone = delegate_record("gone", NOW - MIN);
    gone["heartbeat_at"] = json!(NOW - 10 * MIN);
    put_record(home.config(), &gone);

    let out = home.status_at(NOW, &[]);

    let delegate = &out["delegate"];
    assert_eq!(delegate["session_id"], json!("new"));
    assert_eq!(delegate["since"], json!(NOW - HOUR));
    assert_eq!(delegate["conflicts"], json!(["old"]));
    assert_eq!(
        delegate["modes"],
        json!({ "question": "shadow", "permission": "shadow", "decision": "shadow" })
    );
    assert_eq!(
        delegate["progress"],
        json!({ "question": "0/10", "permission": "0/10", "decision": "0/10" })
    );
}

#[test]
fn status_without_a_delegate_says_so() {
    let home = Home::default();
    put_record(home.config(), &asking(color()));

    let out = home.status_at(NOW, &[]);

    assert_eq!(out["delegate"]["session_id"], Value::Null);
    assert_eq!(out["delegate"]["conflicts"], json!([]));
}

#[test]
fn the_board_names_the_delegate_too() {
    let out = board(json!([
        delegate_record("old", NOW - 2 * HOUR),
        delegate_record("new", NOW - HOUR),
        asking(color()),
    ]));

    assert_eq!(
        out["delegate"],
        json!({ "session_id": "new", "since": NOW - HOUR, "conflicts": ["old"] })
    );
}

fn frame(home: &Home, session: &str) -> Value {
    let now = NOW.to_string();
    let questions = json!([{
        "question": "[kanban-wait w tu-1]\n색은?",
        "header": "Color",
        "multiSelect": false,
        "options": [{ "label": "Red", "description": "" }, { "label": "Blue", "description": "" }],
    }]);
    home.run(
        &["relay-frame", "--now", &now, "--session", session],
        &json!({ "questions": questions }).to_string(),
    )
}

#[test]
fn only_the_delegate_session_frames_a_question_over_a_wait() {
    let home = Home::default();
    let mut worker = asking(color());
    worker["repo_root"] = json!("/work/project-b");
    put_record(home.config(), &worker);
    put_record(home.config(), &delegate_record("m", NOW - HOUR));
    put_record(home.config(), &delegate_record("older", NOW - 2 * HOUR));
    put_record(
        home.config(),
        &session_with(
            "plain",
            json!([event("session.start", None, MIN)]),
            json!({}),
        ),
    );

    let framed = frame(&home, "m");
    let older = frame(&home, "older");
    let plain = frame(&home, "plain");

    assert!(framed["target"].is_object(), "{framed}");
    for out in [older, plain] {
        assert_eq!(out["target"], Value::Null, "{out}");
        assert_eq!(
            out["questions"][0]["question"],
            json!("[kanban-wait w tu-1]\n색은?")
        );
    }
}

#[test]
fn the_shell_may_not_make_the_delegate_commands_only_the_mod_makes() {
    let home = Home::default();
    for command in [
        "atelier kanban command --session m --now 1 --origin composer --args 'delegate act question'",
        "atelier kanban delegate report --now 1",
    ] {
        let out = home.run(&["guard-bash"], &json!({ "command": command }).to_string());
        assert!(out["deny"].is_string(), "{command}: {out}");
    }
    let wake = "atelier kanban delegate wake --session m --now 1 --report-at 18:00 --utc-offset 0";
    let out = home.run(&["guard-bash"], &json!({ "command": wake }).to_string());
    assert_eq!(out["deny"], Value::Null);
}
