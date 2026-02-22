use std::fmt;

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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
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
