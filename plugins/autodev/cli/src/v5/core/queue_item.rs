use serde::{Deserialize, Serialize};

use super::phase::V5QueuePhase;

/// v5 큐 아이템.
///
/// v4 QueueItem과 달리 DataSource 중립적이다.
/// `source_id`로 같은 외부 엔티티에서 파생된 아이템들을 연결한다.
/// `state`는 DataSource가 정의한 워크플로우 상태 (e.g. "analyze", "implement").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V5QueueItem {
    /// 고유 식별자 (e.g. "github:org/repo#42:implement")
    pub work_id: String,
    /// 외부 엔티티 식별자 (e.g. "github:org/repo#42")
    pub source_id: String,
    /// 워크스페이스 식별자
    pub workspace_id: String,
    /// DataSource 정의 워크플로우 상태 (e.g. "analyze", "implement", "review")
    pub state: String,
    /// 큐 phase (Pending → Ready → Running → ...)
    pub phase: V5QueuePhase,
    /// 아이템 제목
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 생성 시각 (RFC3339)
    pub created_at: String,
    /// 마지막 업데이트 시각 (RFC3339)
    pub updated_at: String,
}

impl V5QueueItem {
    pub fn new(work_id: String, source_id: String, workspace_id: String, state: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            work_id,
            source_id,
            workspace_id,
            state,
            phase: V5QueuePhase::Pending,
            title: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// work_id를 규약에 따라 생성한다.
    /// format: "{source_id}:{state}"
    pub fn make_work_id(source_id: &str, state: &str) -> String {
        format!("{source_id}:{state}")
    }
}

/// StateQueue 호환 trait
impl crate::core::state_queue::HasWorkId for V5QueueItem {
    fn work_id(&self) -> &str {
        &self.work_id
    }
}

/// DB row 표현. queue_items 테이블에서 읽고 쓸 때 사용한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V5QueueItemRow {
    pub work_id: String,
    pub source_id: String,
    pub workspace_id: String,
    pub state: String,
    pub phase: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl V5QueueItem {
    /// DB row로 변환.
    pub fn to_row(&self) -> V5QueueItemRow {
        V5QueueItemRow {
            work_id: self.work_id.clone(),
            source_id: self.source_id.clone(),
            workspace_id: self.workspace_id.clone(),
            state: self.state.clone(),
            phase: self.phase.as_str().to_string(),
            title: self.title.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    /// DB row에서 복원.
    pub fn from_row(row: &V5QueueItemRow) -> Result<Self, String> {
        let phase: V5QueuePhase = row.phase.parse()?;
        Ok(Self {
            work_id: row.work_id.clone(),
            source_id: row.source_id.clone(),
            workspace_id: row.workspace_id.clone(),
            state: row.state.clone(),
            phase,
            title: row.title.clone(),
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
        })
    }
}

/// 테스트 팩토리. 통합 테스트에서도 사용하므로 cfg(test)를 걸지 않는다.
pub mod testing {
    use super::*;

    pub fn test_item(source_id: &str, state: &str) -> V5QueueItem {
        let work_id = V5QueueItem::make_work_id(source_id, state);
        V5QueueItem {
            work_id,
            source_id: source_id.to_string(),
            workspace_id: "test-ws".to_string(),
            state: state.to_string(),
            phase: V5QueuePhase::Pending,
            title: Some(format!("Test item: {state}")),
            created_at: "2026-03-22T00:00:00Z".to_string(),
            updated_at: "2026-03-22T00:00:00Z".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn make_work_id_format() {
        let id = V5QueueItem::make_work_id("github:org/repo#42", "implement");
        assert_eq!(id, "github:org/repo#42:implement");
    }

    #[test]
    fn new_creates_pending() {
        let item = V5QueueItem::new(
            "wid".to_string(),
            "sid".to_string(),
            "ws".to_string(),
            "analyze".to_string(),
        );
        assert_eq!(item.phase, V5QueuePhase::Pending);
        assert!(!item.created_at.is_empty());
    }

    #[test]
    fn test_item_factory() {
        let item = test_item("github:org/repo#42", "implement");
        assert_eq!(item.work_id, "github:org/repo#42:implement");
        assert_eq!(item.source_id, "github:org/repo#42");
        assert_eq!(item.state, "implement");
        assert_eq!(item.phase, V5QueuePhase::Pending);
    }

    #[test]
    fn source_id_connects_lineage() {
        let a = test_item("github:org/repo#42", "analyze");
        let i = test_item("github:org/repo#42", "implement");
        let r = test_item("github:org/repo#42", "review");

        assert_eq!(a.source_id, i.source_id);
        assert_eq!(i.source_id, r.source_id);
        assert_ne!(a.work_id, i.work_id);
    }

    #[test]
    fn to_row_roundtrip() {
        let item = test_item("github:org/repo#42", "implement");
        let row = item.to_row();

        assert_eq!(row.work_id, "github:org/repo#42:implement");
        assert_eq!(row.phase, "pending");
        assert_eq!(row.state, "implement");

        let restored = V5QueueItem::from_row(&row).unwrap();
        assert_eq!(restored.work_id, item.work_id);
        assert_eq!(restored.source_id, item.source_id);
        assert_eq!(restored.phase, item.phase);
        assert_eq!(restored.state, item.state);
    }

    #[test]
    fn from_row_invalid_phase() {
        let row = V5QueueItemRow {
            work_id: "w".to_string(),
            source_id: "s".to_string(),
            workspace_id: "ws".to_string(),
            state: "test".to_string(),
            phase: "invalid_phase".to_string(),
            title: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        assert!(V5QueueItem::from_row(&row).is_err());
    }

    #[test]
    fn json_roundtrip() {
        let item = test_item("github:org/repo#42", "analyze");
        let json = serde_json::to_string(&item).unwrap();
        let parsed: V5QueueItem = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.work_id, item.work_id);
        assert_eq!(parsed.phase, item.phase);
    }

    #[test]
    fn has_work_id_trait() {
        use crate::core::state_queue::HasWorkId;
        let item = test_item("github:org/repo#42", "implement");
        assert_eq!(HasWorkId::work_id(&item), "github:org/repo#42:implement");
    }
}
