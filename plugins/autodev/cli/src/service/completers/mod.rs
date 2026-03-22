//! Completers — 데이터 소스별 완료 처리 구현체.
//!
//! `queue_type`에 따라 적절한 `Completer`를 선택하는 팩토리를 제공한다.

pub mod issue;
pub mod pr;
pub mod spec;

use std::sync::Arc;

use crate::core::completer::Completer;
use crate::core::models::QueueType;
use crate::core::repository::SpecRepository;
use crate::infra::gh::Gh;

use self::issue::IssueCompleter;
use self::pr::PrCompleter;
use self::spec::SpecCompleter;

/// `queue_type`에 따라 적절한 `Completer`를 선택한다.
///
/// - `Issue` → `IssueCompleter`
/// - `Pr` → `PrCompleter`
/// - `Knowledge` / `Agent` → `spec_repo` 제공 시 `SpecCompleter`, 없으면 `IssueCompleter` 폴백
pub fn select_completer(
    queue_type: &QueueType,
    gh: Arc<dyn Gh>,
    spec_repo: Option<Arc<dyn SpecRepository + Send + Sync>>,
) -> Box<dyn Completer> {
    match queue_type {
        QueueType::Issue => Box::new(IssueCompleter::new(gh)),
        QueueType::Pr => Box::new(PrCompleter::new(gh)),
        QueueType::Knowledge => Box::new(IssueCompleter::new(gh)),
        QueueType::Agent => {
            if let Some(sr) = spec_repo {
                Box::new(SpecCompleter::new(gh, sr))
            } else {
                Box::new(IssueCompleter::new(gh))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::phase::TaskKind;
    use crate::core::queue_item::testing::*;
    use crate::infra::gh::mock::MockGh;

    #[tokio::test]
    async fn select_issue_completer() {
        let gh = Arc::new(MockGh::new());
        let completer = select_completer(&QueueType::Issue, gh.clone(), None);

        let item = test_issue(42, TaskKind::Analyze);
        let result = completer.complete(&item).await;

        assert_eq!(result, crate::core::completer::CompleteResult::Completed);

        // Verify it called issue_close (IssueCompleter behavior)
        let closed = gh.closed_issues.lock().unwrap();
        assert!(closed.iter().any(|(_, n)| *n == 42));
    }

    #[tokio::test]
    async fn select_pr_completer_checks_review_gate() {
        let gh = Arc::new(MockGh::new());
        let completer = select_completer(&QueueType::Pr, gh.clone(), None);

        // review_iteration = 0 → should require review
        let item = test_pr_with_source(10, TaskKind::Review, Some(42), 0);
        let result = completer.complete(&item).await;

        assert!(matches!(
            result,
            crate::core::completer::CompleteResult::ReviewRequired { .. }
        ));
    }

    #[tokio::test]
    async fn select_pr_completer_completes_reviewed_pr() {
        let gh = Arc::new(MockGh::new());
        let completer = select_completer(&QueueType::Pr, gh.clone(), None);

        // review_iteration = 1 → should complete
        let item = test_pr_with_source(10, TaskKind::Review, Some(42), 1);
        let result = completer.complete(&item).await;

        assert_eq!(result, crate::core::completer::CompleteResult::Completed);

        let merged = gh.merged_prs.lock().unwrap();
        assert!(merged.iter().any(|(_, n)| *n == 10));
    }

    #[tokio::test]
    async fn select_knowledge_falls_back_to_issue() {
        let gh = Arc::new(MockGh::new());
        let completer = select_completer(&QueueType::Knowledge, gh.clone(), None);

        let item = test_issue(5, TaskKind::Extract);
        let result = completer.complete(&item).await;

        assert_eq!(result, crate::core::completer::CompleteResult::Completed);
    }
}
