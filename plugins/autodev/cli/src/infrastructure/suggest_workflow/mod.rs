pub mod mock;
pub mod real;

use anyhow::Result;
use async_trait::async_trait;

pub use real::RealSuggestWorkflow;

use crate::knowledge::models::{RepetitionEntry, SessionEntry, ToolFrequencyEntry};

/// suggest-workflow CLI 추상화
///
/// suggest-workflow query --perspective ... 호출을 래핑하여
/// knowledge extraction에서 세션 데이터를 조회할 수 있게 한다.
#[async_trait]
pub trait SuggestWorkflow: Send + Sync {
    /// tool-frequency perspective 조회
    ///
    /// `session_filter`가 Some이면 `--session-filter` 옵션 추가.
    async fn query_tool_frequency(
        &self,
        session_filter: Option<&str>,
    ) -> Result<Vec<ToolFrequencyEntry>>;

    /// filtered-sessions perspective 조회
    ///
    /// `prompt_pattern`으로 세션 필터링 (예: "[autodev]").
    async fn query_filtered_sessions(
        &self,
        prompt_pattern: &str,
        since: Option<&str>,
        top: Option<u32>,
    ) -> Result<Vec<SessionEntry>>;

    /// repetition perspective 조회 (이상치 탐지)
    ///
    /// `session_filter`가 Some이면 `--session-filter` 옵션 추가.
    async fn query_repetition(
        &self,
        session_filter: Option<&str>,
    ) -> Result<Vec<RepetitionEntry>>;
}
