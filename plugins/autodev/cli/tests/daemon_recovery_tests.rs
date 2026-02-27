use autodev::daemon::recovery;
use autodev::domain::models::{RepoIssue, RepoPull, ResolvedRepo};
use autodev::infrastructure::gh::mock::MockGh;
use autodev::queue::task_queues::{
    issue_phase, make_work_id, merge_phase, pr_phase, IssueItem, MergeItem, PrItem, TaskQueues,
};

fn repo(id: &str, name: &str, issues: Vec<RepoIssue>, pulls: Vec<RepoPull>) -> ResolvedRepo {
    ResolvedRepo {
        id: id.to_string(),
        url: format!("https://github.com/{name}"),
        name: name.to_string(),
        gh_host: None,
        issues,
        pulls,
    }
}

/// autodev:wip 라벨이 있는 이슈 생성
fn wip_issue(number: i64) -> RepoIssue {
    RepoIssue {
        number,
        title: format!("Issue #{number}"),
        body: None,
        author: "user".to_string(),
        labels: vec!["autodev:wip".to_string()],
    }
}

/// autodev:wip 라벨이 있는 PR 생성
fn wip_pull(number: i64) -> RepoPull {
    RepoPull {
        number,
        title: format!("PR #{number}"),
        body: None,
        author: "user".to_string(),
        labels: vec!["autodev:wip".to_string()],
        head_branch: "feat".to_string(),
        base_branch: "main".to_string(),
    }
}

fn make_issue_item(repo_name: &str, number: i64) -> IssueItem {
    IssueItem {
        work_id: make_work_id("issue", repo_name, number),
        repo_id: "r1".to_string(),
        repo_name: repo_name.to_string(),
        repo_url: format!("https://github.com/{repo_name}"),
        github_number: number,
        title: "test".to_string(),
        body: None,
        labels: vec![],
        author: "user".to_string(),
        analysis_report: None,
    }
}

fn make_pr_item(repo_name: &str, number: i64) -> PrItem {
    PrItem {
        work_id: make_work_id("pr", repo_name, number),
        repo_id: "r1".to_string(),
        repo_name: repo_name.to_string(),
        repo_url: format!("https://github.com/{repo_name}"),
        github_number: number,
        title: "test".to_string(),
        head_branch: "feat".to_string(),
        base_branch: "main".to_string(),
        review_comment: None,
        source_issue_number: None,
        review_iteration: 0,
    }
}

fn make_merge_item(repo_name: &str, number: i64) -> MergeItem {
    MergeItem {
        work_id: make_work_id("merge", repo_name, number),
        repo_id: "r1".to_string(),
        repo_name: repo_name.to_string(),
        repo_url: format!("https://github.com/{repo_name}"),
        pr_number: number,
        title: "test".to_string(),
        head_branch: "feat".to_string(),
        base_branch: "main".to_string(),
    }
}

// ═══════════════════════════════════════════════
// 1. 레포 없음 → 0건 복구
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_no_repos_returns_zero() {
    let gh = MockGh::new();
    let queues = TaskQueues::new();

    let result = recovery::recover_orphan_wip(&[], &gh, &queues).await;

    assert_eq!(result.unwrap(), 0);
}

// ═══════════════════════════════════════════════
// 2. wip 항목 없음 → 0건 복구
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_no_wip_items_returns_zero() {
    let gh = MockGh::new();
    let queues = TaskQueues::new();
    let repos = vec![repo("r1", "org/repo", vec![], vec![])];

    let result = recovery::recover_orphan_wip(&repos, &gh, &queues).await;

    assert_eq!(result.unwrap(), 0);
    assert!(gh.removed_labels.lock().unwrap().is_empty());
}

// ═══════════════════════════════════════════════
// 3. active에 있는 항목 → 복구하지 않음 (skip)
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_skips_active_issues() {
    let gh = MockGh::new();

    let mut queues = TaskQueues::new();
    queues
        .issues
        .push(issue_phase::PENDING, make_issue_item("org/repo", 42));

    let repos = vec![repo("r1", "org/repo", vec![wip_issue(42)], vec![])];
    let result = recovery::recover_orphan_wip(&repos, &gh, &queues).await;

    assert_eq!(result.unwrap(), 0);
    assert!(gh.removed_labels.lock().unwrap().is_empty());
}

#[tokio::test]
async fn recovery_skips_active_prs() {
    let gh = MockGh::new();

    let mut queues = TaskQueues::new();
    queues
        .prs
        .push(pr_phase::PENDING, make_pr_item("org/repo", 10));

    let repos = vec![repo("r1", "org/repo", vec![], vec![wip_pull(10)])];
    let result = recovery::recover_orphan_wip(&repos, &gh, &queues).await;

    assert_eq!(result.unwrap(), 0);
    assert!(gh.removed_labels.lock().unwrap().is_empty());
}

