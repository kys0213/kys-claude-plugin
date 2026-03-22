use anyhow::Result;

use crate::core::models::*;
use crate::core::repository::{
    ClawDecisionRepository, HitlRepository, QueueRepository, SpecRepository,
};
use crate::infra::db::Database;

/// HITL 이벤트 목록 조회
pub fn list(db: &Database, repo: Option<&str>, json: bool) -> Result<String> {
    let events = db.hitl_list(repo)?;

    if json {
        return Ok(serde_json::to_string_pretty(&events)?);
    }

    if events.is_empty() {
        return Ok("No HITL events found.\n".to_string());
    }

    let mut output = String::new();
    output.push_str(&format!(
        "{:<38} {:<20} {:<8} {:<10} {}\n",
        "ID", "REPO", "SEVERITY", "STATUS", "SITUATION"
    ));
    output.push_str(&format!("{}\n", "-".repeat(100)));

    for event in &events {
        let situation = if event.situation.len() > 40 {
            format!("{}...", &event.situation[..37])
        } else {
            event.situation.clone()
        };
        output.push_str(&format!(
            "{:<38} {:<20} {:<8} {:<10} {}\n",
            event.id, event.repo_id, event.severity, event.status, situation
        ));
    }

    Ok(output)
}

/// HITL 이벤트 상세 조회
pub fn show(db: &Database, id: &str, json: bool) -> Result<String> {
    let event = db
        .hitl_show(id)?
        .ok_or_else(|| anyhow::anyhow!("HITL event not found: {id}"))?;

    let responses = db.hitl_responses(id)?;

    if json {
        let value = serde_json::json!({
            "event": event,
            "responses": responses,
        });
        return Ok(serde_json::to_string_pretty(&value)?);
    }

    let mut output = String::new();
    output.push_str(&format!("ID:        {}\n", event.id));
    output.push_str(&format!("Repo:      {}\n", event.repo_id));
    if let Some(ref spec_id) = event.spec_id {
        output.push_str(&format!("Spec:      {}\n", spec_id));
    }
    if let Some(ref work_id) = event.work_id {
        output.push_str(&format!("Work:      {}\n", work_id));
    }
    output.push_str(&format!("Severity:  {}\n", event.severity));
    output.push_str(&format!("Status:    {}\n", event.status));
    output.push_str(&format!("Created:   {}\n", event.created_at));
    output.push_str(&format!("\nSituation:\n  {}\n", event.situation));

    if !event.context.is_empty() {
        output.push_str(&format!("\nContext:\n  {}\n", event.context));
    }

    // Parse and display options
    {
        let options = event.parsed_options();
        if !options.is_empty() {
            output.push_str("\nOptions:\n");
            for (i, opt) in options.iter().enumerate() {
                output.push_str(&format!("  [{}] {}\n", i + 1, opt));
            }
        }
    }

    if !responses.is_empty() {
        output.push_str("\nResponses:\n");
        for resp in &responses {
            output.push_str(&format!(
                "  --- (source: {}, at: {})\n",
                resp.source, resp.created_at
            ));
            if let Some(choice) = resp.choice {
                output.push_str(&format!("  Choice: {}\n", choice));
            }
            if let Some(ref msg) = resp.message {
                output.push_str(&format!("  Message: {}\n", msg));
            }
        }
    }

    Ok(output)
}

/// HITL respond 결과: 출력 메시지 + 라우팅 결과.
#[derive(Debug)]
pub struct HitlRespondResult {
    pub output: String,
    /// 라우팅에 의해 수행된 액션 (옵션 텍스트에서 추론, 없으면 None).
    pub action: Option<HitlRespondAction>,
    /// retry 시 새로 생성된 queue item의 work_id.
    pub retry_work_id: Option<String>,
}

