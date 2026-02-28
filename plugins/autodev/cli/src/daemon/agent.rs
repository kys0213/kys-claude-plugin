//! Agent trait 정의.
//!
//! Claude CLI 호출을 추상화하여 Task에서 직접 Claude에 의존하지 않게 한다.
//! 테스트에서는 MockAgent를 주입하여 실제 Claude 호출 없이 검증할 수 있다.

use async_trait::async_trait;

use super::task::{AgentRequest, AgentResponse};

/// Agent — Claude CLI 호출 추상화.
///
/// `AgentRequest`를 받아 `AgentResponse`를 반환한다.
/// 실제 구현체 `ClaudeAgent`는 `Claude` trait을 래핑한다.
#[async_trait]
pub trait Agent: Send + Sync {
    /// 프롬프트를 Agent에게 전달하고 응답을 받는다.
    /// 네트워크/프로세스 오류도 `AgentResponse::error()`로 반환한다 (panic 없음).
    async fn invoke(&self, request: AgentRequest) -> AgentResponse;
}
