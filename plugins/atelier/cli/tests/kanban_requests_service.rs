mod kanban_mocks;

use atelier::kanban::core::request::Sender;
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

    let taken = e.requests().take("s-1", NOW, TTL, None);
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
    let out = e.requests().take("s-1", NOW, TTL, Some("t-1"));
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
    let out = e.requests().take("s-1", NOW, TTL, None);
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
