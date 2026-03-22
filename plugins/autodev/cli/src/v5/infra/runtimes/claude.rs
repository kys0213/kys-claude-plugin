use std::time::Instant;

use async_trait::async_trait;

use crate::infra::claude::{Claude, SessionOptions};
use crate::v5::core::runtime::{
    AgentRuntime, RuntimeCapabilities, RuntimeRequest, RuntimeResponse,
};

/// v4 Claude trait를 래핑하는 v5 AgentRuntime 구현.
///
/// Model resolution priority:
///   1. RuntimeRequest.model (호출 시점 명시)
///   2. default_model (workspace yaml의 runtime.claude.model)
///   3. Claude CLI 기본값
pub struct ClaudeRuntime {
    claude: Box<dyn Claude>,
    default_model: Option<String>,
}

impl ClaudeRuntime {
    pub fn new(claude: Box<dyn Claude>, default_model: Option<String>) -> Self {
        Self {
            claude,
            default_model,
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

        // Model resolution: request.model > default_model > Claude CLI default
        let resolved_model = request.model.or_else(|| self.default_model.clone());

        let opts = SessionOptions {
            output_format: None,
            json_schema: None,
            append_system_prompt: request.system_prompt,
        };

        // model은 현재 v4 Claude trait의 SessionOptions에 포함되지 않으므로
        // 프롬프트에 모델 힌트를 추가하는 방식으로 전달
        let prompt = if let Some(ref model) = resolved_model {
            format!("[model: {model}]\n{}", request.prompt)
        } else {
            request.prompt.clone()
        };

        match self
            .claude
            .run_session(&request.working_dir, &prompt, &opts)
            .await
        {
            Ok(result) => RuntimeResponse {
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
                duration: start.elapsed(),
                token_usage: None,
                session_id: None,
            },
            Err(e) => RuntimeResponse::error(&format!("claude invocation failed: {e}")),
        }
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            supports_tool_use: true,
            supports_structured_output: true,
            supports_session: true,
        }
    }
}
