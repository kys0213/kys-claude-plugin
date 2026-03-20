use std::io::Read as _;
use std::sync::Arc;

use anyhow::Result;

use crate::cli::resolve_repo_id;
use crate::core::models::*;
use crate::core::repository::*;
use crate::infra::db::Database;
use crate::infra::gh::Gh;

/// Maximum number of issues created per `spec verify` invocation.
const MAX_ISSUES_PER_VERIFY: usize = 5;

/// Allowed test runner command prefixes.
///
/// Commands passed to `run_spec_test_commands` must start with one of these
/// prefixes. The list is intentionally conservative — extend it as new runners
/// are adopted.
const ALLOWED_TEST_COMMAND_PREFIXES: &[&str] = &[
    "cargo test",
    "cargo clippy",
    "npm test",
    "npm run test",
    "go test",
    "python -m pytest",
    "make test",
    "bash",
    "sh",
];

/// Maximum bytes collected from a single command's stdout + stderr combined.
const OUTPUT_SIZE_LIMIT: usize = 1024 * 1024; // 1 MB

/// Timeout for a single test command (seconds).
const TEST_COMMAND_TIMEOUT_SECS: u64 = 60;

/// spec_add의 결과: 출력 메시지와 해결된 repo_id.
pub struct SpecAddResult {
    pub output: String,
    pub repo_id: String,
}

/// Parameters for `spec_add`.
pub struct SpecAddParams<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub repo_name: &'a str,
    pub file: Option<&'a str>,
    pub test_commands: Option<&'a str>,
    pub acceptance_criteria: Option<&'a str>,
    /// When false, registration is blocked if required sections are missing.
    pub force: bool,
}

