use std::path::Path;

use autodev::components::analyzer::Analyzer;
use autodev::components::reviewer::Reviewer;
use autodev::infrastructure::claude::mock::MockClaude;
use autodev::infrastructure::claude::output::{ReviewVerdict, Verdict};

// ═══════════════════════════════════════════════
// Reviewer 테스트
// ═══════════════════════════════════════════════

#[tokio::test]
async fn reviewer_success_parses_output() {
    let claude = MockClaude::new();
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
    let claude = MockClaude::new();
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
    let claude = MockClaude::new();
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
    let claude = MockClaude::new();
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
    let claude = MockClaude::new();
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
    let claude = MockClaude::new();
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
    let claude = MockClaude::new();
    // No response enqueued → MockClaude returns exit_code=1

    let reviewer = Reviewer::new(&claude);
    let output = reviewer
        .review_pr(Path::new("/tmp/test"), "/multi-review", None)
        .await
        .unwrap();

    assert_eq!(output.exit_code, 1);
    assert!(output.review.is_empty());
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
    let claude = MockClaude::new();
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
    let claude = MockClaude::new();
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
    let claude = MockClaude::new();
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
    let claude = MockClaude::new();
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
