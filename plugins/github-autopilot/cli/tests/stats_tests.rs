mod mock_fs;
mod mock_git;

use autopilot::cmd::stats::StatsService;
use mock_fs::MockFs;
use mock_git::MockGit;

fn make_svc(git: MockGit, fs: MockFs) -> StatsService {
    StatsService::new(Box::new(git), Box::new(fs))
}

#[test]
fn init_creates_stats_file() {
    let git = MockGit::new();
    let fs = MockFs::new();

    let code = make_svc(git, fs.clone()).init().unwrap();
    assert_eq!(code, 0);

    let written = fs.written_files();
    assert_eq!(written.len(), 1);
    assert!(written[0]
        .0
        .to_str()
        .unwrap()
        .contains("session-stats.json"));

    let stats: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert!(stats["started_at"].is_string());
    assert_eq!(stats["commands"], serde_json::json!({}));
}

#[test]
fn update_accumulates_stats() {
    let git = MockGit::new();
    let fs = MockFs::new();
    let svc = make_svc(git, fs.clone());

    // First update (no existing file — auto-init)
    let code = svc.update("build-issues", 3, 2, 1, 0).unwrap();
    assert_eq!(code, 0);

    // Simulate second update by providing the written state
    let written = fs.written_files();
    let git2 = MockGit::new();
    let fs2 = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/session-stats.json",
        &written[0].1,
    );
    let svc2 = make_svc(git2, fs2.clone());

    let code = svc2.update("build-issues", 1, 1, 0, 0).unwrap();
    assert_eq!(code, 0);

    let written2 = fs2.written_files();
    let stats: serde_json::Value = serde_json::from_str(&written2[0].1).unwrap();
    let cmd = &stats["commands"]["build-issues"];
    assert_eq!(cmd["total_cycles"], 2);
    assert_eq!(cmd["processed"], 4);
    assert_eq!(cmd["success"], 3);
    assert_eq!(cmd["failed"], 1);
}

#[test]
fn update_idle_cycle_increments_consecutive() {
    let git = MockGit::new();
    let fs = MockFs::new();
    let svc = make_svc(git, fs.clone());

    // processed=0 means idle
    let code = svc.update("build-issues", 0, 0, 0, 0).unwrap();
    assert_eq!(code, 0);

    let written = fs.written_files();
    let stats: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    let cmd = &stats["commands"]["build-issues"];
    assert_eq!(cmd["idle_cycles"], 1);
    assert_eq!(cmd["consecutive_idle"], 1);
}

#[test]
fn update_active_cycle_resets_consecutive_idle() {
    let git = MockGit::new();
    // Pre-seed with 3 consecutive idle cycles
    let fs = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/session-stats.json",
        r#"{"started_at":"2026-04-13T00:00:00Z","commands":{"build-issues":{"total_cycles":3,"processed":0,"success":0,"failed":0,"false_positive":0,"idle_cycles":3,"consecutive_idle":3,"agent_calls":0}}}"#,
    );
    let svc = make_svc(git, fs.clone());

    let code = svc.update("build-issues", 2, 2, 0, 0).unwrap();
    assert_eq!(code, 0);

    let written = fs.written_files();
    let stats: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    let cmd = &stats["commands"]["build-issues"];
    assert_eq!(cmd["consecutive_idle"], 0);
    assert_eq!(cmd["idle_cycles"], 3); // idle_cycles doesn't reset
    assert_eq!(cmd["total_cycles"], 4);
}

#[test]
fn show_returns_0_when_no_stats() {
    let git = MockGit::new();
    let fs = MockFs::new();

    let code = make_svc(git, fs).show(None).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn show_with_command_filter() {
    let git = MockGit::new();
    let fs = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/session-stats.json",
        r#"{"started_at":"2026-04-13T00:00:00Z","commands":{"build-issues":{"total_cycles":5,"processed":3,"success":2,"failed":1,"false_positive":0,"idle_cycles":2,"consecutive_idle":0,"agent_calls":3}}}"#,
    );

    let code = make_svc(git, fs).show(Some("build-issues")).unwrap();
    assert_eq!(code, 0);
}