// Recovery function generates work_id = "pr:org/repo:10" for a PR.
// A merge item has work_id = "merge:org/repo:10", which does NOT match.
// Therefore queues.contains("pr:org/repo:10") returns false, and the
// PR is treated as orphaned — wip label gets removed (recovered = 1).
#[tokio::test]
async fn recovery_skips_pr_active_as_merge() {
    let gh = MockGh::new();

    let mut queues = TaskQueues::new();
    queues
        .merges
        .push(merge_phase::PENDING, make_merge_item("org/repo", 10));

    let repos = vec![repo("r1", "org/repo", vec![], vec![wip_pull(10)])];
    let result = recovery::recover_orphan_wip(&repos, &gh, &queues).await;

    // "merge:org/repo:10" != "pr:org/repo:10" → orphan → label removed
    assert_eq!(result.unwrap(), 1);
    let labels = gh.removed_labels.lock().unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(
        labels[0],
        ("org/repo".to_string(), 10, "autodev:wip".to_string())
    );
}

// ═══════════════════════════════════════════════
// 4. orphan 항목 → wip 라벨 제거
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_removes_orphan_issue_label() {
    let gh = MockGh::new();
    let queues = TaskQueues::new();
    let repos = vec![repo("r1", "org/repo", vec![wip_issue(42)], vec![])];

    let result = recovery::recover_orphan_wip(&repos, &gh, &queues).await;

    assert_eq!(result.unwrap(), 1);
    let labels = gh.removed_labels.lock().unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(
        labels[0],
        ("org/repo".to_string(), 42, "autodev:wip".to_string())
    );
}

#[tokio::test]
async fn recovery_removes_orphan_pr_label() {
    let gh = MockGh::new();
    let queues = TaskQueues::new();
    let repos = vec![repo("r1", "org/repo", vec![], vec![wip_pull(15)])];

    let result = recovery::recover_orphan_wip(&repos, &gh, &queues).await;

    assert_eq!(result.unwrap(), 1);
    let labels = gh.removed_labels.lock().unwrap();
    assert_eq!(
        labels[0],
        ("org/repo".to_string(), 15, "autodev:wip".to_string())
    );
}

// ═══════════════════════════════════════════════
// 5. mixed: active + orphan 혼합
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_mixed_active_and_orphan() {
    let gh = MockGh::new();

    let mut queues = TaskQueues::new();
    queues
        .issues
        .push(issue_phase::PENDING, make_issue_item("org/repo", 1));
    queues
        .prs
        .push(pr_phase::PENDING, make_pr_item("org/repo", 3));

    let repos = vec![repo(
        "r1",
        "org/repo",
        vec![wip_issue(1), wip_issue(2)], // #1 active, #2 orphan
        vec![wip_pull(3), wip_pull(4)],   // #3 active, #4 orphan
    )];
    let result = recovery::recover_orphan_wip(&repos, &gh, &queues).await;

    assert_eq!(result.unwrap(), 2);
    let labels = gh.removed_labels.lock().unwrap();
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0].1, 2); // issue #2
    assert_eq!(labels[1].1, 4); // pr #4
}

// ═══════════════════════════════════════════════
// 6. 빈 issues/pulls → 0건 복구 (API 실패 시나리오는 resolve_repos에서 처리)
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_empty_issues_and_pulls() {
    let gh = MockGh::new();
    let queues = TaskQueues::new();
    let repos = vec![
        repo("r1", "org/repo1", vec![], vec![]),
        repo("r2", "org/repo2", vec![], vec![]),
    ];

    let result = recovery::recover_orphan_wip(&repos, &gh, &queues).await;

    assert_eq!(result.unwrap(), 0);
}

// ═══════════════════════════════════════════════
// 7. 여러 레포에서 orphan 복구
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_multiple_repos() {
    let gh = MockGh::new();
    let queues = TaskQueues::new();
    let repos = vec![
        repo("r1", "org/repo1", vec![wip_issue(10)], vec![]),
        repo("r2", "org/repo2", vec![], vec![wip_pull(20)]),
    ];

    let result = recovery::recover_orphan_wip(&repos, &gh, &queues).await;

    assert_eq!(result.unwrap(), 2);
    let labels = gh.removed_labels.lock().unwrap();
    assert_eq!(labels.len(), 2);
    assert_eq!(
        labels[0],
        ("org/repo1".to_string(), 10, "autodev:wip".to_string())
    );
    assert_eq!(
        labels[1],
        ("org/repo2".to_string(), 20, "autodev:wip".to_string())
    );
}
