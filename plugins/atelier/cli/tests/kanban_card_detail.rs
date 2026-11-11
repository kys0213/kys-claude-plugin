mod kanban_support;

use kanban_support::{board, event, session_with, MIN, NOW, SEC};
use serde_json::{json, Value};

fn only(sessions: Value) -> (String, Value) {
    let b = board(sessions);
    b["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .find_map(|c| {
            let card = c["cards"].as_array()?.first()?.clone();
            Some((c["id"].as_str()?.to_string(), card))
        })
        .expect("one card")
}

fn sections(card: &Value) -> Vec<(String, Vec<String>)> {
    card["detail"]["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .map(|s| {
            let lines = s["lines"]
                .as_array()
                .expect("lines")
                .iter()
                .map(|l| l.as_str().expect("line").to_string())
                .collect();
            (s["title"].as_str().expect("title").to_string(), lines)
        })
        .collect()
}

fn section(card: &Value, title: &str) -> Option<Vec<String>> {
    sections(card)
        .into_iter()
        .find(|(t, _)| t == title)
        .map(|(_, lines)| lines)
}

fn actions(card: &Value) -> Vec<String> {
    card["actions"]
        .as_array()
        .expect("actions")
        .iter()
        .map(|a| a.as_str().expect("action").to_string())
        .collect()
}

fn color_ask(ago: u64) -> Value {
    json!({ "tu-1": { "at": NOW - ago, "questions": [{
        "question": "Which color?", "header": "Color", "multi_select": false,
        "options": [{ "label": "Red", "description": null }]
    }] } })
}

fn decision() -> Value {
    json!({
        "ask": "이 설계 초안대로 확정 spec 작성을 시작할까요?",
        "recap": "S3 채널 최소 설계 초안을 정리했어요.",
        "options": ["초안대로 진행", "고칠 행 있음"],
        "at": NOW - 20 * SEC,
    })
}

#[test]
fn an_open_question_waits_for_input_even_when_no_permission_event_came() {
    let (column, card) = only(json!([session_with(
        "s",
        json!([event("turn.start", None, 30 * SEC)]),
        json!({ "asks": color_ask(10 * SEC) })
    )]));
    assert_eq!(column, "needs_input");
    assert_eq!(card["reason"], "질문");
    assert_eq!(card["age"], "10s");
    assert_eq!(card["asks"][0]["tool_use_id"], "tu-1");
}

#[test]
fn an_open_question_waits_for_input_even_after_the_spinner_redraws() {
    let (column, _) = only(json!([session_with(
        "s",
        json!([
            event(
                "classic.PermissionRequest",
                Some("AskUserQuestion"),
                20 * SEC
            ),
            event("ui.spinner", None, 19 * SEC),
        ]),
        json!({ "asks": color_ask(20 * SEC) })
    )]));
    assert_eq!(column, "needs_input");
}

#[test]
fn a_stale_session_with_an_open_question_is_still_stale() {
    let mut record = session_with("s", json!([]), json!({ "asks": color_ask(10 * MIN) }));
    record["heartbeat_at"] = json!(NOW - 10 * MIN);
    let (column, _) = only(json!([record]));
    assert_eq!(column, "stale");
}

#[test]
fn a_decision_left_at_the_end_of_a_turn_waits_for_input() {
    let (column, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 15 * SEC)]),
        json!({ "decision": decision() })
    )]));
    assert_eq!(column, "needs_input");
    assert_eq!(card["reason"], "결정 요청");
    assert_eq!(card["age"], "15s");
    assert_eq!(
        card["detail"]["decision"],
        json!({
            "id": (NOW - 20 * SEC).to_string(),
            "ask": "이 설계 초안대로 확정 spec 작성을 시작할까요?",
            "recap": "S3 채널 최소 설계 초안을 정리했어요.",
            "options": ["초안대로 진행", "고칠 행 있음"],
        })
    );
    assert_eq!(actions(&card), ["decide", "send"]);
}

#[test]
fn a_decision_means_nothing_once_a_new_turn_runs() {
    let (column, card) = only(json!([session_with(
        "s",
        json!([
            event("turn.complete", None, 30 * SEC),
            event("turn.start", None, 5 * SEC),
        ]),
        json!({ "decision": decision(), "turn_id": "t-2" })
    )]));
    assert_eq!(column, "working");
    assert_eq!(card["detail"]["decision"], Value::Null);
}

