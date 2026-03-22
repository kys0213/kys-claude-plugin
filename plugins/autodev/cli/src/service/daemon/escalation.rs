//! 5-Level failure escalation for the daemon task loop.
//!
//! failure_count 증가 → yaml 정책 기반 EscalationLevel 판정 → on_fail 조건부 실행 → 대응 액션 수행.
//!
//! v5 스펙에 따라 workspace yaml의 `escalation` 섹션에서 정책을 읽어온다:
//! - `levels`: failure_count → action 매핑 (retry, retry_with_comment, hitl, skip, replan)
//! - `on_fail`: 실패 시 실행할 script 목록 (retry에서는 실행 안 함)

use crate::core::config::models::EscalationConfig;
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

/// on_fail script를 순차 실행한다. 하나라도 실패하면 경고 로그를 남기지만 계속 진행한다.
fn run_on_fail_scripts(scripts: &[String], work_id: &str) {
    for script in scripts {
        tracing::info!("on_fail: running script for {work_id}");
        let result = std::process::Command::new("sh")
            .arg("-c")
            .arg(script)
            .env("WORK_ID", work_id)
            .output();
        match result {
            Ok(output) if !output.status.success() => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::warn!(
                    "on_fail script failed for {work_id} (exit {}): {stderr}",
                    output.status.code().unwrap_or(-1)
                );
            }
            Err(e) => {
                tracing::warn!("on_fail script execution error for {work_id}: {e}");
            }
            Ok(_) => {
                tracing::debug!("on_fail script succeeded for {work_id}");
            }
        }
    }
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
/// 2. yaml 정책에서 EscalationAction을 산출한다.
/// 3. on_fail 실행 조건을 확인한다 (retry만 skip).
/// 4. 레벨에 따라 대응한다:
///    - Retry: pending으로 되돌린다 (on_fail 실행 안 함).
///    - RetryWithComment: on_fail 실행 + pending으로 되돌린다.
///    - Hitl: on_fail 실행 + HITL 이벤트를 생성하고 제거한다.
///    - Skip: on_fail 실행 + 제거한다.
///    - Replan: on_fail 실행 + HITL 이벤트(replan)를 생성하고 제거한다.
pub fn escalate(
    db: &Database,
    work_id: &str,
    repo_id: &str,
    failure_msg: &str,
) -> EscalationOutcome {
    let cfg = EscalationConfig::default();
    escalate_with_config(db, work_id, repo_id, failure_msg, &cfg)
}

/// yaml 정책을 받아 에스컬레이션을 수행한다.
///
/// `escalate()`의 config-aware 버전. daemon이 workspace yaml에서 로드한
/// `EscalationConfig`를 직접 전달할 수 있다.
pub fn escalate_with_config(
    db: &Database,
    work_id: &str,
    repo_id: &str,
    failure_msg: &str,
    config: &EscalationConfig,
) -> EscalationOutcome {
    // 1. failure_count 증가
    let new_count = match db.queue_increment_failure(work_id) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("failed to increment failure_count for {work_id}: {e}");
            return EscalationOutcome::Remove;
        }
    };

    let action = config.action_for(new_count as u32);
    let level: EscalationLevel = action.into();
    tracing::info!(
        "escalation: {work_id} failure_count={new_count} → level={level} (action={action})"
    );

    // 2. on_fail 조건부 실행 (retry만 skip)
    if config.should_run_on_fail(action) && !config.on_fail.is_empty() {
        run_on_fail_scripts(&config.on_fail, work_id);
    }

    // 3. 레벨별 대응
    match level {
        EscalationLevel::Retry => {
            // 조용한 재시도: on_fail 실행 안 함, worktree 보존
            if let Err(e) = db.queue_transit(work_id, QueuePhase::Running, QueuePhase::Pending) {
                tracing::warn!("failed to reset {work_id} to pending: {e}");
                return EscalationOutcome::Remove;
            }
            EscalationOutcome::Retry
        }
        EscalationLevel::RetryWithComment => {
            // on_fail 실행 완료 + 재시도
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
                    "Done — approve and complete this task".to_string(),
                    "Retry this task".to_string(),
                    "Skip and move on".to_string(),
                    "Replan — update spec and try differently".to_string(),
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
                    "Replan — update spec and decompose differently".to_string(),
                    "Retry with current approach".to_string(),
                    "Skip — abandon this task".to_string(),
                ],
            };
            create_hitl_or_remove(db, work_id, hitl_event)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::models::EscalationAction;

    #[test]
    fn default_config_action_mapping() {
        let cfg = EscalationConfig::default();
        assert_eq!(cfg.action_for(1), EscalationAction::Retry);
        assert_eq!(cfg.action_for(2), EscalationAction::RetryWithComment);
        assert_eq!(cfg.action_for(3), EscalationAction::Hitl);
        assert_eq!(cfg.action_for(4), EscalationAction::Skip);
        assert_eq!(cfg.action_for(5), EscalationAction::Replan);
        // Beyond max level → last defined action
        assert_eq!(cfg.action_for(10), EscalationAction::Replan);
    }

    #[test]
    fn should_run_on_fail_only_skips_retry() {
        let cfg = EscalationConfig::default();
        assert!(!cfg.should_run_on_fail(EscalationAction::Retry));
        assert!(cfg.should_run_on_fail(EscalationAction::RetryWithComment));
        assert!(cfg.should_run_on_fail(EscalationAction::Hitl));
        assert!(cfg.should_run_on_fail(EscalationAction::Skip));
        assert!(cfg.should_run_on_fail(EscalationAction::Replan));
    }

    #[test]
    fn custom_config_overrides_levels() {
        let mut levels = std::collections::BTreeMap::new();
        levels.insert(1, EscalationAction::Retry);
        levels.insert(2, EscalationAction::Retry);
        levels.insert(3, EscalationAction::Skip);
        let cfg = EscalationConfig {
            levels,
            on_fail: vec!["echo fail".to_string()],
        };
        assert_eq!(cfg.action_for(1), EscalationAction::Retry);
        assert_eq!(cfg.action_for(2), EscalationAction::Retry);
        assert_eq!(cfg.action_for(3), EscalationAction::Skip);
        // Beyond max → last defined (Skip)
        assert_eq!(cfg.action_for(99), EscalationAction::Skip);
    }

    #[test]
    fn escalation_level_from_action() {
        assert_eq!(
            EscalationLevel::from(EscalationAction::Retry),
            EscalationLevel::Retry
        );
        assert_eq!(
            EscalationLevel::from(EscalationAction::RetryWithComment),
            EscalationLevel::RetryWithComment
        );
        assert_eq!(
            EscalationLevel::from(EscalationAction::Hitl),
            EscalationLevel::Hitl
        );
        assert_eq!(
            EscalationLevel::from(EscalationAction::Skip),
            EscalationLevel::Skip
        );
        assert_eq!(
            EscalationLevel::from(EscalationAction::Replan),
            EscalationLevel::Replan
        );
    }
}
