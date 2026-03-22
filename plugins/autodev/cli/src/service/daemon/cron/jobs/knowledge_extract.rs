//! knowledge-extract cron job — Find unextracted merged PRs.
//!
//! Per-repo cron job that identifies completed queue items (merged PRs)
//! that haven't had knowledge extraction run yet.
//!
//! The actual extraction is performed by the shell script
//! (`knowledge-extract.sh`) which invokes `autodev agent`.
//! This module provides the pre-check logic to determine if extraction
//! is needed.

use crate::core::models::{QueuePhase, QueueType};
use crate::core::repository::QueueRepository;

/// Count of unextracted completed items for a repo.
#[derive(Debug)]
pub struct KnowledgeExtractCheck {
    /// Number of completed PR items that may need extraction.
    pub completed_pr_count: usize,
    /// Total completed items (issues + PRs).
    pub total_completed_count: usize,
}

/// Check how many completed (Done) PR queue items exist for a repo.
///
/// Knowledge extraction targets completed PRs. This function counts
/// items in Done phase with PR queue type, which the shell script
/// can use as a guard to decide whether to run extraction.
pub fn check_unextracted<D: QueueRepository>(
    db: &D,
    repo_id: &str,
) -> anyhow::Result<KnowledgeExtractCheck> {
    let items = db.queue_list_items(None)?;

    let completed: Vec<_> = items
        .iter()
        .filter(|item| item.repo_id == repo_id && item.phase == QueuePhase::Done)
        .collect();

    let pr_count = completed
        .iter()
        .filter(|item| item.queue_type == QueueType::Pr)
        .count();

    Ok(KnowledgeExtractCheck {
        completed_pr_count: pr_count,
        total_completed_count: completed.len(),
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
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();
        (dir, db, repo_id)
    }

    fn add_item(db: &Database, repo_id: &str, num: i64, qt: QueueType, phase: QueuePhase) {
        db.queue_upsert(&QueueItemRow {
            work_id: format!("work-{num}"),
            repo_id: repo_id.to_string(),
            queue_type: qt,
            phase,
            title: Some("test".to_string()),
            skip_reason: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            task_kind: TaskKind::Analyze,
            github_number: num,
            metadata_json: None,
            failure_count: 0,
            escalation_level: 0,
        })
        .unwrap();
    }

    #[test]
    fn no_items_returns_zero() {
        let (_dir, db, repo_id) = setup();
        let result = check_unextracted(&db, &repo_id).unwrap();
        assert_eq!(result.completed_pr_count, 0);
        assert_eq!(result.total_completed_count, 0);
    }

    #[test]
    fn counts_done_prs() {
        let (_dir, db, repo_id) = setup();
        add_item(&db, &repo_id, 1, QueueType::Pr, QueuePhase::Done);
        add_item(&db, &repo_id, 2, QueueType::Pr, QueuePhase::Done);
        add_item(&db, &repo_id, 3, QueueType::Issue, QueuePhase::Done);

        let result = check_unextracted(&db, &repo_id).unwrap();
        assert_eq!(result.completed_pr_count, 2);
        assert_eq!(result.total_completed_count, 3);
    }

    #[test]
    fn ignores_running_and_pending_items() {
        let (_dir, db, repo_id) = setup();
        add_item(&db, &repo_id, 1, QueueType::Pr, QueuePhase::Running);
        add_item(&db, &repo_id, 2, QueueType::Pr, QueuePhase::Pending);
        add_item(&db, &repo_id, 3, QueueType::Pr, QueuePhase::Done);

        let result = check_unextracted(&db, &repo_id).unwrap();
        assert_eq!(result.completed_pr_count, 1);
    }

    #[test]
    fn ignores_other_repos() {
        let (_dir, db, repo_id) = setup();
        let other = db
            .repo_add("https://github.com/org/other", "org/other")
            .unwrap();
        add_item(&db, &repo_id, 1, QueueType::Pr, QueuePhase::Done);
        add_item(&db, &other, 2, QueueType::Pr, QueuePhase::Done);

        let result = check_unextracted(&db, &repo_id).unwrap();
        assert_eq!(result.completed_pr_count, 1);
    }
}
