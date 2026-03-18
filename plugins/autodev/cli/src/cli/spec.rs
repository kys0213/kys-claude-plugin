use anyhow::Result;

use crate::cli::resolve_repo_id;
use crate::core::models::*;
use crate::core::repository::*;
use crate::infra::db::Database;

/// spec_add의 결과: 출력 메시지와 해결된 repo_id.
pub struct SpecAddResult {
    pub output: String,
    pub repo_id: String,
}

/// Add a new spec
pub fn spec_add(
    db: &Database,
    title: &str,
    body: &str,
    repo_name: &str,
    file: Option<&str>,
    test_commands: Option<&str>,
    acceptance_criteria: Option<&str>,
) -> Result<SpecAddResult> {
    let repo_id = resolve_repo_id(db, repo_name)?;

    let new_spec = NewSpec {
        repo_id: repo_id.clone(),
        title: title.to_string(),
        body: body.to_string(),
        source_path: file.map(|s| s.to_string()),
        test_commands: test_commands.map(|s| s.to_string()),
        acceptance_criteria: acceptance_criteria.map(|s| s.to_string()),
    };

    let id = db.spec_add(&new_spec)?;

    let missing = validate_spec_sections(body);
    let output = if !missing.is_empty() {
        format!(
            "⚠ Missing sections: {}.\n  Recommended: add these as ## headers in the spec body.\n  Tip: use /add-spec in Claude Code for guided section completion.\n{id}",
            missing.join(", ")
        )
    } else {
        id
    };

    Ok(SpecAddResult { output, repo_id })
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

/// 스펙 완료 HITL 이벤트 식별 마커.
const COMPLETION_HITL_MARKER: &str = "ready for completion";

/// Check if a spec is ready for completion and transition to Completing status.
///
/// Verifies the spec is Active, has linked issues, then transitions to
/// Completing and creates a HITL event for final human confirmation.
pub fn spec_check_completion(db: &Database, id: &str) -> Result<(String, NewHitlEvent)> {
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

    // Verify linked issues are done (queue items in done/skipped phase)
    let queue_items = db.queue_list_items(None)?;
    let mut pending_issues = Vec::new();
    for issue in &issues {
        // work_id 형식: "issue:{repo_name}:{issue_number}"
        let matching_item = queue_items.iter().find(|q| {
            q.work_id.ends_with(&format!(":{}", issue.issue_number))
                && q.work_id.starts_with("issue:")
        });
        match matching_item {
            Some(item) if item.phase != QueuePhase::Done && item.skip_reason.is_none() => {
                pending_issues.push(format!("#{} (phase: {})", issue.issue_number, item.phase));
            }
            None => {
                // 큐에 없는 이슈는 아직 처리되지 않은 것으로 간주
            }
            _ => {} // done 또는 skipped
        }
    }

    let issues_warning = if pending_issues.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n⚠ {} issue(s) not yet done: {}",
            pending_issues.len(),
            pending_issues.join(", ")
        )
    };

    // Check for conflicts before completing
    let conflict_warning = if let Some(ref source_path) = spec.source_path {
        let conflicts = find_path_conflicts(db, &spec.id, source_path)?;
        if conflicts.is_empty() {
            String::new()
        } else {
            let mut w = format!(
                "\n\n⚠ {} conflict(s) detected (shared source_path: {}):\n",
                conflicts.len(),
                source_path
            );
            w.push_str(&format_conflict_list(&conflicts));
            w
        }
    } else {
        String::new()
    };

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
            "Spec '{}' is {COMPLETION_HITL_MARKER}. Linked issues: {}",
            spec.title,
            issue_list.join(", ")
        ),
        context: format!(
            "Spec ID: {}\nTitle: {}\nLinked issues: {}\n\nPlease confirm completion or reject to return to Active.{}{}",
            id, spec.title, issue_list.join(", "), issues_warning, conflict_warning
        ),
        options: vec![
            "Confirm completion".to_string(),
            "Reject (return to Active)".to_string(),
        ],
    };

    let event_id = db.hitl_create(&hitl_event)?;

    Ok((
        format!(
            "Spec {id} transitioned to completing. HITL event created: {event_id}\n\
             Respond with 'autodev hitl respond {event_id} --choice 1' to confirm completion."
        ),
        hitl_event,
    ))
}

