use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use super::{Claude, SessionResult};

/// 테스트용 Claude 구현체 — 미리 설정된 응답을 반환
#[allow(dead_code)]
pub struct MockClaude {
    /// 순차적으로 반환할 응답 큐 (FIFO)
    responses: Mutex<Vec<SessionResult>>,
    /// 호출 기록: (cwd, prompt, output_format)
    pub calls: Mutex<Vec<(String, String, Option<String>)>>,
}

impl Default for MockClaude {
    fn default() -> Self {
        Self {
            responses: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[allow(dead_code)]
impl MockClaude {
    pub fn new() -> Self {
        Self::default()
    }

    /// 다음 run_session 호출 시 반환할 응답 추가
    pub fn enqueue_response(&self, stdout: &str, exit_code: i32) {
        self.responses.lock().unwrap().push(SessionResult {
            stdout: stdout.to_string(),
            stderr: String::new(),
            exit_code,
        });
    }

    /// 호출 횟수
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl Claude for MockClaude {
    async fn run_session(
        &self,
        cwd: &Path,
        prompt: &str,
        output_format: Option<&str>,
    ) -> Result<SessionResult> {
        self.calls.lock().unwrap().push((
            cwd.display().to_string(),
            prompt.to_string(),
            output_format.map(String::from),
        ));

        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            Ok(SessionResult {
                stdout: String::new(),
                stderr: "mock: no response configured".to_string(),
                exit_code: 1,
            })
        } else {
            Ok(responses.remove(0))
        }
    }
}
