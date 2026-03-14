use anyhow::Result;

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
    // Find repo_id by name
    let repos = db.repo_list()?;
    let repo = repos
        .iter()
        .find(|r| r.name == repo_name)
        .ok_or_else(|| anyhow::anyhow!("repository not found: {repo_name}"))?;

    // Get repo_id from enabled repos (which has the id field)
    let enabled = db.repo_find_enabled()?;
    let repo_id = enabled
        .iter()
        .find(|r| r.name == repo.name)
        .map(|r| r.id.clone())
        .ok_or_else(|| anyhow::anyhow!("repository not enabled: {repo_name}"))?;

    let new_spec = NewSpec {
        repo_id,
        title: title.to_string(),
        body: body.to_string(),
        source_path: file.map(|s| s.to_string()),
        test_commands: test_commands.map(|s| s.to_string()),
        acceptance_criteria: acceptance_criteria.map(|s| s.to_string()),
    };

    let id = db.spec_add(&new_spec)?;
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
        for s in &specs {
            let issues = db.spec_issues(&s.id)?;
            let issue_count = issues.len();
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
