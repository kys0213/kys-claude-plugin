use super::models::{QueueType, RepoIssue, RepoPull};
use super::phase::TaskKind;
use super::state_queue::HasWorkId;
use super::task_queues::make_work_id;

// ─── Repo Context ───

/// QueueItem 생성에 필요한 레포 공통 정보.
///
/// GitRepository에서 한번 생성하여 여러 QueueItem에 재사용한다.
#[derive(Debug, Clone)]
pub struct RepoRef {
    pub id: String,
    pub name: String,
    pub url: String,
    pub gh_host: Option<String>,
}

// ─── Unified In-Memory Work Item ───

/// 통합 큐 아이템.
///
/// 기존 IssueItem/PrItem을 단일 구조체로 통합한다.
/// `queue_type`으로 Issue/PR 구분, `task_kind`로 작업 종류 결정.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub work_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub repo_url: String,
    pub github_number: i64,
    pub queue_type: QueueType,
    pub task_kind: TaskKind,
    pub title: String,
    pub metadata: ItemMetadata,
    /// GHE hostname (e.g. "git.example.com"). None이면 github.com.
    pub gh_host: Option<String>,
}

/// Issue/PR 고유 메타데이터.
#[derive(Debug, Clone)]
pub enum ItemMetadata {
    Issue {
        body: Option<String>,
        labels: Vec<String>,
        author: String,
        analysis_report: Option<String>,
    },
    Pr {
        head_branch: String,
        base_branch: String,
        review_comment: Option<String>,
        source_issue_number: Option<i64>,
        review_iteration: u32,
    },
}

impl HasWorkId for QueueItem {
    fn work_id(&self) -> &str {
        &self.work_id
    }
}

impl QueueItem {
    /// queue_type + task_kind 매칭 (drain_to_filtered predicate용)
    pub fn is(&self, qt: QueueType, tk: TaskKind) -> bool {
        self.queue_type == qt && self.task_kind == tk
    }

    /// queue_type 매칭 (concurrency count 등)
    pub fn is_type(&self, qt: QueueType) -> bool {
        self.queue_type == qt
    }

    /// concurrency 슬롯을 소비하는 PR 작업인지 (Extract 제외)
    pub fn is_pr_concurrent(&self) -> bool {
        self.queue_type == QueueType::Pr && self.task_kind != TaskKind::Extract
    }

    /// Issue body (Issue일 때만)
    pub fn body(&self) -> Option<&str> {
        match &self.metadata {
            ItemMetadata::Issue { body, .. } => body.as_deref(),
            _ => None,
        }
    }

    /// Issue labels (Issue일 때만). PR이면 None.
    pub fn labels(&self) -> Option<&[String]> {
        match &self.metadata {
            ItemMetadata::Issue { labels, .. } => Some(labels),
            _ => None,
        }
    }

    /// Issue author (Issue일 때만). PR이면 None.
    pub fn author(&self) -> Option<&str> {
        match &self.metadata {
            ItemMetadata::Issue { author, .. } => Some(author),
            _ => None,
        }
    }

    /// Issue analysis report (Issue일 때만)
    pub fn analysis_report(&self) -> Option<&str> {
        match &self.metadata {
            ItemMetadata::Issue {
                analysis_report, ..
            } => analysis_report.as_deref(),
            _ => None,
        }
    }

    /// PR head branch (PR일 때만). Issue이거나 빈 문자열이면 None.
    pub fn head_branch(&self) -> Option<&str> {
        match &self.metadata {
            ItemMetadata::Pr { head_branch, .. } if !head_branch.is_empty() => Some(head_branch),
            _ => None,
        }
    }

    /// PR base branch (PR일 때만). Issue이거나 빈 문자열이면 None.
    pub fn base_branch(&self) -> Option<&str> {
        match &self.metadata {
            ItemMetadata::Pr { base_branch, .. } if !base_branch.is_empty() => Some(base_branch),
            _ => None,
        }
    }

    /// PR review comment (PR일 때만)
    pub fn review_comment(&self) -> Option<&str> {
        match &self.metadata {
            ItemMetadata::Pr { review_comment, .. } => review_comment.as_deref(),
            _ => None,
        }
    }

