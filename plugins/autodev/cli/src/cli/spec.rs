use anyhow::Result;

use crate::cli::resolve_repo_id;
use crate::core::models::*;
use crate::core::repository::*;
use crate::infra::db::Database;

/// Add a new spec
pub fn spec_add(
    db: &Database,
    title: &str,
    body: &str,
    repo_name: &str,
    file: Option<&str>,
    test_commands: Option<&str>,
    acceptance_criteria: Option<&str>,
) -> Result<String> {
    let repo_id = resolve_repo_id(db, repo_name)?;

    let new_spec = NewSpec {
        repo_id,
        title: title.to_string(),
        body: body.to_string(),
        source_path: file.map(|s| s.to_string()),
        test_commands: test_commands.map(|s| s.to_string()),
        acceptance_criteria: acceptance_criteria.map(|s| s.to_string()),
    };

    let id = db.spec_add(&new_spec)?;

    let missing = validate_spec_sections(body);
    if !missing.is_empty() {
        let mut result = format!(
            "⚠ Missing sections: {}. Recommended: add these as ## headers.\n",
            missing.join(", ")
        );
        result.push_str(&id);
        return Ok(result);
    }

    Ok(id)
}

/// List specs
pub fn spec_list(db: &Database, repo: Option<&str>, json: bool) -> Result<String> {
    let specs = db.spec_list(repo)?;

    if json {
        return Ok(serde_json::to_string_pretty(&specs)?);
    }

    let mut output = String::new();
    if specs.is_empty() {
        output.push_str("No specs found.\n");
    } else {
        let issue_counts = db.spec_issue_counts()?;
        for s in &specs {
            let issue_count = issue_counts.get(&s.id).copied().unwrap_or(0);
            output.push_str(&format!(
                "  [{}] {} — {} (issues: {})\n",
                s.status, s.id, s.title, issue_count
            ));
        }
    }
    Ok(output)
}

