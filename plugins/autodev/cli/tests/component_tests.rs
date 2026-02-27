use std::path::Path;

use autodev::components::analyzer::Analyzer;
use autodev::components::merger::{MergeOutcome, Merger};
use autodev::components::reviewer::Reviewer;
use autodev::infrastructure::agent::mock::MockAgent;
use autodev::infrastructure::agent::output::{ReviewVerdict, Verdict};

// ═══════════════════════════════════════════════
// Reviewer 테스트
// ═══════════════════════════════════════════════

#[tokio::test]
async fn reviewer_success_parses_output() {
    let claude = MockAgent::new();
    claude.enqueue_response(r#"{"result": "LGTM - no issues found"}"#, 0);

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "/multi-review", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.review, "LGTM - no issues found");
    assert!(!output.stdout.is_empty());

    // Claude에 json 출력 형식이 전달되었는지 확인
    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].prompt, "/multi-review");
    assert_eq!(calls[0].output_format.as_deref(), Some("json"));
}

#[tokio::test]
async fn reviewer_verdict_approve() {
    let claude = MockAgent::new();
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"approve\",\"summary\":\"LGTM\"}"}"#,
        0,
    );

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "/multi-review", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.verdict, Some(ReviewVerdict::Approve));
    assert_eq!(output.review, "LGTM");
}

#[tokio::test]
async fn reviewer_verdict_request_changes() {
    let claude = MockAgent::new();
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"request_changes\",\"summary\":\"Fix bugs\"}"}"#,
        0,
    );

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "/multi-review", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.verdict, Some(ReviewVerdict::RequestChanges));
    assert_eq!(output.review, "Fix bugs");
}

#[tokio::test]
async fn reviewer_verdict_none_on_non_review_json() {
    let claude = MockAgent::new();
    // Non-ReviewResult JSON → verdict=None, fallback to parse_output
    claude.enqueue_response(r#"{"result": "LGTM - no issues found"}"#, 0);

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "/multi-review", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.verdict, None);
    assert_eq!(output.review, "LGTM - no issues found");
}

#[tokio::test]
async fn reviewer_success_raw_output() {
    let claude = MockAgent::new();
    claude.enqueue_response("Plain text review output", 0);

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "review this PR", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 0);
    // parse_output은 JSON 파싱 실패 시 원본 텍스트 반환
    assert_eq!(output.review, "Plain text review output");
}

#[tokio::test]
async fn reviewer_failure_returns_empty_review() {
    let claude = MockAgent::new();
    claude.enqueue_response("error output", 1);

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "/multi-review", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 1);
    assert!(output.review.is_empty());
    assert_eq!(output.stdout, "error output");
}

#[tokio::test]
async fn reviewer_no_response_returns_failure() {
    let claude = MockAgent::new();
    // No response enqueued → MockAgent returns exit_code=1

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "/multi-review", None)
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
    let claude = MockAgent::new();
    claude.enqueue_response("Merged successfully", 0);

    let merger = Merger::new(&claude);
    let output = merger.merge_pr(Path::new("/tmp/test"), 42).await;

    assert!(matches!(output.outcome, MergeOutcome::Success));
    assert_eq!(output.stdout, "Merged successfully");

    // 프롬프트에 [autodev] 마커와 PR 번호가 포함되었는지 확인
    let calls = claude.calls.lock().unwrap();
    assert!(calls[0].prompt.contains("[autodev] merge: PR #42"));
    assert!(calls[0].prompt.contains("/git-utils:merge-pr 42"));
    assert_eq!(calls[0].output_format, None); // output_format 없음
}

#[tokio::test]
async fn merger_conflict_detected_in_stdout() {
    let claude = MockAgent::new();
    claude.enqueue_response("CONFLICT (content): Merge conflict in src/main.rs", 1);

    let merger = Merger::new(&claude);
    let output = merger.merge_pr(Path::new("/tmp/test"), 10).await;

    assert!(matches!(output.outcome, MergeOutcome::Conflict));
}

