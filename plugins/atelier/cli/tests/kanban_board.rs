mod kanban_support;

use kanban_support::{atelier, board, board_with, event, session, session_with, MIN, NOW, SEC};
use serde_json::{json, Value};
fn column_ids(board: &Value) -> Vec<String> {
    board["columns"]
        .as_array()
        .expect("columns is an array")
        .iter()
        .map(|c| c["id"].as_str().expect("column id").to_string())
        .collect()
}

fn cards_in(board: &Value, column: &str) -> Vec<Value> {
    board["columns"]
        .as_array()
        .expect("columns is an array")
        .iter()
        .find(|c| c["id"] == column)
        .unwrap_or_else(|| panic!("column {column} present"))["cards"]
        .as_array()
        .expect("cards is an array")
        .clone()
}

fn ids_in(board: &Value, column: &str) -> Vec<String> {
    cards_in(board, column)
        .iter()
        .map(|c| c["session_id"].as_str().expect("session_id").to_string())
        .collect()
}

fn column_of(events: Value) -> String {
    let b = board(json!([session("s", events)]));
    column_ids(&b)
        .into_iter()
        .find(|col| ids_in(&b, col).contains(&"s".to_string()))
        .expect("the session lands in some column")
}

fn expired(board: &Value) -> Vec<String> {
    board["expired"]
        .as_array()
        .expect("expired is an array")
        .iter()
        .map(|v| v.as_str().expect("expired id").to_string())
        .collect()
}

#[test]
fn columns_are_fixed_and_ordered() {
    let b = board(json!([]));
    assert_eq!(
        column_ids(&b),
        vec!["needs_input", "working", "idle", "stale"]
    );
    for col in column_ids(&b) {
        assert!(cards_in(&b, &col).is_empty());
    }
    assert!(expired(&b).is_empty());
}

#[test]
fn prompt_and_turn_start_and_tool_results_mean_working() {
    for name in [
        "prompt.submit",
        "turn.start",
        "classic.PostToolUse",
        "classic.PostToolUseFailure",
    ] {
        assert_eq!(
            column_of(json!([event(name, None, 10 * SEC)])),
            "working",
            "{name}"
        );
    }
}

#[test]
fn turn_complete_and_session_start_mean_idle() {
    for name in ["turn.complete", "session.start"] {
        assert_eq!(
            column_of(json!([event(name, None, 10 * SEC)])),
            "idle",
            "{name}"
        );
    }
}

#[test]
fn permission_request_and_question_mean_needs_input() {
    assert_eq!(
        column_of(json!([event(
            "classic.PermissionRequest",
            Some("Bash"),
            SEC
        )])),
        "needs_input"
    );
    assert_eq!(
        column_of(json!([event(
            "classic.PermissionRequest",
            Some("AskUserQuestion"),
            SEC
        )])),
        "needs_input"
    );
}

#[test]
fn notification_is_routed_by_its_type() {
    assert_eq!(
        column_of(json!([event(
            "classic.Notification",
            Some("permission_prompt"),
            SEC
        )])),
        "needs_input"
    );
    assert_eq!(
        column_of(json!([event(
            "classic.Notification",
            Some("elicitation_dialog"),
            SEC
        )])),
        "needs_input"
    );
    assert_eq!(
        column_of(json!([
            event("classic.PostToolUse", None, 70 * SEC),
            event("turn.complete", None, 65 * SEC),
            event("classic.Notification", Some("idle_prompt"), 5 * SEC),
        ])),
        "idle"
    );
}

#[test]
fn the_latest_state_event_wins() {
    assert_eq!(
        column_of(json!([
            event("classic.PermissionRequest", Some("Bash"), 20 * SEC),
            event("classic.PostToolUse", Some("Bash"), 10 * SEC),
        ])),
        "working"
    );
    assert_eq!(
        column_of(json!([
            event("turn.start", None, 20 * SEC),
            event("turn.complete", None, 10 * SEC),
        ])),
        "idle"
    );
}

#[test]
fn events_without_state_fall_back_to_the_previous_state_event() {
    assert_eq!(
        column_of(json!([
            event("classic.PermissionRequest", Some("Bash"), 20 * SEC),
            event("classic.Notification", Some("auth_success"), 10 * SEC),
            event("some.future.event", None, 5 * SEC),
        ])),
        "needs_input"
    );
}

