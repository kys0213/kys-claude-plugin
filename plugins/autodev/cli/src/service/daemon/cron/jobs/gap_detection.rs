//! gap-detection cron job — Spec vs code gap detection with dedupe guard.
//!
//! Per-repo cron job that:
//! 1. Finds active specs for a given repo
//! 2. Checks for open queue items to prevent duplicate gap issues (dedupe)
//! 3. Returns specs that need gap detection (have no open items covering them)
//!
//! The actual spec-vs-code comparison is performed by the shell script
//! (`gap-detection.sh`) which calls `autodev spec verify` and `autodev agent`.
//! This module provides the dedupe guard logic in Rust.

use std::collections::HashSet;

use crate::core::models::{QueuePhase, Spec, SpecStatus};
use crate::core::repository::{QueueRepository, SpecRepository};

/// Result of gap-detection pre-check for a single repo.
#[derive(Debug)]
pub struct GapDetectionResult {
    /// Active specs that have no open queue items (safe to run gap detection).
    pub actionable_specs: Vec<Spec>,
    /// Active specs that already have open queue items (skipped by dedupe).
    pub skipped_specs: Vec<String>,
}

/// Performs dedupe check for gap-detection: returns specs that don't already
/// have open (Pending/Ready/Running) queue items linked to them.
///
/// This prevents the gap-detection cron from creating duplicate issues
/// for gaps that are already being worked on.
pub fn filter_actionable_specs<D: SpecRepository + QueueRepository>(
    db: &D,
    repo_id: &str,
) -> anyhow::Result<GapDetectionResult> {
    // 1. Get active specs for this repo
    let all_specs = db.spec_list_by_status(SpecStatus::Active)?;
    let repo_specs: Vec<Spec> = all_specs
        .into_iter()
        .filter(|s| s.repo_id == repo_id)
        .collect();

    if repo_specs.is_empty() {
        return Ok(GapDetectionResult {
            actionable_specs: Vec::new(),
            skipped_specs: Vec::new(),
        });
    }

    // 2. Get all spec-issue mappings
    let spec_issues = db.spec_issues_all()?;

    // 3. Get open queue items for this repo
    let queue_items = db.queue_list_items(None)?;
    let open_numbers: HashSet<i64> = queue_items
        .iter()
        .filter(|item| {
            item.repo_id == repo_id
                && matches!(
                    item.phase,
                    QueuePhase::Pending | QueuePhase::Ready | QueuePhase::Running
                )
        })
        .map(|item| item.github_number)
        .collect();

    // 4. For each active spec, check if any linked issue is still open
    let mut actionable = Vec::new();
    let mut skipped = Vec::new();

    for spec in repo_specs {
        let linked_issues = spec_issues.get(&spec.id);
        let has_open_item = linked_issues.is_some_and(|issues| {
            issues
                .iter()
                .any(|si| open_numbers.contains(&si.issue_number))
        });

        if has_open_item {
            skipped.push(spec.id.clone());
        } else {
            actionable.push(spec);
        }
    }

    Ok(GapDetectionResult {
        actionable_specs: actionable,
        skipped_specs: skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::*;
    use crate::core::phase::TaskKind;
    use crate::core::repository::*;
    use crate::infra::db::Database;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Database, String) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        db.initialize().unwrap();
        let repo_id = db
            .workspace_add("https://github.com/org/repo", "org/repo")
            .unwrap();
        (dir, db, repo_id)
    }

    fn add_spec(db: &Database, repo_id: &str, title: &str) -> String {
        db.spec_add(&NewSpec {
            repo_id: repo_id.to_string(),
            title: title.to_string(),
            body: "spec body".to_string(),
            source_path: None,
            test_commands: None,
            acceptance_criteria: None,
        })
        .unwrap()
    }

    fn add_queue_item(db: &Database, repo_id: &str, github_number: i64, phase: QueuePhase) {
        let item = QueueItemRow {
            work_id: format!("work-{github_number}"),
            repo_id: repo_id.to_string(),
            queue_type: QueueType::Issue,
            phase,
            title: Some("test item".to_string()),
            skip_reason: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            task_kind: TaskKind::Analyze,
            github_number,
            metadata_json: None,
            failure_count: 0,
            escalation_level: 0,
        };
        db.queue_upsert(&item).unwrap();
    }

    #[test]
    fn no_active_specs_returns_empty() {
        let (_dir, db, repo_id) = setup();

        let result = filter_actionable_specs(&db, &repo_id).unwrap();
        assert!(result.actionable_specs.is_empty());
        assert!(result.skipped_specs.is_empty());
    }

    #[test]
    fn active_spec_without_open_items_is_actionable() {
        let (_dir, db, repo_id) = setup();
        add_spec(&db, &repo_id, "Feature A");

        let result = filter_actionable_specs(&db, &repo_id).unwrap();
        assert_eq!(result.actionable_specs.len(), 1);
        assert_eq!(result.actionable_specs[0].title, "Feature A");
        assert!(result.skipped_specs.is_empty());
    }

    #[test]
    fn spec_with_open_queue_item_is_skipped() {
        let (_dir, db, repo_id) = setup();
        let spec_id = add_spec(&db, &repo_id, "Feature B");

        // Link issue #42 to this spec
        db.spec_link_issue(&spec_id, 42).unwrap();

        // Add an open queue item for issue #42
        add_queue_item(&db, &repo_id, 42, QueuePhase::Running);

        let result = filter_actionable_specs(&db, &repo_id).unwrap();
        assert!(result.actionable_specs.is_empty());
        assert_eq!(result.skipped_specs.len(), 1);
        assert_eq!(result.skipped_specs[0], spec_id);
    }

    #[test]
    fn spec_with_done_queue_item_is_actionable() {
        let (_dir, db, repo_id) = setup();
        let spec_id = add_spec(&db, &repo_id, "Feature C");

        // Link issue #43 to this spec
        db.spec_link_issue(&spec_id, 43).unwrap();

        // The queue item is Done — no longer open
        add_queue_item(&db, &repo_id, 43, QueuePhase::Done);

        let result = filter_actionable_specs(&db, &repo_id).unwrap();
        assert_eq!(result.actionable_specs.len(), 1);
        assert!(result.skipped_specs.is_empty());
    }

    #[test]
    fn mixed_specs_correctly_partitioned() {
        let (_dir, db, repo_id) = setup();
        let spec_a = add_spec(&db, &repo_id, "Feature A");
        let spec_b = add_spec(&db, &repo_id, "Feature B");
        let _spec_c = add_spec(&db, &repo_id, "Feature C");

        // spec_a has open item
        db.spec_link_issue(&spec_a, 10).unwrap();
        add_queue_item(&db, &repo_id, 10, QueuePhase::Pending);

        // spec_b has done item
        db.spec_link_issue(&spec_b, 20).unwrap();
        add_queue_item(&db, &repo_id, 20, QueuePhase::Done);

        // spec_c has no linked items at all

        let result = filter_actionable_specs(&db, &repo_id).unwrap();
        assert_eq!(result.actionable_specs.len(), 2); // B and C
        assert_eq!(result.skipped_specs.len(), 1); // A
        assert_eq!(result.skipped_specs[0], spec_a);
    }

    #[test]
    fn ignores_specs_from_other_repos() {
        let (_dir, db, repo_id) = setup();
        let other_repo_id = db
            .workspace_add("https://github.com/org/other", "org/other")
            .unwrap();

        add_spec(&db, &repo_id, "My Spec");
        add_spec(&db, &other_repo_id, "Other Spec");

        let result = filter_actionable_specs(&db, &repo_id).unwrap();
        assert_eq!(result.actionable_specs.len(), 1);
        assert_eq!(result.actionable_specs[0].title, "My Spec");
    }
}
