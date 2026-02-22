use std::path::Path;

use autodev::components::merger::{MergeOutcome, Merger};
use autodev::components::reviewer::Reviewer;
use autodev::infrastructure::claude::MockClaude;

// ═══════════════════════════════════════════════
// Reviewer 테스트
// ═══════════════════════════════════════════════

#[tokio::test]
async fn reviewer_success_parses_output() {
    let claude = MockClaude::new();
    claude.enqueue_response(r#"{"result": "LGTM - no issues found"}"#, 0);

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "/multi-review")
        .await
        .unwrap();

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.review, "LGTM - no issues found");
    assert!(!output.stdout.is_empty());

    // Claude에 json 출력 형식이 전달되었는지 확인
    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1, "/multi-review");
    assert_eq!(calls[0].2.as_deref(), Some("json"));
}

#[tokio::test]
async fn reviewer_success_raw_output() {
    let claude = MockClaude::new();
    claude.enqueue_response("Plain text review output", 0);

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "review this PR")
        .await
        .unwrap();

    assert_eq!(output.exit_code, 0);
    // parse_output은 JSON 파싱 실패 시 원본 텍스트 반환
    assert_eq!(output.review, "Plain text review output");
}

#[tokio::test]
async fn reviewer_failure_returns_empty_review() {
    let claude = MockClaude::new();
    claude.enqueue_response("error output", 1);

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "/multi-review")
        .await
        .unwrap();

    assert_eq!(output.exit_code, 1);
    assert!(output.review.is_empty());
    assert_eq!(output.stdout, "error output");
}

#[tokio::test]
async fn reviewer_no_response_returns_failure() {
    let claude = MockClaude::new();
    // No response enqueued → MockClaude returns exit_code=1

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "/multi-review")
        .await
        .unwrap();

    assert_eq!(output.exit_code, 1);
    assert!(output.review.is_empty());
}

// ═══════════════════════════════════════════════
// Merger 테스트
// ═══════════════════════════════════════════════

#[tokio::test]
async fn merger_success() {
    let claude = MockClaude::new();
    claude.enqueue_response("Merged successfully", 0);

    let merger = Merger::new(&claude);
    let output = merger.merge_pr(Path::new("/tmp/test"), 42).await;

    assert!(matches!(output.outcome, MergeOutcome::Success));
    assert_eq!(output.stdout, "Merged successfully");

    // 프롬프트에 PR 번호가 포함되었는지 확인
    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls[0].1, "/git-utils:merge-pr 42");
    assert_eq!(calls[0].2, None); // output_format 없음
}

#[tokio::test]
async fn merger_conflict_detected_in_stdout() {
    let claude = MockClaude::new();
    claude.enqueue_response("CONFLICT (content): Merge conflict in src/main.rs", 1);

    let merger = Merger::new(&claude);
    let output = merger.merge_pr(Path::new("/tmp/test"), 10).await;

    assert!(matches!(output.outcome, MergeOutcome::Conflict));
}

#[tokio::test]
async fn merger_conflict_detected_in_stderr() {
    let claude = MockClaude::new();
    // MockClaude는 stderr를 빈 문자열로 설정하므로, stdout에서 확인
    // stderr에 conflict가 있는 경우를 테스트하려면 별도 처리 필요
    // 여기서는 stdout에 conflict가 포함된 경우를 테스트
    claude.enqueue_response("merge conflict detected", 1);

    let merger = Merger::new(&claude);
    let output = merger.merge_pr(Path::new("/tmp/test"), 10).await;

    assert!(matches!(output.outcome, MergeOutcome::Conflict));
}

#[tokio::test]
async fn merger_failure_without_conflict() {
    let claude = MockClaude::new();
    claude.enqueue_response("permission denied", 1);

    let merger = Merger::new(&claude);
    let output = merger.merge_pr(Path::new("/tmp/test"), 10).await;

    assert!(matches!(
        output.outcome,
        MergeOutcome::Failed { exit_code: 1 }
    ));
    assert_eq!(output.stdout, "permission denied");
}

#[tokio::test]
async fn merger_error_no_response() {
    let claude = MockClaude::new();
    // No response → MockClaude returns exit_code=1 with empty stdout

    let merger = Merger::new(&claude);
    let output = merger.merge_pr(Path::new("/tmp/test"), 10).await;

    // MockClaude returns Ok with exit_code=1 (not Err), so this is Failed
    assert!(matches!(output.outcome, MergeOutcome::Failed { .. }));
}

// ═══════════════════════════════════════════════
// Merger — resolve_conflicts 테스트
// ═══════════════════════════════════════════════

#[tokio::test]
async fn merger_resolve_success() {
    let claude = MockClaude::new();
    claude.enqueue_response("Conflicts resolved", 0);

    let merger = Merger::new(&claude);
    let output = merger.resolve_conflicts(Path::new("/tmp/test"), 42).await;

    assert!(matches!(output.outcome, MergeOutcome::Success));

    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls[0].1, "Resolve merge conflicts for PR #42");
}

#[tokio::test]
async fn merger_resolve_failure() {
    let claude = MockClaude::new();
    claude.enqueue_response("Cannot resolve", 1);

    let merger = Merger::new(&claude);
    let output = merger.resolve_conflicts(Path::new("/tmp/test"), 42).await;

    assert!(matches!(
        output.outcome,
        MergeOutcome::Failed { exit_code: 1 }
    ));
}

// ═══════════════════════════════════════════════
// Merger — merge + conflict → resolve 전체 시나리오
// ═══════════════════════════════════════════════

#[tokio::test]
async fn merger_conflict_then_resolve_success() {
    let claude = MockClaude::new();
    // 1차: merge → conflict
    claude.enqueue_response("CONFLICT in file.rs", 1);
    // 2차: resolve → success
    claude.enqueue_response("All conflicts resolved", 0);

    let merger = Merger::new(&claude);

    let merge_output = merger.merge_pr(Path::new("/tmp/test"), 5).await;
    assert!(matches!(merge_output.outcome, MergeOutcome::Conflict));

    let resolve_output = merger.resolve_conflicts(Path::new("/tmp/test"), 5).await;
    assert!(matches!(resolve_output.outcome, MergeOutcome::Success));

    assert_eq!(claude.call_count(), 2);
}

#[tokio::test]
async fn merger_conflict_then_resolve_failure() {
    let claude = MockClaude::new();
    // 1차: merge → conflict
    claude.enqueue_response("conflict in main.rs", 1);
    // 2차: resolve → fail
    claude.enqueue_response("Cannot auto-resolve", 1);

    let merger = Merger::new(&claude);

    let merge_output = merger.merge_pr(Path::new("/tmp/test"), 5).await;
    assert!(matches!(merge_output.outcome, MergeOutcome::Conflict));

    let resolve_output = merger.resolve_conflicts(Path::new("/tmp/test"), 5).await;
    assert!(matches!(
        resolve_output.outcome,
        MergeOutcome::Failed { .. }
    ));
}
