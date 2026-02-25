use std::path::Path;

use autodev::infrastructure::gh::mock::MockGh;
use autodev::queue::repository::*;
use autodev::queue::task_queues::{issue_phase, pr_phase, TaskQueues};
use autodev::queue::Database;

// ─── Helpers ───

fn fixture_response(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/responses")
        .join(name);
    std::fs::read(path).expect("read fixture file")
}

fn open_memory_db() -> Database {
    let db = Database::open(Path::new(":memory:")).expect("open in-memory db");
    db.initialize().expect("initialize schema");
    db
}

fn add_repo(db: &Database, url: &str, name: &str) -> String {
    db.repo_add(url, name).expect("add repo")
}

fn mock_gh_with_fixture(repo_name: &str, endpoint: &str, fixture: &str) -> MockGh {
    let gh = MockGh::new();
    gh.set_paginate(repo_name, endpoint, fixture_response(fixture));
    gh
}

// ═══════════════════════════════════════════════
// 1. Issue scanner
// ═══════════════════════════════════════════════

#[tokio::test]
async fn scan_issues_queues_new_items() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    // Label-Positive: fixture에 autodev:analyze 라벨이 있는 이슈만 반환
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues_with_analyze_label.json");

    let ignore = vec!["dependabot".to_string()];
    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &ignore,
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert!(queues.contains("issue:org/repo:42")); // alice
    assert!(queues.contains("issue:org/repo:43")); // bob
    assert!(!queues.contains("issue:org/repo:44")); // PR-linked → skipped
    assert!(!queues.contains("issue:org/repo:45")); // dependabot → skipped

    // analyze 라벨 제거 + wip 라벨 추가 검증
    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(_, n, l)| *n == 42 && l == "autodev:analyze"));
    assert!(removed
        .iter()
        .any(|(_, n, l)| *n == 43 && l == "autodev:analyze"));

    let added = gh.added_labels.lock().unwrap();
    assert!(added.iter().any(|(_, n, l)| *n == 42 && l == "autodev:wip"));
    assert!(added.iter().any(|(_, n, l)| *n == 43 && l == "autodev:wip"));
}

#[tokio::test]
async fn scan_issues_skips_pr_linked() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues.json");

    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert!(!queues.contains("issue:org/repo:44"));
}

#[tokio::test]
async fn scan_issues_filters_by_label() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues_with_labels.json");

    let labels = Some(vec!["bug".to_string()]);
    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &labels,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert!(queues.contains("issue:org/repo:50")); // has "bug"
    assert!(!queues.contains("issue:org/repo:51")); // only "enhancement"
}

/// Label-Positive: scan()은 API 레벨에서 autodev:analyze 라벨 필터를 적용하므로,
/// 다른 autodev:* 라벨이 있는 이슈는 API 응답에 포함되지 않는다.
/// 이 테스트는 autodev:analyze 라벨이 있는 이슈만 큐에 적재되고
/// analyze→wip 전이가 정상 동작함을 검증한다.
#[tokio::test]
async fn scan_issues_only_picks_up_analyze_label() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    // API가 autodev:analyze 라벨 이슈만 반환하는 시나리오
    let gh = MockGh::new();
    let issues = serde_json::json!([
        {
            "number": 70,
            "title": "Triggered for analysis",
            "body": "analyze me",
            "labels": [{"name": "bug"}, {"name": "autodev:analyze"}],
            "user": {"login": "alice"},
            "pull_request": null
        }
    ]);
    gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());

    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert!(queues.contains("issue:org/repo:70"));

    // analyze 제거 + wip 추가
    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(_, n, l)| *n == 70 && l == "autodev:analyze"));

    let added = gh.added_labels.lock().unwrap();
    assert!(added.iter().any(|(_, n, l)| *n == 70 && l == "autodev:wip"));
}

#[tokio::test]
async fn scan_issues_no_duplicates() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues.json");

    let ignore = vec!["dependabot".to_string()];
    let mut queues = TaskQueues::new();

    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &ignore,
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();
    let count_first = queues.issues.len(issue_phase::PENDING);

    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &ignore,
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();
    let count_second = queues.issues.len(issue_phase::PENDING);

    assert_eq!(count_first, count_second);
}

#[tokio::test]
async fn scan_issues_updates_cursor() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues.json");

    assert!(db
        .cursor_get_last_seen(&repo_id, "issues")
        .unwrap()
        .is_none());

    let ignore = vec!["dependabot".to_string()];
    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &ignore,
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    let cursor = db.cursor_get_last_seen(&repo_id, "issues").unwrap();
    assert!(cursor.is_some());
}

#[tokio::test]
async fn scan_issues_empty_response() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = MockGh::new();
    gh.set_paginate("org/repo", "issues", b"[]".to_vec());

    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert_eq!(queues.issues.len(issue_phase::PENDING), 0);
}

// ═══════════════════════════════════════════════
// 2. PR scanner
// ═══════════════════════════════════════════════

#[tokio::test]
async fn scan_prs_queues_new_items() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "pulls", "pulls.json");

    let ignore = vec!["renovate".to_string()];
    let mut queues = TaskQueues::new();
    autodev::scanner::pulls::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &ignore,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert!(queues.contains("pr:org/repo:100")); // alice
    assert!(!queues.contains("pr:org/repo:101")); // renovate

    // autodev:wip label should have been added for alice's PR
    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(_, n, l)| *n == 100 && l == "autodev:wip"));
}

#[tokio::test]
async fn scan_prs_no_duplicates() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "pulls", "pulls.json");

    let mut queues = TaskQueues::new();
    autodev::scanner::pulls::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        None,
        &mut queues,
    )
    .await
    .unwrap();
    let first = queues.prs.len(pr_phase::PENDING);

    autodev::scanner::pulls::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        None,
        &mut queues,
    )
    .await
    .unwrap();
    let second = queues.prs.len(pr_phase::PENDING);

    assert_eq!(first, second);
}

#[tokio::test]
async fn scan_prs_checks_pending_data() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "pulls", "pulls.json");

    let mut queues = TaskQueues::new();
    autodev::scanner::pulls::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        None,
        &mut queues,
    )
    .await
    .unwrap();

    // Pop from queue and check fields
    let alice_pr = queues.prs.pop(pr_phase::PENDING).unwrap();
    assert_eq!(alice_pr.github_number, 100);
    assert_eq!(alice_pr.head_branch, "feat/user-settings");
    assert_eq!(alice_pr.repo_name, "org/repo");
}

// ═══════════════════════════════════════════════
// 3. gh failure
// ═══════════════════════════════════════════════

#[tokio::test]
async fn scan_issues_gh_failure_returns_error() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    // MockGh에 응답을 설정하지 않으면 에러 반환
    let gh = MockGh::new();
    let mut queues = TaskQueues::new();

    let result = autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &None,
        None,
        &mut queues,
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn scan_prs_gh_failure_returns_error() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    // MockGh에 응답을 설정하지 않으면 에러 반환
    let gh = MockGh::new();
    let mut queues = TaskQueues::new();

    let result = autodev::scanner::pulls::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        None,
        &mut queues,
    )
    .await;
    assert!(result.is_err());
}
