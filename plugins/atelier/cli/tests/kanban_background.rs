mod kanban_support;

use kanban_support::{agent_event, board, event, session_with, MIN, NOW, SEC};
use serde_json::{json, Value};

/// The column and card of the one session on the board.
fn only(sessions: Value) -> (String, Value) {
    let b = board(sessions);
    b["columns"]
        .as_array()
        .expect("columns is an array")
        .iter()
        .find_map(|c| {
            let card = c["cards"].as_array().expect("cards").first()?.clone();
            Some((c["id"].as_str().expect("column id").to_string(), card))
        })
        .expect("one card")
}

fn placed(events: Value, extra: Value) -> (String, Value) {
    only(json!([session_with("s", events, extra)]))
}

fn column(events: Value, extra: Value) -> String {
    placed(events, extra).0
}

fn agent(status: &str, teammate: bool) -> Value {
    json!({ "status": status, "teammate": teammate })
}

fn actions(card: &Value) -> Vec<String> {
    card["actions"]
        .as_array()
        .expect("actions is an array")
        .iter()
        .map(|a| a.as_str().expect("action id").to_string())
        .collect()
}

#[test]
fn a_turn_that_left_agents_workflows_or_cloud_sessions_running_stays_working() {
    for left in ["subagent", "workflow", "cloud session", "shell,subagent"] {
        assert_eq!(
            column(
                json!([
                    event("turn.start", None, MIN),
                    event("turn.complete", Some(left), 30 * SEC),
                ]),
                json!({})
            ),
            "working",
            "{left}"
        );
    }
}

#[test]
fn a_turn_that_left_only_shells_or_monitors_running_is_idle() {
    for left in ["shell", "monitor", "shell,monitor,MCP task", ""] {
        assert_eq!(
            column(
                json!([
                    event("turn.start", None, MIN),
                    event("turn.complete", Some(left), 30 * SEC),
                ]),
                json!({})
            ),
            "idle",
            "{left}"
        );
    }
}

#[test]
fn background_work_counts_its_age_from_when_the_work_began() {
    let swarm = json!([
        event("prompt.submit", None, 10 * MIN),
        event("turn.start", None, 10 * MIN),
        event("turn.complete", Some("subagent,subagent"), 8 * MIN),
        event("prompt.submit", None, 3 * MIN),
        event("turn.start", None, 3 * MIN),
        event("turn.complete", Some("subagent"), 2 * MIN),
    ]);
    let (column, card) = placed(swarm.clone(), json!({}));
    assert_eq!(column, "working");
    assert_eq!(card["age"], "10m");

    let mut done = swarm.as_array().expect("events").clone();
    done.push(event("prompt.submit", None, 90 * SEC));
    done.push(event("turn.complete", None, MIN));
    let (column, card) = placed(Value::Array(done), json!({}));
    assert_eq!(column, "idle");
    assert_eq!(card["age"], "1m");
}

#[test]
fn the_idle_prompt_notification_does_not_end_background_work() {
    assert_eq!(
        column(
            json!([
                event("turn.start", None, 3 * MIN),
                event("turn.complete", Some("workflow"), 2 * MIN),
                event("classic.Notification", Some("idle_prompt"), MIN),
            ]),
            json!({})
        ),
        "working"
    );
}

#[test]
fn teammates_keep_the_session_working_only_while_one_of_them_works() {
    let events = json!([
        event("turn.start", None, 5 * MIN),
        event("turn.complete", Some("teammate,teammate"), 2 * MIN),
    ]);

    let (column, card) = placed(
        events.clone(),
        json!({ "agents": [agent("idle", true), agent("running", true)] }),
    );
    assert_eq!(column, "working");
    assert_eq!(card["age"], "5m");

    let (column, card) = placed(
        events,
        json!({ "agents": [agent("idle", true), agent("idle", true)] }),
    );
    assert_eq!(column, "idle");
    assert_eq!(card["age"], "2m");
}

#[test]
fn a_working_agent_keeps_the_session_working_even_when_the_turn_end_listed_none() {
    for status in ["pending", "running", "waiting"] {
        assert_eq!(
            column(
                json!([event("turn.complete", None, MIN)]),
                json!({ "agents": [agent(status, false)] })
            ),
            "working",
            "{status}"
        );
    }
    for status in ["idle", "completed", "failed", "killed", "something-new"] {
        assert_eq!(
            column(
                json!([event("turn.complete", None, MIN)]),
                json!({ "agents": [agent(status, false)] })
            ),
            "idle",
            "{status}"
        );
    }
}

#[test]
fn a_backgrounded_card_names_what_runs_and_offers_no_action() {
    let (column, card) = placed(
        json!([
            event("turn.start", None, 3 * MIN),
            event(
                "turn.complete",
                Some("subagent,workflow,shell,cloud session"),
                MIN
            ),
        ]),
        json!({ "agents": [
            agent("running", false),
            agent("running", false),
            agent("completed", false),
            agent("running", true),
            agent("idle", true),
        ] }),
    );
    assert_eq!(column, "working");
    assert_eq!(
        card["progress"],
        "▸ 백그라운드: 에이전트 2 · 팀원 1 · 워크플로 1 · 원격 세션 1"
    );
    assert_eq!(card["last_line"], Value::Null);
    assert!(actions(&card).is_empty());
}

