use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;

use crate::v5::core::context::{HistoryEntry, ItemContext, QueueContext, SourceContext};
use crate::v5::core::datasource::DataSource;
use crate::v5::core::queue_item::V5QueueItem;
use crate::v5::core::workspace::WorkspaceConfig;

/// 테스트용 MockDataSource.
///
/// collect()에서 반환할 아이템과 get_context()에서 반환할 컨텍스트를
/// 사전에 설정할 수 있다.
pub struct MockDataSource {
    source_name: String,
    items: Vec<V5QueueItem>,
    contexts: HashMap<String, ItemContext>,
}

impl MockDataSource {
    pub fn new(name: &str) -> Self {
        Self {
            source_name: name.to_string(),
            items: Vec::new(),
            contexts: HashMap::new(),
        }
    }

    /// collect() 시 반환할 아이템 추가.
    pub fn add_item(&mut self, item: V5QueueItem) {
        self.items.push(item);
    }

    /// get_context() 시 반환할 컨텍스트 설정.
    pub fn set_context(&mut self, work_id: &str, context: ItemContext) {
        self.contexts.insert(work_id.to_string(), context);
    }

    /// 기본 ItemContext 생성 헬퍼.
    pub fn default_context(item: &V5QueueItem) -> ItemContext {
        ItemContext {
            work_id: item.work_id.clone(),
            workspace: item.workspace_id.clone(),
            queue: QueueContext {
                phase: item.phase.as_str().to_string(),
                state: item.state.clone(),
                source_id: item.source_id.clone(),
            },
            source: SourceContext {
                source_type: "mock".to_string(),
                url: "https://mock.example.com".to_string(),
                default_branch: Some("main".to_string()),
            },
            issue: None,
            pr: None,
            history: Vec::new(),
            worktree: None,
        }
    }

    /// history와 함께 context 생성.
    pub fn context_with_history(item: &V5QueueItem, history: Vec<HistoryEntry>) -> ItemContext {
        let mut ctx = Self::default_context(item);
        ctx.history = history;
        ctx
    }
}

#[async_trait]
impl DataSource for MockDataSource {
    fn name(&self) -> &str {
        &self.source_name
    }

    async fn collect(&mut self, _workspace: &WorkspaceConfig) -> Result<Vec<V5QueueItem>> {
        Ok(std::mem::take(&mut self.items))
    }

    async fn get_context(&self, item: &V5QueueItem) -> Result<ItemContext> {
        match self.contexts.get(&item.work_id) {
            Some(ctx) => Ok(ctx.clone()),
            None => Ok(Self::default_context(item)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v5::core::queue_item::testing::test_item;

    #[tokio::test]
    async fn collect_returns_added_items() {
        let mut source = MockDataSource::new("github");
        source.add_item(test_item("github:org/repo#1", "analyze"));
        source.add_item(test_item("github:org/repo#2", "implement"));

        let config: WorkspaceConfig = serde_yml::from_str("name: test\nsources: {}").unwrap();
        let items = source.collect(&config).await.unwrap();
        assert_eq!(items.len(), 2);

        // collect는 drain이므로 두 번째 호출은 빈 벡터
        let items = source.collect(&config).await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn get_context_default() {
        let source = MockDataSource::new("github");
        let item = test_item("github:org/repo#1", "analyze");
        let ctx = source.get_context(&item).await.unwrap();
        assert_eq!(ctx.work_id, item.work_id);
        assert_eq!(ctx.source.source_type, "mock");
    }

    #[tokio::test]
    async fn get_context_custom() {
        let mut source = MockDataSource::new("github");
        let item = test_item("github:org/repo#1", "analyze");
        let mut custom_ctx = MockDataSource::default_context(&item);
        custom_ctx.source.source_type = "github".to_string();
        source.set_context(&item.work_id, custom_ctx);

        let ctx = source.get_context(&item).await.unwrap();
        assert_eq!(ctx.source.source_type, "github");
    }
}
