use anyhow::Result;

use crate::core::repository::ClawDecisionRepository;
use crate::infra::db::Database;

/// Claw decisions 목록 조회
pub fn list(db: &Database, repo: Option<&str>, limit: usize, json: bool) -> Result<String> {
    let decisions = db.decision_list(repo, limit)?;

    if json {
        return Ok(serde_json::to_string_pretty(&decisions)?);
    }

    if decisions.is_empty() {
        return Ok("No decisions found.\n".to_string());
    }

    let mut output = String::new();
    output.push_str(&format!(
        "{:<10} {:<10} {:<14} {:<14} {:<30} {}\n",
        "ID", "TYPE", "REPO", "TARGET", "REASONING", "TIME"
    ));
    output.push_str(&format!("{}\n", "-".repeat(90)));

    for d in &decisions {
        let short_id = if d.id.len() > 8 {
            format!("{}...", &d.id[..8])
        } else {
            d.id.clone()
        };

        let target = d.target_work_id.as_deref().unwrap_or("-").to_string();
        let target_display = if target.len() > 12 {
            format!("{}...", &target[..12])
        } else {
            target
        };

        let reasoning = if d.reasoning.len() > 28 {
            format!("\"{}...\"", &d.reasoning[..25])
        } else {
            format!("\"{}\"", d.reasoning)
        };

        let time = format_relative_time(&d.created_at);

        // repo_id is the UUID; we display it truncated
        let repo_display = if d.repo_id.len() > 12 {
            format!("{}...", &d.repo_id[..12])
        } else {
            d.repo_id.clone()
        };

        output.push_str(&format!(
            "{:<10} {:<10} {:<14} {:<14} {:<30} {}\n",
            short_id, d.decision_type, repo_display, target_display, reasoning, time
        ));
    }

    Ok(output)
}

/// Claw decision 상세 조회
pub fn show(db: &Database, id: &str, json: bool) -> Result<String> {
    let decision = db
        .decision_show(id)?
        .ok_or_else(|| anyhow::anyhow!("decision not found: {id}"))?;

    if json {
        return Ok(serde_json::to_string_pretty(&decision)?);
    }

    let mut output = String::new();
    output.push_str(&format!("ID:        {}\n", decision.id));
    output.push_str(&format!("Type:      {}\n", decision.decision_type));
    output.push_str(&format!("Repo:      {}\n", decision.repo_id));
    if let Some(ref spec_id) = decision.spec_id {
        output.push_str(&format!("Spec:      {}\n", spec_id));
    }
    if let Some(ref target) = decision.target_work_id {
        output.push_str(&format!("Target:    {}\n", target));
    }
    output.push_str(&format!("Created:   {}\n", decision.created_at));
    output.push_str(&format!("\nReasoning:\n  {}\n", decision.reasoning));

    if let Some(ref ctx) = decision.context_json {
        output.push_str(&format!("\nContext:\n  {}\n", ctx));
    }

    Ok(output)
}

fn format_relative_time(rfc3339: &str) -> String {
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(rfc3339) else {
        return rfc3339.to_string();
    };
    let now = chrono::Utc::now();
    let elapsed = now.signed_duration_since(dt);
    let secs = elapsed.num_seconds();

    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}min ago", secs / 60)
    } else if secs < 86400 {
        format!("{}hr ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}