/// Add a new spec
///
/// When `force` is false, registration is blocked if required sections are missing.
/// Pass `force = true` to override and register anyway.
pub fn spec_add(db: &Database, params: &SpecAddParams<'_>) -> Result<SpecAddResult> {
    let repo_id = resolve_repo_id(db, params.repo_name)?;

    // Validate required sections before persisting
    let missing = validate_spec_sections(params.body);
    if !missing.is_empty() && !params.force {
        anyhow::bail!(
            "Missing required sections: {}.\n  Add these as ## headers in the spec body, or pass --force to override.\n  Tip: use /add-spec in Claude Code for guided section completion.",
            missing.join(", ")
        );
    }

    let new_spec = NewSpec {
        repo_id: repo_id.clone(),
        title: params.title.to_string(),
        body: params.body.to_string(),
        source_path: params.file.map(|s| s.to_string()),
        test_commands: params.test_commands.map(|s| s.to_string()),
        acceptance_criteria: params.acceptance_criteria.map(|s| s.to_string()),
    };

    let id = db.spec_add(&new_spec)?;

    let output = if !missing.is_empty() {
        // force == true path: warn but still register
        format!(
            "⚠ Missing sections (forced): {}.\n  Recommended: add these as ## headers in the spec body.\n{id}",
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
/// Verifies the spec is Active, has linked issues, runs test commands (if any),
/// then transitions to Completing and creates a HITL event for final human
/// confirmation.
pub fn spec_check_completion(
    db: &Database,
    env: &dyn crate::core::config::Env,
    id: &str,
) -> Result<(String, NewHitlEvent, String)> {
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

    // Execute test_commands if present
    let test_results = run_spec_test_commands(db, env, &spec)?;

    // Transition to Completing
    db.spec_set_status(id, SpecStatus::Completing)?;

    // Severity: HIGH always (completion is a critical decision).
    // If tests failed, this is noted in the situation/context fields.
    let severity = HitlSeverity::High;

    // Build test results section for context
    let test_section = match &test_results {
        Some(results) => {
            let status = if results.all_passed {
                "ALL PASSED"
            } else {
                "SOME FAILED"
            };
            format!("\n\nTest commands: {status}\n{}", results.summary)
        }
        None => String::new(),
    };

    // Build situation with test status
    let test_situation = match &test_results {
        Some(results) if results.all_passed => " All test commands passed.".to_string(),
        Some(results) => format!(
            " {} of {} test command(s) failed.",
            results.failed_count, results.total_count
        ),
        None => String::new(),
    };

    // Create HITL event for final confirmation
    let issue_list: Vec<String> = issues
        .iter()
        .map(|i| format!("#{}", i.issue_number))
        .collect();
    let hitl_event = NewHitlEvent {
        repo_id: spec.repo_id.clone(),
        spec_id: Some(id.to_string()),
        work_id: None,
        severity,
        situation: format!(
            "Spec '{}' is {COMPLETION_HITL_MARKER}. Linked issues: {}.{}",
            spec.title,
            issue_list.join(", "),
            test_situation,
        ),
        context: format!(
            "Spec ID: {}\nTitle: {}\nLinked issues: {}\n\nPlease confirm completion or reject to return to Active.{}{}{}",
            id, spec.title, issue_list.join(", "), issues_warning, conflict_warning, test_section
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
        event_id,
    ))
}

/// Results from running spec test commands.
struct TestCommandResults {
    all_passed: bool,
    failed_count: usize,
    total_count: usize,
    summary: String,
}

/// Execute test_commands defined on a spec in the spec's repo workspace directory.
///
/// Returns `None` if no test_commands are defined or they are empty.
/// Parses `test_commands` as a JSON array of command strings.
///
/// Each command is validated against [`ALLOWED_TEST_COMMAND_PREFIXES`].
/// Output is capped at [`OUTPUT_SIZE_LIMIT`] bytes per command and commands
/// are killed after [`TEST_COMMAND_TIMEOUT_SECS`] seconds.
fn run_spec_test_commands(
    db: &Database,
    env: &dyn crate::core::config::Env,
    spec: &Spec,
) -> Result<Option<TestCommandResults>> {
    let tc_json = match &spec.test_commands {
        Some(tc) if !tc.trim().is_empty() => tc,
        _ => return Ok(None),
    };

    // Parse JSON array of command strings
    let commands: Vec<String> = serde_json::from_str(tc_json)
        .map_err(|e| anyhow::anyhow!("failed to parse test_commands JSON: {e}"))?;

    if commands.is_empty() {
        return Ok(None);
    }

    // Resolve workspace directory for the spec's repo
    let repo_name = resolve_repo_name(db, &spec.repo_id)?;
    let ws_root = crate::core::config::workspaces_path(env);
    let repo_dir = ws_root
        .join(crate::core::config::sanitize_repo_name(&repo_name))
        .join("main");

    let mut summary = String::new();
    let mut failed_count = 0;
    let total_count = commands.len();

    for (i, cmd) in commands.iter().enumerate() {
        // Validate command against allowlist
        if !is_allowed_test_command(cmd) {
            tracing::warn!(
                command = cmd.as_str(),
                "rejected test command: does not match any allowed prefix"
            );
            failed_count += 1;
            summary.push_str(&format!(
                "  [{}/{}] REJECTED: {} — command prefix not in allowlist\n",
                i + 1,
                total_count,
                cmd
            ));
            continue;
        }

        let child = std::process::Command::new("sh")
            .args(["-c", cmd.as_str()])
            .current_dir(&repo_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                failed_count += 1;
                summary.push_str(&format!(
                    "  [{}/{}] ERROR: {} — {}\n",
                    i + 1,
                    total_count,
                    cmd,
                    e
                ));
                continue;
            }
        };

        // Read stdout/stderr with size limit.
        // Take pipes before waiting so we don't deadlock.
        let stdout_bytes = child
            .stdout
            .take()
            .and_then(|mut r| read_limited(&mut r, OUTPUT_SIZE_LIMIT).ok())
            .unwrap_or_default();

        let remaining = OUTPUT_SIZE_LIMIT.saturating_sub(stdout_bytes.len());
        let stderr_bytes = child
            .stderr
            .take()
            .and_then(|mut r| read_limited(&mut r, remaining).ok())
            .unwrap_or_default();

        // Wait with timeout
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(TEST_COMMAND_TIMEOUT_SECS);
        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        timed_out = true;
                        break std::process::ExitStatus::default();
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(_) => {
                    break std::process::ExitStatus::default();
                }
            }
        };

        let success = !timed_out && status.success();
        let stdout = String::from_utf8_lossy(&stdout_bytes);
        let stderr = String::from_utf8_lossy(&stderr_bytes);
        let status_label = if timed_out {
            "TIMEOUT"
        } else if success {
            "PASS"
        } else {
            "FAIL"
        };

        if !success {
            failed_count += 1;
        }

        summary.push_str(&format!(
            "  [{}/{}] {}: {}\n",
            i + 1,
            total_count,
            status_label,
            cmd
        ));

        // Include truncated output for context
        let max_output_len = 500;
        if !stdout.is_empty() {
            let truncated = truncate_output(&stdout, max_output_len);
            summary.push_str(&format!("    stdout: {truncated}\n"));
        }
        if !stderr.is_empty() {
            let truncated_stderr = truncate_output(&stderr, max_output_len);
            if timed_out {
                summary.push_str(&format!(
                    "    stderr: {truncated_stderr}\n    [timed out after {TEST_COMMAND_TIMEOUT_SECS}s]\n"
                ));
            } else {
                summary.push_str(&format!("    stderr: {truncated_stderr}\n"));
            }
        } else if timed_out {
            summary.push_str(&format!(
                "    [timed out after {TEST_COMMAND_TIMEOUT_SECS}s]\n"
            ));
        }
    }

    Ok(Some(TestCommandResults {
        all_passed: failed_count == 0,
        failed_count,
        total_count,
        summary,
    }))
}