#[test]
fn a_tool_call_has_no_state_of_its_own() {
    assert_eq!(
        column_of(json!([
            event("turn.complete", None, 20 * SEC),
            event("tool.call", Some("AskUserQuestion"), 10 * SEC),
        ])),
        "idle"
    );
}

#[test]
fn the_spinner_after_needs_input_means_the_person_answered_and_work_resumed() {
    assert_eq!(
        column_of(json!([
            event("classic.PermissionRequest", Some("Bash"), 20 * SEC),
            event("classic.Notification", Some("permission_prompt"), 14 * SEC),
            event("ui.spinner", None, 10 * SEC),
        ])),
        "working"
    );
    let b = board(json!([session(
        "s",
        json!([
            event("classic.PermissionRequest", Some("Bash"), 20 * SEC),
            event("ui.spinner", None, 10 * SEC),
        ])
    )]));
    assert_eq!(cards_in(&b, "working")[0]["age"], "10s");
}

#[test]
fn the_spinner_after_any_other_state_changes_nothing() {
    assert_eq!(
        column_of(json!([
            event("turn.complete", None, 20 * SEC),
            event("ui.spinner", None, 10 * SEC),
        ])),
        "idle"
    );
    assert_eq!(
        column_of(json!([event("ui.spinner", None, 10 * SEC)])),
        "idle"
    );
    let b = board(json!([session(
        "s",
        json!([
            event("turn.start", None, 20 * SEC),
            event("ui.spinner", None, 10 * SEC),
        ])
    )]));
    assert_eq!(cards_in(&b, "working")[0]["age"], "20s");
}

#[test]
fn no_state_event_at_all_means_idle() {
    assert_eq!(column_of(json!([])), "idle");
}

#[test]
fn needs_input_card_carries_the_reason_and_age() {
    let b = board(json!([session(
        "s",
        json!([event("classic.PermissionRequest", Some("Bash"), 3 * MIN)])
    )]));
    let card = &cards_in(&b, "needs_input")[0];
    assert_eq!(card["reason"], "권한 요청: Bash");
    assert_eq!(card["age"], "3m");

    let b = board(json!([session(
        "s",
        json!([event(
            "classic.PermissionRequest",
            Some("AskUserQuestion"),
            5 * SEC
        )])
    )]));
    assert_eq!(cards_in(&b, "needs_input")[0]["reason"], "질문");
}

#[test]
fn age_counts_from_when_the_session_entered_its_state() {
    let b = board(json!([session(
        "s",
        json!([
            event("turn.start", None, 5 * MIN),
            event("classic.PostToolUse", Some("Bash"), 3 * MIN),
            event("classic.PostToolUse", Some("Edit"), MIN),
        ])
    )]));
    assert_eq!(cards_in(&b, "working")[0]["age"], "5m");

    let b = board(json!([session(
        "s",
        json!([
            event("turn.complete", None, 4 * MIN),
            event("classic.Notification", Some("idle_prompt"), 3 * MIN),
        ])
    )]));
    assert_eq!(cards_in(&b, "idle")[0]["age"], "4m");
}

#[test]
fn age_is_measured_from_the_state_event() {
    let ages = [
        (45 * SEC, "45s"),
        (MIN, "1m"),
        (59 * MIN, "59m"),
        (60 * MIN, "1h"),
        (25 * 60 * MIN, "1d"),
    ];
    for (ago, label) in ages {
        let b = board(json!([session(
            "s",
            json!([event("turn.complete", None, ago)])
        )]));
        assert_eq!(cards_in(&b, "idle")[0]["age"], label, "{ago}ms ago");
    }
}

#[test]
fn a_missing_heartbeat_moves_any_state_to_stale() {
    let mut s = session("s", json!([event("turn.start", None, 10 * MIN)]));
    s["heartbeat_at"] = json!(NOW - 2 * MIN - 1);
    let b = board(json!([s]));
    assert_eq!(ids_in(&b, "stale"), vec!["s"]);
    assert!(ids_in(&b, "working").is_empty());

    let mut s = session(
        "s",
        json!([event("classic.PermissionRequest", None, 10 * MIN)]),
    );
    s["heartbeat_at"] = json!(NOW - 3 * MIN);
    assert_eq!(ids_in(&board(json!([s])), "stale"), vec!["s"]);
}