#[tokio::test]
async fn merger_conflict_detected_in_stderr() {
    let claude = MockAgent::new();
    // MockAgent는 stderr를 빈 문자열로 설정하므로, stdout에서 확인
    // stderr에 conflict가 있는 경우를 테스트하려면 별도 처리 필요
    // 여기서는 stdout에 conflict가 포함된 경우를 테스트
    claude.enqueue_response("merge conflict detected", 1);

    let merger = Merger::new(&claude);
    let output = merger.merge_pr(Path::new("/tmp/test"), 10).await;

    assert!(matches!(output.outcome, MergeOutcome::Conflict));
}

#[tokio::test]
async fn merger_failure_without_conflict() {
    let claude = MockAgent::new();
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
    let claude = MockAgent::new();
    // No response → MockAgent returns exit_code=1 with empty stdout

    let merger = Merger::new(&claude);
    let output = merger.merge_pr(Path::new("/tmp/test"), 10).await;

    // MockAgent returns Ok with exit_code=1 (not Err), so this is Failed
    assert!(matches!(output.outcome, MergeOutcome::Failed { .. }));
}

// ═══════════════════════════════════════════════
// Merger — resolve_conflicts 테스트
// ═══════════════════════════════════════════════

#[tokio::test]
async fn merger_resolve_success() {
    let claude = MockAgent::new();
    claude.enqueue_response("Conflicts resolved", 0);

    let merger = Merger::new(&claude);
    let output = merger.resolve_conflicts(Path::new("/tmp/test"), 42).await;

    assert!(matches!(output.outcome, MergeOutcome::Success));

    let calls = claude.calls.lock().unwrap();
    assert!(calls[0].prompt.contains("Resolve") && calls[0].prompt.contains("PR #42"));
}

#[tokio::test]
async fn merger_resolve_failure() {
    let claude = MockAgent::new();
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
    let claude = MockAgent::new();
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
    let claude = MockAgent::new();
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

// ═══════════════════════════════════════════════
// Analyzer 테스트
// ═══════════════════════════════════════════════

fn make_analysis_json_fixture(verdict: &str, confidence: f64) -> String {
    let inner = format!(
        r##"{{"verdict":"{verdict}","confidence":{confidence},"summary":"Test summary","questions":[],"reason":null,"report":"Analysis Report"}}"##,
    );
    serde_json::json!({ "result": inner }).to_string()
}

#[tokio::test]
async fn analyzer_success_parses_implement() {
    let claude = MockAgent::new();
    claude.enqueue_response(&make_analysis_json_fixture("implement", 0.9), 0);

    let analyzer = Analyzer::new(&claude);
    let output = analyzer
        .analyze(Path::new("/tmp/test"), "analyze issue", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 0);
    let a = output.analysis.expect("should parse analysis");
    assert_eq!(a.verdict, Verdict::Implement);
    assert!((a.confidence - 0.9).abs() < f64::EPSILON);
    assert_eq!(a.report, "Analysis Report");
}

#[tokio::test]
async fn analyzer_success_parses_needs_clarification() {
    let claude = MockAgent::new();
    claude.enqueue_response(&make_analysis_json_fixture("needs_clarification", 0.4), 0);

    let analyzer = Analyzer::new(&claude);
    let output = analyzer
        .analyze(Path::new("/tmp/test"), "analyze issue", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 0);
    let a = output.analysis.expect("should parse");
    assert_eq!(a.verdict, Verdict::NeedsClarification);
}

#[tokio::test]
async fn analyzer_failure_returns_none_analysis() {
    let claude = MockAgent::new();
    claude.enqueue_response("error", 1);

    let analyzer = Analyzer::new(&claude);
    let output = analyzer
        .analyze(Path::new("/tmp/test"), "analyze issue", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 1);
    assert!(output.analysis.is_none());
}

#[tokio::test]
async fn analyzer_malformed_json_returns_none_analysis() {
    let claude = MockAgent::new();
    claude.enqueue_response("not json at all {{{", 0);

    let analyzer = Analyzer::new(&claude);
    let output = analyzer
        .analyze(Path::new("/tmp/test"), "analyze issue", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 0);
    assert!(
        output.analysis.is_none(),
        "malformed JSON should return None"
    );
}