#[test]
fn the_background_line_follows_the_to_do_count() {
    let (_, card) = placed(
        json!([event("turn.complete", Some("workflow"), MIN)]),
        json!({ "todos": [
            { "status": "completed", "active": "Planning" },
            { "status": "in_progress", "active": "Fanning out" },
        ] }),
    );
    assert_eq!(
        card["progress"],
        "할 일 1/2 · Fanning out · ▸ 백그라운드: 워크플로 1"
    );
}

#[test]
fn a_decision_left_at_the_end_of_a_turn_waits_for_input_over_background_work() {
    let (column, card) = placed(
        json!([
            event("turn.start", None, 5 * MIN),
            event("turn.complete", Some("subagent"), 15 * SEC),
        ]),
        json!({ "decision": {
            "ask": "계속할까요?",
            "recap": null,
            "options": ["네"],
            "at": NOW - 20 * SEC,
        } }),
    );
    assert_eq!(column, "needs_input");
    assert_eq!(card["reason"], "결정 요청");
    assert_eq!(card["age"], "15s");
}

#[test]
fn a_running_turn_shows_its_tool_not_the_background() {
    let (column, card) = placed(
        json!([
            event("turn.complete", Some("subagent"), 2 * MIN),
            event("prompt.submit", None, SEC),
        ]),
        json!({
            "agents": [agent("running", false)],
            "activity": { "tool": "Read", "input": { "file_path": "/a/b.rs" }, "at": NOW - SEC },
        }),
    );
    assert_eq!(column, "working");
    assert_eq!(card["progress"], "▸ Read: b.rs");
}

#[test]
fn a_subagent_finishing_a_tool_keeps_the_background_work() {
    let (column, card) = placed(
        json!([
            event("turn.start", None, 5 * MIN),
            event("turn.complete", Some("subagent"), 4 * MIN),
            agent_event("classic.PostToolUse", Some("Bash"), 3 * MIN),
            agent_event("classic.PostToolUseFailure", Some("Read"), 2 * MIN),
        ]),
        json!({ "agents": [agent("running", false)] }),
    );
    assert_eq!(column, "working");
    assert_eq!(card["age"], "5m");
    assert_eq!(card["progress"], "▸ 백그라운드: 에이전트 1");
}

#[test]
fn the_idle_prompt_after_a_subagent_tool_does_not_end_background_work() {
    assert_eq!(
        column(
            json!([
                event("turn.start", None, 5 * MIN),
                event("turn.complete", Some("subagent"), 4 * MIN),
                agent_event("classic.PostToolUse", Some("Bash"), 3 * MIN),
                event("classic.Notification", Some("idle_prompt"), 2 * MIN),
            ]),
            json!({})
        ),
        "working"
    );
}

#[test]
fn a_subagent_permission_request_waits_for_input_then_returns_to_the_background() {
    let asked = json!([
        event("turn.start", None, 6 * MIN),
        event("turn.complete", Some("subagent"), 5 * MIN),
        agent_event("classic.PermissionRequest", Some("Bash"), 3 * MIN),
        event(
            "classic.Notification",
            Some("permission_prompt"),
            3 * MIN - 6 * SEC
        ),
    ]);
    let (column, card) = placed(asked.clone(), json!({}));
    assert_eq!(column, "needs_input");
    assert_eq!(card["reason"], "권한 요청: Bash");
    assert_eq!(card["age"], "3m");

    for answer in [
        agent_event("classic.PostToolUse", Some("Bash"), MIN),
        agent_event("kanban.answer", None, MIN),
        event("ui.spinner", None, MIN),
    ] {
        let mut events = asked.as_array().expect("events").clone();
        events.push(answer.clone());
        let (column, card) = placed(Value::Array(events), json!({}));
        assert_eq!(column, "working", "{answer}");
        assert_eq!(card["age"], "6m", "{answer}");
        assert_eq!(card["progress"], "▸ 백그라운드 작업", "{answer}");
    }
}

#[test]
fn a_subagent_finishing_a_tool_does_not_answer_the_main_loops_permission_request() {
    let (column, card) = placed(
        json!([
            event("turn.start", None, 3 * MIN),
            event("classic.PermissionRequest", Some("Bash"), 2 * MIN),
            agent_event("classic.PostToolUse", Some("Read"), MIN),
        ]),
        json!({}),
    );
    assert_eq!(column, "needs_input");
    assert_eq!(card["reason"], "권한 요청: Bash");
    assert_eq!(card["age"], "2m");
}

#[test]
fn a_subagent_finishing_a_tool_does_not_wake_an_idle_session() {
    assert_eq!(
        column(
            json!([
                event("turn.complete", None, 2 * MIN),
                agent_event("classic.PostToolUse", Some("Bash"), MIN),
            ]),
            json!({})
        ),
        "idle"
    );
}

#[test]
fn a_malformed_agents_field_skips_the_record() {
    for agents in [
        json!("nope"),
        json!([{ "status": 1, "teammate": false }]),
        json!([{ "status": "running" }]),
    ] {
        let b = board(json!([session_with(
            "s",
            json!([event("turn.start", None, SEC)]),
            json!({ "agents": agents.clone() })
        )]));
        let total: usize = b["columns"]
            .as_array()
            .expect("columns")
            .iter()
            .map(|c| c["cards"].as_array().expect("cards").len())
            .sum();
        assert_eq!(total, 0, "{agents}");
    }
}