#[test]
fn a_heartbeat_exactly_at_the_threshold_is_not_stale() {
    let mut s = session("s", json!([event("turn.start", None, 10 * MIN)]));
    s["heartbeat_at"] = json!(NOW - 2 * MIN);
    assert_eq!(ids_in(&board(json!([s])), "working"), vec!["s"]);
}

#[test]
fn a_long_running_tool_with_a_live_heartbeat_stays_working() {
    let s = session("s", json!([event("turn.start", None, 90 * MIN)]));
    assert_eq!(ids_in(&board(json!([s])), "working"), vec!["s"]);
}

#[test]
fn ended_sessions_expire_immediately() {
    let mut s = session("s", json!([event("turn.complete", None, SEC)]));
    s["ended_reason"] = json!("prompt_input_exit");
    let b = board(json!([s]));
    assert_eq!(expired(&b), vec!["s"]);
    for col in column_ids(&b) {
        assert!(ids_in(&b, &col).is_empty(), "{col}");
    }
}

#[test]
fn sessions_silent_past_expire_after_expire() {
    let mut old = session("old", json!([event("turn.complete", None, 30 * 60 * MIN)]));
    old["heartbeat_at"] = json!(NOW - 24 * 60 * MIN - 1);
    let mut edge = session("edge", json!([event("turn.complete", None, 30 * 60 * MIN)]));
    edge["heartbeat_at"] = json!(NOW - 24 * 60 * MIN);
    let b = board(json!([old, edge]));
    assert_eq!(expired(&b), vec!["old"]);
    assert_eq!(ids_in(&b, "stale"), vec!["edge"]);
}

#[test]
fn needs_input_and_working_list_the_longest_waiting_first() {
    let b = board(json!([
        session("new", json!([event("turn.start", None, 10 * SEC)])),
        session("old", json!([event("turn.start", None, 50 * SEC)])),
    ]));
    assert_eq!(ids_in(&b, "working"), vec!["old", "new"]);

    let b = board(json!([
        session(
            "new",
            json!([event("classic.PermissionRequest", None, 10 * SEC)])
        ),
        session(
            "old",
            json!([event("classic.PermissionRequest", None, 50 * SEC)])
        ),
    ]));
    assert_eq!(ids_in(&b, "needs_input"), vec!["old", "new"]);
}

#[test]
fn idle_and_stale_list_the_most_recent_first() {
    let b = board(json!([
        session("old", json!([event("turn.complete", None, 50 * SEC)])),
        session("new", json!([event("turn.complete", None, 10 * SEC)])),
    ]));
    assert_eq!(ids_in(&b, "idle"), vec!["new", "old"]);
}

#[test]
fn ties_break_by_session_id() {
    let b = board(json!([
        session("b", json!([event("turn.complete", None, 10 * SEC)])),
        session("a", json!([event("turn.complete", None, 10 * SEC)])),
    ]));
    assert_eq!(ids_in(&b, "idle"), vec!["a", "b"]);
}

#[test]
fn self_is_marked() {
    let b = board(json!([
        session("self-id", json!([event("turn.complete", None, SEC)])),
        session("other", json!([event("turn.complete", None, 2 * SEC)])),
    ]));
    let cards = cards_in(&b, "idle");
    assert_eq!(cards[0]["is_self"], true);
    assert_eq!(cards[1]["is_self"], false);
}

#[test]
fn label_prefers_the_repo_name_then_the_cwd_basename() {
    let mut with_repo = session("r", json!([]));
    with_repo["repo"] = json!("kys0213/kys-claude-plugin");
    let mut trailing = session("t", json!([]));
    trailing["cwd"] = json!("/work/project-b/");
    let mut root = session("z", json!([]));
    root["cwd"] = json!("/");
    let b = board(json!([with_repo, trailing, root]));
    let label = |id: &str| {
        cards_in(&b, "idle")
            .into_iter()
            .find(|c| c["session_id"] == id)
            .expect("card")["label"]
            .clone()
    };
    assert_eq!(label("r"), "kys-claude-plugin");
    assert_eq!(label("t"), "project-b");
    assert_eq!(label("z"), "/");
}