#[test]
fn each_column_offers_its_own_actions() {
    let working = session_with(
        "w",
        json!([event("turn.start", None, 5 * SEC)]),
        json!({ "turn_id": "t-1" }),
    );
    let (_, card) = only(json!([working]));
    assert_eq!(actions(&card), ["stop"]);
    assert_eq!(card["turn_id"], "t-1");

    let idle = session_with(
        "i",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({}),
    );
    assert_eq!(actions(&only(json!([idle])).1), ["send"]);

    let asking = session_with(
        "q",
        json!([event("turn.start", None, 30 * SEC)]),
        json!({ "asks": color_ask(10 * SEC), "turn_id": "t-3" }),
    );
    assert_eq!(actions(&only(json!([asking])).1), ["answer", "stop"]);

    let permission = session_with(
        "p",
        json!([event("classic.PermissionRequest", Some("Bash"), 10 * SEC)]),
        json!({ "turn_id": "t-4" }),
    );
    assert_eq!(actions(&only(json!([permission])).1), ["stop"]);

    let mut stale = session_with(
        "x",
        json!([event("turn.start", None, 10 * MIN)]),
        json!({ "turn_id": "t-5" }),
    );
    stale["heartbeat_at"] = json!(NOW - 10 * MIN);
    assert!(actions(&only(json!([stale])).1).is_empty());
}

#[test]
fn the_kanban_request_header_is_not_shown_as_the_subtitle_or_the_prompt() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({
            "first_prompt": "[칸반 요청 · api 세션의 오케스트레이터]\n\nAdd a health check",
            "last_prompt": "[칸반 요청 · 오케스트레이터]\n\nRun lint",
        })
    )]));
    assert_eq!(card["subtitle"], "Add a health check");
    assert_eq!(
        section(&card, "직전 요청"),
        Some(vec!["Run lint".to_string()])
    );
}

#[test]
fn injected_reminders_are_stripped_from_the_subtitle_and_the_prompt() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({
            "first_prompt": "<system-reminder> You are operating in a git worktree.\n</system-reminder>\nFix the login bug",
            "last_prompt": "<system-reminder>\nworktree\n</system-reminder> Add a test",
        })
    )]));
    assert_eq!(card["subtitle"], "Fix the login bug");
    assert_eq!(
        section(&card, "직전 요청"),
        Some(vec!["Add a test".to_string()])
    );
}

#[test]
fn a_first_prompt_that_is_only_a_cut_reminder_falls_back_to_the_last_prompt() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({
            "first_prompt": "<system-reminder> You are operating in a git worktree at /w and the rest was cut",
            "last_prompt": "Refactor the parser",
        })
    )]));
    assert_eq!(card["subtitle"], "Refactor the parser");
}

#[test]
fn the_label_comes_from_the_remote_then_the_repo_root() {
    let label =
        |extra: Value| only(json!([session_with("s", json!([]), extra)])).1["label"].clone();
    assert_eq!(
        label(
            json!({ "remote": "git@github.com:kys0213/kys-claude-plugin.git", "repo_root": "/w/x" })
        ),
        "kys-claude-plugin"
    );
    assert_eq!(
        label(json!({ "remote": "https://github.com/o/web-app", "repo_root": "/w/x" })),
        "web-app"
    );
    assert_eq!(
        label(json!({ "remote": null, "repo_root": "/work/main-tree" })),
        "main-tree"
    );
    assert_eq!(
        label(json!({ "repo": "o/internal", "remote": "git@h:o/other.git" })),
        "internal"
    );
}

#[test]
fn an_idle_card_shows_what_was_done_just_before() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({
            "last_prompt": "Fix the flaky test",
            "last_answer": "\n\nFixed the race in `worker.rs`.\n\nAll 42 tests pass now.",
        })
    )]));
    assert_eq!(card["last_line"], "직전: Fixed the race in `worker.rs`.");
    assert_eq!(
        sections(&card),
        [
            (
                "직전 요청".to_string(),
                vec!["Fix the flaky test".to_string()]
            ),
            (
                "직전 응답".to_string(),
                vec![
                    "Fixed the race in `worker.rs`.".to_string(),
                    "All 42 tests pass now.".to_string()
                ]
            ),
        ]
    );
}