/// Resolve the repo name from a repo_id by looking up enabled repos.
fn resolve_repo_name(db: &Database, repo_id: &str) -> Result<String> {
    let repos = db.repo_find_enabled()?;
    repos
        .iter()
        .find(|r| r.id == repo_id)
        .map(|r| r.name.clone())
        .ok_or_else(|| anyhow::anyhow!("repository not found for id: {repo_id}"))
}

/// Truncate output to a maximum length, appending "..." if truncated.
fn truncate_output(s: &str, max_len: usize) -> String {
    let trimmed = s.trim();
    let char_count = trimmed.chars().count();
    if char_count <= max_len {
        trimmed.replace('\n', " | ")
    } else {
        let truncated: String = trimmed.chars().take(max_len).collect();
        format!("{}...", truncated.replace('\n', " | "))
    }
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

    // 3. Git-diff-based file overlap: compare modified files across active specs
    {
        let env = crate::core::config::RealEnv;
        let ws_root = crate::core::config::workspaces_path(&env);
        let my_files = collect_spec_files(db, &spec, &all_items, &ws_root);

        if !my_files.is_empty() {
            for other_spec in &active_specs {
                if other_spec.id == spec.id || other_spec.repo_id != spec.repo_id {
                    continue;
                }
                let other_files = collect_spec_files(db, other_spec, &all_items, &ws_root);
                let overlap: Vec<&String> = my_files.intersection(&other_files).collect();
                if !overlap.is_empty() {
                    conflict_count += 1;
                    let mut file_list = overlap.clone();
                    file_list.sort();
                    output.push_str(&format!(
                        "File conflict with spec {} ({}):\n  {} overlapping file(s):\n",
                        other_spec.id,
                        other_spec.title,
                        file_list.len()
                    ));
                    for f in file_list.iter().take(10) {
                        output.push_str(&format!("    {f}\n"));
                    }
                    if overlap.len() > 10 {
                        output.push_str(&format!("    ... and {} more\n", overlap.len() - 10));
                    }
                    output.push('\n');
                }
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

/// Collect modified files for a spec by running `git diff --name-only` on its PR branches.
fn collect_spec_files(
    db: &Database,
    spec: &Spec,
    all_items: &[QueueItemRow],
    ws_root: &std::path::Path,
) -> std::collections::HashSet<String> {
    use crate::core::queue_item::QueueItem;

    let mut files = std::collections::HashSet::new();
    let issues = db.spec_issues(&spec.id).unwrap_or_default();

    for issue in &issues {
        // Find PR queue items for this issue
        for item in all_items {
            if !item.work_id.ends_with(&format!(":{}", issue.issue_number)) {
                continue;
            }
            // Extract head_branch from PR metadata
            if let Some(ref json) = item.metadata_json {
                if let Some(crate::core::queue_item::ItemMetadata::Pr(pr)) =
                    QueueItem::metadata_from_json(json)
                {
                    if pr.head_branch.is_empty() {
                        continue;
                    }
                    let repo_name = crate::core::config::sanitize_repo_name(
                        item.work_id.split(':').nth(1).unwrap_or(""),
                    );
                    let repo_dir = ws_root.join(&repo_name).join("main");
                    match git_diff_name_only(&repo_dir, &pr.head_branch) {
                        Ok(diff_files) => files.extend(diff_files),
                        Err(e) => tracing::warn!("git diff skipped for {}: {e}", pr.head_branch),
                    }
                }
            }
        }
    }

    files
}

/// Run `git diff --name-only HEAD...<branch>` and return the set of modified files.
fn git_diff_name_only(repo_dir: &std::path::Path, branch: &str) -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", &format!("HEAD...{branch}")])
        .current_dir(repo_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "git diff failed in {} for branch {}: {}",
            repo_dir.display(),
            branch,
            stderr.trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
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

/// Truncate a string to at most `max_chars` characters, appending "..." if truncated.
/// Safe for multibyte characters (Korean, emoji, etc.).
fn truncate_chars(s: &str, max_chars: usize) -> String {
    let truncated: String = s.chars().take(max_chars).collect();
    if truncated.len() < s.len() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

/// Parse acceptance criteria text into individual criterion lines.
/// Each non-empty line (optionally prefixed with `- ` or `* `) is one criterion.
fn parse_criteria(acceptance_criteria: &str) -> Vec<String> {
    acceptance_criteria
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .unwrap_or(line)
                .to_string()
        })
        .collect()
}

/// Verify a spec's acceptance criteria and optionally create GitHub issues for unmet criteria.
///
/// Returns a human-readable report of the verification result.
pub async fn spec_verify(
    db: &Database,
    gh: &Arc<dyn Gh>,
    id: &str,
    create_issues: bool,
    gh_host: Option<&str>,
) -> Result<String> {
    let spec = db
        .spec_show(id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {id}"))?;

    let acceptance_criteria = spec
        .acceptance_criteria
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("spec {id} has no acceptance_criteria defined"))?;

    let criteria = parse_criteria(acceptance_criteria);
    if criteria.is_empty() {
        return Ok(format!("spec {id}: no acceptance criteria to verify.\n"));
    }

    // Resolve repo name from repo_id
    let enabled = db.repo_find_enabled()?;
    let repo = enabled
        .iter()
        .find(|r| r.id == spec.repo_id)
        .ok_or_else(|| anyhow::anyhow!("repo not found for id: {}", spec.repo_id))?;
    let repo_name = &repo.name;

    let linked_issues = db.spec_issues(id)?;
    let all_items = db.queue_list_items(None)?;

    // Check which criteria are "met" by having a linked done issue
    let done_count = linked_issues
        .iter()
        .filter(|si| {
            all_items.iter().any(|q| {
                q.work_id.ends_with(&format!(":{}", si.issue_number)) && q.phase == QueuePhase::Done
            })
        })
        .count();

    // Build report of unmet criteria (criteria without corresponding done issues)
    // For simplicity, consider criteria unmet if total done issues < total criteria
    let unmet_criteria: Vec<&String> = if done_count >= criteria.len() {
        Vec::new()
    } else {
        criteria.iter().skip(done_count).collect()
    };

    let mut output = String::new();
    output.push_str(&format!("Spec: {} — {}\n", spec.id, spec.title));
    output.push_str(&format!(
        "Criteria: {} total, {} met, {} unmet\n\n",
        criteria.len(),
        criteria.len() - unmet_criteria.len(),
        unmet_criteria.len()
    ));

    if unmet_criteria.is_empty() {
        output.push_str("All acceptance criteria are met.\n");
        return Ok(output);
    }

    for (i, criterion) in unmet_criteria.iter().enumerate() {
        let status = if i < MAX_ISSUES_PER_VERIFY || !create_issues {
            "UNMET"
        } else {
            "UNMET (skipped)"
        };
        output.push_str(&format!("  [{status}] {criterion}\n"));
    }

    if !create_issues {
        return Ok(output);
    }

    // Create issues for unmet criteria (with dedup and rate limiting)
    let mut created_count = 0;
    let mut skipped_dedup = 0;

    for criterion in unmet_criteria.iter().take(MAX_ISSUES_PER_VERIFY) {
        let title = format!(
            "[spec:{}] {}",
            truncate_chars(&spec.title, 20),
            truncate_chars(criterion, 57)
        );

        // Dedup: check if an open issue with the same title already exists
        let existing = gh.issue_list_open(repo_name, &title, gh_host).await;
        if existing.iter().any(|t| t == &title) {
            skipped_dedup += 1;
            output.push_str(&format!("  [DEDUP] issue already exists: {title}\n"));
            continue;
        }

        let body = format!(
            "Auto-created by `spec verify --create-issues`\n\n\
             **Spec:** {} ({})\n\
             **Criterion:** {}\n",
            spec.title, spec.id, criterion
        );

        if gh.create_issue(repo_name, &title, &body, gh_host).await {
            created_count += 1;
            output.push_str(&format!("  [CREATED] {title}\n"));
        } else {
            output.push_str(&format!("  [FAILED] could not create issue: {title}\n"));
        }
    }

    let remaining = unmet_criteria.len().saturating_sub(MAX_ISSUES_PER_VERIFY);
    if remaining > 0 {
        output.push_str(&format!(
            "\n⚠ {remaining} additional unmet criteria exceeded the limit of {MAX_ISSUES_PER_VERIFY} issues per invocation. \
             Re-run after resolving existing issues.\n"
        ));
    }

    output.push_str(&format!(
        "\nSummary: {created_count} created, {skipped_dedup} deduplicated, {remaining} skipped (limit)\n"
    ));

    Ok(output)
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

    output.push_str(&format!("\nIssues: {done_count}/{} done\n", issues.len()));
    for issue in &issues {
        output.push_str(&format!("  #{}\n", issue.issue_number));
    }

    output.push_str(&format!(
        "\nHITL: {hitl_total} total ({hitl_pending} pending)\n"
    ));
    output.push_str(&format!("Decisions: {}\n", decisions.len()));

    Ok(output)
}

/// 스펙 관련 결정 이력 조회
pub fn spec_decisions(db: &Database, spec_id: &str, limit: usize, json: bool) -> Result<String> {
    // Verify spec exists
    let _spec = db
        .spec_show(spec_id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {spec_id}"))?;

    let decisions = db.decision_list_by_spec(spec_id, limit)?;

    if json {
        return Ok(serde_json::to_string_pretty(&decisions)?);
    }

    let mut output = String::new();
    if decisions.is_empty() {
        output.push_str(&format!("No decisions found for spec {spec_id}.\n"));
    } else {
        output.push_str(&format!(
            "Decisions for spec {spec_id} (showing up to {limit}):\n"
        ));
        for d in &decisions {
            let target = d
                .target_work_id
                .as_deref()
                .map(|w| format!(" target={w}"))
                .unwrap_or_default();
            output.push_str(&format!(
                "  [{}] {} —{} {}\n",
                d.decision_type, d.id, target, d.reasoning
            ));
        }
    }
    Ok(output)
}

/// Check all active specs and auto-trigger completion for those whose linked issues are all done.
///
/// Returns a list of (spec_id, NewHitlEvent) for each spec that was transitioned to Completing.
/// Errors on individual specs are logged and skipped.
pub fn check_completable_specs(
    db: &Database,
    env: &dyn crate::core::config::Env,
) -> Vec<(String, NewHitlEvent, String)> {
    let active_specs = match db.spec_list_by_status(SpecStatus::Active) {
        Ok(specs) => specs,
        Err(e) => {
            tracing::warn!("check_completable_specs: failed to list active specs: {e}");
            return Vec::new();
        }
    };

    let queue_items = match db.queue_list_items(None) {
        Ok(items) => items,
        Err(e) => {
            tracing::warn!("check_completable_specs: failed to list queue items: {e}");
            return Vec::new();
        }
    };

    let mut triggered = Vec::new();

    for spec in &active_specs {
        let issues = match db.spec_issues(&spec.id) {
            Ok(issues) => issues,
            Err(e) => {
                tracing::warn!(
                    "check_completable_specs: failed to load issues for spec {}: {e}",
                    spec.id
                );
                continue;
            }
        };

        // Skip specs with no linked issues
        if issues.is_empty() {
            continue;
        }

        // Check if ALL linked issues are done or skipped in the queue
        let all_done = issues.iter().all(|issue| {
            let matching_item = queue_items.iter().find(|q| {
                q.work_id.ends_with(&format!(":{}", issue.issue_number))
                    && q.work_id.starts_with("issue:")
            });
            match matching_item {
                Some(item) => item.phase == QueuePhase::Done || item.skip_reason.is_some(),
                None => false, // Not in queue yet = not done
            }
        });

        if all_done {
            tracing::info!(
                "spec auto-completion: all issues done for spec {} ('{}')",
                spec.id,
                spec.title
            );
            match spec_check_completion(db, env, &spec.id) {
                Ok((_output, hitl_event, hitl_id)) => {
                    tracing::info!("spec auto-completion: triggered HITL for spec {}", spec.id);
                    triggered.push((spec.id.clone(), hitl_event, hitl_id));
                }
                Err(e) => {
                    tracing::warn!(
                        "spec auto-completion: failed to trigger completion for spec {}: {e}",
                        spec.id
                    );
                }
            }
        }
    }

    triggered
}

// ─── Acceptance Criteria Verification ───

/// A single parsed acceptance criterion.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AcceptanceCriterion {
    /// The criterion text (without the checkbox prefix).
    pub text: String,
    /// Whether this criterion is already checked in the markdown.
    pub checked: bool,
}

/// Result of verifying acceptance criteria for a spec.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerifyResult {
    pub spec_id: String,
    pub spec_title: String,
    pub total: usize,
    pub met: usize,
    pub unmet: usize,
    pub criteria: Vec<AcceptanceCriterion>,
}

impl VerifyResult {
    /// Return only the unmet criteria.
    pub fn unmet_criteria(&self) -> Vec<&AcceptanceCriterion> {
        self.criteria.iter().filter(|c| !c.checked).collect()
    }
}

impl std::fmt::Display for VerifyResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Spec: {} — {}\nAcceptance criteria: {}/{} met\n",
            self.spec_id, self.spec_title, self.met, self.total
        )?;
        for c in &self.criteria {
            let mark = if c.checked { "x" } else { " " };
            writeln!(f, "  - [{mark}] {}", c.text)?;
        }
        Ok(())
    }
}

