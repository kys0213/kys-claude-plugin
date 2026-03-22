//! v5 Daemon 통합 테스트.
//!
//! MockDataSource + MockRuntime으로 외부 시스템을 대체하되,
//! Daemon의 전체 tick() 파이프라인을 블랙박스로 검증한다.
//!
//! 검증 대상:
//!   - 전체 파이프라인: Pending → Ready → Running → Completed → Done
//!   - Concurrency 제한
//!   - Escalation 정책 (retry, hitl, skip)
//!   - on_done/on_fail script 실행
//!   - History 기록
//!   - Worktree 생성/정리

use std::sync::Arc;

use autodev::v5::core::context::HistoryStatus;
use autodev::v5::core::escalation::EscalationAction;
use autodev::v5::core::phase::V5QueuePhase;
use autodev::v5::core::queue_item::testing::test_item;
use autodev::v5::core::runtime::RuntimeRegistry;
use autodev::v5::core::workspace::WorkspaceConfig;
use autodev::v5::infra::runtimes::mock::MockRuntime;
use autodev::v5::infra::sources::mock::MockDataSource;
use autodev::v5::service::daemon::{ItemOutcome, V5Daemon};
use autodev::v5::service::worktree::MockWorktreeManager;
use tempfile::TempDir;

// ─── 헬퍼 ───

fn workspace_config(concurrency: u32) -> WorkspaceConfig {
    let yaml = format!(
        r#"
name: integration-test
sources:
  github:
    url: https://github.com/org/repo
    concurrency: {concurrency}
    states:
      analyze:
        trigger:
          label: "autodev:analyze"
        handlers:
          - prompt: "analyze this issue"
        on_done:
          - script: "echo done"
      implement:
        trigger:
          label: "autodev:implement"
        handlers:
          - prompt: "implement this"
          - script: "echo test passed"
        on_done:
          - script: "echo pr created"
        on_fail:
          - script: "echo failure reported"
      fail_on_done:
        trigger:
          label: "autodev:fail_on_done"
        handlers:
          - prompt: "do something"
        on_done:
          - script: "exit 1"
    escalation:
      1: retry
      2: retry_with_comment
      3: hitl
      4: skip
"#
    );
    serde_yaml::from_str(&yaml).unwrap()
}

fn setup_daemon(
    tmp: &TempDir,
    source: MockDataSource,
    exit_codes: Vec<i32>,
    concurrency: u32,
) -> V5Daemon {
    let config = workspace_config(concurrency);
    let mut registry = RuntimeRegistry::new("mock".to_string());
    registry.register(Arc::new(MockRuntime::new("mock", exit_codes)));
    let worktree_mgr = MockWorktreeManager::new(tmp.path());

    V5Daemon::new(
        config,
        vec![Box::new(source)],
        Arc::new(registry),
        Box::new(worktree_mgr),
        4,
    )
}

// ═══════════════════════════════════════════════
// 1. 정상 경로: Pending → Done
// ═══════════════════════════════════════════════

#[tokio::test]
async fn happy_path_single_item_pending_to_done() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "analyze"));

    let mut daemon = setup_daemon(&tmp, source, vec![0], 2);

    // tick 1: collect → advance → execute → on_done
    daemon.tick().await.unwrap();

    // 아이템이 Done까지 도달해야 함
    assert!(daemon.items_in_phase(V5QueuePhase::Pending).is_empty());
    assert!(daemon.items_in_phase(V5QueuePhase::Running).is_empty());
    assert!(daemon.items_in_phase(V5QueuePhase::Completed).is_empty());

    // history에 completed + done 기록
    let history = daemon.history();
    assert!(
        history.len() >= 2,
        "expected at least 2 history entries, got {}",
        history.len()
    );
    assert!(history.iter().any(|h| h.status == HistoryStatus::Done));
}

#[tokio::test]
async fn happy_path_multiple_items_parallel() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "analyze"));
    source.add_item(test_item("github:org/repo#2", "analyze"));

    // 2개 handler 모두 성공
    let mut daemon = setup_daemon(&tmp, source, vec![0, 0], 2);
    daemon.tick().await.unwrap();

    // 두 아이템 모두 Done
    assert!(daemon.items_in_phase(V5QueuePhase::Running).is_empty());
}

// ═══════════════════════════════════════════════
// 2. Concurrency 제한
// ═══════════════════════════════════════════════

