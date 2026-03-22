//! ClaudeRuntime — Claude CLI 기반 AgentRuntime 구현체.
//!
//! 기존 `Claude` trait을 래핑하여 v5 `AgentRuntime` 인터페이스를 제공한다.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use crate::core::runtime::{
    AgentRuntime, RuntimeCapabilities, RuntimeRequest, RuntimeResponse, TokenUsage,
};
use crate::infra::claude::{Claude, SessionOptions};

/// Claude CLI를 래핑하는 AgentRuntime 구현체.
///
/// 기존 `Claude` trait의 `run_session`을 호출하고,
/// 결과를 `RuntimeResponse`로 변환한다.
pub struct ClaudeRuntime {
    claude: Arc<dyn Claude>,
    default_model: String,
}

impl ClaudeRuntime {
    pub fn new(claude: Arc<dyn Claude>) -> Self {
        Self {
            claude,
            default_model: "sonnet".to_string(),
        }
    }

    pub fn with_default_model(mut self, model: &str) -> Self {
        self.default_model = model.to_string();
        self
    }

    /// Claude CLI stderr에서 토큰 사용량을 파싱한다.
    ///
    /// Claude는 stderr에 JSON lines 형식으로 usage 이벤트를 출력한다.
    fn parse_token_usage_from_stderr(stderr: &str) -> Option<TokenUsage> {
        let mut input_tokens: i64 = 0;
        let mut output_tokens: i64 = 0;
        let mut cache_write_tokens: i64 = 0;
        let mut cache_read_tokens: i64 = 0;
        let mut found = false;

        for line in stderr.lines() {
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line.trim()) {
                if let Some(v) = obj.get("input_tokens").and_then(|v| v.as_i64()) {
                    input_tokens += v;
                    found = true;
                }
                if let Some(v) = obj.get("output_tokens").and_then(|v| v.as_i64()) {
                    output_tokens += v;
                }
                if let Some(v) = obj
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_i64())
                {
                    cache_write_tokens += v;
                }
                if let Some(v) = obj.get("cache_read_input_tokens").and_then(|v| v.as_i64()) {
                    cache_read_tokens += v;
                }
            }
        }

        if found {
            Some(TokenUsage {
                input_tokens,
                output_tokens,
                cache_write_tokens,
                cache_read_tokens,
            })
        } else {
            None
        }
    }
}

#[async_trait]
impl AgentRuntime for ClaudeRuntime {
    fn name(&self) -> &str {
        "claude"
    }

    async fn invoke(&self, request: RuntimeRequest) -> RuntimeResponse {
        let start = Instant::now();

        let opts = SessionOptions {
            output_format: request.structured_output.as_ref().map(|s| s.format.clone()),
            json_schema: request
                .structured_output
                .as_ref()
                .and_then(|s| s.schema.clone()),
            append_system_prompt: request.system_prompt.clone(),
        };

        match self
            .claude
            .run_session(&request.working_dir, &request.prompt, &opts)
            .await
        {
            Ok(result) => {
                let token_usage = Self::parse_token_usage_from_stderr(&result.stderr);

                RuntimeResponse {
                    exit_code: result.exit_code,
                    stdout: result.stdout,
                    stderr: result.stderr,
                    duration: start.elapsed(),
                    token_usage,
                    session_id: None,
                }
            }
            Err(e) => {
                let mut resp = RuntimeResponse::error(e);
                resp.duration = start.elapsed();
                resp
            }
        }
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            structured_output: true,
            session_resume: true,
            models: vec![
                "opus".to_string(),
                "sonnet".to_string(),
                "haiku".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::claude::mock::MockClaude;
    use std::path::PathBuf;
    use std::time::Duration;

    #[tokio::test]
    async fn invoke_maps_session_result_to_runtime_response() {
        let claude = Arc::new(MockClaude::new());
        claude.enqueue_response("output text", 0);
        let runtime = ClaudeRuntime::new(claude);

        let request = RuntimeRequest {
            working_dir: PathBuf::from("/tmp/test"),
            prompt: "test prompt".to_string(),
            model: None,
            system_prompt: None,
            structured_output: None,
            session_id: None,
        };

        let response = runtime.invoke(request).await;

        assert_eq!(response.exit_code, 0);
        assert_eq!(response.stdout, "output text");
        assert!(response.is_success());
        assert!(response.duration < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn invoke_maps_error_to_error_response() {
        let claude = Arc::new(MockClaude::new());
        // No response enqueued
        let runtime = ClaudeRuntime::new(claude);

        let request = RuntimeRequest {
            working_dir: PathBuf::from("/tmp/test"),
            prompt: "test".to_string(),
            model: None,
            system_prompt: None,
            structured_output: None,
            session_id: None,
        };

        let response = runtime.invoke(request).await;
        // MockClaude returns exit_code 1 with empty stdout when no response configured
        assert_eq!(response.exit_code, 1);
        assert!(!response.is_success());
    }

    #[tokio::test]
    async fn invoke_passes_structured_output_options() {
        let claude = Arc::new(MockClaude::new());
        claude.enqueue_response("{}", 0);
        let runtime = ClaudeRuntime::new(claude.clone());

        let request = RuntimeRequest {
            working_dir: PathBuf::from("/tmp/test"),
            prompt: "test".to_string(),
            model: None,
            system_prompt: Some("be concise".to_string()),
            structured_output: Some(crate::core::runtime::StructuredOutput {
                format: "json".to_string(),
                schema: Some(r#"{"type":"object"}"#.to_string()),
            }),
            session_id: None,
        };

        runtime.invoke(request).await;

        let calls = claude.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].output_format, Some("json".to_string()));
        assert_eq!(
            calls[0].json_schema,
            Some(r#"{"type":"object"}"#.to_string())
        );
        assert_eq!(
            calls[0].append_system_prompt,
            Some("be concise".to_string())
        );
    }

    #[test]
    fn capabilities_reports_structured_output_and_session_resume() {
        let claude = Arc::new(MockClaude::new());
        let runtime = ClaudeRuntime::new(claude);
        let caps = runtime.capabilities();

        assert!(caps.structured_output);
        assert!(caps.session_resume);
        assert!(caps.models.contains(&"sonnet".to_string()));
    }

    #[test]
    fn with_default_model_sets_model() {
        let claude = Arc::new(MockClaude::new());
        let runtime = ClaudeRuntime::new(claude).with_default_model("opus");
        assert_eq!(runtime.default_model, "opus");
    }
}