    /// PR source issue number (PR일 때만)
    pub fn source_issue_number(&self) -> Option<i64> {
        match &self.metadata {
            ItemMetadata::Pr {
                source_issue_number,
                ..
            } => *source_issue_number,
            _ => None,
        }
    }

    /// PR review iteration (PR일 때만). Issue이면 None.
    pub fn review_iteration(&self) -> Option<u32> {
        match &self.metadata {
            ItemMetadata::Pr {
                review_iteration, ..
            } => Some(*review_iteration),
            _ => None,
        }
    }

    /// Set review_comment on PR metadata
    pub fn set_review_comment(&mut self, comment: Option<String>) {
        if let ItemMetadata::Pr { review_comment, .. } = &mut self.metadata {
            *review_comment = comment;
        }
    }

    /// Increment review_iteration on PR metadata, returning the new value
    pub fn increment_review_iteration(&mut self) -> u32 {
        if let ItemMetadata::Pr {
            review_iteration, ..
        } = &mut self.metadata
        {
            *review_iteration += 1;
            *review_iteration
        } else {
            0
        }
    }

    /// Issue QueueItem 생성
    pub fn new_issue(
        repo: &RepoRef,
        github_number: i64,
        task_kind: TaskKind,
        title: String,
        body: Option<String>,
        labels: Vec<String>,
        author: String,
    ) -> Self {
        Self {
            work_id: make_work_id(QueueType::Issue, &repo.name, github_number),
            repo_id: repo.id.clone(),
            repo_name: repo.name.clone(),
            repo_url: repo.url.clone(),
            github_number,
            queue_type: QueueType::Issue,
            task_kind,
            title,
            metadata: ItemMetadata::Issue {
                body,
                labels,
                author,
                analysis_report: None,
            },
            gh_host: repo.gh_host.clone(),
        }
    }

    /// RepoIssue → QueueItem 변환 (scan/reconcile 공통 헬퍼)
    pub fn from_issue(repo: &RepoRef, issue: &RepoIssue, task_kind: TaskKind) -> Self {
        Self::new_issue(
            repo,
            issue.number,
            task_kind,
            issue.title.clone(),
            issue.body.clone(),
            issue.labels.clone(),
            issue.author.clone(),
        )
    }

    /// RepoPull → QueueItem 변환 (scan/reconcile 공통 헬퍼)
    pub fn from_pull(repo: &RepoRef, pull: &RepoPull, task_kind: TaskKind) -> Self {
        Self::new_pr(
            repo,
            pull.number,
            task_kind,
            pull.title.clone(),
            ItemMetadata::Pr {
                head_branch: pull.head_branch.clone(),
                base_branch: pull.base_branch.clone(),
                review_comment: None,
                source_issue_number: pull.source_issue_number(),
                review_iteration: pull.review_iteration(),
            },
        )
    }

    /// PR QueueItem 생성
    pub fn new_pr(
        repo: &RepoRef,
        github_number: i64,
        task_kind: TaskKind,
        title: String,
        metadata: ItemMetadata,
    ) -> Self {
        debug_assert!(matches!(metadata, ItemMetadata::Pr { .. }));
        Self {
            work_id: make_work_id(QueueType::Pr, &repo.name, github_number),
            repo_id: repo.id.clone(),
            repo_name: repo.name.clone(),
            repo_url: repo.url.clone(),
            github_number,
            queue_type: QueueType::Pr,
            task_kind,
            title,
            metadata,
            gh_host: repo.gh_host.clone(),
        }
    }
}

/// 테스트 전용 QueueItem 팩토리.
///
/// 모든 테스트 모듈에서 `use crate::core::queue_item::testing::*;`로 사용.
#[cfg(test)]
pub mod testing {
    use super::*;

    /// 기본 테스트 RepoRef ("r1", "org/repo").
    pub fn test_repo() -> RepoRef {
        RepoRef {
            id: "r1".into(),
            name: "org/repo".into(),
            url: "https://github.com/org/repo".into(),
            gh_host: None,
        }
    }

