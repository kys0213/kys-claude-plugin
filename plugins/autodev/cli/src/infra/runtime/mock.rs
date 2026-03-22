//! MockAgentRuntime — 테스트용 AgentRuntime 구현체.

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;

use crate::core::runtime::{
    AgentRuntime, RuntimeCapabilities, RuntimeRequest, RuntimeResponse, TokenUsage,
};

/// 호출 기록용 구조체.
#[derive(Debug)]
#[allow(dead_code)]
pub struct MockRuntimeCall {
    pub prompt: String,
    pub working_dir: String,
    pub model: Option<String>,
}

/// 테스트용 AgentRuntime — 미리 설정된 응답을 반환한다.
#[allow(dead_code)]
pub struct MockAgentRuntime {
    name: String,
    responses: Mutex<Vec<RuntimeResponse>>,
    pub calls: Mutex<Vec<MockRuntimeCall>>,
}

#[allow(dead_code)]
impl MockAgentRuntime {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            responses: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// 다음 invoke 호출 시 반환할 응답을 추가한다.
    pub fn enqueue_response(&self, stdout: &str, exit_code: i32) {
        self.responses.lock().unwrap().push(RuntimeResponse {
            exit_code,
            stdout: stdout.to_string(),
            stderr: String::new(),
            duration: Duration::from_millis(50),
            token_usage: Some(TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_write_tokens: 0,
                cache_read_tokens: 0,
            }),
            session_id: None,
        });
    }

    /// 호출 횟수.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl AgentRuntime for MockAgentRuntime {
    fn name(&self) -> &str {
        &self.name
    }

    async fn invoke(&self, request: RuntimeRequest) -> RuntimeResponse {
        self.calls.lock().unwrap().push(MockRuntimeCall {
            prompt: request.prompt,
            working_dir: request.working_dir.display().to_string(),
            model: request.model,
        });

        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            RuntimeResponse {
                exit_code: 1,
                stdout: String::new(),
                stderr: "mock: no response configured".to_string(),
                duration: Duration::from_millis(1),
                token_usage: None,
                session_id: None,
            }
        } else {
            responses.remove(0)
        }
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            structured_output: true,
            session_resume: false,
            models: vec!["mock-model".to_string()],
        }
    }
}