/// HITL 이벤트에 응답하고 선택된 옵션에 따라 라우팅한다.
///
/// 라우팅 규칙 (옵션 텍스트 기반):
///   "done"   → queue item을 Done 처리
///   "retry"  → queue item을 Pending으로 되돌려 재시도
///   "skip"   → queue item을 Skipped 처리
///   "replan" → 스펙 수정 제안 기록 (HITL 유지)
pub fn respond(
    db: &Database,
    id: &str,
    choice: Option<i32>,
    message: Option<&str>,
) -> Result<HitlRespondResult> {
    // Verify event exists
    let event = db
        .hitl_show(id)?
        .ok_or_else(|| anyhow::anyhow!("HITL event not found: {id}"))?;

    if matches!(event.status, HitlStatus::Responded | HitlStatus::Applied) {
        anyhow::bail!("HITL event already responded: {id}");
    }

    if choice.is_none() && message.is_none() {
        anyhow::bail!("must provide --choice or --message");
    }

    let response = NewHitlResponse {
        event_id: id.to_string(),
        choice,
        message: message.map(|s| s.to_string()),
        source: "cli".to_string(),
    };

    db.hitl_respond(&response)?;

    // Determine routing action from the chosen option text
    let action = choice.and_then(|c| {
        let options = event.parsed_options();
        let idx = (c - 1) as usize;
        options
            .get(idx)
            .and_then(|text| HitlRespondAction::from_option_text(text))
    });

    let mut output = format!("Responded to HITL event {id}\n");
    let mut retry_work_id = None;

    // Execute routing based on action
    if let Some(act) = action {
        let route_result = route_respond(db, &event, act, message)?;
        output.push_str(&route_result.message);
        retry_work_id = route_result.retry_work_id;
    }

    Ok(HitlRespondResult {
        output,
        action,
        retry_work_id,
    })
}

/// 라우팅 실행 결과.
struct RouteResult {
    message: String,
    retry_work_id: Option<String>,
}

/// HITL 응답에 따른 라우팅 실행.
fn route_respond(
    db: &Database,
    event: &HitlEvent,
    action: HitlRespondAction,
    message: Option<&str>,
) -> Result<RouteResult> {
    let work_id = event.work_id.as_deref();

    match action {
        HitlRespondAction::Done => route_done(db, event, work_id),
        HitlRespondAction::Retry => route_retry(db, work_id),
        HitlRespondAction::Skip => route_skip(db, work_id),
        HitlRespondAction::Replan => route_replan(db, event, message),
    }
}

/// Done: queue item을 Done 처리.
fn route_done(db: &Database, event: &HitlEvent, work_id: Option<&str>) -> Result<RouteResult> {
    if let Some(wid) = work_id {
        db.queue_remove(wid)?;
        // Record decision
        record_hitl_decision(db, &event.repo_id, DecisionType::Advance, wid, "HITL done");
        Ok(RouteResult {
            message: format!("  → routed: done (queue item {wid} → Done)\n"),
            retry_work_id: None,
        })
    } else {
        Ok(RouteResult {
            message: "  → routed: done (no linked queue item)\n".to_string(),
            retry_work_id: None,
        })
    }
}

/// Retry: queue item을 Pending으로 되돌린다.
fn route_retry(db: &Database, work_id: Option<&str>) -> Result<RouteResult> {
    if let Some(wid) = work_id {
        // Try to transition from any active phase to Pending
        let current_phase = db.queue_get_phase(wid)?;
        if let Some(phase) = current_phase {
            if phase != QueuePhase::Done && phase != QueuePhase::Skipped {
                let _ = db.queue_transit(wid, phase, QueuePhase::Pending)?;
            }
        }
        Ok(RouteResult {
            message: format!("  → routed: retry (queue item {wid} → Pending)\n"),
            retry_work_id: Some(wid.to_string()),
        })
    } else {
        Ok(RouteResult {
            message: "  → routed: retry (no linked queue item to retry)\n".to_string(),
            retry_work_id: None,
        })
    }
}

/// Skip: queue item을 Skipped 처리.
fn route_skip(db: &Database, work_id: Option<&str>) -> Result<RouteResult> {
    if let Some(wid) = work_id {
        db.queue_skip(wid, Some("HITL response: skip"))?;
        Ok(RouteResult {
            message: format!("  → routed: skip (queue item {wid} → Skipped)\n"),
            retry_work_id: None,
        })
    } else {
        Ok(RouteResult {
            message: "  → routed: skip (no linked queue item)\n".to_string(),
            retry_work_id: None,
        })
    }
}

/// Replan: 스펙 수정 제안을 decision으로 기록한다.
fn route_replan(db: &Database, event: &HitlEvent, message: Option<&str>) -> Result<RouteResult> {
    let reasoning = message.unwrap_or("HITL replan: spec revision requested");
    let target = event.work_id.as_deref();

    record_hitl_decision(
        db,
        &event.repo_id,
        DecisionType::Replan,
        target.unwrap_or("unknown"),
        reasoning,
    );

    // If spec is linked, note the replan in output
    let spec_note = if let Some(ref spec_id) = event.spec_id {
        format!(" (spec: {spec_id})")
    } else {
        String::new()
    };

    Ok(RouteResult {
        message: format!("  → routed: replan{spec_note} — revision suggestion recorded\n"),
        retry_work_id: None,
    })
}

