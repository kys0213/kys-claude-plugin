//! AgentRuntime trait — v5 LLM 실행 추상화.
//!
//! LLM 실행 시스템(Claude, Gemini, Codex, ...)을 추상화한다.
//! 새 LLM 추가 = 새 AgentRuntime impl, 코어 변경 0 (OCP).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// LLM 실행 시스템 추상화.
///
/// handler의 prompt 타입 액션이 실행될 때 `invoke()`를 경유한다.
/// `working_dir`에는 worktree 경로가 설정된다.
///
/// # 모델 결정 우선순위
/// 1. `RuntimeRequest.model` -- 호출 시 명시 (최우선)
/// 2. handler의 runtime/model -- DataSource state config
/// 3. workspace yaml의 runtime 기본값
/// 4. 런타임 내장 기본 모델
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    /// 런타임 이름 (예: "claude", "gemini", "codex").
    fn name(&self) -> &str;

    /// 프롬프트를 LLM에 전달하고 응답을 받는다.
    async fn invoke(&self, request: RuntimeRequest) -> RuntimeResponse;

    /// 이 런타임이 지원하는 기능 목록.
    fn capabilities(&self) -> RuntimeCapabilities;
}

/// LLM 실행 요청.
#[derive(Debug, Clone)]
pub struct RuntimeRequest {
    /// 작업 디렉토리 (worktree 경로)
    pub working_dir: PathBuf,
    /// Agent에 보낼 프롬프트
    pub prompt: String,
    /// 사용할 모델 (None이면 런타임 기본값)
    pub model: Option<String>,
    /// system prompt 추가
    pub system_prompt: Option<String>,
    /// 구조화된 출력 스키마
    pub structured_output: Option<StructuredOutput>,
    /// 세션 ID (이전 대화 이어가기)
    pub session_id: Option<String>,
}

/// 구조화된 출력 요청.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredOutput {
    /// 출력 포맷 (예: "json")
    pub format: String,
    /// JSON schema 문자열
    pub schema: Option<String>,
}

/// LLM 실행 응답.
#[derive(Debug, Clone)]
pub struct RuntimeResponse {
    /// 프로세스 exit code
    pub exit_code: i32,
    /// 표준 출력
    pub stdout: String,
    /// 표준 에러
    pub stderr: String,
    /// 실행 시간
    pub duration: Duration,
    /// 토큰 사용량 (런타임이 지원하는 경우)
    pub token_usage: Option<TokenUsage>,
    /// 세션 ID (대화 이어가기용)
    pub session_id: Option<String>,
}

impl RuntimeResponse {
    /// 성공 여부 확인.
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// 에러 응답 생성.
    pub fn error(msg: impl ToString) -> Self {
        Self {
            exit_code: -1,
            stdout: String::new(),
            stderr: msg.to_string(),
            duration: Duration::ZERO,
            token_usage: None,
            session_id: None,
        }
    }
}

/// 토큰 사용량.
///
/// Daemon이 `token_usage` 테이블에 자동 저장한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// 입력 토큰 수
    pub input_tokens: i64,
    /// 출력 토큰 수
    pub output_tokens: i64,
    /// 캐시 쓰기 토큰 수
    pub cache_write_tokens: i64,
    /// 캐시 읽기 토큰 수
    pub cache_read_tokens: i64,
}

/// 런타임이 지원하는 기능 목록.
#[derive(Debug, Clone)]
pub struct RuntimeCapabilities {
    /// 구조화된 출력 지원 여부
    pub structured_output: bool,
    /// 세션 이어가기 지원 여부
    pub session_resume: bool,
    /// 지원하는 모델 목록
    pub models: Vec<String>,
}

/// 런타임 레지스트리 — 이름으로 런타임을 조회한다.
///
/// workspace yaml의 handler에서 지정한 runtime 이름을
/// 실제 AgentRuntime 구현체로 매핑한다.
pub struct RuntimeRegistry {
    runtimes: HashMap<String, Arc<dyn AgentRuntime>>,
    default_name: String,
}

impl RuntimeRegistry {
    /// 새 레지스트리를 생성한다.
    pub fn new(default_name: String) -> Self {
        Self {
            runtimes: HashMap::new(),
            default_name,
        }
    }

    /// 런타임을 등록한다.
    pub fn register(&mut self, runtime: Arc<dyn AgentRuntime>) {
        self.runtimes.insert(runtime.name().to_string(), runtime);
    }

    /// 이름으로 런타임을 조회한다. 없으면 기본 런타임을 반환한다.
    pub fn resolve(&self, name: &str) -> Option<Arc<dyn AgentRuntime>> {
        self.runtimes
            .get(name)
            .or_else(|| self.runtimes.get(&self.default_name))
            .cloned()
    }

    /// 등록된 런타임 이름 목록.
    pub fn names(&self) -> Vec<&str> {
        self.runtimes.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;

    /// 테스트용 RuntimeRequest 생성.
    pub fn test_runtime_request(prompt: &str) -> RuntimeRequest {
        RuntimeRequest {
            working_dir: PathBuf::from("/tmp/test-worktree"),
            prompt: prompt.to_string(),
            model: None,
            system_prompt: None,
            structured_output: None,
            session_id: None,
        }
    }

    /// 테스트용 성공 RuntimeResponse 생성.
    pub fn test_runtime_response(stdout: &str) -> RuntimeResponse {
        RuntimeResponse {
            exit_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
            duration: Duration::from_millis(100),
            token_usage: Some(TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_write_tokens: 0,
                cache_read_tokens: 0,
            }),
            session_id: None,
        }
    }
}
