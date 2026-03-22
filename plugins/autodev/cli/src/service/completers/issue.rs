//! IssueCompleter — Issue 큐 아이템의 완료 처리.
//!
//! 완료 동작:
//! 1. GitHub 이슈 close
//! 2. `autodev:done` 라벨 추가
//! 3. `autodev:wip` 라벨 제거
//! 4. 완료 코멘트 게시

use std::sync::Arc;

use async_trait::async_trait;

use crate::core::completer::{CompleteResult, Completer};
use crate::core::labels;
use crate::core::queue_item::QueueItem;
use crate::infra::gh::Gh;

/// Issue 완료 처리 구현체.
pub struct IssueCompleter {
    gh: Arc<dyn Gh>,
}

impl IssueCompleter {
    pub fn new(gh: Arc<dyn Gh>) -> Self {
        Self { gh }
    }
}

#[async_trait]
impl Completer for IssueCompleter {
    async fn complete(&self, item: &QueueItem) -> CompleteResult {
        let gh_host = item.gh_host.as_deref();
        let repo = &item.repo_name;
        let number = item.github_number;

        // 1. add-first: autodev:done 라벨 추가
        self.gh.label_add(repo, number, labels::DONE, gh_host).await;

        // 2. autodev:wip 라벨 제거
        self.gh
            .label_remove(repo, number, labels::WIP, gh_host)
            .await;

        // 3. 완료 코멘트 게시
        let comment = format!(
            "<!-- autodev:done -->\n\
             ## Autodev: Issue #{number} completed\n\n\
             This issue has been processed and marked as done by autodev."
        );
        self.gh.issue_comment(repo, number, &comment, gh_host).await;

        // 4. 이슈 close
        self.gh.issue_close(repo, number, gh_host).await;

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
    async fn complete_adds_done_label_and_closes_issue() {
        let gh = Arc::new(MockGh::new());
        let item = test_issue(42, TaskKind::Analyze);

        let completer = IssueCompleter::new(gh.clone());
        let result = completer.complete(&item).await;

        assert_eq!(result, CompleteResult::Completed);

        // done 라벨 추가 확인
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 42 && l == labels::DONE));

        // wip 라벨 제거 확인
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed.iter().any(|(_, n, l)| *n == 42 && l == labels::WIP));

        // 이슈 close 확인
        let closed = gh.closed_issues.lock().unwrap();
        assert!(closed.iter().any(|(_, n)| *n == 42));

        // 완료 코멘트 확인
        let comments = gh.posted_comments.lock().unwrap();
        assert!(comments
            .iter()
            .any(|(_, n, body)| *n == 42 && body.contains("<!-- autodev:done -->")));
    }

    #[tokio::test]
    async fn complete_uses_add_first_ordering() {
        let gh = Arc::new(MockGh::new());
        let item = test_issue(42, TaskKind::Analyze);

        let completer = IssueCompleter::new(gh.clone());
        completer.complete(&item).await;

        gh.assert_add_before_remove(42, labels::DONE, labels::WIP);
    }
}
