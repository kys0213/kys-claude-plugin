//! TaskSource trait 정의.
//!
//! GitHub 이슈/PR 스캔 등 외부 소스에서 Task를 생성하는 인터페이스.
//! TaskManager가 TaskSource를 poll하여 실행 가능한 Task를 수집한다.

use async_trait::async_trait;

use super::task::{Task, TaskResult};

/// Task를 생성하는 외부 소스.
///
/// 구현체 예시:
/// - `GitHubTaskSource`: GitHub 이슈/PR 스캔 → Task 생성
///
/// 생명주기:
/// 1. `TaskManager.tick()` → `poll()` 호출 → 새 Task 수집
/// 2. Task 실행 완료 → `apply()` 호출 → 큐 상태 반영
#[async_trait]
pub trait TaskSource: Send + Sync {
    /// 새로운 Task를 수집한다.
    /// repo sync, recovery, scan을 수행하고 실행 가능한 Task를 반환한다.
    async fn poll(&mut self) -> Vec<Box<dyn Task>>;

    /// 완료된 Task의 결과를 소스에 반영한다.
    /// 큐 상태 전이(Remove, Push 등)를 적용한다.
    fn apply(&mut self, result: &TaskResult);
}
