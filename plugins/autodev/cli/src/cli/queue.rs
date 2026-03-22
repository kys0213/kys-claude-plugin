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
    /// The assigned hitl_id if a HITL event was created (for notification dispatch).
    pub hitl_id: Option<String>,
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
    let (hitl_event, hitl_id) = if item.queue_type == QueueType::Pr {
        extract_review_iteration(&item.metadata_json)
            .and_then(|ri| check_review_overflow(db, &item.repo_id, work_id, ri))
            .map_or((None, None), |(ev, id)| (Some(ev), id))
    } else {
        (None, None)
    };

    Ok(QueueAdvanceResult {
        output: format!("advanced: {work_id} ({before} → {after})"),
        hitl_event,
        hitl_id,
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
        DecisionType::Noop => "no-op",
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
) -> Option<(NewHitlEvent, Option<String>)> {
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
        let hitl_id = db.hitl_create(&event).ok();
        Some((event, hitl_id))
    } else {
        None
    }
}

/// Completed 아이템의 on_done 처리 결과.
#[derive(Debug)]
pub struct QueueDoneResult {
    pub output: String,
    pub hitl_event: Option<NewHitlEvent>,
    pub hitl_id: Option<String>,
}

/// Completed 아이템을 Done으로 전이한다.
///
/// V5 흐름: evaluate cron이 완료 판정 후 `autodev queue done <work_id>`를 호출한다.
/// on_done script 인프라가 구현되면 여기서 script를 실행하고,
/// 실패 시 Failed로 전이한다. 현재는 즉시 Done으로 전이한다.
pub fn queue_done(db: &Database, work_id: &str, reason: Option<&str>) -> Result<QueueDoneResult> {
    let item = db
        .queue_get_item(work_id)?
        .ok_or_else(|| anyhow::anyhow!("queue item not found: {work_id}"))?;

    if item.phase != QueuePhase::Completed {
        anyhow::bail!(
            "queue done requires Completed phase, but {work_id} is in {} phase",
            item.phase
        );
    }

    // TODO: on_done script 실행 (V5 workspace.yaml 인프라 구현 시)
    // script 성공 → Done, script 실패 → Failed

    db.queue_transit(work_id, QueuePhase::Completed, QueuePhase::Done)?;

    // Decision 기록
    record_decision(
        db,
        &item.repo_id,
        DecisionType::Advance,
        work_id,
        Some(reason.unwrap_or("evaluate: done")),
    );

    Ok(QueueDoneResult {
        output: format!("done: {work_id} (completed → done)"),
        hitl_event: None,
        hitl_id: None,
    })
}

/// Completed 아이템을 Hitl로 전이하고 HITL 이벤트를 생성한다.
///
/// V5 흐름: evaluate cron이 사람 판단 필요로 분류 후
/// `autodev queue hitl <work_id> --reason "..."` 를 호출한다.
pub fn queue_hitl(db: &Database, work_id: &str, reason: &str) -> Result<QueueDoneResult> {
    let item = db
        .queue_get_item(work_id)?
        .ok_or_else(|| anyhow::anyhow!("queue item not found: {work_id}"))?;

    if item.phase != QueuePhase::Completed {
        anyhow::bail!(
            "queue hitl requires Completed phase, but {work_id} is in {} phase",
            item.phase
        );
    }

    db.queue_transit(work_id, QueuePhase::Completed, QueuePhase::Hitl)?;

    // HITL 이벤트 생성
    let event = NewHitlEvent {
        repo_id: item.repo_id.clone(),
        spec_id: None,
        work_id: Some(work_id.to_string()),
        severity: HitlSeverity::Medium,
        situation: reason.to_string(),
        context: format!("work_id: {work_id}"),
        options: vec![
            "Mark as done".to_string(),
            "Retry".to_string(),
            "Skip".to_string(),
        ],
    };
    let hitl_id = db.hitl_create(&event).ok();

    // Decision 기록
    record_decision(db, &item.repo_id, DecisionType::Hitl, work_id, Some(reason));

    Ok(QueueDoneResult {
        output: format!("hitl: {work_id} (completed → hitl, reason: {reason})"),
        hitl_event: Some(event),
        hitl_id,
    })
}