#[test]
fn a_working_card_shows_the_request_the_whole_todo_list_and_the_recent_tools() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.start", None, 30 * SEC)]),
        json!({
            "last_prompt": "Ship the release",
            "turn_id": "t-1",
            "todos": [
                { "status": "completed", "active": "Planning" },
                { "status": "in_progress", "active": "Running tests" },
                { "status": "pending", "active": "Tagging" },
            ],
            "recent": [
                { "tool": "Read", "input": { "file_path": "/w/src/lib.rs" }, "at": NOW - 20 * SEC },
                { "tool": "Bash", "input": { "command": "cargo test" }, "at": NOW - 3 * SEC },
            ],
        })
    )]));
    assert_eq!(card["last_line"], Value::Null);
    assert_eq!(
        sections(&card),
        [
            ("요청".to_string(), vec!["Ship the release".to_string()]),
            (
                "할 일".to_string(),
                vec![
                    "✓ Planning".to_string(),
                    "▸ Running tests".to_string(),
                    "· Tagging".to_string()
                ]
            ),
            (
                "최근 도구".to_string(),
                vec![
                    "Bash: cargo test · 3s".to_string(),
                    "Read: lib.rs · 20s".to_string()
                ]
            ),
        ]
    );
}

#[test]
fn a_long_todo_list_folds_the_completed_items() {
    let mut todos: Vec<Value> = (0..9)
        .map(|i| json!({ "status": "completed", "active": format!("done {i}") }))
        .collect();
    todos.push(json!({ "status": "in_progress", "active": "now" }));
    todos.push(json!({ "status": "pending", "active": "next" }));
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.start", None, 30 * SEC)]),
        json!({ "todos": todos })
    )]));
    assert_eq!(
        section(&card, "할 일"),
        Some(vec![
            "✓ 완료 9개".to_string(),
            "▸ now".to_string(),
            "· next".to_string()
        ])
    );
}

#[test]
fn a_permission_the_mod_can_take_an_answer_for_offers_allow_and_deny() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("classic.PermissionRequest", Some("Bash"), 10 * SEC)]),
        json!({
            "turn_id": "t-1",
            "permission": { "id": "t-1-7", "tool": "Bash", "input": { "command": "npm publish" }, "at": NOW - 10 * SEC },
        })
    )]));
    assert_eq!(actions(&card), ["permit", "stop"]);
    assert_eq!(card["permission_id"], "t-1-7");

    let (_, older) = only(json!([session_with(
        "s",
        json!([event("classic.PermissionRequest", Some("Bash"), 10 * SEC)]),
        json!({ "turn_id": "t-1", "permission": { "tool": "Bash", "input": {}, "at": NOW - 10 * SEC } })
    )]));
    assert_eq!(actions(&older), ["stop"]);
    assert_eq!(older["permission_id"], Value::Null);
}

#[test]
fn a_permission_wait_shows_what_it_asks_for_and_the_reply_before_it() {
    let (column, card) = only(json!([session_with(
        "s",
        json!([event("classic.PermissionRequest", Some("Bash"), 10 * SEC)]),
        json!({
            "permission": { "tool": "Bash", "input": { "command": "npm publish", "description": "Publish" }, "at": NOW - 10 * SEC },
            "pending_context": "Tests pass. Publishing now.",
            "last_prompt": "Release 1.2",
        })
    )]));
    assert_eq!(column, "needs_input");
    assert_eq!(
        sections(&card),
        [
            (
                "기다리는 것".to_string(),
                vec!["Bash: npm publish".to_string()]
            ),
            ("요청".to_string(), vec!["Release 1.2".to_string()]),
            (
                "기다리기 직전 응답".to_string(),
                vec!["Tests pass. Publishing now.".to_string()]
            ),
        ]
    );
}

#[test]
fn control_characters_are_dropped_from_replies() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({ "last_answer": "\u{1b}[31mred\u{1b}[0m done\u{7}" })
    )]));
    assert_eq!(
        section(&card, "직전 응답"),
        Some(vec!["red done".to_string()])
    );
}

