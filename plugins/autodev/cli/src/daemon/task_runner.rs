//! TaskRunner trait 정의.
//!
//! Task의 생명주기(before_invoke → Agent → after_invoke)를 실행한다.
//! Daemon은 TaskRunner를 통해 Task를 실행하며, 직접 Agent를 호출하지 않는다.

use async_trait::async_trait;

use super::task::{Task, TaskResult};

/// Task 실행기.
///
/// Task 생명주기를 실행한다:
/// 1. `task.before_invoke()` → `AgentRequest` 또는 `SkipReason`
/// 2. `SkipReason`이면 → `TaskResult::skipped()` 반환
/// 3. `AgentRequest`면 → `Agent.invoke()` 호출
/// 4. `task.after_invoke(response)` → `TaskResult` 반환
#[async_trait]
pub trait TaskRunner: Send + Sync {
    /// Task를 실행하고 결과를 반환한다.
    async fn run(&self, task: Box<dyn Task>) -> TaskResult;
}
