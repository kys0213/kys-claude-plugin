//! TaskManager trait 정의.
//!
//! TaskSource에서 Task를 수집하고, Daemon에게 실행 가능한 Task를 제공한다.
//! Daemon은 TaskManager를 통해서만 Task를 얻으며, 직접 큐나 소스에 접근하지 않는다.

use async_trait::async_trait;

use super::task::{Task, TaskResult};

/// Task 수집 및 분배 관리자.
///
/// Daemon과 TaskSource 사이의 중재자 역할:
/// - `tick()`: 주기적으로 TaskSource를 poll하여 Task 수집
/// - `drain_ready()`: 실행 가능한 Task를 Daemon에 전달
/// - `apply()`: 완료된 Task 결과를 TaskSource에 반영
#[async_trait]
pub trait TaskManager: Send + Sync {
    /// 주기적 하우스키핑.
    /// 모든 TaskSource를 poll하여 새 Task를 수집한다.
    async fn tick(&mut self);

    /// 실행 가능한 Task를 모두 꺼내 반환한다.
    /// 호출 후 내부 ready 목록은 비워진다.
    fn drain_ready(&mut self) -> Vec<Box<dyn Task>>;

    /// 완료된 Task 결과를 모든 TaskSource에 반영한다.
    fn apply(&mut self, result: TaskResult);
}