/// Show a single spec
pub fn spec_show(db: &Database, id: &str, json: bool) -> Result<String> {
    let spec = db
        .spec_show(id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {id}"))?;

    if json {
        return Ok(serde_json::to_string_pretty(&spec)?);
    }

    let issues = db.spec_issues(id)?;

    let mut output = String::new();
    output.push_str(&format!("ID:     {}\n", spec.id));
    output.push_str(&format!("Title:  {}\n", spec.title));
    output.push_str(&format!("Status: {}\n", spec.status));
    output.push_str(&format!("Repo:   {}\n", spec.repo_id));

    if let Some(ref path) = spec.source_path {
        output.push_str(&format!("File:   {path}\n"));
    }

    output.push_str(&format!("\nBody:\n{}\n", spec.body));

    if let Some(ref tc) = spec.test_commands {
        output.push_str(&format!("\nTest commands: {tc}\n"));
    }
    if let Some(ref ac) = spec.acceptance_criteria {
        output.push_str(&format!("\nAcceptance criteria:\n{ac}\n"));
    }

    if !issues.is_empty() {
        output.push_str("\nLinked issues:\n");
        for issue in &issues {
            output.push_str(&format!("  #{}\n", issue.issue_number));
        }
    }

    output.push_str(&format!("\nCreated: {}\n", spec.created_at));
    output.push_str(&format!("Updated: {}\n", spec.updated_at));

    Ok(output)
}

/// Update a spec's body, test_commands, acceptance_criteria
pub fn spec_update(
    db: &Database,
    id: &str,
    body: Option<&str>,
    test_commands: Option<&str>,
    acceptance_criteria: Option<&str>,
) -> Result<()> {
    // Load existing spec to preserve unchanged fields
    let existing = db
        .spec_show(id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {id}"))?;

    let new_body = body.unwrap_or(&existing.body);
    let new_tc = test_commands.or(existing.test_commands.as_deref());
    let new_ac = acceptance_criteria.or(existing.acceptance_criteria.as_deref());

    db.spec_update(id, new_body, new_tc, new_ac)?;
    println!("updated: {id}");
    Ok(())
}

/// Pause a spec
pub fn spec_pause(db: &Database, id: &str) -> Result<()> {
    db.spec_set_status(id, SpecStatus::Paused)?;
    println!("paused: {id}");
    Ok(())
}

/// Resume a spec
pub fn spec_resume(db: &Database, id: &str) -> Result<()> {
    db.spec_set_status(id, SpecStatus::Active)?;
    println!("resumed: {id}");
    Ok(())
}

/// Link an issue to a spec
pub fn spec_link(db: &Database, spec_id: &str, issue_number: i64) -> Result<()> {
    db.spec_link_issue(spec_id, issue_number)?;
    println!("linked: spec={spec_id} issue=#{issue_number}");
    Ok(())
}

/// Unlink an issue from a spec
pub fn spec_unlink(db: &Database, spec_id: &str, issue_number: i64) -> Result<()> {
    db.spec_unlink_issue(spec_id, issue_number)?;
    println!("unlinked: spec={spec_id} issue=#{issue_number}");
    Ok(())
}

/// Check if a spec is ready for completion and transition to Completing status.
///
/// Verifies the spec is Active, has linked issues, then transitions to
/// Completing and creates a HITL event for final human confirmation.
pub fn spec_check_completion(db: &Database, id: &str) -> Result<String> {
    let spec = db
        .spec_show(id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {id}"))?;

    if spec.status != SpecStatus::Active {
        anyhow::bail!(
            "spec {id} is not active (current status: {}). Only active specs can be completed.",
            spec.status
        );
    }

    let issues = db.spec_issues(id)?;
    if issues.is_empty() {
        anyhow::bail!("spec {id} has no linked issues. Link issues before completing.");
    }

    // Check for conflicts before completing
    let mut conflict_warning = String::new();
    let active_specs = db.spec_list_active_with_source_path()?;
    if let Some(ref source_path) = spec.source_path {
        let conflicts: Vec<_> = active_specs
            .iter()
            .filter(|s| s.id != spec.id)
            .filter(|s| {
                s.source_path
                    .as_ref()
                    .is_some_and(|p| paths_overlap(p, source_path))
            })
            .collect();
        if !conflicts.is_empty() {
            conflict_warning.push_str(&format!(
                "\n\n⚠ {} conflict(s) detected (shared source_path: {}):\n",
                conflicts.len(),
                source_path
            ));
            for c in &conflicts {
                conflict_warning.push_str(&format!(
                    "  - {} ({}): {}\n",
                    c.id,
                    c.source_path.as_deref().unwrap_or("?"),
                    c.title
                ));
            }
        }
    }

    // Transition to Completing
    db.spec_set_status(id, SpecStatus::Completing)?;

    // Create HITL event for final confirmation
    let issue_list: Vec<String> = issues
        .iter()
        .map(|i| format!("#{}", i.issue_number))
        .collect();
    let hitl_event = NewHitlEvent {
        repo_id: spec.repo_id.clone(),
        spec_id: Some(id.to_string()),
        work_id: None,
        severity: HitlSeverity::High,
        situation: format!(
            "Spec '{}' is ready for completion. Linked issues: {}",
            spec.title,
            issue_list.join(", ")
        ),
        context: format!(
            "Spec ID: {}\nTitle: {}\nLinked issues: {}\n\nPlease confirm completion or reject to return to Active.{}",
            id, spec.title, issue_list.join(", "), conflict_warning
        ),
        options: vec![
            "Confirm completion".to_string(),
            "Reject (return to Active)".to_string(),
        ],
    };

    let event_id = db.hitl_create(&hitl_event)?;

    Ok(format!(
        "Spec {id} transitioned to completing. HITL event created: {event_id}\n\
         Respond with 'autodev hitl respond {event_id} --choice 1' to confirm completion."
    ))
}

/// Validate that a spec body contains required sections.
///
/// Returns a list of missing section names. Checks for section headers
/// (## or ### prefixed) case-insensitively.
fn validate_spec_sections(body: &str) -> Vec<String> {
    let required: &[(&str, &[&str])] = &[
        ("요구사항", &["requirements", "요구사항"]),
        ("아키텍처", &["architecture", "아키텍처", "컴포넌트"]),
        ("기술 스택", &["tech stack", "기술 스택", "기술스택"]),
        ("테스트", &["test", "테스트"]),
        (
            "수용 기준",
            &["acceptance criteria", "수용 기준", "수용기준"],
        ),
    ];

    let lower_body = body.to_lowercase();
    let mut missing = Vec::new();

    for (name, keywords) in required {
        let found = keywords.iter().any(|kw| {
            let kw_lower = kw.to_lowercase();
            // Check for markdown headers: ## or ### followed by the keyword
            lower_body.contains(&format!("## {kw_lower}"))
                || lower_body.contains(&format!("### {kw_lower}"))
        });
        if !found {
            missing.push(name.to_string());
        }
    }

    missing
}

/// Detect specs that share the same source_path (potential file conflicts).
pub fn spec_conflicts(db: &Database, spec_id: &str) -> Result<String> {
    let spec = db
        .spec_show(spec_id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {spec_id}"))?;

    let source_path = match &spec.source_path {
        Some(p) => p.clone(),
        None => {
            return Ok(format!(
                "Spec {spec_id} has no source_path — cannot detect conflicts.\n"
            ))
        }
    };

    let active_specs = db.spec_list_active_with_source_path()?;
    let conflicts: Vec<&Spec> = active_specs
        .iter()
        .filter(|s| s.id != spec.id)
        .filter(|s| {
            s.source_path
                .as_ref()
                .is_some_and(|p| paths_overlap(p, &source_path))
        })
        .collect();

    if conflicts.is_empty() {
        return Ok(format!("No conflicts detected for spec {spec_id}.\n"));
    }

    let mut output = format!(
        "⚠ {} conflict(s) detected for spec {spec_id} ({}):\n\n",
        conflicts.len(),
        source_path
    );
    for c in &conflicts {
        output.push_str(&format!(
            "  - {} ({}): {}\n",
            c.id,
            c.source_path.as_deref().unwrap_or("?"),
            c.title
        ));
    }
    output.push_str("\nConsider sequencing these specs or resolving file overlaps.\n");
    Ok(output)
}

/// Check if two paths overlap (same path or parent-child relationship).
fn paths_overlap(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

/// Prioritize specs by setting priority order based on the given ID list.
pub fn spec_prioritize(db: &Database, ids: &[String]) -> Result<String> {
    for (i, id) in ids.iter().enumerate() {
        db.spec_set_priority(id, (i + 1) as i32)?;
    }
    Ok(format!("Prioritized {} specs", ids.len()))
}
