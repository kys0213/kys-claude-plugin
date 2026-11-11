mod kanban_mocks;

use atelier::kanban::core::request::{OpenAsk, Sender, Waits};
use kanban_mocks::{Edges, FakeLauncher, MapProbe};
use serde_json::json;

const NOW: u64 = 1_800_000_000_000;
const TTL: u64 = 60_000;

#[test]
fn a_sent_prompt_is_queued_under_the_next_id_and_taken_once() {
    let e = Edges::default();
    let out = e.requests().send("s-1", Sender::Pane, None, "Run lint");
    assert_eq!(
        out,
        json!({ "queued": { "session_id": "s-1", "id": "r1", "kind": "prompt" } })
    );
    assert_eq!(e.inbox.ids("s-1"), ["r1"]);

    let taken = e.requests().take("s-1", NOW, TTL, None, &Waits::default());
    assert_eq!(
        taken["actions"],
        json!([{ "kind": "prompt", "text": "Run lint", "body": "Run lint", "from": "pane" }])
    );
    assert!(e.inbox.ids("s-1").is_empty());
}

#[test]
fn a_prompt_over_the_length_limit_is_refused() {
    let e = Edges::default();
    let out = e
        .requests()
        .send("s-1", Sender::Agent, None, &"x".repeat(8_001));
    assert!(out["error"].is_string(), "{out}");
    assert!(e.inbox.ids("s-1").is_empty());
}

#[test]
fn a_request_older_than_the_ttl_is_expired_whatever_its_kind() {
    let e = Edges::default();
    e.requests().send("s-1", Sender::Pane, None, "late");
    e.requests().stop("s-1", "t-1");
    *e.inbox.age_ms.borrow_mut() = TTL + 1;
    let out = e
        .requests()
        .take("s-1", NOW, TTL, Some("t-1"), &Waits::default());
    assert_eq!(out["actions"], json!([]));
    assert_eq!(
        out["results"],
        json!([
            { "id": "r1", "kind": "prompt", "status": "expired" },
            { "id": "r2", "kind": "stop", "status": "expired" },
        ])
    );
}

#[test]
fn an_unreadable_request_reports_an_unknown_kind() {
    let e = Edges::default();
    e.inbox.put_raw("s-1", "r9", "{\"v\":2}");
    let out = e.requests().take("s-1", NOW, TTL, None, &Waits::default());
    assert_eq!(
        out["results"],
        json!([{ "id": "r9", "kind": "unknown", "status": "invalid" }])
    );
}

#[test]
fn spawn_launches_claude_bg_at_a_main_root_under_the_agent_header() {
    let e = Edges {
        probe: MapProbe::default().with("/w/api", "/w/api"),
        ..Default::default()
    };
    let out = e
        .requests()
        .spawn("/w/api", " Add a check ", Some("check-1"), Some("main"));
    assert_eq!(
        out,
        json!({ "spawned": { "short_id": "abc123", "cwd": "/w/api", "worktree": "check-1" } })
    );
    assert_eq!(
        e.launcher.launches.borrow().as_slice(),
        [(
            "/w/api".to_string(),
            vec![
                "--bg".to_string(),
                "--worktree".to_string(),
                "check-1".to_string(),
                "[칸반 요청 · main 세션의 오케스트레이터]\n\nAdd a check".to_string(),
            ]
        )]
    );
}

#[test]
fn spawn_refuses_a_subdirectory_or_a_linked_worktree() {
    let e = Edges {
        probe: MapProbe::default()
            .with("/w/api/src", "/w/api")
            .with_git_dir("/w/api-wt", "/w/api-wt", "/w/api/.git/worktrees/api-wt"),
        ..Default::default()
    };
    for cwd in ["/w/api/src", "/w/api-wt", "/elsewhere"] {
        let out = e.requests().spawn(cwd, "x", None, None);
        assert!(out["error"].is_string(), "{cwd} → {out}");
    }
    assert!(e.launcher.launches.borrow().is_empty());
}

#[test]
fn spawn_refuses_an_empty_task_and_a_worktree_name_over_the_limit() {
    let e = Edges {
        probe: MapProbe::default().with("/w/api", "/w/api"),
        ..Default::default()
    };
    let long = "a".repeat(41);
    for (text, worktree) in [("   ", None), ("x", Some(long.as_str()))] {
        let out = e.requests().spawn("/w/api", text, worktree, None);
        assert!(out["error"].is_string(), "{text:?} {worktree:?} → {out}");
    }
    assert!(e.launcher.launches.borrow().is_empty());
}

#[test]
fn spawn_reports_what_a_failed_launch_printed() {
    let e = Edges {
        probe: MapProbe::default().with("/w/api", "/w/api"),
        launcher: FakeLauncher {
            out: Ok((false, String::new(), "not logged in".to_string())),
            launches: Default::default(),
        },
        ..Default::default()
    };
    let out = e.requests().spawn("/w/api", "x", None, None);
    assert_eq!(out["error"], "새 세션을 띄우지 못했어요: not logged in");
}