#[test]
fn subtitle_prefers_the_title_then_the_first_prompt_folded_and_truncated() {
    let mut titled = session("a", json!([]));
    titled["title"] = json!("Fix login bug");
    titled["first_prompt"] = json!("ignored");
    let mut prompted = session("b", json!([]));
    prompted["first_prompt"] = json!(format!("line one\n\n   line two {}", "x".repeat(80)));
    let bare = session("c", json!([]));
    let b = board(json!([titled, prompted, bare]));
    let subtitle = |id: &str| {
        cards_in(&b, "idle")
            .into_iter()
            .find(|c| c["session_id"] == id)
            .expect("card")["subtitle"]
            .clone()
    };
    assert_eq!(subtitle("a"), "Fix login bug");
    let folded = subtitle("b");
    let folded = folded.as_str().expect("subtitle string");
    assert!(folded.starts_with("line one line two xxx"), "{folded}");
    assert_eq!(folded.chars().count(), 60);
    assert!(folded.ends_with('…'));
    assert_eq!(subtitle("c"), Value::Null);
}

#[test]
fn unknown_versions_and_malformed_records_are_skipped_not_expired() {
    let mut future = session("future", json!([]));
    future["v"] = json!(2);
    let b = board(json!([
        future,
        { "v": 1, "session_id": "broken" },
        "not an object",
        session("ok", json!([])),
    ]));
    assert_eq!(ids_in(&b, "idle"), vec!["ok"]);
    assert!(expired(&b).is_empty());
}

#[test]
fn malformed_stdin_reports_an_error_and_still_exits_zero() {
    let now = NOW.to_string();
    let args = [
        "--now",
        now.as_str(),
        "--stale-after",
        "2m",
        "--expire-after",
        "24h",
        "--width",
        "100",
    ];
    for stdin in ["", "not json", "{}", r#"{"sessions": 3}"#] {
        let b = board_with(&args, stdin);
        assert!(b["error"].is_string(), "stdin {stdin:?}: {b}");
        assert!(b.get("columns").is_none());
    }
}

#[test]
fn durations_accept_each_unit() {
    let now = NOW.to_string();
    for (stale, expect_stale) in [
        ("119999ms", true),
        ("120s", false),
        ("2m", false),
        ("1h", false),
        ("1d", false),
    ] {
        let mut s = session("s", json!([event("turn.start", None, SEC)]));
        s["heartbeat_at"] = json!(NOW - 2 * MIN);
        let b = board_with(
            &[
                "--now",
                &now,
                "--stale-after",
                stale,
                "--expire-after",
                "30d",
                "--width",
                "100",
            ],
            &json!({ "sessions": [s] }).to_string(),
        );
        assert_eq!(
            ids_in(&b, "stale") == vec!["s"],
            expect_stale,
            "--stale-after {stale}"
        );
    }
}

#[test]
fn missing_or_invalid_arguments_print_no_board() {
    let stdin = json!({ "sessions": [] }).to_string();
    for args in [
        vec![
            "--stale-after",
            "2m",
            "--expire-after",
            "24h",
            "--width",
            "100",
        ],
        vec!["--now", "1", "--expire-after", "24h", "--width", "100"],
        vec!["--now", "1", "--stale-after", "2m", "--width", "100"],
        vec!["--now", "1", "--stale-after", "2m", "--expire-after", "24h"],
        vec![
            "--now",
            "1",
            "--stale-after",
            "2m",
            "--expire-after",
            "24h",
            "--width",
            "wide",
        ],
        vec![
            "--now",
            "1",
            "--stale-after",
            "2 minutes",
            "--expire-after",
            "24h",
            "--width",
            "100",
        ],
        vec![
            "--now",
            "soon",
            "--stale-after",
            "2m",
            "--expire-after",
            "24h",
            "--width",
            "100",
        ],
    ] {
        let out = atelier()
            .args(["kanban", "board"])
            .args(&args)
            .write_stdin(stdin.clone())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert!(
            serde_json::from_slice::<Value>(&out).is_err(),
            "{args:?} printed a board"
        );
    }
}

fn layout_at(width: &str) -> Value {
    let now = NOW.to_string();
    board_with(
        &[
            "--now",
            &now,
            "--stale-after",
            "2m",
            "--expire-after",
            "24h",
            "--width",
            width,
        ],
        &json!({ "sessions": [] }).to_string(),
    )["layout"]
        .clone()
}

#[test]
fn a_pane_wide_enough_for_every_column_lays_them_side_by_side() {
    assert_eq!(
        layout_at("120"),
        json!({ "direction": "row", "column_width": 29, "column_gap": 1 })
    );
    assert_eq!(
        layout_at("107"),
        json!({ "direction": "row", "column_width": 26, "column_gap": 1 })
    );
}