/// Parse acceptance criteria from markdown checklist format.
///
/// Recognizes lines matching:
/// - `- [ ] text` (unchecked)
/// - `- [x] text` or `- [X] text` (checked)
/// - `* [ ] text` / `* [x] text` (bullet variant)
///
/// Lines that don't match the checklist pattern are ignored.
fn parse_acceptance_criteria(text: &str) -> Vec<AcceptanceCriterion> {
    let mut criteria = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // Match "- [ ] ...", "- [x] ...", "* [ ] ...", "* [x] ..."
        let rest = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "));
        let Some(rest) = rest else { continue };
        if let Some(text) = rest.strip_prefix("[ ] ") {
            criteria.push(AcceptanceCriterion {
                text: text.trim().to_string(),
                checked: false,
            });
        } else if let Some(text) = rest
            .strip_prefix("[x] ")
            .or_else(|| rest.strip_prefix("[X] "))
        {
            criteria.push(AcceptanceCriterion {
                text: text.trim().to_string(),
                checked: true,
            });
        }
    }
    criteria
}

/// Cross-reference acceptance criteria with linked issues.
///
/// For each unmet criterion, check if any linked issue title contains the
/// criterion text (case-insensitive fuzzy match). If a matching issue is
/// in the Done phase, mark the criterion as met.
fn cross_reference_with_issues(criteria: &mut [AcceptanceCriterion], db: &Database, spec_id: &str) {
    let issues = db.spec_issues(spec_id).unwrap_or_default();
    if issues.is_empty() {
        return;
    }

    let queue_items = db.queue_list_items(None).unwrap_or_default();

    for criterion in criteria.iter_mut() {
        if criterion.checked {
            continue;
        }
        // Check if any linked issue that is Done matches this criterion
        for issue in &issues {
            let is_done = queue_items.iter().any(|q| {
                q.work_id.ends_with(&format!(":{}", issue.issue_number))
                    && q.work_id.starts_with("issue:")
                    && q.phase == QueuePhase::Done
            });
            if is_done {
                // Check if the issue title relates to this criterion (fuzzy match)
                if let Some(item) = queue_items.iter().find(|q| {
                    q.work_id.ends_with(&format!(":{}", issue.issue_number))
                        && q.work_id.starts_with("issue:")
                }) {
                    if let Some(ref title) = item.title {
                        let criterion_lower = criterion.text.to_lowercase();
                        let title_lower = title.to_lowercase();
                        // Simple keyword overlap: check if significant words match
                        let criterion_words: Vec<&str> = criterion_lower
                            .split_whitespace()
                            .filter(|w| w.len() > 3)
                            .collect();
                        let matched = criterion_words
                            .iter()
                            .any(|word| title_lower.contains(word));
                        if matched {
                            criterion.checked = true;
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// Verify acceptance criteria for a spec.
///
/// Parses the spec's `acceptance_criteria` field as a markdown checklist,
/// cross-references with linked issues, and returns a verification result.
pub fn verify_acceptance_criteria(db: &Database, id: &str) -> Result<VerifyResult> {
    let spec = db
        .spec_show(id)?
        .ok_or_else(|| anyhow::anyhow!("spec not found: {id}"))?;

    let ac_text = spec.acceptance_criteria.as_deref().unwrap_or("");

    let mut criteria = parse_acceptance_criteria(ac_text);

    if criteria.is_empty() && !ac_text.is_empty() {
        // If no checklist items found, treat each non-empty line as an unchecked criterion
        for line in ac_text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("---") {
                criteria.push(AcceptanceCriterion {
                    text: trimmed.to_string(),
                    checked: false,
                });
            }
        }
    }

    // Cross-reference with linked issues
    cross_reference_with_issues(&mut criteria, db, id);

    let met = criteria.iter().filter(|c| c.checked).count();
    let unmet = criteria.len() - met;

    Ok(VerifyResult {
        spec_id: id.to_string(),
        spec_title: spec.title.clone(),
        total: criteria.len(),
        met,
        unmet,
        criteria,
    })
}

/// Format unmet acceptance criteria as a GitHub issue body.
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

/// Check whether a command string starts with an allowed test runner prefix.
fn is_allowed_test_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    ALLOWED_TEST_COMMAND_PREFIXES
        .iter()
        .any(|prefix| trimmed == *prefix || trimmed.starts_with(&format!("{prefix} ")))
}

/// Read up to `limit` bytes from a reader, discarding the rest.
fn read_limited(reader: &mut dyn std::io::Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(limit.min(8192));
    reader.take(limit as u64).read_to_end(&mut buf)?;
    Ok(buf)
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

    #[test]
    fn parse_acceptance_criteria_checklist() {
        let text =
            "- [ ] API endpoint returns 200\n- [x] Database migration applied\n- [ ] Error handling for edge cases";
        let criteria = parse_acceptance_criteria(text);
        assert_eq!(criteria.len(), 3);
        assert!(!criteria[0].checked);
        assert_eq!(criteria[0].text, "API endpoint returns 200");
        assert!(criteria[1].checked);
        assert_eq!(criteria[1].text, "Database migration applied");
        assert!(!criteria[2].checked);
    }

    #[test]
    fn parse_acceptance_criteria_star_bullet() {
        let text = "* [ ] First criterion\n* [X] Second criterion";
        let criteria = parse_acceptance_criteria(text);
        assert_eq!(criteria.len(), 2);
        assert!(!criteria[0].checked);
        assert!(criteria[1].checked);
    }

    #[test]
    fn parse_acceptance_criteria_ignores_non_checklist() {
        let text = "Some random text\n## Header\n- [ ] Valid item\n- Regular list item";
        let criteria = parse_acceptance_criteria(text);
        assert_eq!(criteria.len(), 1);
        assert_eq!(criteria[0].text, "Valid item");
    }

    #[test]
    fn parse_acceptance_criteria_empty() {
        let criteria = parse_acceptance_criteria("");
        assert!(criteria.is_empty());
    }

    #[test]
    fn verify_result_display() {
        let result = VerifyResult {
            spec_id: "spec-1".to_string(),
            spec_title: "Test Spec".to_string(),
            total: 3,
            met: 1,
            unmet: 2,
            criteria: vec![
                AcceptanceCriterion {
                    text: "Done item".to_string(),
                    checked: true,
                },
                AcceptanceCriterion {
                    text: "Pending item".to_string(),
                    checked: false,
                },
            ],
        };
        let output = result.to_string();
        assert!(output.contains("1/3 met"));
        assert!(output.contains("[x] Done item"));
        assert!(output.contains("[ ] Pending item"));
    }

    #[test]
    fn verify_result_unmet_criteria() {
        let result = VerifyResult {
            spec_id: "s1".to_string(),
            spec_title: "T".to_string(),
            total: 2,
            met: 1,
            unmet: 1,
            criteria: vec![
                AcceptanceCriterion {
                    text: "met".to_string(),
                    checked: true,
                },
                AcceptanceCriterion {
                    text: "unmet".to_string(),
                    checked: false,
                },
            ],
        };
        let unmet = result.unmet_criteria();
        assert_eq!(unmet.len(), 1);
        assert_eq!(unmet[0].text, "unmet");
    }

    #[test]
    fn truncate_chars_ascii() {
        assert_eq!(truncate_chars("hello world", 5), "hello...");
        assert_eq!(truncate_chars("hello", 5), "hello");
        assert_eq!(truncate_chars("hi", 5), "hi");
    }

    #[test]
    fn truncate_chars_multibyte_korean() {
        let korean = "한글 테스트 문자열입니다";
        let result = truncate_chars(korean, 5);
        assert_eq!(result, "한글 테스...");
        // Must not panic
    }

    #[test]
    fn truncate_chars_emoji() {
        let emoji = "🎉🎊🎈🎁🎀🎆🎇";
        let result = truncate_chars(emoji, 3);
        assert_eq!(result, "🎉🎊🎈...");
    }

    #[test]
    fn parse_criteria_basic() {
        let ac = "- criterion one\n- criterion two\n* criterion three\ncriterion four\n\n";
        let criteria = parse_criteria(ac);
        assert_eq!(criteria.len(), 4);
        assert_eq!(criteria[0], "criterion one");
        assert_eq!(criteria[1], "criterion two");
        assert_eq!(criteria[2], "criterion three");
        assert_eq!(criteria[3], "criterion four");
    }

    #[test]
    fn parse_criteria_empty() {
        assert!(parse_criteria("").is_empty());
        assert!(parse_criteria("   \n  \n").is_empty());
    }
}