/// HITL 응답으로 스펙 완료를 확정하거나 거부한다.
///
/// - choice 1 (Confirm completion): Completing → Completed
/// - choice 2 (Reject): Completing → Active
///
/// 스펙 완료 관련 HITL 이벤트가 아니면 None을 반환한다.
pub fn handle_spec_completion_response(
    db: &Database,
    event: &HitlEvent,
    choice: Option<i32>,
) -> Option<String> {
    // 스펙 완료 관련 HITL인지 확인
    let spec_id = event.spec_id.as_deref()?;
    if !event.situation.contains(COMPLETION_HITL_MARKER) {
        return None;
    }

    let spec = db.spec_show(spec_id).ok()??;
    if spec.status != SpecStatus::Completing {
        return None;
    }

    match choice {
        Some(1) => {
            // Confirm → Completed
            if let Err(e) = db.spec_set_status(spec_id, SpecStatus::Completed) {
                return Some(format!("Failed to complete spec {spec_id}: {e}"));
            }
            Some(format!("Spec {spec_id} marked as Completed"))
        }
        Some(2) => {
            // Reject → Active
            if let Err(e) = db.spec_set_status(spec_id, SpecStatus::Active) {
                return Some(format!("Failed to reactivate spec {spec_id}: {e}"));
            }
            Some(format!("Spec {spec_id} returned to Active"))
        }
        _ => None,
    }
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

/// (source_path, conflicting specs)
type ConflictInfo = (String, Vec<Spec>);

/// Resolve a spec and find its path conflicts. Returns None if no source_path is set.
fn resolve_conflicts(db: &Database, spec_id: &str) -> Result<(Spec, Option<ConflictInfo>)> {
    let spec = db
        .spec_show(spec_id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {spec_id}"))?;

    let result = match &spec.source_path {
        Some(p) => {
            let conflicts = find_path_conflicts(db, &spec.id, p)?;
            if conflicts.is_empty() {
                None
            } else {
                Some((p.clone(), conflicts))
            }
        }
        None => None,
    };

    Ok((spec, result))
}

/// Detect specs that share the same source_path or have concurrent queue items (potential conflicts).
pub fn spec_conflicts(db: &Database, spec_id: &str) -> Result<String> {
    let spec = db
        .spec_show(spec_id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {spec_id}"))?;

    let mut output = String::new();
    let mut conflict_count = 0;

    // 1. Path-based conflicts (existing logic)
    if let Some(ref source_path) = spec.source_path {
        let path_conflicts = find_path_conflicts(db, &spec.id, source_path)?;
        if !path_conflicts.is_empty() {
            conflict_count += path_conflicts.len();
            output.push_str(&format!(
                "Path conflicts (shared source_path: {source_path}):\n"
            ));
            output.push_str(&format_conflict_list(&path_conflicts));
            output.push('\n');
        }
    }

    // 2. Queue-based conflicts: detect specs with concurrent running/ready items in same repo
    let spec_issues = db.spec_issues(spec_id)?;
    let all_items = db.queue_list_items(None)?;
    let active_specs = db.spec_list_by_status(SpecStatus::Active)?;

    // Find running/ready items belonging to THIS spec's issues
    let my_active_work_ids: std::collections::HashSet<_> = spec_issues
        .iter()
        .filter_map(|si| {
            all_items.iter().find(|q| {
                q.work_id.ends_with(&format!(":{}", si.issue_number))
                    && (q.phase == QueuePhase::Running || q.phase == QueuePhase::Ready)
            })
        })
        .map(|q| q.work_id.as_str())
        .collect();

    if !my_active_work_ids.is_empty() {
        // Check other active specs for concurrent items in the same repo
        for other_spec in &active_specs {
            if other_spec.id == spec.id || other_spec.repo_id != spec.repo_id {
                continue;
            }
            let other_issues = db.spec_issues(&other_spec.id)?;
            let concurrent: Vec<_> = other_issues
                .iter()
                .filter_map(|si| {
                    all_items.iter().find(|q| {
                        q.work_id.ends_with(&format!(":{}", si.issue_number))
                            && (q.phase == QueuePhase::Running || q.phase == QueuePhase::Ready)
                    })
                })
                .collect();
            if !concurrent.is_empty() {
                conflict_count += 1;
                output.push_str(&format!(
                    "Queue conflict with spec {} ({}):\n  {} concurrent active item(s)\n\n",
                    other_spec.id,
                    other_spec.title,
                    concurrent.len()
                ));
            }
        }
    }

    if conflict_count == 0 {
        return Ok(format!("No conflicts detected for spec {spec_id}.\n"));
    }

    Ok(format!(
        "⚠ {conflict_count} conflict(s) detected for spec {spec_id}:\n\n{output}\
         Consider sequencing these specs or resolving file overlaps.\n"
    ))
}

/// Find active specs whose source_path overlaps with the given path, excluding `exclude_id`.
fn find_path_conflicts(db: &Database, exclude_id: &str, source_path: &str) -> Result<Vec<Spec>> {
    let active_specs = db.spec_list_by_status(SpecStatus::Active)?;
    Ok(active_specs
        .into_iter()
        .filter(|s| s.id != exclude_id)
        .filter(|s| {
            s.source_path
                .as_ref()
                .is_some_and(|p| paths_overlap(p, source_path))
        })
        .collect())
}

/// Format a list of conflicting specs for display.
fn format_conflict_list(conflicts: &[Spec]) -> String {
    let mut out = String::new();
    for c in conflicts {
        out.push_str(&format!(
            "  - {} ({}): {}\n",
            c.id,
            c.source_path.as_deref().unwrap_or("?"),
            c.title
        ));
    }
    out
}

/// Check if two paths overlap (same path or parent-child relationship).
fn paths_overlap(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let a_is_parent = b.starts_with(a) && b.as_bytes().get(a.len()) == Some(&b'/');
    let b_is_parent = a.starts_with(b) && a.as_bytes().get(b.len()) == Some(&b'/');
    a_is_parent || b_is_parent
}

/// Detect conflicts and create a HITL event if conflicts are found.
///
/// Returns the conflict output and the created HITL event ID (if any).
pub fn spec_conflicts_with_hitl(db: &Database, spec_id: &str) -> Result<(String, Option<String>)> {
    let (spec, resolved) = resolve_conflicts(db, spec_id)?;

    let Some((source_path, conflicts)) = resolved else {
        return Ok((format!("No conflicts for spec {spec_id}.\n"), None));
    };

    let mut options = vec![format!("Prioritize current: {}", spec.title)];
    for c in &conflicts {
        options.push(format!("Prioritize: {} ({})", c.title, c.id));
    }
    options.push("Sequence them manually".to_string());

    let hitl_event = NewHitlEvent {
        repo_id: spec.repo_id.clone(),
        spec_id: Some(spec.id.clone()),
        work_id: None,
        severity: HitlSeverity::Medium,
        situation: format!(
            "Spec conflict: {} spec(s) modify overlapping path '{}'",
            conflicts.len(),
            source_path
        ),
        context: format!(
            "Spec '{}' ({}) conflicts with:\n{}",
            spec.title,
            spec.id,
            format_conflict_list(&conflicts)
        ),
        options,
    };

    let hitl_id = db.hitl_create(&hitl_event)?;

    let output = format!(
        "⚠ {} conflict(s) for spec {spec_id} → HITL event created: {hitl_id}\n",
        conflicts.len()
    );
    Ok((output, Some(hitl_id)))
}

/// Prioritize specs by setting priority order based on the given ID list.
pub fn spec_prioritize(db: &Database, ids: &[String]) -> Result<String> {
    for (i, id) in ids.iter().enumerate() {
        db.spec_set_priority(id, (i + 1) as i32)?;
    }
    Ok(format!("Prioritized {} specs", ids.len()))
}

/// 스펙 진행 상태 조회: 이슈 진척, HITL 이벤트, 결정 이력을 집계한다.
pub fn spec_status(db: &Database, id: &str, json: bool) -> Result<String> {
    let spec = db
        .spec_show(id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {id}"))?;

    let issues = db.spec_issues(id)?;
    let decisions = db.decision_list_by_spec(id, 100)?;
    let (hitl_total, hitl_pending) = db.hitl_count_by_spec(id)?;

    // Count done issues by checking queue item phases
    let all_items = db.queue_list_items(None)?;
    let done_count = issues
        .iter()
        .filter(|si| {
            all_items.iter().any(|q| {
                q.work_id.ends_with(&format!(":{}", si.issue_number)) && q.phase == QueuePhase::Done
            })
        })
        .count();

    if json {
        let value = serde_json::json!({
            "id": spec.id,
            "title": spec.title,
            "status": spec.status.to_string(),
            "priority": spec.priority,
            "issues": {
                "total": issues.len(),
                "done": done_count,
            },
            "hitl": {
                "total": hitl_total,
                "pending": hitl_pending,
            },
            "decisions": decisions.len(),
        });
        return Ok(serde_json::to_string_pretty(&value)?);
    }

    let mut output = String::new();
    output.push_str(&format!("Spec: {} — {}\n", spec.id, spec.title));
    output.push_str(&format!("Status: {}\n", spec.status));
    if let Some(p) = spec.priority {
        output.push_str(&format!("Priority: {p}\n"));
    }

    output.push_str(&format!(
        "\nIssues: {done_count}/{} done\n",
        issues.len()
    ));
    for issue in &issues {
        output.push_str(&format!("  #{}\n", issue.issue_number));
    }

    output.push_str(&format!(
        "\nHITL: {hitl_total} total ({hitl_pending} pending)\n"
    ));
    output.push_str(&format!("Decisions: {}\n", decisions.len()));

    Ok(output)
}

/// 스펙의 repo에 대해 claw-evaluate를 즉시 트리거한다.
pub fn spec_evaluate(db: &Database, id: &str) -> Result<String> {
    let spec = db
        .spec_show(id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {id}"))?;

    // Force-trigger claw-evaluate for the spec's repo
    let _ = db.cron_reset_last_run(crate::cli::cron::CLAW_EVALUATE_JOB, Some(&spec.repo_id));

    Ok(format!(
        "Triggered claw-evaluate for spec {id} (repo: {})\n",
        spec.repo_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_overlap_same_path() {
        assert!(paths_overlap("src/main.rs", "src/main.rs"));
    }

    #[test]
    fn paths_overlap_parent_child() {
        assert!(paths_overlap("src", "src/main.rs"));
        assert!(paths_overlap("src/main.rs", "src"));
    }

    #[test]
    fn paths_overlap_no_overlap() {
        assert!(!paths_overlap("src/main.rs", "src/lib.rs"));
        assert!(!paths_overlap("src/foo", "src/foobar"));
    }

    #[test]
    fn paths_overlap_partial_prefix_not_overlap() {
        // "src/fo" is not a parent of "src/foo" — no slash boundary
        assert!(!paths_overlap("src/fo", "src/foo"));
    }

    #[test]
    fn format_conflict_list_renders_correctly() {
        let conflicts = vec![Spec {
            id: "spec-1".to_string(),
            repo_id: "repo".to_string(),
            title: "Add feature".to_string(),
            body: String::new(),
            status: SpecStatus::Active,
            source_path: Some("src/lib.rs".to_string()),
            test_commands: None,
            acceptance_criteria: None,
            priority: None,
            created_at: String::new(),
            updated_at: String::new(),
        }];
        let output = format_conflict_list(&conflicts);
        assert!(output.contains("spec-1"));
        assert!(output.contains("src/lib.rs"));
        assert!(output.contains("Add feature"));
    }
}
