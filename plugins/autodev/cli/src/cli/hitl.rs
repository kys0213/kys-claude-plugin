use anyhow::Result;

use crate::core::models::*;
use crate::core::repository::{HitlRepository, SpecRepository};
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
    if let Ok(options) = serde_json::from_str::<Vec<String>>(&event.options) {
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

/// HITL 이벤트에 응답
pub fn respond(
    db: &Database,
    id: &str,
    choice: Option<i32>,
    message: Option<&str>,
) -> Result<String> {
    // Verify event exists
    let event = db
        .hitl_show(id)?
        .ok_or_else(|| anyhow::anyhow!("HITL event not found: {id}"))?;

    if matches!(event.status, HitlStatus::Responded) {
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

    Ok(format!("Responded to HITL event {id}\n"))
}

/// 타임아웃 초과 HITL 만료 처리
pub fn timeout(db: &Database, hours: i64, action: &str) -> Result<String> {
    let expired = db.hitl_expired_list(hours)?;
    if expired.is_empty() {
        return Ok("No expired HITL events found".to_string());
    }

    let mut results = Vec::new();
    for event in &expired {
        db.hitl_set_status(&event.id, HitlStatus::Expired)?;

        match action {
            "pause-spec" => {
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
            _ => {
                results.push(format!("  {} → expired", event.id));
            }
        }
    }

    Ok(format!(
        "Processed {} expired events (action: {}):\n{}",
        expired.len(),
        action,
        results.join("\n")
    ))
}
