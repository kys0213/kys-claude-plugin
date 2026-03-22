//! PrCompleter — PR 큐 아이템의 완료 처리.
//!
//! 완료 동작:
//! 1. 검증 게이트: review_iteration >= 1 확인
//! 2. PR merge
//! 3. 소스 이슈 close + autodev:done 라벨
//! 4. PR에 autodev:done 라벨 추가

use std::sync::Arc;

use async_trait::async_trait;

use crate::core::completer::{CompleteResult, Completer};
use crate::core::labels;
use crate::core::queue_item::QueueItem;
use crate::infra::gh::Gh;

/// PR 완료 처리 구현체.
pub struct PrCompleter {
    gh: Arc<dyn Gh>,
}

impl PrCompleter {
    pub fn new(gh: Arc<dyn Gh>) -> Self {
        Self { gh }
    }
}

#[async_trait]
impl Completer for PrCompleter {
    async fn complete(&self, item: &QueueItem) -> CompleteResult {
        let gh_host = item.gh_host.as_deref();
        let repo = &item.repo_name;
        let number = item.github_number;

        // 1. 검증 게이트: review_iteration >= 1
        let review_iteration = item.review_iteration_or_zero();
        if review_iteration < 1 {
            return CompleteResult::ReviewRequired {
                reason: format!(
                    "PR #{number} has review_iteration={review_iteration}, requires >= 1"
                ),
            };
        }

        // 2. PR merge
        let merged = self.gh.pr_merge(repo, number, gh_host).await;
        if !merged {
            return CompleteResult::EscalationNeeded {
                reason: format!("PR #{number} merge failed"),
            };
        }

        // 3. PR에 autodev:done 라벨 추가 (add-first)
        self.gh.label_add(repo, number, labels::DONE, gh_host).await;

        // 4. autodev:wip 라벨 제거
        self.gh
            .label_remove(repo, number, labels::WIP, gh_host)
            .await;

        // 5. iteration 라벨 정리
        if review_iteration > 0 {
            self.gh
                .label_remove(
                    repo,
                    number,
                    &labels::iteration_label(review_iteration),
                    gh_host,
                )
                .await;
        }

        // 6. 소스 이슈 완료 처리
        if let Some(issue_num) = item.source_issue_number() {
            self.gh
                .label_add(repo, issue_num, labels::DONE, gh_host)
                .await;
            self.gh
                .label_remove(repo, issue_num, labels::IMPLEMENTING, gh_host)
                .await;
            self.gh.issue_close(repo, issue_num, gh_host).await;
        }

        CompleteResult::Completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::phase::TaskKind;
    use crate::core::queue_item::testing::*;
    use crate::infra::gh::mock::MockGh;

    #[tokio::test]
    async fn complete_rejects_pr_without_review() {
        let gh = Arc::new(MockGh::new());
        // review_iteration = 0
        let item = test_pr_with_source(10, TaskKind::Review, Some(42), 0);

        let completer = PrCompleter::new(gh.clone());
        let result = completer.complete(&item).await;

        assert!(matches!(result, CompleteResult::ReviewRequired { .. }));

        // merge 호출되지 않아야 함
        let merged = gh.merged_prs.lock().unwrap();
        assert!(merged.is_empty());
    }

    #[tokio::test]
    async fn complete_merges_pr_with_review() {
        let gh = Arc::new(MockGh::new());
        // review_iteration = 1
        let item = test_pr_with_source(10, TaskKind::Review, Some(42), 1);

        let completer = PrCompleter::new(gh.clone());
        let result = completer.complete(&item).await;

        assert_eq!(result, CompleteResult::Completed);

        // merge 호출 확인
        let merged = gh.merged_prs.lock().unwrap();
        assert!(merged.iter().any(|(_, n)| *n == 10));

        // PR done 라벨 확인
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::DONE));

        // 소스 이슈 done 라벨 확인
        assert!(added.iter().any(|(_, n, l)| *n == 42 && l == labels::DONE));

        // 소스 이슈 close 확인
        let closed = gh.closed_issues.lock().unwrap();
        assert!(closed.iter().any(|(_, n)| *n == 42));
    }

    #[tokio::test]
    async fn complete_handles_pr_without_source_issue() {
        let gh = Arc::new(MockGh::new());
        // review_iteration = 2, no source issue
        let item = test_pr_with_source(10, TaskKind::Review, None, 2);

        let completer = PrCompleter::new(gh.clone());
        let result = completer.complete(&item).await;

        assert_eq!(result, CompleteResult::Completed);

        // merge 호출 확인
        let merged = gh.merged_prs.lock().unwrap();
        assert!(merged.iter().any(|(_, n)| *n == 10));

        // 소스 이슈 close 호출되지 않아야 함
        let closed = gh.closed_issues.lock().unwrap();
        assert!(closed.is_empty());
    }

    #[tokio::test]
    async fn complete_uses_add_first_ordering() {
        let gh = Arc::new(MockGh::new());
        let item = test_pr_with_source(10, TaskKind::Review, Some(42), 1);

        let completer = PrCompleter::new(gh.clone());
        completer.complete(&item).await;

        // PR: done before wip removal
        gh.assert_add_before_remove(10, labels::DONE, labels::WIP);
        // Source issue: done before implementing removal
        gh.assert_add_before_remove(42, labels::DONE, labels::IMPLEMENTING);
    }

    #[tokio::test]
    async fn complete_cleans_up_iteration_label() {
        let gh = Arc::new(MockGh::new());
        let item = test_pr_with_source(10, TaskKind::Review, None, 3);

        let completer = PrCompleter::new(gh.clone());
        completer.complete(&item).await;

        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, n, l)| *n == 10 && l == &labels::iteration_label(3)));
    }
}