/// Failed 아이템의 on_done script를 재실행한다.
///
/// V5 흐름: on_done script 실패로 Failed 상태가 된 아이템을
/// 재시도한다. script 성공 시 Done, 실패 시 Failed 유지.
/// on_done script 인프라가 구현되면 실제 script를 실행한다.
/// 현재는 즉시 Done으로 전이한다.
pub fn queue_retry_script(db: &Database, work_id: &str) -> Result<String> {
    let item = db
        .queue_get_item(work_id)?
        .ok_or_else(|| anyhow::anyhow!("queue item not found: {work_id}"))?;

    if item.phase != QueuePhase::Failed {
        anyhow::bail!(
            "queue retry-script requires Failed phase, but {work_id} is in {} phase",
            item.phase
        );
    }

    // TODO: on_done script 재실행 (V5 workspace.yaml 인프라 구현 시)
    // script 성공 → Done, script 실패 → Failed 유지

    db.queue_transit(work_id, QueuePhase::Failed, QueuePhase::Done)?;

    // Decision 기록
    record_decision(
        db,
        &item.repo_id,
        DecisionType::Advance,
        work_id,
        Some("retry-script: on_done re-executed"),
    );

    Ok(format!("retry-script: {work_id} (failed → done)"))
}

/// 단일 큐 아이템 상세 조회
pub fn queue_show(db: &Database, work_id: &str, json: bool) -> Result<String> {
    let item = db
        .queue_get_item(work_id)?
        .ok_or_else(|| anyhow::anyhow!("queue item not found: {work_id}"))?;

    if json {
        return Ok(serde_json::to_string_pretty(&item)?);
    }

    let title = item.title.as_deref().unwrap_or("-");
    let skip = item
        .skip_reason
        .as_ref()
        .map(|r| format!("\nSkip reason: {r}"))
        .unwrap_or_default();
    let metadata = item.metadata_json.as_deref().unwrap_or("-");

    Ok(format!(
        "Work ID:    {}\nRepo ID:    {}\nType:       {}\nPhase:      {}\nTitle:      {}\nTask kind:  {}\nGH number:  #{}\nFailures:   {}\nEscalation: {}\nCreated:    {}\nUpdated:    {}\nMetadata:   {}{}\n",
        item.work_id,
        item.repo_id,
        item.queue_type,
        item.phase,
        title,
        item.task_kind,
        item.github_number,
        item.failure_count,
        item.escalation_level,
        item.created_at,
        item.updated_at,
        metadata,
        skip,
    ))
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

/// 아이템 컨텍스트 조회 (script용 정보 조회)
///
/// v5 spec에서 script가 아이템 정보를 조회하는 유일한 방법.
/// `autodev context $WORK_ID --json` 형태로 사용.
pub fn queue_context(db: &Database, work_id: &str, json: bool) -> Result<String> {
    use crate::core::repository::WorkspaceRepository;

    let item = db
        .queue_get_item(work_id)?
        .ok_or_else(|| anyhow::anyhow!("queue item not found: {work_id}"))?;

    // Resolve repo info
    let repos = db.workspace_list()?;
    let repo = repos.iter().find(|r| {
        // repo_id is the internal ID; match by checking enabled repos
        let enabled = db.workspace_find_enabled().unwrap_or_default();
        enabled
            .iter()
            .any(|e| e.id == item.repo_id && e.name == r.name)
    });

    let repo_url = repo.map(|r| r.url.as_str()).unwrap_or("");
    let repo_name = repo.map(|r| r.name.as_str()).unwrap_or("");

    if json {
        let mut context = serde_json::json!({
            "work_id": item.work_id,
            "queue": {
                "phase": item.phase.as_str(),
                "type": item.queue_type.to_string(),
                "task_kind": item.task_kind.to_string(),
                "failure_count": item.failure_count,
                "escalation_level": item.escalation_level,
            },
            "source": {
                "url": repo_url,
                "repo_name": repo_name,
            },
            "issue": {
                "number": item.github_number,
                "title": item.title,
            },
        });
        if let Some(ref metadata) = item.metadata_json {
            if let Ok(meta_value) = serde_json::from_str::<serde_json::Value>(metadata) {
                context["metadata"] = meta_value;
            }
        }
        return Ok(serde_json::to_string_pretty(&context)?);
    }

    let title = item.title.as_deref().unwrap_or("-");
    Ok(format!(
        "Work ID:    {}\nRepo:       {} ({})\nType:       {}\nPhase:      {}\nTask kind:  {}\nGH number:  #{}\nTitle:      {}\nFailures:   {}\nEscalation: {}\n",
        item.work_id,
        repo_name,
        repo_url,
        item.queue_type,
        item.phase,
        item.task_kind,
        item.github_number,
        title,
        item.failure_count,
        item.escalation_level,
    ))
}