#[test]
fn the_last_delivery_of_a_kanban_request_is_shown() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({ "inbox": { "id": "r1", "kind": "prompt", "status": "busy", "at": NOW - 4 * SEC } })
    )]));
    assert_eq!(
        card["detail"]["delivery"],
        "보낸 요청: 작업 중이라 받지 않았어요 · 4s"
    );
}

#[test]
fn each_delivery_outcome_reads_as_a_sentence() {
    for (kind, status, line) in [
        ("prompt", "delivered", "보낸 요청: 전달됐어요 · 4s"),
        ("prompt", "expired", "보낸 요청: 시간이 지나 버렸어요 · 4s"),
        ("stop", "stale", "중지 요청: 끝난 턴이라 버렸어요 · 4s"),
        ("answer", "delivered", "질문 답: 전달됐어요 · 4s"),
        ("answer", "stale", "질문 답: 끝난 질문이라 버렸어요 · 4s"),
        (
            "answer",
            "invalid",
            "질문 답: 답 수가 맞지 않아 버렸어요 · 4s",
        ),
        ("decide", "stale", "결정 답: 끝난 결정이라 버렸어요 · 4s"),
        ("decide", "busy", "결정 답: 작업 중이라 받지 않았어요 · 4s"),
        (
            "answer",
            "refused",
            "질문 답: 대리 조건이 맞지 않아 버렸어요 · 4s",
        ),
        (
            "permit",
            "refused",
            "권한 답: 대리 조건이 맞지 않아 버렸어요 · 4s",
        ),
        ("unknown", "invalid", "보낸 요청: invalid · 4s"),
        ("prompt", "queued-later", "보낸 요청: queued-later · 4s"),
    ] {
        let (_, card) = only(json!([session_with(
            "s",
            json!([event("turn.complete", None, 5 * SEC)]),
            json!({ "inbox": { "id": "r1", "kind": kind, "status": status, "at": NOW - 4 * SEC } })
        )]));
        assert_eq!(card["detail"]["delivery"], line, "{kind} {status}");
    }
}

#[test]
fn cards_carry_where_the_session_runs_and_its_permission_mode() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([]),
        json!({ "repo_root": "/work/project-a", "permission_mode": "auto" })
    )]));
    assert_eq!(card["cwd"], "/work/project-a");
    assert_eq!(card["repo_root"], "/work/project-a");
    assert_eq!(card["permission_mode"], "auto");
}

#[test]
fn sessions_are_grouped_by_repository_root_for_the_orchestrator() {
    let idle = session_with(
        "a1",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({ "repo_root": "/w/api", "remote": "git@h:o/api.git", "permission_mode": "default", "last_prompt": "Fix CORS", "last_answer": "Fixed CORS.", "inbox": { "id": "r1", "kind": "prompt", "status": "delivered", "at": NOW - 4 * SEC } }),
    );
    let mut worktree = session_with(
        "a2",
        json!([event("turn.start", None, 5 * SEC)]),
        json!({ "repo_root": "/w/api", "remote": "git@h:o/api.git", "turn_id": "t" }),
    );
    worktree["cwd"] = json!("/w/api/.claude/worktrees/x");
    let mut notes = session_with("n1", json!([]), json!({}));
    notes["cwd"] = json!("/tmp/notes");
    let b = board(json!([idle, worktree, notes]));
    assert_eq!(
        b["repos"],
        json!([
            {
                "label": "api",
                "root": "/w/api",
                "sessions": [
                    { "session_id": "a2", "column": "working", "cwd": "/w/api/.claude/worktrees/x", "permission_mode": null, "actions": ["stop"], "subtitle": null, "last_line": null, "delivery": null, "wait": null },
                    { "session_id": "a1", "column": "idle", "cwd": "/work/project-a", "permission_mode": "default", "actions": ["send"], "subtitle": "Fix CORS", "last_line": "직전: Fixed CORS.", "delivery": "보낸 요청: 전달됐어요 · 4s", "wait": null },
                ],
            },
            {
                "label": "notes",
                "root": null,
                "sessions": [
                    { "session_id": "n1", "column": "idle", "cwd": "/tmp/notes", "permission_mode": null, "actions": ["send"], "subtitle": null, "last_line": null, "delivery": null, "wait": null },
                ],
            },
        ])
    );
}

