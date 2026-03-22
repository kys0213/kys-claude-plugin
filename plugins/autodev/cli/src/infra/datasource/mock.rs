//! MockDataSource — 테스트용 DataSource 구현체.

use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use crate::core::datasource::{DataSource, ItemContext, WorkspaceConfig};
use crate::core::queue_item::QueueItem;

/// 테스트용 DataSource — 미리 설정된 아이템과 컨텍스트를 반환한다.
#[allow(dead_code)]
pub struct MockDataSource {
    name: String,
    items: Mutex<Vec<Vec<QueueItem>>>,
    contexts: Mutex<Vec<ItemContext>>,
    pub collect_count: Mutex<u32>,
}

#[allow(dead_code)]
impl MockDataSource {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            items: Mutex::new(Vec::new()),
            contexts: Mutex::new(Vec::new()),
            collect_count: Mutex::new(0),
        }
    }

    /// 다음 collect 호출 시 반환할 아이템 배치를 추가한다.
    pub fn enqueue_items(&self, items: Vec<QueueItem>) {
        self.items.lock().unwrap().push(items);
    }

    /// get_context 호출 시 반환할 컨텍스트를 추가한다.
    pub fn enqueue_context(&self, ctx: ItemContext) {
        self.contexts.lock().unwrap().push(ctx);
    }
}

#[async_trait]
impl DataSource for MockDataSource {
    fn name(&self) -> &str {
        &self.name
    }

    async fn collect(&self, _workspace: &WorkspaceConfig) -> Result<Vec<QueueItem>> {
        *self.collect_count.lock().unwrap() += 1;
        let mut items = self.items.lock().unwrap();
        if items.is_empty() {
            Ok(vec![])
        } else {
            Ok(items.remove(0))
        }
    }

    async fn get_context(&self, _item: &QueueItem) -> Result<ItemContext> {
        let mut contexts = self.contexts.lock().unwrap();
        if contexts.is_empty() {
            anyhow::bail!("mock: no context configured")
        } else {
            Ok(contexts.remove(0))
        }
    }
}