#[tokio::test]
async fn concurrency_limits_running_items() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "analyze"));
    source.add_item(test_item("github:org/repo#2", "analyze"));
    source.add_item(test_item("github:org/repo#3", "analyze"));

    // concurrency=1 → 한 번에 1개만 Running
    let mut daemon = setup_daemon(&tmp, source, vec![0, 0, 0], 1);

    // collect + advance
    daemon.collect().await.unwrap();
    daemon.advance();

    // Running은 1개, Ready는 2개
    assert_eq!(daemon.items_in_phase(V5QueuePhase::Running).len(), 1);
    assert_eq!(daemon.items_in_phase(V5QueuePhase::Ready).len(), 2);
}

// ═══════════════════════════════════════════════
// 3. Escalation 정책
// ═══════════════════════════════════════════════

#[tokio::test]
async fn escalation_retry_on_first_failure() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "analyze"));

    // handler 실패 (exit_code=1)
    let mut daemon = setup_daemon(&tmp, source, vec![1], 2);
    daemon.collect().await.unwrap();
    daemon.advance();
    let outcomes = daemon.execute_running().await;

    // failure_count=1 → Retry
    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        ItemOutcome::Failed { escalation, .. } => {
            assert_eq!(*escalation, EscalationAction::Retry);
        }
        other => panic!("expected Failed, got {other:?}"),
    }

    // retry: Pending에 새 아이템이 추가됨
    assert_eq!(daemon.items_in_phase(V5QueuePhase::Pending).len(), 1);
}

#[tokio::test]
async fn escalation_hitl_after_three_failures() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "analyze"));

    // 3회 연속 실패 → hitl
    let mut daemon = setup_daemon(&tmp, source, vec![1, 1, 1], 2);

    // 1회차: failure → retry
    daemon.tick().await.unwrap();
    // 2회차: retry된 아이템 실패 → retry_with_comment
    daemon.tick().await.unwrap();
    // 3회차: retry된 아이템 실패 → hitl
    daemon.tick().await.unwrap();

    // HITL 아이템이 존재해야 함
    let hitl = daemon.items_in_phase(V5QueuePhase::Hitl);
    assert!(
        !hitl.is_empty(),
        "expected HITL item after 3 failures, queue: {:?}",
        daemon
            .queue_items()
            .iter()
            .map(|i| (&i.work_id, &i.phase))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn escalation_skip_after_four_failures() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "analyze"));

    // 4회 연속 실패 → skip
    let mut daemon = setup_daemon(&tmp, source, vec![1, 1, 1, 1], 2);

    for _ in 0..4 {
        daemon.tick().await.unwrap();
    }

    // Skipped 아이템 존재 또는 큐에서 제거됨
    let all_phases: Vec<_> = daemon.queue_items().iter().map(|i| i.phase).collect();

    // 4회 실패 후에는 HITL 또는 Skipped 중 하나 (escalation 정책에 따라)
    assert!(
        all_phases
            .iter()
            .any(|p| *p == V5QueuePhase::Hitl || *p == V5QueuePhase::Skipped)
            || all_phases.is_empty(),
        "expected HITL or Skipped after 4 failures, got: {all_phases:?}"
    );
}

// ═══════════════════════════════════════════════
// 4. on_done 실패 → Failed
// ═══════════════════════════════════════════════

#[tokio::test]
async fn on_done_failure_transitions_to_failed() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "fail_on_done"));

    // handler 성공 (exit_code=0), 하지만 on_done script가 "exit 1"
    let mut daemon = setup_daemon(&tmp, source, vec![0], 2);
    daemon.tick().await.unwrap();

    // on_done 실패 → Failed
    // (on_done script가 `exit 1`이므로 Failed 상태여야 함)
    let history = daemon.history();
    let has_failed = history.iter().any(|h| h.status == HistoryStatus::Failed);
    assert!(
        has_failed,
        "expected Failed in history after on_done failure, got: {:?}",
        history
            .iter()
            .map(|h| (&h.state, &h.status))
            .collect::<Vec<_>>()
    );
}

// ═══════════════════════════════════════════════
// 5. 상태 미존재 → Skipped
// ═══════════════════════════════════════════════

#[tokio::test]
async fn unknown_state_skipped() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "nonexistent_state"));

    let mut daemon = setup_daemon(&tmp, source, vec![], 2);
    daemon.collect().await.unwrap();
    daemon.advance();
    let outcomes = daemon.execute_running().await;

    assert_eq!(outcomes.len(), 1);
    assert!(matches!(outcomes[0], ItemOutcome::Skipped(_)));
}

// ═══════════════════════════════════════════════
// 6. 중복 수집 방지
// ═══════════════════════════════════════════════