#[test]
fn a_narrower_pane_stacks_the_columns_at_full_width() {
    assert_eq!(
        layout_at("106"),
        json!({ "direction": "column", "column_width": 106, "column_gap": 1 })
    );
    assert_eq!(
        layout_at("0"),
        json!({ "direction": "column", "column_width": 0, "column_gap": 1 })
    );
}

#[test]
fn columns_and_cards_carry_the_lines_the_pane_draws() {
    let b = board(json!([
        session(
            "other",
            json!([event("classic.PermissionRequest", Some("Bash"), 3 * MIN)])
        ),
        session("self-id", json!([event("turn.complete", None, 5 * SEC)])),
    ]));
    let column = |id: &str| {
        b["columns"]
            .as_array()
            .expect("columns")
            .iter()
            .find(|c| c["id"] == id)
            .expect("column")
            .clone()
    };
    assert_eq!(column("needs_input")["heading"], "입력 대기 1");
    assert_eq!(column("working")["heading"], "작업 중 0");
    assert_eq!(
        cards_in(&b, "needs_input")[0]["headline"],
        "project-a · 권한 요청: Bash · 3m"
    );
    assert_eq!(cards_in(&b, "idle")[0]["headline"], "▶ project-a · 5s");
}

fn only_card(sessions: Value) -> Value {
    let b = board(sessions);
    column_ids(&b)
        .iter()
        .flat_map(|col| cards_in(&b, col))
        .next()
        .expect("one card")
}

const TASKS: &str = r#"{
    "1": { "subject": "Plan", "active": null, "status": "completed" },
    "2": { "subject": "Test", "active": "Running tests", "status": "in_progress" },
    "3": { "subject": "Ship", "active": null, "status": "pending" },
    "4": { "subject": "Drop", "active": null, "status": "deleted" }
}"#;

fn tasks() -> Value {
    serde_json::from_str(TASKS).expect("tasks fixture")
}

fn bash_activity() -> Value {
    json!({ "tool": "Bash", "input": { "command": "cargo test", "description": "Run tests" }, "at": NOW - SEC })
}

#[test]
fn an_answer_from_the_kanban_means_working() {
    assert_eq!(
        column_of(json!([
            event(
                "classic.PermissionRequest",
                Some("AskUserQuestion"),
                20 * SEC
            ),
            event("kanban.answer", None, 10 * SEC),
        ])),
        "working"
    );
}

#[test]
fn progress_counts_tasks_without_deleted_ones_and_names_the_one_in_progress() {
    let card = only_card(json!([session_with(
        "s",
        json!([event("turn.complete", None, SEC)]),
        json!({ "tasks": tasks() })
    )]));
    assert_eq!(card["progress"], "할 일 1/3 · Running tests");
}

#[test]
fn progress_falls_back_to_the_subject_then_to_todo_write() {
    let card = only_card(json!([session_with(
        "s",
        json!([event("turn.complete", None, SEC)]),
        json!({ "tasks": { "7": { "subject": "Write docs", "active": null, "status": "in_progress" } } })
    )]));
    assert_eq!(card["progress"], "할 일 0/1 · Write docs");

    let card = only_card(json!([session_with(
        "s",
        json!([event("turn.complete", None, SEC)]),
        json!({ "todos": [
            { "status": "completed", "active": "Reading" },
            { "status": "in_progress", "active": "Writing docs" },
        ] })
    )]));
    assert_eq!(card["progress"], "할 일 1/2 · Writing docs");
}

#[test]
fn tasks_win_over_todo_write_when_both_exist() {
    let card = only_card(json!([session_with(
        "s",
        json!([event("turn.complete", None, SEC)]),
        json!({ "tasks": tasks(), "todos": [{ "status": "completed", "active": "x" }] })
    )]));
    assert_eq!(card["progress"], "할 일 1/3 · Running tests");
}

#[test]
fn the_current_tool_is_shown_only_while_working() {
    let working = only_card(json!([session_with(
        "s",
        json!([event("turn.start", None, SEC)]),
        json!({ "activity": bash_activity(), "tasks": tasks() })
    )]));
    assert_eq!(
        working["progress"],
        "할 일 1/3 · Running tests · ▸ Bash: Run tests"
    );

    let idle = only_card(json!([session_with(
        "s",
        json!([event("turn.complete", None, SEC)]),
        json!({ "activity": bash_activity() })
    )]));
    assert_eq!(idle["progress"], Value::Null);
}

