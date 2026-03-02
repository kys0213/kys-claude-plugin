//! TaskManager trait 정의.
//!
//! TaskSource에서 Task를 수집하고, Daemon에게 실행 가능한 Task를 제공한다.
//! Daemon은 TaskManager를 통해서만 Task를 얻으며, 직접 큐나 소스에 접근하지 않는다.

use async_trait::async_trait;

use super::status::StatusItem;
use super::task::{Task, TaskResult};

/// Task 수집 및 분배 관리자.
///
/// Daemon과 TaskSource 사이의 중재자 역할:
/// - `tick()`: 주기적으로 TaskSource를 poll하여 Task 수집
/// - `drain_ready()`: 실행 가능한 Task를 Daemon에 전달
/// - `pop_ready()`: 실행 가능한 Task를 하나씩 꺼낸다 (인플라이트 제한 대응)
/// - `apply()`: 완료된 Task 결과를 TaskSource에 반영
/// - `active_items()`: status heartbeat용 활성 아이템 목록 반환
#[async_trait(?Send)]
pub trait TaskManager: Send {
    /// 주기적 하우스키핑.
    /// 모든 TaskSource를 poll하여 새 Task를 수집한다.
    async fn tick(&mut self);

    /// 실행 가능한 Task를 모두 꺼내 반환한다.
    /// 호출 후 내부 ready 목록은 비워진다.
    fn drain_ready(&mut self) -> Vec<Box<dyn Task>>;

    /// 실행 가능한 Task를 하나 꺼낸다.
    /// 인플라이트 제한에 맞춰 하나씩 꺼내 spawn할 때 사용한다.
    fn pop_ready(&mut self) -> Option<Box<dyn Task>>;

    /// 완료된 Task 결과를 모든 TaskSource에 반영한다.
    fn apply(&mut self, result: &TaskResult);

    /// 현재 활성 아이템 목록을 반환한다 (status heartbeat용).
    fn active_items(&self) -> Vec<StatusItem>;
}