#[test]
fn a_permit_answer_is_waited_for_by_the_permission_it_names() {
    let e = Edges::default();
    let out = e.requests().permit("s-1", "p-1", "allow", Sender::Pane);
    assert_eq!(
        out,
        json!({ "queued": { "session_id": "s-1", "id": "p-1", "kind": "permit" } })
    );
    assert!(e.inbox.ids("s-1").is_empty());

    assert_eq!(
        e.requests().wait_permit("s-1", "p-2", NOW, TTL, 2_000),
        json!({ "decision": null })
    );
    assert_eq!(
        e.requests().wait_permit("s-1", "p-1", NOW, TTL, 2_000),
        json!({ "decision": "allow", "from": "pane" })
    );
    assert_eq!(
        e.requests().wait_permit("s-1", "p-1", NOW, TTL, 2_000),
        json!({ "decision": null })
    );
}

#[test]
fn waiting_for_a_permit_pauses_until_the_wait_is_over() {
    let e = Edges::default();
    e.requests().wait_permit("s-1", "p-1", NOW, TTL, 1_000);
    assert_eq!(*e.pause.paused_ms.borrow(), 1_000);
}

#[test]
fn a_permit_answer_older_than_the_ttl_is_dropped() {
    let e = Edges::default();
    e.requests().permit("s-1", "p-1", "deny", Sender::Pane);
    *e.inbox.age_ms.borrow_mut() = TTL + 1;
    assert_eq!(
        e.requests().wait_permit("s-1", "p-1", NOW, TTL, 2_000),
        json!({ "decision": null, "status": "expired" })
    );
}

#[test]
fn a_permit_needs_allow_or_deny_and_plain_ids() {
    let e = Edges::default();
    for (session, request, decision) in [
        ("s-1", "p-1", "maybe"),
        ("s/1", "p-1", "allow"),
        ("s-1", "../p", "allow"),
    ] {
        let out = e
            .requests()
            .permit(session, request, decision, Sender::Pane);
        assert!(
            out["error"].is_string(),
            "{session} {request} {decision} → {out}"
        );
    }
}

fn answers(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| a.to_string()).collect()
}

fn open(asks: &[(&str, usize)], decision_at: Option<&str>) -> Waits {
    Waits {
        asks: asks
            .iter()
            .map(|(id, questions)| OpenAsk {
                id: id.to_string(),
                questions: *questions,
            })
            .collect(),
        decision_at: decision_at.map(str::to_string),
    }
}

#[test]
fn a_reply_reaches_its_open_question_once() {
    let e = Edges::default();
    let out = e
        .requests()
        .reply("s-1", "tu-1", &answers(&["Red"]), Sender::Pane);
    assert_eq!(
        out,
        json!({ "queued": { "session_id": "s-1", "id": "r1", "kind": "answer" } })
    );
    e.requests()
        .reply("s-1", "tu-1", &answers(&["Blue"]), Sender::Pane);

    let taken = e
        .requests()
        .take("s-1", NOW, TTL, Some("t-1"), &open(&[("tu-1", 1)], None));
    assert_eq!(
        taken["actions"],
        json!([{ "kind": "answer", "tool_use_id": "tu-1", "answers": ["Red"], "from": "pane" }])
    );
    assert_eq!(
        taken["results"],
        json!([
            { "id": "r1", "kind": "answer", "status": "delivered" },
            { "id": "r2", "kind": "answer", "status": "stale" },
        ])
    );
}

#[test]
fn a_decide_counts_as_the_one_prompt_a_take_runs() {
    let e = Edges::default();
    e.requests().decide("s-1", "1700", "네", Sender::Pane);
    e.requests().send("s-1", Sender::Pane, None, "later");
    let taken = e
        .requests()
        .take("s-1", NOW, TTL, None, &open(&[], Some("1700")));
    assert_eq!(
        taken["actions"],
        json!([{ "kind": "decide", "decision_at": "1700", "text": "네", "from": "pane" }])
    );
    assert_eq!(taken["results"][1]["status"], "busy");
}

#[test]
fn an_expired_answer_is_expired_before_it_is_judged() {
    let e = Edges::default();
    e.requests()
        .reply("s-1", "tu-1", &answers(&["Red"]), Sender::Pane);
    *e.inbox.age_ms.borrow_mut() = TTL + 1;
    let taken = e
        .requests()
        .take("s-1", NOW, TTL, None, &open(&[("tu-1", 1)], None));
    assert_eq!(taken["results"][0]["status"], "expired");
}

#[test]
fn agent_answers_are_refused_and_nothing_is_queued() {
    let e = Edges::default();
    for out in [
        e.requests()
            .reply("s-1", "tu-1", &answers(&["Red"]), Sender::Agent),
        e.requests().decide("s-1", "1700", "네", Sender::Agent),
        e.requests().permit("s-1", "p-1", "allow", Sender::Agent),
    ] {
        assert!(
            out["error"]
                .as_str()
                .is_some_and(|e| e.contains("장부가 필요해요")),
            "{out}"
        );
    }
    assert!(e.inbox.ids("s-1").is_empty());
    assert!(e.inbox.permits.borrow().is_empty());
}

#[test]
fn a_reply_needs_at_least_one_answer() {
    let e = Edges::default();
    let out = e.requests().reply("s-1", "tu-1", &[], Sender::Pane);
    assert!(out["error"].is_string(), "{out}");
}
