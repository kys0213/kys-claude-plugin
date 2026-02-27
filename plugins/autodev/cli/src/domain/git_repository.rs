use anyhow::Result;
use serde::Deserialize;

use crate::domain::labels;
use crate::domain::models::{RepoIssue, RepoPull};
use crate::domain::repository::ScanCursorRepository;
use crate::infrastructure::gh::Gh;
use crate::queue::state_queue::StateQueue;
use crate::queue::task_queues::{
    issue_phase, make_work_id, merge_phase, pr_phase, IssueItem, MergeItem, PrItem,
};

// ─── Private serde types for scanning ───

#[derive(Debug, Deserialize)]
struct ScanIssue {
    number: i64,
    title: String,
    body: Option<String>,
    labels: Vec<ScanLabel>,
    user: ScanUser,
    pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ScanLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ScanUser {
    login: String,
}

#[derive(Debug, Deserialize)]
struct ScanPull {
    number: i64,
    title: String,
    #[allow(dead_code)]
    body: Option<String>,
    user: ScanUser,
    head: ScanBranch,
    base: ScanBranch,
    updated_at: String,
    #[serde(default)]
    labels: Vec<ScanLabel>,
}

#[derive(Debug, Deserialize)]
struct ScanBranch {
    #[serde(rename = "ref")]
    ref_name: String,
}

fn has_autodev_label(scan_labels: &[ScanLabel]) -> bool {
    scan_labels.iter().any(|l| l.name.starts_with("autodev:"))
}

// ─── GitRepository Aggregate ───

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

    // ─── Scanning ───

    /// `autodev:analyze` 라벨이 있는 이슈를 스캔하여 issue_queue(Pending)에 추가.
    ///
    /// 라벨 전이: analyze 제거 → wip 추가 (트리거 소비)
    pub async fn scan_issues(
        &mut self,
        gh: &dyn Gh,
        db: &dyn ScanCursorRepository,
        ignore_authors: &[String],
        filter_labels: &Option<Vec<String>>,
    ) -> Result<()> {
        let params: Vec<(&str, &str)> = vec![
            ("state", "open"),
            ("labels", labels::ANALYZE),
            ("per_page", "30"),
        ];

        let stdout = gh
            .api_paginate(&self.name, "issues", &params, self.gh_host.as_deref())
            .await?;

        let issues: Vec<ScanIssue> = serde_json::from_slice(&stdout)?;

        for issue in &issues {
            if issue.pull_request.is_some() {
                continue;
            }

            if ignore_authors.contains(&issue.user.login) {
                continue;
            }

            if let Some(fl) = filter_labels {
                let issue_labels: Vec<&str> =
                    issue.labels.iter().map(|l| l.name.as_str()).collect();
                if !fl.iter().any(|l| issue_labels.contains(&l.as_str())) {
                    continue;
                }
            }

            let work_id = make_work_id("issue", &self.name, issue.number);

            if self.contains(&work_id) {
                continue;
            }

            let label_names: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

            let item = IssueItem {
                work_id,
                repo_id: self.id.clone(),
                repo_name: self.name.clone(),
                repo_url: self.url.clone(),
                github_number: issue.number,
                title: issue.title.clone(),
                body: issue.body.clone(),
                labels: label_names,
                author: issue.user.login.clone(),
                analysis_report: None,
            };

            gh.label_remove(
                &self.name,
                issue.number,
                labels::ANALYZE,
                self.gh_host.as_deref(),
            )
            .await;
            gh.label_add(
                &self.name,
                issue.number,
                labels::WIP,
                self.gh_host.as_deref(),
            )
            .await;
            self.issue_queue.push(issue_phase::PENDING, item);
            tracing::info!("issue #{}: autodev:analyze → wip (Pending)", issue.number);
        }

        let now = chrono::Utc::now().to_rfc3339();
        db.cursor_upsert(&self.id, "issues", &now)?;

        Ok(())
    }

