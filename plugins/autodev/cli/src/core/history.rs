use serde::{Deserialize, Serialize};

/// Append-only history entry for source_id lineage tracking.
///
/// 같은 `source_id`의 모든 상태 전이 이벤트가 시간순으로 축적된다.
/// 삭제 불가(append-only), 실패 횟수 등은 history에서 동적으로 계산한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub source_id: String,
    pub work_id: String,
    /// 작업 상태 (e.g. "analyze", "implement", "review")
    pub state: String,
    /// 상태 (e.g. "running", "done", "failed")
    pub status: String,
    /// 시도 횟수 (같은 state 내에서의 재시도 번호)
    pub attempt: i32,
    /// 성공 시 요약
    pub summary: Option<String>,
    /// 실패 시 에러 메시지
    pub error: Option<String>,
    pub created_at: String,
}

/// History 추가용 입력 모델.
pub struct NewHistoryEntry {
    pub source_id: String,
    pub work_id: String,
    pub state: String,
    pub status: String,
    pub attempt: i32,
    pub summary: Option<String>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_entry_serialize_roundtrip() {
        let entry = HistoryEntry {
            id: 1,
            source_id: "github:org/repo#42".into(),
            work_id: "github:org/repo#42:analyze".into(),
            state: "analyze".into(),
            status: "done".into(),
            attempt: 1,
            summary: Some("구현 가능".into()),
            error: None,
            created_at: "2024-01-01T00:00:00Z".into(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let restored: HistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.source_id, "github:org/repo#42");
        assert_eq!(restored.state, "analyze");
        assert_eq!(restored.status, "done");
        assert_eq!(restored.attempt, 1);
        assert_eq!(restored.summary.as_deref(), Some("구현 가능"));
        assert!(restored.error.is_none());
    }
}