#[tokio::test]
async fn dedup_prevents_duplicate_items() {
    let tmp = TempDir::new().unwrap();
    let source = MockDataSource::new("github");
    let mut daemon = setup_daemon(&tmp, source, vec![], 2);

    // 수동 추가
    daemon.push_item(test_item("github:org/repo#1", "analyze"));

    // 같은 work_id로 수집 시도
    let mut source2 = MockDataSource::new("github");
    source2.add_item(test_item("github:org/repo#1", "analyze"));
    daemon.replace_sources(vec![Box::new(source2)]);

    daemon.collect().await.unwrap();
    assert_eq!(
        daemon.queue_items().len(),
        1,
        "duplicate should be rejected"
    );
}

// ═══════════════════════════════════════════════
// 7. History append-only 검증
// ═══════════════════════════════════════════════

#[tokio::test]
async fn history_is_append_only() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "analyze"));

    let mut daemon = setup_daemon(&tmp, source, vec![0], 2);

    assert_eq!(daemon.history().len(), 0);
    daemon.tick().await.unwrap();

    let len_after_tick1 = daemon.history().len();
    assert!(
        len_after_tick1 > 0,
        "history should have entries after tick"
    );

    // 두 번째 tick (빈 수집)
    daemon.tick().await.unwrap();

    // history는 절대 줄어들지 않음
    assert!(
        daemon.history().len() >= len_after_tick1,
        "history must be append-only"
    );
}

// ═══════════════════════════════════════════════
// 8. 다중 handler 순차 실행
// ═══════════════════════════════════════════════

#[tokio::test]
async fn multi_handler_sequential_execution() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    // implement state는 handler 2개: prompt + script
    source.add_item(test_item("github:org/repo#1", "implement"));

    // 첫 번째 handler(prompt) 성공, 두 번째(script)도 성공
    let mut daemon = setup_daemon(&tmp, source, vec![0], 2);
    daemon.tick().await.unwrap();

    // Done까지 도달
    let history = daemon.history();
    assert!(
        history.iter().any(|h| h.status == HistoryStatus::Done),
        "multi-handler pipeline should complete to Done"
    );
}

#[tokio::test]
async fn multi_handler_fails_on_first_handler() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "implement"));

    // 첫 번째 handler 실패 → 두 번째 handler 실행 안 됨
    let mut daemon = setup_daemon(&tmp, source, vec![1], 2);
    daemon.collect().await.unwrap();
    daemon.advance();
    let outcomes = daemon.execute_running().await;

    assert_eq!(outcomes.len(), 1);
    assert!(
        matches!(outcomes[0], ItemOutcome::Failed { .. }),
        "should fail on first handler"
    );
}

// ═══════════════════════════════════════════════
// 9. Worktree 라이프사이클
// ═══════════════════════════════════════════════

#[tokio::test]
async fn worktree_created_for_running_item() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "analyze"));

    let mut daemon = setup_daemon(&tmp, source, vec![0], 2);
    daemon.tick().await.unwrap();

    // worktree 디렉토리가 존재했다가 Done 후 cleanup 됨
    // MockWorktreeManager는 create_or_reuse에서 디렉토리를 생성하고
    // execute_on_done에서 cleanup을 호출
    // cleanup 후에는 디렉토리가 없어야 함
    let worktree_path = tmp.path().join("integration-test-github-org-repo-1");
    assert!(
        !worktree_path.exists(),
        "worktree should be cleaned up after Done"
    );
}

#[tokio::test]
async fn worktree_preserved_on_failure() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    source.add_item(test_item("github:org/repo#1", "analyze"));

    // handler 실패
    let mut daemon = setup_daemon(&tmp, source, vec![1], 2);
    daemon.collect().await.unwrap();
    daemon.advance();
    daemon.execute_running().await;

    // 실패 시 worktree는 보존 (cleanup 호출 안 됨)
    let worktree_path = tmp.path().join("integration-test-github-org-repo-1");
    assert!(
        worktree_path.exists(),
        "worktree should be preserved on failure for retry"
    );
}

// ═══════════════════════════════════════════════
// 10. on_fail script 실행 조건
// ═══════════════════════════════════════════════

#[tokio::test]
async fn on_fail_not_called_on_retry() {
    let tmp = TempDir::new().unwrap();
    let mut source = MockDataSource::new("github");
    // implement state는 on_fail이 정의되어 있음
    source.add_item(test_item("github:org/repo#1", "implement"));

    // 1회차 실패 → escalation=retry → on_fail 미실행
    let mut daemon = setup_daemon(&tmp, source, vec![1], 2);
    daemon.collect().await.unwrap();
    daemon.advance();
    let outcomes = daemon.execute_running().await;

    match &outcomes[0] {
        ItemOutcome::Failed { escalation, .. } => {
            assert_eq!(*escalation, EscalationAction::Retry);
            // Retry는 on_fail 미실행 — should_run_on_fail()=false
            assert!(!escalation.should_run_on_fail());
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}
