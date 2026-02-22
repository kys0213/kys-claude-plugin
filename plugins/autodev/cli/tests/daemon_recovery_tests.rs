use autodev::active::ActiveItems;
use autodev::daemon::recovery;
use autodev::infrastructure::gh::MockGh;
use autodev::queue::models::EnabledRepo;

fn repo(id: &str, name: &str) -> EnabledRepo {
    EnabledRepo {
        id: id.to_string(),
        url: format!("https://github.com/{name}"),
        name: name.to_string(),
    }
}

/// autodev:wip 라벨이 있는 이슈 JSON 생성
fn wip_issue_json(number: i64) -> serde_json::Value {
    serde_json::json!({
        "number": number,
        "state": "open",
        "labels": [{"name": "autodev:wip"}]
    })
}

/// autodev:wip 라벨이 있는 PR JSON 생성
fn wip_pr_json(number: i64) -> serde_json::Value {
    serde_json::json!({
        "number": number,
        "state": "open",
        "labels": [{"name": "autodev:wip"}],
        "pull_request": {"url": "https://api.github.com/repos/org/repo/pulls/1"}
    })
}

// ═══════════════════════════════════════════════
// 1. 레포 없음 → 0건 복구
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_no_repos_returns_zero() {
    let gh = MockGh::new();
    let active = ActiveItems::new();

    let result = recovery::recover_orphan_wip(&[], &gh, &active, None::<&str>).await;

    assert_eq!(result.unwrap(), 0);
}

// ═══════════════════════════════════════════════
// 2. wip 항목 없음 → 0건 복구
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_no_wip_items_returns_zero() {
    let gh = MockGh::new();
    gh.set_paginate("org/repo", "issues", b"[]".to_vec());
    let active = ActiveItems::new();
    let repos = vec![repo("r1", "org/repo")];

    let result = recovery::recover_orphan_wip(&repos, &gh, &active, None::<&str>).await;

    assert_eq!(result.unwrap(), 0);
    assert!(gh.removed_labels.lock().unwrap().is_empty());
}

// ═══════════════════════════════════════════════
// 3. active에 있는 항목 → 복구하지 않음 (skip)
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_skips_active_issues() {
    let items = serde_json::to_vec(&vec![wip_issue_json(42)]).unwrap();
    let gh = MockGh::new();
    gh.set_paginate("org/repo", "issues", items);

    let mut active = ActiveItems::new();
    active.insert("issue", "r1", 42);

    let repos = vec![repo("r1", "org/repo")];
    let result = recovery::recover_orphan_wip(&repos, &gh, &active, None::<&str>).await;

    assert_eq!(result.unwrap(), 0);
    assert!(gh.removed_labels.lock().unwrap().is_empty());
}

#[tokio::test]
async fn recovery_skips_active_prs() {
    let items = serde_json::to_vec(&vec![wip_pr_json(10)]).unwrap();
    let gh = MockGh::new();
    gh.set_paginate("org/repo", "issues", items);

    let mut active = ActiveItems::new();
    active.insert("pr", "r1", 10);

    let repos = vec![repo("r1", "org/repo")];
    let result = recovery::recover_orphan_wip(&repos, &gh, &active, None::<&str>).await;

    assert_eq!(result.unwrap(), 0);
    assert!(gh.removed_labels.lock().unwrap().is_empty());
}

#[tokio::test]
async fn recovery_skips_pr_active_as_merge() {
    let items = serde_json::to_vec(&vec![wip_pr_json(10)]).unwrap();
    let gh = MockGh::new();
    gh.set_paginate("org/repo", "issues", items);

    let mut active = ActiveItems::new();
    active.insert("merge", "r1", 10);

    let repos = vec![repo("r1", "org/repo")];
    let result = recovery::recover_orphan_wip(&repos, &gh, &active, None::<&str>).await;

    assert_eq!(result.unwrap(), 0);
    assert!(gh.removed_labels.lock().unwrap().is_empty());
}

// ═══════════════════════════════════════════════
// 4. orphan 항목 → wip 라벨 제거
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_removes_orphan_issue_label() {
    let items = serde_json::to_vec(&vec![wip_issue_json(42)]).unwrap();
    let gh = MockGh::new();
    gh.set_paginate("org/repo", "issues", items);

    let active = ActiveItems::new();
    let repos = vec![repo("r1", "org/repo")];

    let result = recovery::recover_orphan_wip(&repos, &gh, &active, None::<&str>).await;

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
    let items = serde_json::to_vec(&vec![wip_pr_json(15)]).unwrap();
    let gh = MockGh::new();
    gh.set_paginate("org/repo", "issues", items);

    let active = ActiveItems::new();
    let repos = vec![repo("r1", "org/repo")];

    let result = recovery::recover_orphan_wip(&repos, &gh, &active, None::<&str>).await;

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
    let items = serde_json::to_vec(&vec![
        wip_issue_json(1), // active → skip
        wip_issue_json(2), // orphan → recover
        wip_pr_json(3),    // active (merge) → skip
        wip_pr_json(4),    // orphan → recover
    ])
    .unwrap();
    let gh = MockGh::new();
    gh.set_paginate("org/repo", "issues", items);

    let mut active = ActiveItems::new();
    active.insert("issue", "r1", 1);
    active.insert("merge", "r1", 3);

    let repos = vec![repo("r1", "org/repo")];
    let result = recovery::recover_orphan_wip(&repos, &gh, &active, None::<&str>).await;

    assert_eq!(result.unwrap(), 2);
    let labels = gh.removed_labels.lock().unwrap();
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0].1, 2); // issue #2
    assert_eq!(labels[1].1, 4); // pr #4
}

// ═══════════════════════════════════════════════
// 6. API 실패 → 에러 로그만, 계속 진행
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_api_failure_continues() {
    let gh = MockGh::new();
    // repo1: no mock → api_paginate fails
    // repo2: empty response
    gh.set_paginate("org/repo2", "issues", b"[]".to_vec());

    let active = ActiveItems::new();
    let repos = vec![repo("r1", "org/repo1"), repo("r2", "org/repo2")];

    let result = recovery::recover_orphan_wip(&repos, &gh, &active, None::<&str>).await;

    // repo1 fails silently, repo2 succeeds with 0
    assert_eq!(result.unwrap(), 0);
}

// ═══════════════════════════════════════════════
// 7. 여러 레포에서 orphan 복구
// ═══════════════════════════════════════════════

#[tokio::test]
async fn recovery_multiple_repos() {
    let gh = MockGh::new();
    gh.set_paginate(
        "org/repo1",
        "issues",
        serde_json::to_vec(&vec![wip_issue_json(10)]).unwrap(),
    );
    gh.set_paginate(
        "org/repo2",
        "issues",
        serde_json::to_vec(&vec![wip_pr_json(20)]).unwrap(),
    );

    let active = ActiveItems::new();
    let repos = vec![repo("r1", "org/repo1"), repo("r2", "org/repo2")];

    let result = recovery::recover_orphan_wip(&repos, &gh, &active, None::<&str>).await;

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
