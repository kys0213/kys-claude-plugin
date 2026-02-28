//! ClaudeAgent — Agent trait의 실제 구현체.
//!
//! Claude CLI를 래핑하여 Task에서 직접 Claude에 의존하지 않게 한다.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use super::agent::Agent;
use super::task::{AgentRequest, AgentResponse};
use crate::infrastructure::claude::Claude;

/// Claude CLI를 래핑하는 Agent 구현체.
pub struct ClaudeAgent {
    claude: Arc<dyn Claude>,
}

impl ClaudeAgent {
    pub fn new(claude: Arc<dyn Claude>) -> Self {
        Self { claude }
    }
}

#[async_trait]
impl Agent for ClaudeAgent {
    async fn invoke(&self, request: AgentRequest) -> AgentResponse {
        let start = Instant::now();

        match self
            .claude
            .run_session(&request.working_dir, &request.prompt, &request.session_opts)
            .await
        {
            Ok(result) => AgentResponse {
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
                duration: start.elapsed(),
            },
            Err(e) => {
                let mut resp = AgentResponse::error(e);
                resp.duration = start.elapsed();
                resp
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::claude::mock::MockClaude;
    use std::path::PathBuf;
    use std::time::Duration;

    use crate::infrastructure::claude::SessionOptions;

    #[tokio::test]
    async fn maps_session_result_to_response() {
        let claude = Arc::new(MockClaude::new());
        claude.enqueue_response("output text", 0);
        let agent = ClaudeAgent::new(claude);

        let request = AgentRequest {
            working_dir: PathBuf::from("/tmp/test"),
            prompt: "test prompt".to_string(),
            session_opts: SessionOptions::default(),
        };

        let response = agent.invoke(request).await;

        assert_eq!(response.exit_code, 0);
        assert_eq!(response.stdout, "output text");
        assert!(response.duration < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn maps_nonzero_exit_to_response() {
        let claude = Arc::new(MockClaude::new());
        claude.enqueue_response("error output", 1);
        let agent = ClaudeAgent::new(claude);

        let request = AgentRequest {
            working_dir: PathBuf::from("/tmp/test"),
            prompt: "test".to_string(),
            session_opts: SessionOptions::default(),
        };

        let response = agent.invoke(request).await;

        assert_eq!(response.exit_code, 1);
        assert_eq!(response.stdout, "error output");
    }

    #[tokio::test]
    async fn maps_error_to_error_response() {
        let claude = Arc::new(MockClaude::new());
        // No response enqueued → will return default error
        let agent = ClaudeAgent::new(claude);

        let request = AgentRequest {
            working_dir: PathBuf::from("/tmp/test"),
            prompt: "test".to_string(),
            session_opts: SessionOptions::default(),
        };

        let response = agent.invoke(request).await;

        // MockClaude returns exit_code 1 with empty stdout when no response configured
        assert_eq!(response.exit_code, 1);
    }
}
