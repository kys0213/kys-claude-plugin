use anyhow::Result;

use crate::core::models::{QueuePhase, QueueType};
use crate::core::repository::QueueRepository;
use crate::infra::db::Database;

/// 큐 아이템을 다음 phase로 전이한다
pub fn queue_advance(db: &Database, work_id: &str) -> Result<String> {
    let before = db
        .queue_get_phase(work_id)?
        .ok_or_else(|| anyhow::anyhow!("queue item not found: {work_id}"))
        .map(|p| p.to_string())?;

    db.queue_advance(work_id)?;

    let after = db
        .queue_get_phase(work_id)?
        .map(|p| p.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(format!("advanced: {work_id} ({before} → {after})"))
}

/// 큐 아이템을 skip 처리한다
pub fn queue_skip(db: &Database, work_id: &str, reason: Option<&str>) -> Result<String> {
    db.queue_skip(work_id, reason)?;

    let mut output = format!("skipped: {work_id}");
    if let Some(r) = reason {
        output.push_str(&format!(" (reason: {r})"));
    }
    Ok(output)
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
            "  [{}] {} — {} ({}){}\n",
            item.queue_type, item.work_id, item.phase, title, skip
        ));
    }
    Ok(output)
}