#[test]
fn a_wrong_typed_detail_field_skips_the_record() {
    let b = board(json!([session_with(
        "s",
        json!([]),
        json!({ "decision": "yes" })
    )]));
    let total: usize = b["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .map(|c| c["cards"].as_array().expect("cards").len())
        .sum();
    assert_eq!(total, 0);
}

#[test]
fn a_question_stays_open_through_a_queued_prompt_and_other_tools() {
    let (column, card) = only(json!([session_with(
        "s",
        json!([
            event(
                "classic.PermissionRequest",
                Some("AskUserQuestion"),
                30 * SEC
            ),
            event("prompt.submit", None, 20 * SEC),
            event("classic.PostToolUse", Some("Bash"), 15 * SEC),
            event("classic.PermissionRequest", Some("Bash"), 10 * SEC),
        ]),
        json!({ "asks": color_ask(30 * SEC) })
    )]));
    assert_eq!(column, "needs_input");
    assert_eq!(card["reason"], "질문");
    assert_eq!(card["asks"][0]["tool_use_id"], "tu-1");
}

#[test]
fn a_question_ends_with_its_own_result_or_the_turn() {
    for ended in [
        event("classic.PostToolUse", Some("AskUserQuestion"), 5 * SEC),
        event(
            "classic.PostToolUseFailure",
            Some("AskUserQuestion"),
            5 * SEC,
        ),
        event("kanban.answer", None, 5 * SEC),
        event("turn.complete", None, 5 * SEC),
    ] {
        let (_, card) = only(json!([session_with(
            "s",
            json!([event("turn.start", None, 30 * SEC), ended.clone()]),
            json!({ "asks": color_ask(10 * SEC) })
        )]));
        assert_eq!(card["asks"], json!([]), "{ended}");
    }
}

#[test]
fn the_spinner_does_not_end_a_question_even_without_its_record() {
    let (column, card) = only(json!([session_with(
        "s",
        json!([
            event(
                "classic.PermissionRequest",
                Some("AskUserQuestion"),
                20 * SEC
            ),
            event("ui.spinner", None, 19 * SEC),
        ]),
        json!({})
    )]));
    assert_eq!(column, "needs_input");
    assert_eq!(card["reason"], "질문");
}

#[test]
fn the_request_shown_is_the_latest_one_a_person_wrote() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({
            "last_prompt": "<side-sessions-event>None of the sessions</side-sessions-event>",
            "prompts": [
                { "text": "Fix CORS", "origin": "composer", "at": NOW - 9 * MIN },
                { "text": "Run lint", "origin": "kanban", "at": NOW - 8 * MIN },
                { "text": "<side-sessions-event>None of the sessions you started is working now</side-sessions-event>", "origin": "sdk", "at": NOW - 7 * MIN },
                { "text": "<task-notification>done</task-notification>", "origin": "task-notification", "at": NOW - 6 * MIN },
                { "text": "Scheduled check", "origin": "scheduled-trigger", "at": NOW - 5 * MIN },
            ],
        })
    )]));
    assert_eq!(
        section(&card, "직전 요청"),
        Some(vec!["Run lint".to_string()])
    );
}

#[test]
fn an_event_written_as_a_first_prompt_is_not_the_subtitle() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({
            "first_prompt": "<side-sessions-event>None of the sessions you started is working now",
            "prompts": [{ "text": "Fix CORS", "origin": "sdk", "at": NOW - MIN }],
        })
    )]));
    assert_eq!(card["subtitle"], "Fix CORS");
}

#[test]
fn an_event_set_as_the_session_title_is_not_the_subtitle() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({
            "title": "<task-notification> <task-id>a0258</task-id>",
            "first_prompt": "Fix CORS",
        })
    )]));
    assert_eq!(card["subtitle"], "Fix CORS");
}

#[test]
fn a_record_without_prompts_still_shows_its_last_prompt() {
    let (_, card) = only(json!([session_with(
        "s",
        json!([event("turn.complete", None, 5 * SEC)]),
        json!({ "last_prompt": "Old style record" })
    )]));
    assert_eq!(
        section(&card, "직전 요청"),
        Some(vec!["Old style record".to_string()])
    );
}