/// HITL 응답으로 인한 decision 기록 헬퍼.
fn record_hitl_decision(
    db: &Database,
    repo_id: &str,
    decision_type: DecisionType,
    work_id: &str,
    reasoning: &str,
) {
    let _ = db.decision_add(&NewClawDecision {
        repo_id: repo_id.to_string(),
        spec_id: None,
        decision_type,
        target_work_id: Some(work_id.to_string()),
        reasoning: reasoning.to_string(),
        context_json: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::repository::*;

    fn setup_test_db() -> (tempfile::TempDir, Database) {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = Database::open(&db_path).unwrap();
        db.initialize().unwrap();
        (tmp, db)
    }

    fn create_repo(db: &Database) -> String {
        db.workspace_add("https://github.com/org/repo", "org/repo")
            .unwrap()
    }

    fn create_hitl_event(db: &Database, repo_id: &str, work_id: Option<&str>) -> String {
        db.hitl_create(&NewHitlEvent {
            repo_id: repo_id.to_string(),
            spec_id: None,
            work_id: work_id.map(|s| s.to_string()),
            severity: HitlSeverity::High,
            situation: "Test situation".to_string(),
            context: "Test context".to_string(),
            options: vec![
                "Done — approve this".to_string(),
                "Retry this task".to_string(),
                "Skip and move on".to_string(),
                "Replan — revise approach".to_string(),
            ],
        })
        .unwrap()
    }

    fn create_queue_item(db: &Database, repo_id: &str, work_id: &str) {
        use crate::core::phase::TaskKind;
        db.queue_upsert(&QueueItemRow {
            work_id: work_id.to_string(),
            source_id: String::new(),
            repo_id: repo_id.to_string(),
            queue_type: QueueType::Issue,
            phase: QueuePhase::Running,
            title: Some("Test item".to_string()),
            skip_reason: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            task_kind: TaskKind::Implement,
            github_number: 42,
            metadata_json: None,
            failure_count: 0,
            escalation_level: 0,
        })
        .unwrap();
    }

    // ─── HitlRespondAction::from_option_text tests ───

    #[test]
    fn action_from_done_text() {
        assert_eq!(
            HitlRespondAction::from_option_text("Done — approve this"),
            Some(HitlRespondAction::Done)
        );
        assert_eq!(
            HitlRespondAction::from_option_text("Complete the task"),
            Some(HitlRespondAction::Done)
        );
        assert_eq!(
            HitlRespondAction::from_option_text("Approve and merge"),
            Some(HitlRespondAction::Done)
        );
    }

    #[test]
    fn action_from_retry_text() {
        assert_eq!(
            HitlRespondAction::from_option_text("Retry this task"),
            Some(HitlRespondAction::Retry)
        );
        assert_eq!(
            HitlRespondAction::from_option_text("Force retry with current approach"),
            Some(HitlRespondAction::Retry)
        );
    }

    #[test]
    fn action_from_skip_text() {
        assert_eq!(
            HitlRespondAction::from_option_text("Skip and move on"),
            Some(HitlRespondAction::Skip)
        );
        assert_eq!(
            HitlRespondAction::from_option_text("Abandon this task"),
            Some(HitlRespondAction::Skip)
        );
    }

    #[test]
    fn action_from_replan_text() {
        assert_eq!(
            HitlRespondAction::from_option_text("Replan — update spec"),
            Some(HitlRespondAction::Replan)
        );
        assert_eq!(
            HitlRespondAction::from_option_text("Revise the approach"),
            Some(HitlRespondAction::Replan)
        );
    }

    #[test]
    fn action_from_unknown_text() {
        assert_eq!(HitlRespondAction::from_option_text("Something else"), None);
    }

    // ─── respond routing integration tests ───

    #[test]
    fn respond_routes_done() {
        let (_tmp, db) = setup_test_db();
        let repo_id = create_repo(&db);
        let work_id = "issue-42";
        create_queue_item(&db, &repo_id, work_id);
        let hitl_id = create_hitl_event(&db, &repo_id, Some(work_id));

        let result = respond(&db, &hitl_id, Some(1), None).unwrap();
        assert_eq!(result.action, Some(HitlRespondAction::Done));
        assert!(result.output.contains("done"));

        // Queue item should be Done
        let phase = db.queue_get_phase(work_id).unwrap();
        assert_eq!(phase, Some(QueuePhase::Done));
    }

    #[test]
    fn respond_routes_retry() {
        let (_tmp, db) = setup_test_db();
        let repo_id = create_repo(&db);
        let work_id = "issue-43";
        create_queue_item(&db, &repo_id, work_id);
        let hitl_id = create_hitl_event(&db, &repo_id, Some(work_id));

        let result = respond(&db, &hitl_id, Some(2), None).unwrap();
        assert_eq!(result.action, Some(HitlRespondAction::Retry));
        assert!(result.output.contains("retry"));
        assert_eq!(result.retry_work_id, Some(work_id.to_string()));

        // Queue item should be Pending
        let phase = db.queue_get_phase(work_id).unwrap();
        assert_eq!(phase, Some(QueuePhase::Pending));
    }

    #[test]
    fn respond_routes_skip() {
        let (_tmp, db) = setup_test_db();
        let repo_id = create_repo(&db);
        let work_id = "issue-44";
        create_queue_item(&db, &repo_id, work_id);
        let hitl_id = create_hitl_event(&db, &repo_id, Some(work_id));

        let result = respond(&db, &hitl_id, Some(3), None).unwrap();
        assert_eq!(result.action, Some(HitlRespondAction::Skip));
        assert!(result.output.contains("skip"));

        // Queue item should be Skipped
        let phase = db.queue_get_phase(work_id).unwrap();
        assert_eq!(phase, Some(QueuePhase::Skipped));
    }

    #[test]
    fn respond_routes_replan() {
        let (_tmp, db) = setup_test_db();
        let repo_id = create_repo(&db);
        let work_id = "issue-45";
        create_queue_item(&db, &repo_id, work_id);
        let hitl_id = create_hitl_event(&db, &repo_id, Some(work_id));

        let result = respond(&db, &hitl_id, Some(4), Some("Need new approach")).unwrap();
        assert_eq!(result.action, Some(HitlRespondAction::Replan));
        assert!(result.output.contains("replan"));

        // A decision should be recorded
        let decisions = db.decision_list(Some("org/repo"), 10).unwrap();
        assert!(!decisions.is_empty());
        let replan_decision = decisions
            .iter()
            .find(|d| d.decision_type == DecisionType::Replan);
        assert!(replan_decision.is_some());
    }

    #[test]
    fn respond_no_routing_for_message_only() {
        let (_tmp, db) = setup_test_db();
        let repo_id = create_repo(&db);
        let hitl_id = create_hitl_event(&db, &repo_id, None);

        let result = respond(&db, &hitl_id, None, Some("Just a comment")).unwrap();
        // No choice → no routing
        assert_eq!(result.action, None);
    }

    #[test]
    fn respond_already_responded_fails() {
        let (_tmp, db) = setup_test_db();
        let repo_id = create_repo(&db);
        let hitl_id = create_hitl_event(&db, &repo_id, None);

        respond(&db, &hitl_id, Some(1), None).unwrap();

        // Second respond should fail
        let result = respond(&db, &hitl_id, Some(2), None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already responded"));
    }
}

/// Result of timeout processing, containing both display output and expired events.
pub struct TimeoutResult {
    pub output: String,
    pub expired_events: Vec<HitlEvent>,
}

/// 타임아웃 초과 HITL 만료 처리
pub fn timeout(db: &Database, hours: i64, action: TimeoutAction) -> Result<TimeoutResult> {
    let expired = db.hitl_expired_list(hours)?;
    if expired.is_empty() {
        return Ok(TimeoutResult {
            output: "No expired HITL events found".to_string(),
            expired_events: Vec::new(),
        });
    }

    let mut results = Vec::new();
    for event in &expired {
        // Remind는 알림만 재발송하므로 상태를 변경하지 않는다.
        if action != TimeoutAction::Remind {
            db.hitl_set_status(&event.id, HitlStatus::Expired)?;
        }

        match action {
            TimeoutAction::PauseSpec => {
                if let Some(ref spec_id) = event.spec_id {
                    db.spec_set_status(spec_id, SpecStatus::Paused)?;
                    results.push(format!(
                        "  {} → expired (spec {} paused)",
                        event.id, spec_id
                    ));
                } else {
                    results.push(format!("  {} → expired (no spec linked)", event.id));
                }
            }
            TimeoutAction::Expire => {
                results.push(format!("  {} → expired", event.id));
            }
            TimeoutAction::Remind => {
                results.push(format!("  {} → remind sent", event.id));
            }
            TimeoutAction::Skip => {
                if let Some(ref work_id) = event.work_id {
                    db.queue_skip(work_id, Some("HITL timeout — auto-skipped"))?;
                    results.push(format!("  {} → skipped ({})", event.id, work_id));
                } else {
                    results.push(format!("  {} → expired (no work_id to skip)", event.id));
                }
            }
        }
    }

    Ok(TimeoutResult {
        output: format!(
            "Processed {} events (action: {}):\n{}",
            expired.len(),
            action,
            results.join("\n")
        ),
        expired_events: expired,
    })
}