    /// `autodev:approved-analysis` 라벨이 있는 이슈를 스캔하여 issue_queue(Ready)에 추가.
    ///
    /// 라벨 전이: approved-analysis 제거, analyzed 제거 → implementing 추가
    pub async fn scan_approved_issues(&mut self, gh: &dyn Gh) -> Result<()> {
        let params: Vec<(&str, &str)> = vec![
            ("state", "open"),
            ("labels", labels::APPROVED_ANALYSIS),
            ("per_page", "30"),
        ];

        let stdout = gh
            .api_paginate(&self.name, "issues", &params, self.gh_host.as_deref())
            .await?;

        let issues: Vec<ScanIssue> = serde_json::from_slice(&stdout)?;

        for issue in &issues {
            if issue.pull_request.is_some() {
                continue;
            }

            let work_id = make_work_id("issue", &self.name, issue.number);

            if self.contains(&work_id) {
                continue;
            }

            gh.label_remove(
                &self.name,
                issue.number,
                labels::APPROVED_ANALYSIS,
                self.gh_host.as_deref(),
            )
            .await;
            gh.label_remove(
                &self.name,
                issue.number,
                labels::ANALYZED,
                self.gh_host.as_deref(),
            )
            .await;
            gh.label_add(
                &self.name,
                issue.number,
                labels::IMPLEMENTING,
                self.gh_host.as_deref(),
            )
            .await;

            let label_names: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

            let item = IssueItem {
                work_id,
                repo_id: self.id.clone(),
                repo_name: self.name.clone(),
                repo_url: self.url.clone(),
                github_number: issue.number,
                title: issue.title.clone(),
                body: issue.body.clone(),
                labels: label_names,
                author: issue.user.login.clone(),
                analysis_report: None,
            };

            self.issue_queue.push(issue_phase::READY, item);
            tracing::info!(
                "queued approved issue #{}: {} → Ready",
                issue.number,
                issue.title
            );
        }

        Ok(())
    }

    /// 새로 업데이트된 PR을 스캔하여 pr_queue(Pending)에 추가.
    ///
    /// cursor 기반 증분 스캔으로 이전 스캔 이후 업데이트된 PR만 처리한다.
    /// autodev: 라벨이 이미 있는 PR은 건너뛴다.
    pub async fn scan_pulls(
        &mut self,
        gh: &dyn Gh,
        db: &dyn ScanCursorRepository,
        ignore_authors: &[String],
    ) -> Result<()> {
        let since = db.cursor_get_last_seen(&self.id, "pulls")?;

        let mut params: Vec<(&str, &str)> = vec![
            ("state", "open"),
            ("sort", "updated"),
            ("direction", "desc"),
            ("per_page", "30"),
        ];

        let since_owned;
        if let Some(ref s) = since {
            since_owned = s.clone();
            params.push(("since", &since_owned));
        }

        let stdout = gh
            .api_paginate(&self.name, "pulls", &params, self.gh_host.as_deref())
            .await?;

        let prs: Vec<ScanPull> = serde_json::from_slice(&stdout)?;
        let mut latest_updated = since;

        for pr in &prs {
            if let Some(ref s) = latest_updated {
                if pr.updated_at <= *s {
                    continue;
                }
            }

            if ignore_authors.contains(&pr.user.login) {
                continue;
            }

            if has_autodev_label(&pr.labels) {
                if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
                    latest_updated = Some(pr.updated_at.clone());
                }
                continue;
            }

            let work_id = make_work_id("pr", &self.name, pr.number);

            if self.contains(&work_id) {
                if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
                    latest_updated = Some(pr.updated_at.clone());
                }
                continue;
            }

            let item = PrItem {
                work_id,
                repo_id: self.id.clone(),
                repo_name: self.name.clone(),
                repo_url: self.url.clone(),
                github_number: pr.number,
                title: pr.title.clone(),
                head_branch: pr.head.ref_name.clone(),
                base_branch: pr.base.ref_name.clone(),
                review_comment: None,
                source_issue_number: None,
                review_iteration: 0,
            };

            gh.label_add(&self.name, pr.number, labels::WIP, self.gh_host.as_deref())
                .await;
            self.pr_queue.push(pr_phase::PENDING, item);
            tracing::info!("queued PR #{}: {}", pr.number, pr.title);

