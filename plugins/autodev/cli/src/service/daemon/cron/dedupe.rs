//! Gap-detection dedupe guard.
//!
//! Before gap-detection creates a new issue, this guard checks whether
//! an open queue item already covers the same gap. This prevents
//! infinite issue proliferation from repeated cron runs.
//!
//! ## Flow
//!
//! ```text
//! gap detected → query open items (Pending/Ready/Running)
//!   → matching item exists → skip (already in progress)
//!   → no match → proceed with issue creation
//! ```

use crate::core::models::QueuePhase;
use crate::core::repository::QueueRepository;

/// A gap discovered by gap-detection.
///
/// Contains the information needed for dedupe comparison against
/// existing queue items.
#[derive(Debug, Clone)]
pub struct Gap {
    /// Repository ID this gap belongs to.
    pub repo_id: String,
    /// Human-readable title describing the gap.
    pub title: String,
}

/// Result of a dedupe check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DedupeResult {
    /// No matching open item found — safe to create a new issue.
    Unique,
    /// An existing open item already covers this gap.
    Duplicate {
        /// The work_id of the matching item.
        existing_work_id: String,
    },
}

/// Check whether a gap is already covered by an open queue item.
///
/// Queries open items (Pending, Ready, Running) for the given repo and
/// compares their titles against the gap title using normalized matching.
///
/// Returns `DedupeResult::Duplicate` if a matching item is found,
/// `DedupeResult::Unique` otherwise.
pub fn check_duplicate<DB: QueueRepository>(db: &DB, gap: &Gap) -> DedupeResult {
    let active_items = match db.queue_load_active(&gap.repo_id) {
        Ok(items) => items,
        Err(e) => {
            tracing::warn!(
                "dedupe: failed to load active items for repo {}: {e}",
                gap.repo_id
            );
            // Fail-open: if we cannot query, allow issue creation
            // to avoid silently dropping gaps.
            return DedupeResult::Unique;
        }
    };

    let normalized_gap = normalize(&gap.title);

    for item in &active_items {
        if !is_open_phase(item.phase) {
            continue;
        }

        if let Some(ref item_title) = item.title {
            if titles_match(&normalized_gap, &normalize(item_title)) {
                tracing::info!(
                    "dedupe: gap '{}' matches existing item '{}' (work_id={})",
                    gap.title,
                    item_title,
                    item.work_id,
                );
                return DedupeResult::Duplicate {
                    existing_work_id: item.work_id.clone(),
                };
            }
        }
    }

    DedupeResult::Unique
}

/// Check multiple gaps at once, returning only the unique ones.
///
/// Convenience wrapper that filters out duplicates and logs skipped gaps.
pub fn filter_unique<DB: QueueRepository>(db: &DB, gaps: Vec<Gap>) -> Vec<Gap> {
    gaps.into_iter()
        .filter(|gap| {
            let result = check_duplicate(db, gap);
            match result {
                DedupeResult::Unique => true,
                DedupeResult::Duplicate { existing_work_id } => {
                    tracing::info!(
                        "dedupe: skipping gap '{}' — already tracked by {existing_work_id}",
                        gap.title,
                    );
                    false
                }
            }
        })
        .collect()
}

/// Whether the phase represents an "open" (in-progress) item.
fn is_open_phase(phase: QueuePhase) -> bool {
    match phase {
        QueuePhase::Pending
        | QueuePhase::Ready
        | QueuePhase::Running
        | QueuePhase::Completed
        | QueuePhase::Hitl => true,
        QueuePhase::Done | QueuePhase::Skipped | QueuePhase::Failed => false,
    }
}

