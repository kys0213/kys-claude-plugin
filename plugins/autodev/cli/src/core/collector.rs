//! Collector trait 정의.
//!
//! GitHub 이슈/PR 스캔 등 외부 소스에서 Task를 생성하는 인터페이스.
//! TaskManager가 Collector를 poll하여 실행 가능한 Task를 수집한다.

use async_trait::async_trait;

use crate::core::task::{Task, TaskResult};
use crate::service::daemon::status::StatusItem;

/// Task를 생성하는 외부 소스.
///
/// 구현체 예시:
/// - `GitHubTaskSource`: GitHub 이슈/PR 스캔 → Task 생성
///
/// 생명주기:
/// 1. `TaskManager.tick()` → `poll()` 호출 → 새 Task 수집
/// 2. Task 실행 완료 → `apply()` 호출 → 큐 상태 반영
#[async_trait(?Send)]
pub trait Collector: Send {
    /// Scan external sources and return new Tasks ready for execution.
    async fn poll(&mut self) -> Vec<Box<dyn Task>>;

    /// Drain tasks that are ready for execution.
    ///
    /// poll()이 수집만 담당하고, drain_tasks()가 실행 가능한 Task를 꺼낸다.
    fn drain_tasks(&mut self) -> Vec<Box<dyn Task>>;

    /// Apply completed task results back to internal state.
    fn apply(&mut self, result: &TaskResult);

    /// Return currently active items for status reporting.
    fn active_items(&self) -> Vec<StatusItem>;

    /// 워크스페이스(레포)별 concurrency 상한을 반환한다.
    /// key = repo_name, value = concurrency limit (0이면 제한 없음).
    fn workspace_limits(&self) -> Vec<(String, usize)>;
}
