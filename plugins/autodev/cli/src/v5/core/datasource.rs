use anyhow::Result;
use async_trait::async_trait;

use super::context::ItemContext;
use super::queue_item::V5QueueItem;
use super::workspace::WorkspaceConfig;

/// v5 DataSource trait.
///
/// 외부 시스템(GitHub 등)에서 큐 아이템을 수집하고 컨텍스트를 제공한다.
/// DataSource는 워크플로우 상태(state)와 trigger 조건을 소유하며,
/// Core는 queue phase 전이와 handler 실행만 담당한다.
#[async_trait]
pub trait DataSource: Send + Sync {
    /// DataSource 이름 (e.g. "github")
    fn name(&self) -> &str;

    /// 외부 시스템을 스캔하여 새 큐 아이템을 수집한다.
    ///
    /// workspace config의 states에 정의된 trigger 조건을 검사하여
    /// 매칭되는 아이템을 V5QueueItem으로 변환한다.
    async fn collect(&mut self, workspace: &WorkspaceConfig) -> Result<Vec<V5QueueItem>>;

    /// 큐 아이템의 전체 컨텍스트를 구성한다.
    ///
    /// on_done/on_fail script에서 `autodev context $WORK_ID --json`으로 조회할 때
    /// 이 메서드의 반환값이 JSON 출력된다.
    async fn get_context(&self, item: &V5QueueItem) -> Result<ItemContext>;
}
