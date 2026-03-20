//! 5-Level failure escalation for the daemon task loop.
//!
//! failure_count 증가 → EscalationLevel 판정 → 대응 액션 수행.
//! Retry/Comment: 아이템을 pending으로 되돌려 재시도.
//! Hitl/Replan: HITL 이벤트 생성 후 큐에서 제거.
//! Skip: 즉시 제거 (기존 동작).

use crate::core::models::{EscalationLevel, HitlSeverity, NewHitlEvent, QueuePhase};
use crate::core::repository::{HitlRepository, QueueRepository};
use crate::infra::db::Database;

/// 에스컬레이션 처리 결과.
pub enum EscalationOutcome {
    /// 아이템을 pending으로 되돌려 재시도한다. apply(Remove)를 건너뛴다.
    Retry,
    /// 아이템을 제거한다. 기본 동작을 그대로 수행한다.
    Remove,
    /// HITL 이벤트를 생성하고 아이템을 제거한다. 알림 발송이 필요하다.
    /// `String` is the assigned hitl_id from the database.
    RemoveWithHitl(NewHitlEvent, String),
}

/// HITL 이벤트를 DB에 저장하고, 성공 시 RemoveWithHitl, 실패 시 Remove를 반환한다.
fn create_hitl_or_remove(
    db: &Database,
    work_id: &str,
    hitl_event: NewHitlEvent,
) -> EscalationOutcome {
    match db.hitl_create(&hitl_event) {
        Ok(hitl_id) => EscalationOutcome::RemoveWithHitl(hitl_event, hitl_id),
        Err(e) => {
            tracing::warn!("failed to create HITL event for {work_id}: {e}");
            EscalationOutcome::Remove
        }
    }
}

/// 실패한 태스크에 대해 에스컬레이션을 수행한다.
///
/// 1. failure_count를 1 증가시킨다.
/// 2. EscalationLevel을 산출한다.
/// 3. 레벨에 따라 대응한다:
///    - Retry: pending으로 되돌린다.
///    - Comment: GitHub에 코멘트를 남기고 pending으로 되돌린다.
///    - Hitl: HITL 이벤트를 생성하고 제거한다.
///    - Skip: 제거한다 (기존 동작).
///    - Replan: HITL 이벤트(replan)를 생성하고 제거한다.
pub fn escalate(
    db: &Database,
    work_id: &str,
    repo_id: &str,
    failure_msg: &str,
) -> EscalationOutcome {
    // 1. failure_count 증가
    let new_count = match db.queue_increment_failure(work_id) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("failed to increment failure_count for {work_id}: {e}");
            return EscalationOutcome::Remove;
        }
    };

    let level = EscalationLevel::from(new_count);
    tracing::info!("escalation: {work_id} failure_count={new_count} → level={level}");

    match level {
        EscalationLevel::Retry | EscalationLevel::Comment => {
            // pending으로 되돌려 재시도 (Comment 레벨의 코멘트는 태스크 자체가 이미 남김)
            if let Err(e) = db.queue_transit(work_id, QueuePhase::Running, QueuePhase::Pending) {
                tracing::warn!("failed to reset {work_id} to pending: {e}");
                return EscalationOutcome::Remove;
            }
            EscalationOutcome::Retry
        }
        EscalationLevel::Hitl => {
            let hitl_event = NewHitlEvent {
                repo_id: repo_id.to_string(),
                spec_id: None,
                work_id: Some(work_id.to_string()),
                severity: HitlSeverity::High,
                situation: format!("Task failed {new_count} times — human intervention required"),
                context: failure_msg.to_string(),
                options: vec![
                    "Retry this task".to_string(),
                    "Skip and move on".to_string(),
                    "Reassign or replan".to_string(),
                ],
            };
            create_hitl_or_remove(db, work_id, hitl_event)
        }
        EscalationLevel::Skip => EscalationOutcome::Remove,
        EscalationLevel::Replan => {
            let hitl_event = NewHitlEvent {
                repo_id: repo_id.to_string(),
                spec_id: None,
                work_id: Some(work_id.to_string()),
                severity: HitlSeverity::High,
                situation: format!("Task failed {new_count} times — replan recommended"),
                context: format!(
                    "Repeated failures suggest the approach needs revision.\n\
                     Last error: {failure_msg}"
                ),
                options: vec![
                    "Replan: update spec and decompose differently".to_string(),
                    "Force retry with current approach".to_string(),
                    "Abandon this task".to_string(),
                ],
            };
            create_hitl_or_remove(db, work_id, hitl_event)
        }
    }
}
