use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

/// v5 AgentRuntime trait.
///
/// LLM 실행을 추상화한다. Claude, Gemini 등 다양한 런타임을 동일 인터페이스로 사용.
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    /// 런타임 이름 (e.g. "claude", "gemini")
    fn name(&self) -> &str;

    /// 프롬프트 실행
    async fn invoke(&self, request: RuntimeRequest) -> RuntimeResponse;

    /// 런타임이 지원하는 기능
    fn capabilities(&self) -> RuntimeCapabilities;
}

/// 런타임 호출 요청.
#[derive(Debug, Clone)]
pub struct RuntimeRequest {
    pub working_dir: PathBuf,
    pub prompt: String,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub session_id: Option<String>,
}

/// 런타임 호출 응답.
#[derive(Debug, Clone)]
pub struct RuntimeResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
    pub token_usage: Option<TokenUsage>,
    pub session_id: Option<String>,
}

impl RuntimeResponse {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    pub fn error(message: &str) -> Self {
        Self {
            exit_code: -1,
            stdout: String::new(),
            stderr: message.to_string(),
            duration: Duration::ZERO,
            token_usage: None,
            session_id: None,
        }
    }
}

/// 토큰 사용량.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

/// 런타임 기능 플래그.
#[derive(Debug, Clone, Default)]
pub struct RuntimeCapabilities {
    pub supports_tool_use: bool,
    pub supports_structured_output: bool,
    pub supports_session: bool,
}

/// 런타임 레지스트리.
///
/// 이름으로 런타임을 resolve한다. 없으면 default 런타임을 반환.
pub struct RuntimeRegistry {
    runtimes: HashMap<String, Arc<dyn AgentRuntime>>,
    default_name: String,
}

impl RuntimeRegistry {
    pub fn new(default_name: String) -> Self {
        Self {
            runtimes: HashMap::new(),
            default_name,
        }
    }

    pub fn register(&mut self, runtime: Arc<dyn AgentRuntime>) {
        let name = runtime.name().to_string();
        self.runtimes.insert(name, runtime);
    }

    /// 이름으로 런타임을 resolve한다. 없으면 default를 반환.
    pub fn resolve(&self, name: &str) -> Option<Arc<dyn AgentRuntime>> {
        self.runtimes
            .get(name)
            .or_else(|| self.runtimes.get(&self.default_name))
            .cloned()
    }

    pub fn default_runtime(&self) -> Option<Arc<dyn AgentRuntime>> {
        self.runtimes.get(&self.default_name).cloned()
    }

    pub fn default_name(&self) -> &str {
        &self.default_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyRuntime {
        name: String,
    }

    #[async_trait]
    impl AgentRuntime for DummyRuntime {
        fn name(&self) -> &str {
            &self.name
        }
        async fn invoke(&self, _request: RuntimeRequest) -> RuntimeResponse {
            RuntimeResponse {
                exit_code: 0,
                stdout: "ok".to_string(),
                stderr: String::new(),
                duration: Duration::from_secs(1),
                token_usage: None,
                session_id: None,
            }
        }
        fn capabilities(&self) -> RuntimeCapabilities {
            RuntimeCapabilities::default()
        }
    }

    #[test]
    fn registry_resolve() {
        let mut registry = RuntimeRegistry::new("claude".to_string());
        registry.register(Arc::new(DummyRuntime {
            name: "claude".to_string(),
        }));
        registry.register(Arc::new(DummyRuntime {
            name: "gemini".to_string(),
        }));

        assert_eq!(registry.resolve("claude").unwrap().name(), "claude");
        assert_eq!(registry.resolve("gemini").unwrap().name(), "gemini");
    }

    #[test]
    fn registry_fallback_to_default() {
        let mut registry = RuntimeRegistry::new("claude".to_string());
        registry.register(Arc::new(DummyRuntime {
            name: "claude".to_string(),
        }));

        // 없는 런타임 요청 → default로 fallback
        let resolved = registry.resolve("nonexistent").unwrap();
        assert_eq!(resolved.name(), "claude");
    }

    #[test]
    fn registry_empty_returns_none() {
        let registry = RuntimeRegistry::new("claude".to_string());
        assert!(registry.resolve("anything").is_none());
        assert!(registry.default_runtime().is_none());
    }

    #[test]
    fn runtime_response_success() {
        let ok = RuntimeResponse {
            exit_code: 0,
            stdout: "done".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(1),
            token_usage: None,
            session_id: None,
        };
        assert!(ok.success());

        let fail = RuntimeResponse::error("boom");
        assert!(!fail.success());
        assert_eq!(fail.exit_code, -1);
        assert_eq!(fail.stderr, "boom");
    }
}