            if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
                latest_updated = Some(pr.updated_at.clone());
            }
        }

        if let Some(last_seen) = latest_updated {
            db.cursor_upsert(&self.id, "pulls", &last_seen)?;
        }

        Ok(())
    }

    /// `autodev:done` 라벨이 붙은 PR을 스캔하여 merge_queue(Pending)에 추가.
    ///
    /// 라벨 전이: done 제거 → wip 추가
    pub async fn scan_merges(&mut self, gh: &dyn Gh) -> Result<()> {
        let params: Vec<(&str, &str)> = vec![
            ("state", "open"),
            ("labels", labels::DONE),
            ("per_page", "30"),
        ];

        let stdout = gh
            .api_paginate(&self.name, "issues", &params, self.gh_host.as_deref())
            .await?;

        let items: Vec<serde_json::Value> = serde_json::from_slice(&stdout)?;

        for item in &items {
            if item.get("pull_request").is_none() {
                continue;
            }

            let number = match item["number"].as_i64() {
                Some(n) if n > 0 => n,
                _ => continue,
            };

            let merge_work_id = make_work_id("merge", &self.name, number);

            if self.contains(&merge_work_id) {
                continue;
            }

            let pr_params: Vec<(&str, &str)> = vec![];
            let pr_data = gh
                .api_paginate(
                    &self.name,
                    &format!("pulls/{number}"),
                    &pr_params,
                    self.gh_host.as_deref(),
                )
                .await;

            let (head_branch, base_branch, title) = match pr_data {
                Ok(data) => {
                    let pr: serde_json::Value =
                        serde_json::from_slice(&data).unwrap_or(serde_json::Value::Null);
                    (
                        pr["head"]["ref"].as_str().unwrap_or("").to_string(),
                        pr["base"]["ref"].as_str().unwrap_or("main").to_string(),
                        pr["title"].as_str().unwrap_or("").to_string(),
                    )
                }
                Err(_) => continue,
            };

            let merge_item = MergeItem {
                work_id: merge_work_id,
                repo_id: self.id.clone(),
                repo_name: self.name.clone(),
                repo_url: self.url.clone(),
                pr_number: number,
                title,
                head_branch,
                base_branch,
            };

            gh.label_remove(&self.name, number, labels::DONE, self.gh_host.as_deref())
                .await;
            gh.label_add(&self.name, number, labels::WIP, self.gh_host.as_deref())
                .await;
            self.merge_queue.push(merge_phase::PENDING, merge_item);
            tracing::info!("queued merge PR #{number}");
        }

        Ok(())
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
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::infrastructure::gh::mock::MockGh;
    use crate::queue::task_queues::{issue_phase, make_work_id, merge_phase, pr_phase};

    // ─── Mock ScanCursorRepository ───

    struct MockCursorRepo {
        last_seen: Mutex<HashMap<(String, String), String>>,
    }

    impl MockCursorRepo {
        fn new() -> Self {
            Self {
                last_seen: Mutex::new(HashMap::new()),
            }
        }
    }

    impl ScanCursorRepository for MockCursorRepo {
        fn cursor_get_last_seen(&self, repo_id: &str, target: &str) -> Result<Option<String>> {
            Ok(self
                .last_seen
                .lock()
                .unwrap()
                .get(&(repo_id.to_string(), target.to_string()))
                .cloned())
        }

        fn cursor_upsert(&self, repo_id: &str, target: &str, last_seen: &str) -> Result<()> {
            self.last_seen.lock().unwrap().insert(
                (repo_id.to_string(), target.to_string()),
                last_seen.to_string(),
            );
            Ok(())
        }

        fn cursor_should_scan(&self, _repo_id: &str, _interval_secs: i64) -> Result<bool> {
            Ok(true)
        }
    }

    // ─── Test Helpers ───

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

    // ═══════════════════════════════════════════════════
    // State & Queue Tests
    // ═══════════════════════════════════════════════════

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

    // ═══════════════════════════════════════════════════
    // Scanning Tests
    // ═══════════════════════════════════════════════════

    #[tokio::test]
    async fn scan_issues_adds_analyze_issues_to_queue() {
        let gh = MockGh::new();
        let db = MockCursorRepo::new();

        let issues_json = serde_json::json!([
            {
                "number": 1,
                "title": "bug report",
                "body": "fix it",
                "user": {"login": "alice"},
                "labels": [{"name": "autodev:analyze"}, {"name": "bug"}]
            },
            {
                "number": 2,
                "title": "feature PR",
                "body": null,
                "user": {"login": "bob"},
                "labels": [{"name": "autodev:analyze"}],
                "pull_request": {}
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        let mut repo = make_repo();
        repo.scan_issues(&gh, &db, &[], &None).await.unwrap();

        // PR (#2) is filtered out, only issue #1 added
        assert_eq!(repo.issue_queue.len(issue_phase::PENDING), 1);
        let item = repo.issue_queue.pop(issue_phase::PENDING).unwrap();
        assert_eq!(item.github_number, 1);
        assert_eq!(item.title, "bug report");

        // Label transitions: analyze removed, wip added
        let removed = gh.removed_labels.lock().unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(
            removed[0],
            ("org/repo".to_string(), 1, "autodev:analyze".to_string())
        );

        let added = gh.added_labels.lock().unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(
            added[0],
            ("org/repo".to_string(), 1, "autodev:wip".to_string())
        );
    }

    #[tokio::test]
    async fn scan_issues_skips_ignored_authors() {
        let gh = MockGh::new();
        let db = MockCursorRepo::new();

        let issues_json = serde_json::json!([
            {
                "number": 1,
                "title": "from bot",
                "body": null,
                "user": {"login": "bot"},
                "labels": [{"name": "autodev:analyze"}]
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        let mut repo = make_repo();
        repo.scan_issues(&gh, &db, &["bot".to_string()], &None)
            .await
            .unwrap();

        assert_eq!(repo.issue_queue.len(issue_phase::PENDING), 0);
    }

    #[tokio::test]
    async fn scan_issues_dedup_existing_queue_items() {
        let gh = MockGh::new();
        let db = MockCursorRepo::new();

        let issues_json = serde_json::json!([
            {
                "number": 1,
                "title": "already queued",
                "body": null,
                "user": {"login": "alice"},
                "labels": [{"name": "autodev:analyze"}]
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        let mut repo = make_repo();
        // Pre-populate queue with the same issue
        repo.issue_queue
            .push(issue_phase::PENDING, issue_item("org/repo", 1));

        repo.scan_issues(&gh, &db, &[], &None).await.unwrap();

        // Still only 1 item (no duplicate)
        assert_eq!(repo.issue_queue.len(issue_phase::PENDING), 1);
    }

    #[tokio::test]
    async fn scan_issues_applies_filter_labels() {
        let gh = MockGh::new();
        let db = MockCursorRepo::new();

        let issues_json = serde_json::json!([
            {
                "number": 1,
                "title": "matching label",
                "body": null,
                "user": {"login": "alice"},
                "labels": [{"name": "autodev:analyze"}, {"name": "priority:high"}]
            },
            {
                "number": 2,
                "title": "no matching label",
                "body": null,
                "user": {"login": "bob"},
                "labels": [{"name": "autodev:analyze"}]
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        let mut repo = make_repo();
        let filter = Some(vec!["priority:high".to_string()]);
        repo.scan_issues(&gh, &db, &[], &filter).await.unwrap();

        // Only issue #1 matches the filter
        assert_eq!(repo.issue_queue.len(issue_phase::PENDING), 1);
        let item = repo.issue_queue.pop(issue_phase::PENDING).unwrap();
        assert_eq!(item.github_number, 1);
    }

    #[tokio::test]
    async fn scan_approved_issues_adds_to_ready_queue() {
        let gh = MockGh::new();

        let issues_json = serde_json::json!([
            {
                "number": 5,
                "title": "approved issue",
                "body": "implement this",
                "user": {"login": "alice"},
                "labels": [{"name": "autodev:approved-analysis"}, {"name": "autodev:analyzed"}]
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        let mut repo = make_repo();
        repo.scan_approved_issues(&gh).await.unwrap();

        assert_eq!(repo.issue_queue.len(issue_phase::READY), 1);
        let item = repo.issue_queue.pop(issue_phase::READY).unwrap();
        assert_eq!(item.github_number, 5);

        // Label transitions: approved-analysis removed, analyzed removed, implementing added
        let removed = gh.removed_labels.lock().unwrap();
        assert_eq!(removed.len(), 2);
        assert!(removed.iter().any(|r| r.2 == "autodev:approved-analysis"));
        assert!(removed.iter().any(|r| r.2 == "autodev:analyzed"));

        let added = gh.added_labels.lock().unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].2, "autodev:implementing");
    }

    #[tokio::test]
    async fn scan_pulls_adds_new_prs_to_queue() {
        let gh = MockGh::new();
        let db = MockCursorRepo::new();

        let pulls_json = serde_json::json!([
            {
                "number": 10,
                "title": "fix bug",
                "body": "Closes #1",
                "user": {"login": "alice"},
                "head": {"ref": "fix-bug"},
                "base": {"ref": "main"},
                "updated_at": "2025-01-01T00:00:00Z",
                "labels": []
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&pulls_json).unwrap(),
        );

        let mut repo = make_repo();
        repo.scan_pulls(&gh, &db, &[]).await.unwrap();

        assert_eq!(repo.pr_queue.len(pr_phase::PENDING), 1);
        let item = repo.pr_queue.pop(pr_phase::PENDING).unwrap();
        assert_eq!(item.github_number, 10);
        assert_eq!(item.head_branch, "fix-bug");

        // wip label added
        let added = gh.added_labels.lock().unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(
            added[0],
            ("org/repo".to_string(), 10, "autodev:wip".to_string())
        );
    }

    #[tokio::test]
    async fn scan_pulls_skips_autodev_labeled_prs() {
        let gh = MockGh::new();
        let db = MockCursorRepo::new();

        let pulls_json = serde_json::json!([
            {
                "number": 10,
                "title": "already reviewed",
                "body": null,
                "user": {"login": "alice"},
                "head": {"ref": "fix-bug"},
                "base": {"ref": "main"},
                "updated_at": "2025-01-01T00:00:00Z",
                "labels": [{"name": "autodev:done"}]
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&pulls_json).unwrap(),
        );

        let mut repo = make_repo();
        repo.scan_pulls(&gh, &db, &[]).await.unwrap();

        assert_eq!(repo.pr_queue.len(pr_phase::PENDING), 0);
    }

    #[tokio::test]
    async fn scan_pulls_updates_cursor() {
        let gh = MockGh::new();
        let db = MockCursorRepo::new();

        let pulls_json = serde_json::json!([
            {
                "number": 10,
                "title": "fix bug",
                "body": null,
                "user": {"login": "alice"},
                "head": {"ref": "fix-bug"},
                "base": {"ref": "main"},
                "updated_at": "2025-06-01T12:00:00Z",
                "labels": []
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&pulls_json).unwrap(),
        );

        let mut repo = make_repo();
        repo.scan_pulls(&gh, &db, &[]).await.unwrap();

        // Cursor should be updated to the PR's updated_at
        let cursor = db
            .cursor_get_last_seen("repo-id-1", "pulls")
            .unwrap()
            .unwrap();
        assert_eq!(cursor, "2025-06-01T12:00:00Z");
    }

    #[tokio::test]
    async fn scan_merges_adds_done_prs_to_merge_queue() {
        let gh = MockGh::new();

        // issues endpoint returns done PR
        let issues_json = serde_json::json!([
            {
                "number": 20,
                "title": "done PR",
                "labels": [{"name": "autodev:done"}],
                "pull_request": {}
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        // PR detail
        let pr_detail = serde_json::json!({
            "number": 20,
            "title": "done PR",
            "head": {"ref": "feature-branch"},
            "base": {"ref": "main"}
        });
        gh.set_paginate(
            "org/repo",
            "pulls/20",
            serde_json::to_vec(&pr_detail).unwrap(),
        );

        let mut repo = make_repo();
        repo.scan_merges(&gh).await.unwrap();

        assert_eq!(repo.merge_queue.len(merge_phase::PENDING), 1);
        let item = repo.merge_queue.pop(merge_phase::PENDING).unwrap();
        assert_eq!(item.pr_number, 20);
        assert_eq!(item.head_branch, "feature-branch");

        // Label transitions: done removed, wip added
        let removed = gh.removed_labels.lock().unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(
            removed[0],
            ("org/repo".to_string(), 20, "autodev:done".to_string())
        );

        let added = gh.added_labels.lock().unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(
            added[0],
            ("org/repo".to_string(), 20, "autodev:wip".to_string())
        );
    }

    #[tokio::test]
    async fn scan_merges_skips_non_pr_issues() {
        let gh = MockGh::new();

        // issues endpoint returns a regular issue (no pull_request field)
        let issues_json = serde_json::json!([
            {
                "number": 30,
                "title": "regular issue",
                "labels": [{"name": "autodev:done"}]
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        let mut repo = make_repo();
        repo.scan_merges(&gh).await.unwrap();

        assert_eq!(repo.merge_queue.len(merge_phase::PENDING), 0);
    }
}