    /// 커스텀 repo 이름으로 RepoRef 생성.
    pub fn test_repo_named(name: &str) -> RepoRef {
        RepoRef {
            id: "r1".into(),
            name: name.into(),
            url: format!("https://github.com/{name}"),
            gh_host: None,
        }
    }

    /// 테스트용 Issue QueueItem.
    pub fn test_issue(number: i64, task_kind: TaskKind) -> QueueItem {
        QueueItem::new_issue(
            &test_repo(),
            number,
            task_kind,
            format!("Issue #{number}"),
            Some("test body".into()),
            vec![],
            "user".into(),
        )
    }

    /// 테스트용 PR QueueItem.
    pub fn test_pr(number: i64, task_kind: TaskKind) -> QueueItem {
        QueueItem::new_pr(
            &test_repo(),
            number,
            task_kind,
            format!("PR #{number}"),
            ItemMetadata::Pr {
                head_branch: "autodev/issue-42".into(),
                base_branch: "main".into(),
                review_comment: None,
                source_issue_number: None,
                review_iteration: 0,
            },
        )
    }

    /// source_issue_number 지정 가능한 PR QueueItem.
    pub fn test_pr_with_source(
        number: i64,
        task_kind: TaskKind,
        source_issue: Option<i64>,
        review_iteration: u32,
    ) -> QueueItem {
        QueueItem::new_pr(
            &test_repo(),
            number,
            task_kind,
            format!("PR #{number}"),
            ItemMetadata::Pr {
                head_branch: "autodev/issue-42".into(),
                base_branch: "main".into(),
                review_comment: None,
                source_issue_number: source_issue,
                review_iteration,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn new_issue_creates_correct_work_id() {
        let item = QueueItem::new_issue(
            &test_repo(),
            42,
            TaskKind::Analyze,
            "Fix bug".into(),
            Some("body".into()),
            vec!["bug".into()],
            "alice".into(),
        );
        assert_eq!(item.work_id, "issue:org/repo:42");
        assert_eq!(item.queue_type, QueueType::Issue);
        assert_eq!(item.task_kind, TaskKind::Analyze);
        assert_eq!(item.body(), Some("body"));
        assert_eq!(item.author(), Some("alice"));
        assert_eq!(item.labels().unwrap(), &["bug"]);
    }

    #[test]
    fn new_pr_creates_correct_work_id() {
        let item = QueueItem::new_pr(
            &test_repo(),
            10,
            TaskKind::Review,
            "PR #10".into(),
            ItemMetadata::Pr {
                head_branch: "feat".into(),
                base_branch: "main".into(),
                review_comment: None,
                source_issue_number: Some(42),
                review_iteration: 0,
            },
        );
        assert_eq!(item.work_id, "pr:org/repo:10");
        assert_eq!(item.queue_type, QueueType::Pr);
        assert_eq!(item.task_kind, TaskKind::Review);
        assert_eq!(item.head_branch(), Some("feat"));
        assert_eq!(item.base_branch(), Some("main"));
        assert_eq!(item.source_issue_number(), Some(42));
        assert_eq!(item.review_iteration(), Some(0));
    }

    #[test]
    fn has_work_id_trait() {
        let item = QueueItem::new_issue(
            &test_repo(),
            1,
            TaskKind::Analyze,
            "title".into(),
            None,
            vec![],
            "user".into(),
        );
        assert_eq!(HasWorkId::work_id(&item), "issue:org/repo:1");
    }

    #[test]
    fn pr_mutation_helpers() {
        let mut item = QueueItem::new_pr(
            &test_repo(),
            10,
            TaskKind::Review,
            "PR".into(),
            ItemMetadata::Pr {
                head_branch: "feat".into(),
                base_branch: "main".into(),
                review_comment: None,
                source_issue_number: None,
                review_iteration: 0,
            },
        );

        item.set_review_comment(Some("Fix error handling".into()));
        assert_eq!(item.review_comment(), Some("Fix error handling"));

        let new_iter = item.increment_review_iteration();
        assert_eq!(new_iter, 1);
        assert_eq!(item.review_iteration(), Some(1));
    }
}