#[test]
fn the_tool_target_is_the_first_known_field_with_paths_cut_to_the_file_name() {
    let progress = |tool: &str, input: Value| {
        only_card(json!([session_with(
            "s",
            json!([event("turn.start", None, SEC)]),
            json!({ "activity": { "tool": tool, "input": input, "at": NOW - SEC } })
        )]))["progress"]
            .clone()
    };
    assert_eq!(
        progress(
            "Edit",
            json!({ "file_path": "/w/src/main.rs", "old_string": "a" })
        ),
        "▸ Edit: main.rs"
    );
    assert_eq!(
        progress("Grep", json!({ "pattern": "fn main", "path": "/w/src" })),
        "▸ Grep: fn main"
    );
    assert_eq!(
        progress("Bash", json!({ "command": "ls\n  -la" })),
        "▸ Bash: ls -la"
    );
    assert_eq!(progress("TaskList", json!({})), "▸ TaskList");
    assert_eq!(
        progress("Bash", json!({ "description": "x".repeat(50) })),
        format!("▸ Bash: {}…", "x".repeat(39))
    );
}

#[test]
fn a_stale_session_shows_no_progress() {
    let mut record = session_with(
        "s",
        json!([event("turn.start", None, 10 * MIN)]),
        json!({ "activity": bash_activity(), "tasks": tasks() }),
    );
    record["heartbeat_at"] = json!(NOW - 5 * MIN);
    let card = only_card(json!([record]));
    assert_eq!(card["progress"], Value::Null);
}

fn color_question() -> Value {
    json!({
        "question": "Which color?",
        "header": "Color",
        "multi_select": false,
        "options": [
            { "label": "Red", "description": "warm" },
            { "label": "Blue", "description": "cool" },
        ],
    })
}

#[test]
fn an_open_question_is_carried_on_the_card_while_it_waits() {
    let asks = json!({
        "tu-2": { "questions": [color_question()], "at": NOW - 5 * SEC },
        "tu-1": { "questions": [color_question()], "at": NOW - 9 * SEC },
    });
    let card = only_card(json!([session_with(
        "s",
        json!([event(
            "classic.PermissionRequest",
            Some("AskUserQuestion"),
            5 * SEC
        )]),
        json!({ "asks": asks })
    )]));
    assert_eq!(
        card["asks"],
        json!([
            {
                "tool_use_id": "tu-1",
                "questions": [{
                    "question": "Which color?",
                    "header": "Color",
                    "multi_select": false,
                    "options": [
                        { "label": "Red", "description": "warm" },
                        { "label": "Blue", "description": "cool" },
                    ],
                }],
            },
            {
                "tool_use_id": "tu-2",
                "questions": [{
                    "question": "Which color?",
                    "header": "Color",
                    "multi_select": false,
                    "options": [
                        { "label": "Red", "description": "warm" },
                        { "label": "Blue", "description": "cool" },
                    ],
                }],
            },
        ])
    );
}

#[test]
fn a_question_is_not_offered_once_the_session_moved_on() {
    let asks = json!({ "tu-1": { "questions": [color_question()], "at": NOW - 9 * SEC } });
    for events in [
        json!([event("turn.complete", None, 5 * SEC)]),
        json!([event("turn.start", None, 5 * SEC)]),
        json!([
            event(
                "classic.PermissionRequest",
                Some("AskUserQuestion"),
                9 * SEC
            ),
            event("classic.PostToolUse", Some("AskUserQuestion"), 5 * SEC),
        ]),
    ] {
        let card = only_card(json!([session_with("s", events, json!({ "asks": asks }))]));
        assert_eq!(card["asks"], json!([]));
    }
}

#[test]
fn a_malformed_progress_or_question_field_skips_the_record() {
    for extra in [
        json!({ "activity": { "tool": 7, "input": {}, "at": NOW } }),
        json!({ "tasks": { "1": { "subject": "a", "status": 3 } } }),
        json!({ "todos": [{ "status": "pending" }] }),
        json!({ "asks": { "tu": { "questions": "nope", "at": NOW } } }),
    ] {
        let b = board(json!([session_with(
            "s",
            json!([event("turn.start", None, SEC)]),
            extra.clone()
        )]));
        let total: usize = column_ids(&b).iter().map(|c| cards_in(&b, c).len()).sum();
        assert_eq!(total, 0, "{extra}");
        assert!(expired(&b).is_empty(), "{extra}");
    }
}
