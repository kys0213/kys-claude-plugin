use serde::{Deserialize, Serialize};

use super::models::{QueueItemRow, QueuePhase, QueueType, RepoIssue, RepoPull};
use super::phase::TaskKind;
use super::state_queue::HasWorkId;
use super::task_queues::{make_source_id, make_work_id};

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

// ─── Typed Metadata ───

/// PR 전용 메타데이터. `QueueItem::new_pr`의 파라미터 타입.
///
/// `ItemMetadata::Pr`와 1:1 대응하지만, 독립 struct로 분리하여
/// `new_pr` 호출부에서 Issue 메타데이터를 실수로 전달할 수 없도록 한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrMetadata {
    pub head_branch: String,
    pub base_branch: String,
    pub review_comment: Option<String>,
    pub source_issue_number: Option<i64>,
    pub review_iteration: u32,
}

// ─── Internal Metadata Enum ───

/// QueueItem 내부 메타데이터. 외부에서 직접 생성하지 않는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum ItemMetadata {
    Issue {
        body: Option<String>,
        labels: Vec<String>,
        author: String,
        analysis_report: Option<String>,
    },
    Pr(PrMetadata),
}

// ─── Unified In-Memory Work Item ───

/// 통합 큐 아이템.
///
/// 기존 IssueItem/PrItem을 단일 구조체로 통합한다.
/// `queue_type`으로 Issue/PR 구분, `task_kind`로 작업 종류 결정.
///
/// v5: `source_id`로 같은 외부 엔티티에서 파생된 아이템을 연결한다.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub work_id: String,
    /// 같은 외부 엔티티에서 파생된 아이템을 연결하는 식별자.
    /// 형식: "github:{repo_name}#{number}"
    pub source_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub repo_url: String,
    pub github_number: i64,
    pub queue_type: QueueType,
    pub task_kind: TaskKind,
    pub title: String,
    pub(crate) metadata: ItemMetadata,
    /// GHE hostname (e.g. "git.example.com"). None이면 github.com.
    pub gh_host: Option<String>,
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
            ItemMetadata::Pr(pr) if !pr.head_branch.is_empty() => Some(&pr.head_branch),
            _ => None,
        }
    }

    /// PR base branch (PR일 때만). Issue이거나 빈 문자열이면 None.
    pub fn base_branch(&self) -> Option<&str> {
        match &self.metadata {
            ItemMetadata::Pr(pr) if !pr.base_branch.is_empty() => Some(&pr.base_branch),
            _ => None,
        }
    }

    /// PR review comment (PR일 때만)
    pub fn review_comment(&self) -> Option<&str> {
        match &self.metadata {
            ItemMetadata::Pr(pr) => pr.review_comment.as_deref(),
            _ => None,
        }
    }

    /// PR source issue number (PR일 때만)
    pub fn source_issue_number(&self) -> Option<i64> {
        match &self.metadata {
            ItemMetadata::Pr(pr) => pr.source_issue_number,
            _ => None,
        }
    }

    /// PR review iteration (PR일 때만). Issue이면 None.
    pub fn review_iteration(&self) -> Option<u32> {
        match &self.metadata {
            ItemMetadata::Pr(pr) => Some(pr.review_iteration),
            _ => None,
        }
    }

    /// PR review iteration (PR이 아니면 0).
    pub fn review_iteration_or_zero(&self) -> u32 {
        self.review_iteration().unwrap_or(0)
    }

    /// task_kind를 Review로 전이 (ImproveTask 완료 후 re-review 경로)
    pub fn transition_to_review(&mut self) {
        self.task_kind = TaskKind::Review;
    }

    /// task_kind를 Improve로 전이 (ReviewTask의 request_changes 경로)
    pub fn transition_to_improve(&mut self) {
        self.task_kind = TaskKind::Improve;
    }

    /// Set review_comment on PR metadata
    pub fn set_review_comment(&mut self, comment: Option<String>) {
        if let ItemMetadata::Pr(pr) = &mut self.metadata {
            pr.review_comment = comment;
        }
    }

    /// Increment review_iteration on PR metadata, returning the new value
    pub fn increment_review_iteration(&mut self) -> u32 {
        if let ItemMetadata::Pr(pr) = &mut self.metadata {
            pr.review_iteration += 1;
            pr.review_iteration
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
            work_id: make_work_id(QueueType::Issue, &repo.name, github_number, task_kind),
            source_id: make_source_id(&repo.name, github_number),
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
            PrMetadata {
                head_branch: pull.head_branch.clone(),
                base_branch: pull.base_branch.clone(),
                review_comment: None,
                source_issue_number: pull.source_issue_number(),
                review_iteration: pull.review_iteration(),
            },
        )
    }

    /// 메타데이터를 JSON 문자열로 직렬화한다.
    pub fn metadata_json(&self) -> Option<String> {
        serde_json::to_string(&self.metadata).ok()
    }

    /// JSON 문자열에서 ItemMetadata를 역직렬화한다.
    pub(crate) fn metadata_from_json(json: &str) -> Option<ItemMetadata> {
        serde_json::from_str(json).ok()
    }

    /// QueueItem을 DB 행으로 변환한다.
    pub fn to_row(&self, phase: QueuePhase) -> QueueItemRow {
        let now = chrono::Utc::now().to_rfc3339();
        QueueItemRow {
            work_id: self.work_id.clone(),
            source_id: self.source_id.clone(),
            repo_id: self.repo_id.clone(),
            queue_type: self.queue_type.clone(),
            phase,
            title: Some(self.title.clone()),
            skip_reason: None,
            created_at: now.clone(),
            updated_at: now,
            task_kind: self.task_kind,
            github_number: self.github_number,
            metadata_json: self.metadata_json(),
            failure_count: 0,
            escalation_level: 0,
        }
    }

    /// DB 행에서 QueueItem을 복원한다.
    pub fn from_row(
        row: &QueueItemRow,
        repo_name: &str,
        repo_url: &str,
        gh_host: Option<&str>,
    ) -> Option<Self> {
        let metadata = match &row.metadata_json {
            Some(json) => Self::metadata_from_json(json)?,
            None => match row.queue_type {
                QueueType::Issue => ItemMetadata::Issue {
                    body: None,
                    labels: vec![],
                    author: String::new(),
                    analysis_report: None,
                },
                QueueType::Pr | QueueType::Knowledge | QueueType::Agent => {
                    ItemMetadata::Pr(PrMetadata {
                        head_branch: String::new(),
                        base_branch: String::new(),
                        review_comment: None,
                        source_issue_number: None,
                        review_iteration: 0,
                    })
                }
            },
        };

        Some(Self {
            work_id: row.work_id.clone(),
            source_id: row.source_id.clone(),
            repo_id: row.repo_id.clone(),
            repo_name: repo_name.to_string(),
            repo_url: repo_url.to_string(),
            github_number: row.github_number,
            queue_type: row.queue_type.clone(),
            task_kind: row.task_kind,
            title: row.title.clone().unwrap_or_default(),
            metadata,
            gh_host: gh_host.map(|s| s.to_string()),
        })
    }

    /// PR QueueItem 생성
    pub fn new_pr(
        repo: &RepoRef,
        github_number: i64,
        task_kind: TaskKind,
        title: String,
        pr: PrMetadata,
    ) -> Self {
        Self {
            work_id: make_work_id(QueueType::Pr, &repo.name, github_number, task_kind),
            source_id: make_source_id(&repo.name, github_number),
            repo_id: repo.id.clone(),
            repo_name: repo.name.clone(),
            repo_url: repo.url.clone(),
            github_number,
            queue_type: QueueType::Pr,
            task_kind,
            title,
            metadata: ItemMetadata::Pr(pr),
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
            PrMetadata {
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
            PrMetadata {
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
        assert_eq!(item.work_id, "github:org/repo#42:analyze");
        assert_eq!(item.source_id, "github:org/repo#42");
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
            PrMetadata {
                head_branch: "feat".into(),
                base_branch: "main".into(),
                review_comment: None,
                source_issue_number: Some(42),
                review_iteration: 0,
            },
        );
        assert_eq!(item.work_id, "github:org/repo#10:review");
        assert_eq!(item.source_id, "github:org/repo#10");
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
        assert_eq!(HasWorkId::work_id(&item), "github:org/repo#1:analyze");
    }

    #[test]
    fn pr_mutation_helpers() {
        let mut item = QueueItem::new_pr(
            &test_repo(),
            10,
            TaskKind::Review,
            "PR".into(),
            PrMetadata {
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

    #[test]
    fn empty_head_branch_returns_none() {
        let item = QueueItem::new_pr(
            &test_repo(),
            10,
            TaskKind::Review,
            "PR".into(),
            PrMetadata {
                head_branch: String::new(),
                base_branch: String::new(),
                review_comment: None,
                source_issue_number: None,
                review_iteration: 0,
            },
        );
        assert_eq!(item.head_branch(), None);
        assert_eq!(item.base_branch(), None);
    }

    #[test]
    fn metadata_json_roundtrip_issue() {
        let item = test_issue(42, TaskKind::Analyze);
        let json = item.metadata_json().unwrap();
        let meta = QueueItem::metadata_from_json(&json).unwrap();
        match meta {
            ItemMetadata::Issue { body, author, .. } => {
                assert_eq!(body.as_deref(), Some("test body"));
                assert_eq!(author, "user");
            }
            _ => panic!("expected Issue metadata"),
        }
    }

    #[test]
    fn metadata_json_roundtrip_pr() {
        let item = test_pr_with_source(10, TaskKind::Review, Some(42), 3);
        let json = item.metadata_json().unwrap();
        let meta = QueueItem::metadata_from_json(&json).unwrap();
        match meta {
            ItemMetadata::Pr(pr) => {
                assert_eq!(pr.head_branch, "autodev/issue-42");
                assert_eq!(pr.base_branch, "main");
                assert_eq!(pr.source_issue_number, Some(42));
                assert_eq!(pr.review_iteration, 3);
            }
            _ => panic!("expected Pr metadata"),
        }
    }

    #[test]
    fn queue_item_to_row_roundtrip() {
        let item = test_issue(42, TaskKind::Analyze);
        let row = item.to_row(QueuePhase::Pending);

        assert_eq!(row.work_id, "github:org/repo#42:analyze");
        assert_eq!(row.source_id, "github:org/repo#42");
        assert_eq!(row.task_kind, TaskKind::Analyze);
        assert_eq!(row.github_number, 42);
        assert!(row.metadata_json.is_some());

        let restored =
            QueueItem::from_row(&row, "org/repo", "https://github.com/org/repo", None).unwrap();
        assert_eq!(restored.work_id, item.work_id);
        assert_eq!(restored.source_id, item.source_id);
        assert_eq!(restored.github_number, item.github_number);
        assert_eq!(restored.task_kind, item.task_kind);
        assert_eq!(restored.body(), item.body());
        assert_eq!(restored.author(), item.author());
    }

    #[test]
    fn from_row_with_null_metadata_creates_default() {
        let mut row = test_issue(42, TaskKind::Analyze).to_row(QueuePhase::Pending);
        row.metadata_json = None;

        let restored =
            QueueItem::from_row(&row, "org/repo", "https://github.com/org/repo", None).unwrap();
        assert_eq!(restored.work_id, "github:org/repo#42:analyze");
        assert_eq!(restored.source_id, "github:org/repo#42");
        // Default Issue metadata has empty fields
        assert_eq!(restored.body(), None);
        assert_eq!(restored.author(), Some(""));
    }

    #[test]
    fn source_id_links_related_items() {
        let analyze = test_issue(42, TaskKind::Analyze);
        let implement = test_issue(42, TaskKind::Implement);

        // Same source_id, different work_id
        assert_eq!(analyze.source_id, implement.source_id);
        assert_ne!(analyze.work_id, implement.work_id);
        assert_eq!(analyze.source_id, "github:org/repo#42");
    }
}
