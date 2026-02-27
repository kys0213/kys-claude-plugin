use crate::domain::models::{RepoIssue, RepoPull};
use crate::infrastructure::gh::Gh;
use crate::queue::state_queue::StateQueue;
use crate::queue::task_queues::{IssueItem, MergeItem, PrItem};

/// Git repository aggregate.
///
/// 하나의 GitHub 저장소에 대한 모든 상태를 캡슐화한다:
/// - 식별 정보 (DB 원본)
/// - 설정 (per-repo config)
/// - GitHub 상태 스냅샷 (issues, pulls)
/// - 작업 큐 (issue, pr, merge)
pub struct GitRepository {
    id: String,
    name: String,
    url: String,
    gh_host: Option<String>,

    // GitHub state (refreshable)
    issues: Vec<RepoIssue>,
    pulls: Vec<RepoPull>,

    // Per-repo work queues
    pub issue_queue: StateQueue<IssueItem>,
    pub pr_queue: StateQueue<PrItem>,
    pub merge_queue: StateQueue<MergeItem>,
}

impl GitRepository {
    pub(crate) fn new(id: String, name: String, url: String, gh_host: Option<String>) -> Self {
        Self {
            id,
            name,
            url,
            gh_host,
            issues: Vec::new(),
            pulls: Vec::new(),
            issue_queue: StateQueue::new(),
            pr_queue: StateQueue::new(),
            merge_queue: StateQueue::new(),
        }
    }

    // ─── Identity ───

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn gh_host(&self) -> Option<&str> {
        self.gh_host.as_deref()
    }

    // ─── GitHub State ───

    pub fn issues(&self) -> &[RepoIssue] {
        &self.issues
    }

    pub fn pulls(&self) -> &[RepoPull] {
        &self.pulls
    }

    pub(crate) fn set_github_state(&mut self, issues: Vec<RepoIssue>, pulls: Vec<RepoPull>) {
        self.issues = issues;
        self.pulls = pulls;
    }

    /// GitHub API를 통해 issues/pulls를 다시 fetch한다.
    pub async fn refresh(&mut self, gh: &dyn Gh) {
        let issues = fetch_issues(gh, &self.name, self.gh_host.as_deref()).await;
        let pulls = fetch_pulls(gh, &self.name, self.gh_host.as_deref()).await;
        self.issues = issues;
        self.pulls = pulls;
    }

    // ─── Queue Access ───

    /// 어떤 큐든 해당 work_id가 존재하는지 확인
    pub fn contains(&self, work_id: &str) -> bool {
        self.issue_queue.contains(work_id)
            || self.pr_queue.contains(work_id)
            || self.merge_queue.contains(work_id)
    }

    /// 전체 큐 아이템 수
    pub fn total_items(&self) -> usize {
        self.issue_queue.total() + self.pr_queue.total() + self.merge_queue.total()
    }
}

// ─── GitHub API Helpers ───

pub(crate) async fn fetch_issues(
    gh: &dyn Gh,
    repo_name: &str,
    gh_host: Option<&str>,
) -> Vec<RepoIssue> {
    match gh
        .api_paginate(
            repo_name,
            "issues",
            &[("state", "open"), ("per_page", "100")],
            gh_host,
        )
        .await
    {
        Ok(data) => {
            let raw: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap_or_default();
            raw.iter().filter_map(RepoIssue::from_json).collect()
        }
        Err(e) => {
            tracing::warn!("failed to fetch issues for {repo_name}: {e}");
            Vec::new()
        }
    }
}

