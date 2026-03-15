use std::path::Path;

use autodev::core::models::QueueType;
use autodev::service::daemon::status::{write_status, DaemonStatus, StatusItem};
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
                queue_type: QueueType::Issue,
                repo_name: "org/repo".to_string(),
                number: 1,
                title: "Fix bug".to_string(),
                phase: "Pending".to_string(),
            },
            StatusItem {
                work_id: "pr:org/repo:10".to_string(),
                queue_type: QueueType::Pr,
                repo_name: "org/repo".to_string(),
                number: 10,
                title: "Add feature".to_string(),
                phase: "ReviewDone".to_string(),
            },
        ],
        wip: 2,
    };
    write_status(&path, &status);

    let items = views::query_active_items(&path);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].queue_type, QueueType::Issue);
    assert_eq!(items[0].number, 1);
    assert_eq!(items[0].status, "Pending");
    assert_eq!(items[1].queue_type, QueueType::Pr);
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
fn test_label_counts_reads_wip_from_status_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("daemon.status.json");

    let status = DaemonStatus {
        updated_at: "2026-02-23T14:00:00+09:00".to_string(),
        uptime_secs: 100,
        active_items: vec![],
        wip: 3,
    };
    write_status(&path, &status);

    let counts = views::query_label_counts(&path);
    assert_eq!(counts.wip, 3);
    // done/skip/failed are not tracked in status file
    assert_eq!(counts.done, 0);
    assert_eq!(counts.skip, 0);
    assert_eq!(counts.failed, 0);
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