/// Normalize a title for comparison.
///
/// Lowercases, collapses whitespace, and strips leading/trailing whitespace.
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Determine whether two normalized titles refer to the same gap.
///
/// Uses exact match on normalized form. This is intentionally strict
/// to avoid false-positive deduplication. LLM-based similarity can be
/// added as a future enhancement if needed.
fn titles_match(a: &str, b: &str) -> bool {
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{QueueItemRow, QueuePhase, QueueType};
    use crate::core::phase::TaskKind;

    // ─── In-memory QueueRepository stub ───

    struct StubQueueRepo {
        items: Vec<QueueItemRow>,
    }

    impl StubQueueRepo {
        fn new(items: Vec<QueueItemRow>) -> Self {
            Self { items }
        }

        fn empty() -> Self {
            Self { items: vec![] }
        }
    }

    impl QueueRepository for StubQueueRepo {
        fn queue_get_phase(&self, _work_id: &str) -> anyhow::Result<Option<QueuePhase>> {
            Ok(None)
        }
        fn queue_advance(&self, _work_id: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn queue_skip(&self, _work_id: &str, _reason: Option<&str>) -> anyhow::Result<()> {
            Ok(())
        }
        fn queue_list_items(&self, _repo: Option<&str>) -> anyhow::Result<Vec<QueueItemRow>> {
            Ok(self.items.clone())
        }
        fn queue_upsert(&self, _item: &QueueItemRow) -> anyhow::Result<()> {
            Ok(())
        }
        fn queue_remove(&self, _work_id: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn queue_load_active(&self, repo_id: &str) -> anyhow::Result<Vec<QueueItemRow>> {
            Ok(self
                .items
                .iter()
                .filter(|i| i.repo_id == repo_id)
                .cloned()
                .collect())
        }
        fn queue_transit(
            &self,
            _work_id: &str,
            _from: QueuePhase,
            _to: QueuePhase,
        ) -> anyhow::Result<bool> {
            Ok(true)
        }
        fn queue_get_item(&self, _work_id: &str) -> anyhow::Result<Option<QueueItemRow>> {
            Ok(None)
        }
        fn queue_increment_failure(&self, _work_id: &str) -> anyhow::Result<i32> {
            Ok(0)
        }
        fn queue_get_failure_count(&self, _work_id: &str) -> anyhow::Result<i32> {
            Ok(0)
        }
    }

    fn make_item(work_id: &str, repo_id: &str, title: &str, phase: QueuePhase) -> QueueItemRow {
        QueueItemRow {
            work_id: work_id.to_string(),
            repo_id: repo_id.to_string(),
            queue_type: QueueType::Issue,
            phase,
            title: Some(title.to_string()),
            skip_reason: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            task_kind: TaskKind::Analyze,
            github_number: 1,
            metadata_json: None,
            failure_count: 0,
            escalation_level: 0,
        }
    }

    // ═══════════════════════════════════════════════
    // check_duplicate tests
    // ═══════════════════════════════════════════════

    #[test]
    fn unique_when_no_active_items() {
        let db = StubQueueRepo::empty();
        let gap = Gap {
            repo_id: "r1".into(),
            title: "Missing auth validation".into(),
        };
        assert_eq!(check_duplicate(&db, &gap), DedupeResult::Unique);
    }

    #[test]
    fn unique_when_no_matching_title() {
        let db = StubQueueRepo::new(vec![make_item(
            "issue:org/repo:1",
            "r1",
            "Fix login bug",
            QueuePhase::Pending,
        )]);
        let gap = Gap {
            repo_id: "r1".into(),
            title: "Missing auth validation".into(),
        };
        assert_eq!(check_duplicate(&db, &gap), DedupeResult::Unique);
    }

    #[test]
    fn duplicate_when_exact_title_match_pending() {
        let db = StubQueueRepo::new(vec![make_item(
            "issue:org/repo:1",
            "r1",
            "Missing auth validation",
            QueuePhase::Pending,
        )]);
        let gap = Gap {
            repo_id: "r1".into(),
            title: "Missing auth validation".into(),
        };
        assert_eq!(
            check_duplicate(&db, &gap),
            DedupeResult::Duplicate {
                existing_work_id: "issue:org/repo:1".into(),
            }
        );
    }

    #[test]
    fn duplicate_when_exact_title_match_ready() {
        let db = StubQueueRepo::new(vec![make_item(
            "issue:org/repo:2",
            "r1",
            "Missing auth validation",
            QueuePhase::Ready,
        )]);
        let gap = Gap {
            repo_id: "r1".into(),
            title: "Missing auth validation".into(),
        };
        assert_eq!(
            check_duplicate(&db, &gap),
            DedupeResult::Duplicate {
                existing_work_id: "issue:org/repo:2".into(),
            }
        );
    }

    #[test]
    fn duplicate_when_exact_title_match_running() {
        let db = StubQueueRepo::new(vec![make_item(
            "issue:org/repo:3",
            "r1",
            "Missing auth validation",
            QueuePhase::Running,
        )]);
        let gap = Gap {
            repo_id: "r1".into(),
            title: "Missing auth validation".into(),
        };
        assert_eq!(
            check_duplicate(&db, &gap),
            DedupeResult::Duplicate {
                existing_work_id: "issue:org/repo:3".into(),
            }
        );
    }

    #[test]
    fn unique_when_item_is_done() {
        let db = StubQueueRepo::new(vec![make_item(
            "issue:org/repo:1",
            "r1",
            "Missing auth validation",
            QueuePhase::Done,
        )]);
        let gap = Gap {
            repo_id: "r1".into(),
            title: "Missing auth validation".into(),
        };
        // Done items are excluded by queue_load_active, but the phase
        // filter also handles it as defense-in-depth.
        assert_eq!(check_duplicate(&db, &gap), DedupeResult::Unique);
    }

    #[test]
    fn unique_when_item_is_skipped() {
        let db = StubQueueRepo::new(vec![make_item(
            "issue:org/repo:1",
            "r1",
            "Missing auth validation",
            QueuePhase::Skipped,
        )]);
        let gap = Gap {
            repo_id: "r1".into(),
            title: "Missing auth validation".into(),
        };
        assert_eq!(check_duplicate(&db, &gap), DedupeResult::Unique);
    }

    #[test]
    fn duplicate_case_insensitive() {
        let db = StubQueueRepo::new(vec![make_item(
            "issue:org/repo:1",
            "r1",
            "Missing Auth Validation",
            QueuePhase::Pending,
        )]);
        let gap = Gap {
            repo_id: "r1".into(),
            title: "missing auth validation".into(),
        };
        assert_eq!(
            check_duplicate(&db, &gap),
            DedupeResult::Duplicate {
                existing_work_id: "issue:org/repo:1".into(),
            }
        );
    }

    #[test]
    fn duplicate_with_whitespace_normalization() {
        let db = StubQueueRepo::new(vec![make_item(
            "issue:org/repo:1",
            "r1",
            "Missing  auth   validation",
            QueuePhase::Pending,
        )]);
        let gap = Gap {
            repo_id: "r1".into(),
            title: "Missing auth validation".into(),
        };
        assert_eq!(
            check_duplicate(&db, &gap),
            DedupeResult::Duplicate {
                existing_work_id: "issue:org/repo:1".into(),
            }
        );
    }

    #[test]
    fn unique_across_different_repos() {
        let db = StubQueueRepo::new(vec![make_item(
            "issue:org/other:1",
            "r2",
            "Missing auth validation",
            QueuePhase::Pending,
        )]);
        let gap = Gap {
            repo_id: "r1".into(),
            title: "Missing auth validation".into(),
        };
        // queue_load_active filters by repo_id, so r2 items are not returned for r1
        assert_eq!(check_duplicate(&db, &gap), DedupeResult::Unique);
    }

    // ═══════════════════════════════════════════════
    // filter_unique tests
    // ═══════════════════════════════════════════════

    #[test]
    fn filter_unique_removes_duplicates() {
        let db = StubQueueRepo::new(vec![make_item(
            "issue:org/repo:1",
            "r1",
            "Missing auth validation",
            QueuePhase::Pending,
        )]);
        let gaps = vec![
            Gap {
                repo_id: "r1".into(),
                title: "Missing auth validation".into(),
            },
            Gap {
                repo_id: "r1".into(),
                title: "Add rate limiting".into(),
            },
        ];
        let unique = filter_unique(&db, gaps);
        assert_eq!(unique.len(), 1);
        assert_eq!(unique[0].title, "Add rate limiting");
    }

    #[test]
    fn filter_unique_keeps_all_when_no_duplicates() {
        let db = StubQueueRepo::empty();
        let gaps = vec![
            Gap {
                repo_id: "r1".into(),
                title: "Gap A".into(),
            },
            Gap {
                repo_id: "r1".into(),
                title: "Gap B".into(),
            },
        ];
        let unique = filter_unique(&db, gaps);
        assert_eq!(unique.len(), 2);
    }

    // ═══════════════════════════════════════════════
    // normalize / titles_match tests
    // ═══════════════════════════════════════════════

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize("Hello WORLD"), "hello world");
    }

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize("a   b\t\nc"), "a b c");
    }

    #[test]
    fn normalize_trims() {
        assert_eq!(normalize("  hello  "), "hello");
    }

    #[test]
    fn titles_match_exact() {
        assert!(titles_match("hello world", "hello world"));
    }

    #[test]
    fn titles_match_different() {
        assert!(!titles_match("hello world", "goodbye world"));
    }

    #[test]
    fn is_open_phase_correctness() {
        assert!(is_open_phase(QueuePhase::Pending));
        assert!(is_open_phase(QueuePhase::Ready));
        assert!(is_open_phase(QueuePhase::Running));
        assert!(!is_open_phase(QueuePhase::Done));
        assert!(!is_open_phase(QueuePhase::Skipped));
    }
}
