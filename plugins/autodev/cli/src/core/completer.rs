//! Completer trait — 데이터 소스별 완료 처리 추상화.
//!
//! Claw가 `done`을 판단하면 아이템의 `queue_type`에 따라
//! 다른 완료 처리를 수행한다.
//!
//! - IssueCompleter: 이슈 close + autodev:done 라벨 + 완료 코멘트
//! - PrCompleter: 검증 게이트 + merge + 소스 이슈 close + autodev:done 라벨
//! - SpecCompleter: 스펙 상태 업데이트 + 관련 이슈/PR 정리

use std::fmt;

use async_trait::async_trait;

use super::queue_item::QueueItem;

/// 완료 처리 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteResult {
    /// 완료 처리 성공
    Completed,
    /// 검증 게이트 미충족 — review로 되돌림
    ReviewRequired { reason: String },
    /// 완료 처리 실패 — HITL 에스컬레이션 필요
    EscalationNeeded { reason: String },
}

impl fmt::Display for CompleteResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompleteResult::Completed => write!(f, "completed"),
            CompleteResult::ReviewRequired { reason } => {
                write!(f, "review required: {reason}")
            }
            CompleteResult::EscalationNeeded { reason } => {
                write!(f, "escalation needed: {reason}")
            }
        }
    }
}

/// 데이터 소스별 완료 처리 인터페이스.
///
/// `queue_type`에 따라 적절한 구현체가 선택되며,
/// 각 구현체는 해당 데이터 소스에 맞는 완료 처리를 수행한다.
#[async_trait]
pub trait Completer: Send + Sync {
    /// 큐 아이템의 완료 처리를 수행한다.
    ///
    /// - `Completed`: 성공적으로 완료됨
    /// - `ReviewRequired`: 검증 미충족 — review 단계로 되돌려야 함
    /// - `EscalationNeeded`: 완료 실패 — HITL 에스컬레이션 필요
    async fn complete(&self, item: &QueueItem) -> CompleteResult;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_result_display() {
        assert_eq!(CompleteResult::Completed.to_string(), "completed");
        assert_eq!(
            CompleteResult::ReviewRequired {
                reason: "no review".into()
            }
            .to_string(),
            "review required: no review"
        );
        assert_eq!(
            CompleteResult::EscalationNeeded {
                reason: "merge failed".into()
            }
            .to_string(),
            "escalation needed: merge failed"
        );
    }
}
