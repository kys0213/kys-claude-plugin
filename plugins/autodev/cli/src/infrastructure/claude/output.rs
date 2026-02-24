use std::fmt;
use std::sync::LazyLock;

use schemars::JsonSchema;
use serde::Deserialize;

/// claude -p --output-format json 결과 파싱
#[derive(Debug, Deserialize)]
pub struct ClaudeJsonOutput {
    pub result: Option<String>,
    pub error: Option<String>,
}

/// JSON 출력 파싱 시도, 실패하면 원본 텍스트 반환
pub fn parse_output(stdout: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<ClaudeJsonOutput>(stdout) {
        parsed
            .result
            .or(parsed.error)
            .unwrap_or_else(|| stdout.to_string())
    } else {
        stdout.to_string()
    }
}

/// 이슈 분석 verdict 타입
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Implement,
    NeedsClarification,
    Wontfix,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Implement => write!(f, "implement"),
            Verdict::NeedsClarification => write!(f, "needs_clarification"),
            Verdict::Wontfix => write!(f, "wontfix"),
        }
    }
}

/// 이슈 분석 결과 구조체
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AnalysisResult {
    pub verdict: Verdict,
    /// 0.0 ~ 1.0
    pub confidence: f64,
    pub summary: String,
    /// needs_clarification일 때 질문 목록
    #[serde(default)]
    pub questions: Vec<String>,
    /// wontfix 사유
    pub reason: Option<String>,
    /// 전체 분석 리포트 (구현 단계에서 사용)
    pub report: String,
}

/// PR 리뷰 verdict 타입
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Approve,
    RequestChanges,
}

impl fmt::Display for ReviewVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReviewVerdict::Approve => write!(f, "approve"),
            ReviewVerdict::RequestChanges => write!(f, "request_changes"),
        }
    }
}

/// PR 리뷰 결과 구조체
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReviewResult {
    pub verdict: ReviewVerdict,
    pub summary: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub comments: Vec<ReviewComment>,
}

/// PR 리뷰 개별 댓글 (향후 PR review API의 line comment 게시에 사용)
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[allow(dead_code)]
pub struct ReviewComment {
    pub path: String,
    pub line: Option<u32>,
    pub body: String,
}

/// AnalysisResult JSON schema (한 번만 생성)
pub static ANALYSIS_SCHEMA: LazyLock<String> =
    LazyLock::new(|| serde_json::to_string(&schemars::schema_for!(AnalysisResult)).unwrap());

/// ReviewResult JSON schema (한 번만 생성)
pub static REVIEW_SCHEMA: LazyLock<String> =
    LazyLock::new(|| serde_json::to_string(&schemars::schema_for!(ReviewResult)).unwrap());

/// claude -p 리뷰 결과를 ReviewResult로 파싱 시도
/// 1차: stdout가 claude JSON envelope이면 result 필드 추출 후 파싱
/// 2차: stdout 자체를 직접 파싱
/// 실패 시 None 반환 (호출측에서 exit_code 기반 fallback)
pub fn parse_review(stdout: &str) -> Option<ReviewResult> {
    if let Ok(envelope) = serde_json::from_str::<ClaudeJsonOutput>(stdout) {
        if let Some(inner) = envelope.result {
            if let Ok(review) = serde_json::from_str::<ReviewResult>(&inner) {
                return Some(review);
            }
        }
    }
    serde_json::from_str::<ReviewResult>(stdout).ok()
}

/// claude -p 분석 결과를 AnalysisResult로 파싱 시도
/// 1차: stdout가 claude JSON envelope이면 result 필드 추출 후 파싱
/// 2차: stdout 자체를 직접 파싱
/// 실패 시 None 반환 (호출측에서 fallback 처리)
pub fn parse_analysis(stdout: &str) -> Option<AnalysisResult> {
    // claude --output-format json 결과: { "result": "<escaped json string>" }
    if let Ok(envelope) = serde_json::from_str::<ClaudeJsonOutput>(stdout) {
        if let Some(inner) = envelope.result {
            if let Ok(analysis) = serde_json::from_str::<AnalysisResult>(&inner) {
                return Some(analysis);
            }
        }
    }

    // 직접 파싱 시도 (claude가 raw JSON을 반환한 경우)
    serde_json::from_str::<AnalysisResult>(stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_review_approve_from_envelope() {
        let stdout = r#"{"result": "{\"verdict\":\"approve\",\"summary\":\"LGTM\"}"}"#;
        let result = parse_review(stdout).expect("should parse");
        assert_eq!(result.verdict, ReviewVerdict::Approve);
        assert_eq!(result.summary, "LGTM");
        assert!(result.comments.is_empty());
    }

    #[test]
    fn parse_review_request_changes_from_envelope() {
        let stdout = r#"{"result": "{\"verdict\":\"request_changes\",\"summary\":\"Fix error handling\",\"comments\":[{\"path\":\"src/main.rs\",\"line\":42,\"body\":\"Missing null check\"}]}"}"#;
        let result = parse_review(stdout).expect("should parse");
        assert_eq!(result.verdict, ReviewVerdict::RequestChanges);
        assert_eq!(result.summary, "Fix error handling");
        assert_eq!(result.comments.len(), 1);
        assert_eq!(result.comments[0].path, "src/main.rs");
        assert_eq!(result.comments[0].line, Some(42));
    }

    #[test]
    fn parse_review_raw_json_without_envelope() {
        let stdout = r#"{"verdict":"approve","summary":"All good"}"#;
        let result = parse_review(stdout).expect("should parse");
        assert_eq!(result.verdict, ReviewVerdict::Approve);
        assert_eq!(result.summary, "All good");
    }

    #[test]
    fn parse_review_malformed_returns_none() {
        // plain text — not JSON
        assert!(parse_review("LGTM - no issues found").is_none());
    }

    #[test]
    fn parse_review_envelope_with_non_review_result_returns_none() {
        // envelope의 result가 ReviewResult JSON이 아닌 일반 텍스트
        let stdout = r#"{"result": "LGTM"}"#;
        assert!(parse_review(stdout).is_none());
    }

    #[test]
    fn parse_review_missing_verdict_returns_none() {
        // verdict 필드 누락
        let stdout = r#"{"summary":"All good"}"#;
        assert!(parse_review(stdout).is_none());
    }

    #[test]
    fn analysis_schema_is_valid_json_with_required_fields() {
        let schema: serde_json::Value =
            serde_json::from_str(&ANALYSIS_SCHEMA).expect("ANALYSIS_SCHEMA should be valid JSON");
        let props = schema["properties"]
            .as_object()
            .expect("should have properties");
        assert!(props.contains_key("verdict"), "schema should have verdict");
        assert!(props.contains_key("summary"), "schema should have summary");
        assert!(props.contains_key("report"), "schema should have report");
    }

    #[test]
    fn review_schema_is_valid_json_with_required_fields() {
        let schema: serde_json::Value =
            serde_json::from_str(&REVIEW_SCHEMA).expect("REVIEW_SCHEMA should be valid JSON");
        let props = schema["properties"]
            .as_object()
            .expect("should have properties");
        assert!(props.contains_key("verdict"), "schema should have verdict");
        assert!(props.contains_key("summary"), "schema should have summary");
    }
}
