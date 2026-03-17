use anyhow::Result;

use crate::core::models::{
    DecisionType, HitlSeverity, NewClawDecision, NewHitlEvent, QueuePhase, QueueType,
};
use crate::core::queue_item::{ItemMetadata, QueueItem};
use crate::core::repository::{ClawDecisionRepository, HitlRepository, QueueRepository};
use crate::infra::db::Database;

/// queue_advance의 결과: 출력 메시지와 생성된 HITL 이벤트.
pub struct QueueAdvanceResult {
    pub output: String,
    pub hitl_event: Option<NewHitlEvent>,
}

/// 큐 아이템을 다음 phase로 전이한다.
/// Claw의 CLI 진입점으로서 claw_decisions에 advance 기록을 남긴다.
/// PR의 review_iteration이 max_iterations 이상이면 HITL 이벤트를 자동 생성한다.
pub fn queue_advance(
    db: &Database,
    work_id: &str,
    reason: Option<&str>,
) -> Result<QueueAdvanceResult> {
    // advance 전에 item 조회 (decision 기록 + HITL 체크 + before phase 취득)
    let item = db
        .queue_get_item(work_id)?
        .ok_or_else(|| anyhow::anyhow!("queue item not found: {work_id}"))?;
    let before = item.phase.to_string();

    db.queue_advance(work_id)?;

    let after = db
        .queue_get_phase(work_id)?
        .map(|p| p.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Decision 기록
    record_decision(db, &item.repo_id, DecisionType::Advance, work_id, reason);

    // H3: PR review iteration 임계값 초과 시 HITL 자동 생성
    let hitl_event = if item.queue_type == QueueType::Pr {
        extract_review_iteration(&item.metadata_json)
            .and_then(|ri| check_review_overflow(db, &item.repo_id, work_id, ri))
    } else {
        None
    };

    Ok(QueueAdvanceResult {
        output: format!("advanced: {work_id} ({before} → {after})"),
        hitl_event,
    })
}

/// 큐 아이템을 skip 처리한다.
/// Claw의 CLI 진입점으로서 claw_decisions에 skip 기록을 남긴다.
pub fn queue_skip(db: &Database, work_id: &str, reason: Option<&str>) -> Result<String> {
    // Decision 기록: skip 전에 item 조회 (skip 후에도 조회 가능하지만 일관성을 위해 선행)
    if let Ok(Some(item)) = db.queue_get_item(work_id) {
        record_decision(db, &item.repo_id, DecisionType::Skip, work_id, reason);
    }

    db.queue_skip(work_id, reason)?;

    let mut output = format!("skipped: {work_id}");
    if let Some(r) = reason {
        output.push_str(&format!(" (reason: {r})"));
    }
    Ok(output)
}

/// claw_decisions에 판단 기록을 남기는 공통 헬퍼.
fn record_decision(
    db: &Database,
    repo_id: &str,
    decision_type: DecisionType,
    work_id: &str,
    reason: Option<&str>,
) {
    let default_reason = match decision_type {
        DecisionType::Advance => "manual advance",
        DecisionType::Skip => "manual skip",
        DecisionType::Hitl => "manual hitl",
        DecisionType::Replan => "manual replan",
    };
    let _ = db.decision_add(&NewClawDecision {
        repo_id: repo_id.to_string(),
        spec_id: None,
        decision_type,
        target_work_id: Some(work_id.to_string()),
        reasoning: reason.unwrap_or(default_reason).to_string(),
        context_json: None,
    });
}

/// metadata_json에서 review_iteration을 추출한다.
/// 기존 QueueItem::metadata_from_json()을 재사용하여 ItemMetadata enum을 파싱한다.
fn extract_review_iteration(metadata_json: &Option<String>) -> Option<u32> {
    let json = metadata_json.as_deref()?;
    let meta = QueueItem::metadata_from_json(json)?;
    match meta {
        ItemMetadata::Pr(pr) => Some(pr.review_iteration),
        ItemMetadata::Issue { .. } => None,
    }
}

/// review_iteration >= max_iterations이면 HITL 이벤트를 생성한다.
///
/// NOTE: max_iterations는 ReviewStage::default().max_iterations와 동일한 기본값(2)을 사용한다.
/// CLI에서는 per-repo config 로드 없이 실행되므로, 여기서는 기본값을 사용한다.
/// Claw cron이 구현되면 config에서 로드된 값을 전달하게 된다.
///
/// 생성된 HITL 이벤트를 반환하여 호출자가 알림을 발송할 수 있게 한다.
fn check_review_overflow(
    db: &Database,
    repo_id: &str,
    work_id: &str,
    review_iteration: u32,
) -> Option<NewHitlEvent> {
    let max_iterations = crate::core::config::models::ReviewStage::default().max_iterations;
    if review_iteration >= max_iterations {
        let event = NewHitlEvent {
            repo_id: repo_id.to_string(),
            spec_id: None,
            work_id: Some(work_id.to_string()),
            severity: HitlSeverity::Medium,
            situation: format!(
                "PR review iteration ({review_iteration}) reached maximum ({max_iterations})"
            ),
            context: format!("work_id: {work_id}"),
            options: vec![
                "Continue reviewing".to_string(),
                "Skip this PR".to_string(),
                "Merge as-is".to_string(),
            ],
        };
        let _ = db.hitl_create(&event);
        Some(event)
    } else {
        None
    }
}

/// DB 기반 큐 아이템 목록 조회
pub fn queue_list_db(
    db: &Database,
    repo: Option<&str>,
    json: bool,
    state: Option<&str>,
    unextracted: bool,
) -> Result<String> {
    let mut items = db.queue_list_items(repo)?;

    // Apply --state filter
    if let Some(phase_filter) = state {
        if let Ok(phase) = phase_filter.parse::<QueuePhase>() {
            items.retain(|item| item.phase == phase);
        }
    }

    // Apply --unextracted filter: done + pr type + no skip_reason
    if unextracted {
        items.retain(|item| {
            item.phase == QueuePhase::Done
                && item.queue_type == QueueType::Pr
                && item.skip_reason.is_none()
        });
    }

    if json {
        return Ok(serde_json::to_string_pretty(&items)?);
    }

    if items.is_empty() {
        return Ok("(no queue items)\n".to_string());
    }

    let mut output = String::new();
    for item in &items {
        let title = item.title.as_deref().unwrap_or("-");
        let skip = item
            .skip_reason
            .as_ref()
            .map(|r| format!(" [reason: {r}]"))
            .unwrap_or_default();
        output.push_str(&format!(
            "  [{}] {} — {} ({}) [{}/#{}]{}\n",
            item.queue_type,
            item.work_id,
            item.phase,
            title,
            item.task_kind,
            item.github_number,
            skip
        ));
    }
    Ok(output)
}
