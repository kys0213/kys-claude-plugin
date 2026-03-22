//! DataSource trait — v5 외부 시스템 추상화.
//!
//! 외부 시스템(GitHub, Jira, Slack, ...)에서 작업 아이템을 수집하고
//! 해당 아이템의 컨텍스트를 조회하는 인터페이스.
//!
//! 새 외부 시스템 추가 = 새 DataSource impl, 코어 변경 0 (OCP).

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::queue_item::QueueItem;

/// 외부 시스템에서 아이템을 수집하고 컨텍스트를 조회하는 인터페이스.
///
/// 각 DataSource는 자기 시스템의 상태 표현으로 워크플로우를 정의한다.
/// 코어는 DataSource 내부를 모른다. collect() 결과를 큐에 넣고,
/// 상태 전이만 관리한다.
///
/// # 역할
/// 1. 수집(collect) — 어떤 조건에서 아이템을 감지하는가 (trigger)
/// 2. 컨텍스트(get_context) — 해당 아이템의 외부 시스템 정보를 조회하는가
#[async_trait]
pub trait DataSource: Send + Sync {
    /// DataSource 이름 (예: "github", "jira").
    fn name(&self) -> &str;

    /// 외부 시스템에서 trigger 조건에 매칭되는 새 아이템을 감지한다.
    ///
    /// workspace 설정에 정의된 sources 섹션에 따라 스캔 조건이 결정된다.
    async fn collect(&self, workspace: &WorkspaceConfig) -> Result<Vec<QueueItem>>;

    /// 해당 아이템의 외부 시스템 컨텍스트를 조회한다.
    ///
    /// `autodev context $WORK_ID --json` CLI가 내부적으로 호출한다.
    async fn get_context(&self, item: &QueueItem) -> Result<ItemContext>;
}

/// Workspace 설정 — DataSource가 수집 시 참조하는 설정.
///
/// workspace yaml에서 로드되며, sources 섹션에 DataSource별 설정을 포함한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// workspace 이름 (예: "auth-project")
    pub name: String,
    /// DataSource별 설정 (key: DataSource name, value: JSON)
    pub sources: HashMap<String, SourceConfig>,
    /// 동시 실행 제한
    pub concurrency: u32,
}

/// DataSource별 설정.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    /// 외부 시스템 URL (예: "https://github.com/org/repo")
    pub url: String,
    /// 스캔 주기 (초)
    pub scan_interval_secs: u64,
    /// DataSource 레벨 동시 실행 제한
    pub concurrency: u32,
}

/// 아이템의 외부 시스템 컨텍스트.
///
/// `autodev context $WORK_ID --json`의 응답 구조.
/// DataSource마다 source_data에 시스템별 정보를 담는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemContext {
    /// 큐 아이템 식별자
    pub work_id: String,
    /// workspace 이름
    pub workspace: String,
    /// 큐 상태 정보
    pub queue: QueueContext,
    /// DataSource별 정보 (GitHub: issue/pr, Jira: ticket, ...)
    pub source: SourceContext,
    /// 아이템 계보의 이벤트 히스토리 (append-only)
    pub history: Vec<HistoryEntry>,
    /// worktree 경로
    pub worktree: Option<String>,
}

/// 큐 상태 정보.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueContext {
    /// 현재 phase (예: "Running")
    pub phase: String,
    /// 현재 state (예: "implement")
    pub state: String,
    /// 동일 엔티티의 아이템들을 연결하는 source_id
    pub source_id: String,
}

/// DataSource 출처 정보.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceContext {
    /// DataSource 타입 (예: "github", "jira")
    #[serde(rename = "type")]
    pub source_type: String,
    /// 외부 시스템 URL
    pub url: String,
    /// 기본 브랜치 (git 기반 시스템)
    pub default_branch: Option<String>,
    /// DataSource별 추가 데이터 (GitHub: issue/pr 정보 등)
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 아이템 계보의 히스토리 엔트리 (append-only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// 처리 단계 (예: "analyze", "implement")
    pub state: String,
    /// 결과 상태 (예: "done", "failed", "running")
    pub status: String,
    /// 시도 횟수
    pub attempt: u32,
    /// 요약 또는 에러 메시지
    pub summary: Option<String>,
    /// 에러 메시지
    pub error: Option<String>,
}

#[cfg(test)]
pub mod testing {
    use super::*;

    /// 테스트용 WorkspaceConfig 생성.
    pub fn test_workspace_config() -> WorkspaceConfig {
        let mut sources = HashMap::new();
        sources.insert(
            "github".to_string(),
            SourceConfig {
                url: "https://github.com/org/repo".to_string(),
                scan_interval_secs: 300,
                concurrency: 1,
            },
        );
        WorkspaceConfig {
            name: "test-workspace".to_string(),
            sources,
            concurrency: 2,
        }
    }

    /// 테스트용 ItemContext 생성.
    pub fn test_item_context(work_id: &str) -> ItemContext {
        ItemContext {
            work_id: work_id.to_string(),
            workspace: "test-workspace".to_string(),
            queue: QueueContext {
                phase: "Running".to_string(),
                state: "implement".to_string(),
                source_id: "github:org/repo#42".to_string(),
            },
            source: SourceContext {
                source_type: "github".to_string(),
                url: "https://github.com/org/repo".to_string(),
                default_branch: Some("main".to_string()),
                extra: HashMap::new(),
            },
            history: vec![],
            worktree: Some("/tmp/autodev/test-42".to_string()),
        }
    }
}
