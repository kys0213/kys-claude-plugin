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