pub(crate) async fn fetch_pulls(
    gh: &dyn Gh,
    repo_name: &str,
    gh_host: Option<&str>,
) -> Vec<RepoPull> {
    match gh
        .api_paginate(
            repo_name,
            "pulls",
            &[("state", "open"), ("per_page", "100")],
            gh_host,
        )
        .await
    {
        Ok(data) => {
            let raw: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap_or_default();
            raw.iter().filter_map(RepoPull::from_json).collect()
        }
        Err(e) => {
            tracing::warn!("failed to fetch pulls for {repo_name}: {e}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::task_queues::{issue_phase, make_work_id, pr_phase};

    fn make_repo() -> GitRepository {
        GitRepository::new(
            "repo-id-1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        )
    }

    fn issue_item(repo_name: &str, number: i64) -> IssueItem {
        IssueItem {
            work_id: make_work_id("issue", repo_name, number),
            repo_id: "repo-id-1".to_string(),
            repo_name: repo_name.to_string(),
            repo_url: format!("https://github.com/{repo_name}"),
            github_number: number,
            title: format!("Issue #{number}"),
            body: None,
            labels: vec![],
            author: "user".to_string(),
            analysis_report: None,
        }
    }

    fn pr_item(repo_name: &str, number: i64) -> PrItem {
        PrItem {
            work_id: make_work_id("pr", repo_name, number),
            repo_id: "repo-id-1".to_string(),
            repo_name: repo_name.to_string(),
            repo_url: format!("https://github.com/{repo_name}"),
            github_number: number,
            title: format!("PR #{number}"),
            head_branch: "feature".to_string(),
            base_branch: "main".to_string(),
            review_comment: None,
            source_issue_number: None,
            review_iteration: 0,
        }
    }

    #[test]
    fn new_repository_has_empty_state() {
        let repo = make_repo();

        assert_eq!(repo.id(), "repo-id-1");
        assert_eq!(repo.name(), "org/repo");
        assert_eq!(repo.url(), "https://github.com/org/repo");
        assert!(repo.gh_host().is_none());
        assert!(repo.issues().is_empty());
        assert!(repo.pulls().is_empty());
        assert_eq!(repo.total_items(), 0);
    }

    #[test]
    fn set_github_state_populates_issues_and_pulls() {
        let mut repo = make_repo();
        let issue = RepoIssue {
            number: 1,
            title: "bug".to_string(),
            body: None,
            author: "user".to_string(),
            labels: vec!["bug".to_string()],
        };
        let pull = RepoPull {
            number: 10,
            title: "fix".to_string(),
            body: None,
            author: "user".to_string(),
            labels: vec![],
            head_branch: "fix-branch".to_string(),
            base_branch: "main".to_string(),
        };

        repo.set_github_state(vec![issue], vec![pull]);

        assert_eq!(repo.issues().len(), 1);
        assert_eq!(repo.pulls().len(), 1);
        assert_eq!(repo.issues()[0].number, 1);
        assert_eq!(repo.pulls()[0].number, 10);
    }

    #[test]
    fn contains_checks_all_queues() {
        let mut repo = make_repo();

        let i = issue_item("org/repo", 42);
        let p = pr_item("org/repo", 10);

        repo.issue_queue.push(issue_phase::PENDING, i);
        repo.pr_queue.push(pr_phase::PENDING, p);

        assert!(repo.contains("issue:org/repo:42"));
        assert!(repo.contains("pr:org/repo:10"));
        assert!(!repo.contains("issue:org/repo:99"));
    }

    #[test]
    fn total_items_sums_all_queues() {
        let mut repo = make_repo();
        assert_eq!(repo.total_items(), 0);

        repo.issue_queue
            .push(issue_phase::PENDING, issue_item("org/repo", 1));
        repo.issue_queue
            .push(issue_phase::PENDING, issue_item("org/repo", 2));
        repo.pr_queue
            .push(pr_phase::PENDING, pr_item("org/repo", 3));

        assert_eq!(repo.total_items(), 3);
    }

    #[test]
    fn gh_host_returns_configured_value() {
        let repo = GitRepository::new(
            "id".to_string(),
            "org/repo".to_string(),
            "url".to_string(),
            Some("github.example.com".to_string()),
        );
        assert_eq!(repo.gh_host(), Some("github.example.com"));
    }
}
