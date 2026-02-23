use std::path::Path;

use autodev::daemon::status::{write_status, DaemonStatus, StatusCounters, StatusItem};
use autodev::tui::views;

// ─── query_active_items ───

#[test]
fn test_active_items_no_status_file_returns_empty() {
    let items = views::query_active_items(Path::new("/tmp/nonexistent-status.json"));
    assert!(items.is_empty());
}

#[test]
fn test_active_items_reads_from_status_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("daemon.status.json");

    let status = DaemonStatus {
        updated_at: "2026-02-23T14:00:00+09:00".to_string(),
        uptime_secs: 100,
        active_items: vec![
            StatusItem {
                work_id: "issue:org/repo:1".to_string(),
                queue_type: "issue".to_string(),
                repo_name: "org/repo".to_string(),
                number: 1,
                title: "Fix bug".to_string(),
                phase: "Pending".to_string(),
            },
            StatusItem {
                work_id: "pr:org/repo:10".to_string(),
                queue_type: "pr".to_string(),
                repo_name: "org/repo".to_string(),
                number: 10,
                title: "Add feature".to_string(),
                phase: "ReviewDone".to_string(),
            },
        ],
        counters: StatusCounters {
            wip: 2,
            done: 5,
            skip: 1,
            failed: 0,
        },
    };
    write_status(&path, &status);

    let items = views::query_active_items(&path);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].queue_type, "issue");
    assert_eq!(items[0].number, 1);
    assert_eq!(items[0].status, "Pending");
    assert_eq!(items[1].queue_type, "pr");
    assert_eq!(items[1].number, 10);
}

// ─── query_label_counts ───

#[test]
fn test_label_counts_no_status_file_returns_zeros() {
    let counts = views::query_label_counts(Path::new("/tmp/nonexistent-status.json"));
    assert_eq!(counts.wip, 0);
    assert_eq!(counts.done, 0);
    assert_eq!(counts.skip, 0);
    assert_eq!(counts.failed, 0);
}

#[test]
fn test_label_counts_reads_from_status_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("daemon.status.json");

    let status = DaemonStatus {
        updated_at: "2026-02-23T14:00:00+09:00".to_string(),
        uptime_secs: 100,
        active_items: vec![],
        counters: StatusCounters {
            wip: 3,
            done: 10,
            skip: 2,
            failed: 1,
        },
    };
    write_status(&path, &status);

    let counts = views::query_label_counts(&path);
    assert_eq!(counts.wip, 3);
    assert_eq!(counts.done, 10);
    assert_eq!(counts.skip, 2);
    assert_eq!(counts.failed, 1);
}

// ─── Panel navigation ───

#[test]
fn test_panel_navigation() {
    let mut state = views::AppState::new();
    assert_eq!(state.active_panel, views::Panel::ActiveItems);

    state.next_panel();
    assert_eq!(state.active_panel, views::Panel::Labels);

    state.next_panel();
    assert_eq!(state.active_panel, views::Panel::Logs);

    state.next_panel();
    assert_eq!(state.active_panel, views::Panel::Repos);

    state.next_panel();
    assert_eq!(state.active_panel, views::Panel::ActiveItems);
}
