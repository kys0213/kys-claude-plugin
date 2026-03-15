use anyhow::Result;
use serde::Deserialize;

use crate::core::labels;
use crate::core::models::{HasLabels, QueuePhase, QueueType, RepoIssue, RepoPull};
use crate::core::phase::TaskKind;
use crate::core::queue_item::{PrMetadata, QueueItem, RepoRef};
use crate::core::repository::{QueueRepository, ScanCursorRepository};
use crate::core::state_queue::StateQueue;
use crate::core::task_queues::make_work_id;
use crate::infra::gh::Gh;

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

// ─── GitRepository Aggregate ───

/// Git repository aggregate.
///
/// 하나의 GitHub 저장소에 대한 모든 상태를 캡슐화한다:
/// - 식별 정보 (DB 원본)
/// - 설정 (per-repo config)
/// - GitHub 상태 스냅샷 (issues, pulls)
/// - 작업 큐 (unified)
pub struct GitRepository {
    id: String,
    name: String,
    url: String,
    gh_host: Option<String>,

    // GitHub state (refreshable)
    issues: Vec<RepoIssue>,
    pulls: Vec<RepoPull>,

    // Unified work queue
    pub queue: StateQueue<QueueItem>,

    // Per-repo concurrency limits (set during scan)
    pub issue_concurrency: usize,
    pub pr_concurrency: usize,
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
            queue: StateQueue::new(),
            issue_concurrency: 1,
            pr_concurrency: 1,
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

    pub fn repo_ref(&self) -> RepoRef {
        RepoRef {
            id: self.id.clone(),
            name: self.name.clone(),
            url: self.url.clone(),
            gh_host: self.gh_host.clone(),
        }
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

    /// 큐에 해당 work_id가 존재하는지 확인
    pub fn contains(&self, work_id: &str) -> bool {
        self.queue.contains(work_id)
    }

    /// 전체 큐 아이템 수
    pub fn total_items(&self) -> usize {
        self.queue.total()
    }

    // ─── Queue Helpers ───

    /// Issue를 QueueItem으로 변환하여 Pending 큐에 추가한다.
    #[allow(clippy::too_many_arguments)]
    fn enqueue_issue(
        &mut self,
        db: &dyn QueueRepository,
        number: i64,
        task_kind: TaskKind,
        title: String,
        body: Option<String>,
        labels: Vec<String>,
        author: String,
    ) {
        let repo = self.repo_ref();
        let item = QueueItem::new_issue(&repo, number, task_kind, title, body, labels, author);
        if self.queue.push(QueuePhase::Pending, item.clone()) {
            if let Err(e) = db.queue_upsert(&item.to_row(QueuePhase::Pending)) {
                tracing::error!("queue_upsert failed for {}: {e}", item.work_id);
            }
        }
    }

    /// PR을 QueueItem으로 변환하여 Pending 큐에 추가한다.
    fn enqueue_pr(
        &mut self,
        db: &dyn QueueRepository,
        number: i64,
        task_kind: TaskKind,
        title: String,
        meta: PrMetadata,
    ) {
        let repo = self.repo_ref();
        let item = QueueItem::new_pr(&repo, number, task_kind, title, meta);
        if self.queue.push(QueuePhase::Pending, item.clone()) {
            if let Err(e) = db.queue_upsert(&item.to_row(QueuePhase::Pending)) {
                tracing::error!("queue_upsert failed for {}: {e}", item.work_id);
            }
        }
    }

    /// DB에서 활성 큐 아이템을 로드하여 인메모리 큐에 적재한다.
    pub fn load_from_db(&mut self, db: &dyn QueueRepository) {
        match db.queue_load_active(&self.id) {
            Ok(rows) => {
                for row in rows {
                    if let Some(item) =
                        QueueItem::from_row(&row, &self.name, &self.url, self.gh_host())
                    {
                        self.queue.push(row.phase, item);
                    }
                }
            }
            Err(e) => tracing::error!("queue DB load failed for {}: {e}", self.name),
        }
    }

    // ─── Scanning ───

    /// `autodev:analyze` 라벨이 있는 이슈를 스캔하여 queue(Pending, Analyze)에 추가.
    ///
    /// 라벨 전이: analyze 제거 → wip 추가 (트리거 소비)
    pub async fn scan_issues<DB: ScanCursorRepository + QueueRepository>(
        &mut self,
        gh: &dyn Gh,
        db: &DB,
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

            let work_id = make_work_id(QueueType::Issue, &self.name, issue.number);

            if self.contains(&work_id) {
                continue;
            }

            let label_names: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

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
            self.enqueue_issue(
                db,
                issue.number,
                TaskKind::Analyze,
                issue.title.clone(),
                issue.body.clone(),
                label_names,
                issue.user.login.clone(),
            );
            tracing::info!("issue #{}: autodev:analyze → wip (Pending)", issue.number);
        }

        let now = chrono::Utc::now().to_rfc3339();
        db.cursor_upsert(&self.id, "issues", &now)?;

        Ok(())
    }

    /// `autodev:approved-analysis` 라벨이 있는 이슈를 스캔하여 queue(Pending, Implement)에 추가.
    ///
    /// 라벨 전이: approved-analysis 제거, analyzed 제거 → implementing 추가
    pub async fn scan_approved_issues(
        &mut self,
        gh: &dyn Gh,
        db: &dyn QueueRepository,
    ) -> Result<()> {
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

            let work_id = make_work_id(QueueType::Issue, &self.name, issue.number);

            if self.contains(&work_id) {
                continue;
            }

            // add-first: add IMPLEMENTING before removing old labels
            gh.label_add(
                &self.name,
                issue.number,
                labels::IMPLEMENTING,
                self.gh_host.as_deref(),
            )
            .await;
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

            let label_names: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

            self.enqueue_issue(
                db,
                issue.number,
                TaskKind::Implement,
                issue.title.clone(),
                issue.body.clone(),
                label_names,
                issue.user.login.clone(),
            );
            tracing::info!(
                "queued approved issue #{}: {} → Ready",
                issue.number,
                issue.title
            );
        }

        Ok(())
    }

    /// `autodev:wip` 라벨이 있는 open PR을 스캔하여 queue(Pending, Review)에 추가.
    ///
    /// Label-Positive 모델: `autodev:wip` 라벨이 있는 PR만 scan 대상.
    /// 외부 PR은 사람이 수동으로 `autodev:wip`를 추가해야 리뷰 대상이 됨.
    pub async fn scan_pulls(
        &mut self,
        gh: &dyn Gh,
        db: &dyn QueueRepository,
        ignore_authors: &[String],
    ) -> Result<()> {
        // Scan wip-labeled PRs → Pending (review)
        self.scan_pulls_by_label(gh, db, ignore_authors, labels::WIP, TaskKind::Review)
            .await?;

        // Scan changes-requested PRs → Pending (improve)
        self.scan_pulls_by_label(
            gh,
            db,
            ignore_authors,
            labels::CHANGES_REQUESTED,
            TaskKind::Improve,
        )
        .await?;

        Ok(())
    }

    /// 특정 라벨의 PR을 스캔하여 지정된 TaskKind로 큐에 추가.
    async fn scan_pulls_by_label(
        &mut self,
        gh: &dyn Gh,
        db: &dyn QueueRepository,
        ignore_authors: &[String],
        label: &str,
        target_kind: TaskKind,
    ) -> Result<()> {
        let params: Vec<(&str, &str)> =
            vec![("state", "open"), ("labels", label), ("per_page", "30")];

        let stdout = gh
            .api_paginate(&self.name, "issues", &params, self.gh_host.as_deref())
            .await?;

        let items: Vec<serde_json::Value> = serde_json::from_slice(&stdout)?;

        for item in &items {
            // issues API는 PR도 포함 — pull_request 필드가 있어야 PR
            if item.get("pull_request").is_none() {
                continue;
            }

            let number = match item["number"].as_i64() {
                Some(n) if n > 0 => n,
                _ => continue,
            };

            let author = item["user"]["login"].as_str().unwrap_or("");
            if ignore_authors.iter().any(|a| a == author) {
                continue;
            }

            let work_id = make_work_id(QueueType::Pr, &self.name, number);

            if self.contains(&work_id) {
                continue;
            }

            // PR 상세 정보 (head/base branch) 조회
            let pr_data = gh
                .api_paginate(
                    &self.name,
                    &format!("pulls/{number}"),
                    &[],
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

            let label_names: Vec<&str> = item["labels"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|l| l["name"].as_str()).collect())
                .unwrap_or_default();

            self.enqueue_pr(
                db,
                number,
                target_kind,
                title,
                PrMetadata {
                    head_branch,
                    base_branch,
                    review_comment: None,
                    source_issue_number: None,
                    review_iteration: labels::parse_iteration(&label_names),
                },
            );
            tracing::info!(
                "queued PR #{number} ({label} → Pending/{kind})",
                kind = target_kind.as_str()
            );
        }

        Ok(())
    }

    /// `autodev:done` + merged + NOT `autodev:extracted` PR을 스캔하여
    /// queue(Pending, Extract)에 추가 (지식 추출 대상).
    ///
    /// Label-Positive: done 라벨 + closed(merged) 상태 + extracted 라벨 없음
    pub async fn scan_done_merged(&mut self, gh: &dyn Gh, db: &dyn QueueRepository) -> Result<()> {
        let params: Vec<(&str, &str)> = vec![
            ("state", "closed"),
            ("labels", labels::DONE),
            ("per_page", "30"),
        ];

        let stdout = gh
            .api_paginate(&self.name, "issues", &params, self.gh_host.as_deref())
            .await?;

        let items: Vec<serde_json::Value> = serde_json::from_slice(&stdout)?;

        for item in &items {
            // issues API — PR만 대상
            if item.get("pull_request").is_none() {
                continue;
            }

            let number = match item["number"].as_i64() {
                Some(n) if n > 0 => n,
                _ => continue,
            };

            // extracted 또는 extract-failed 라벨이 있으면 스킵
            let item_labels: Vec<&str> = item["labels"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|l| l["name"].as_str())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if item_labels.contains(&labels::EXTRACTED)
                || item_labels.contains(&labels::EXTRACT_FAILED)
            {
                continue;
            }

            let work_id = make_work_id(QueueType::Pr, &self.name, number);
            if self.contains(&work_id) {
                continue;
            }

            // PR 상세 정보 조회 (merged 여부 확인)
            let pr_data = gh
                .api_paginate(
                    &self.name,
                    &format!("pulls/{number}"),
                    &[],
                    self.gh_host.as_deref(),
                )
                .await;

            let (merged, head_branch, base_branch, title) = match pr_data {
                Ok(data) => {
                    let pr: serde_json::Value =
                        serde_json::from_slice(&data).unwrap_or(serde_json::Value::Null);
                    (
                        pr["merged"].as_bool().unwrap_or(false),
                        pr["head"]["ref"].as_str().unwrap_or("").to_string(),
                        pr["base"]["ref"].as_str().unwrap_or("main").to_string(),
                        pr["title"].as_str().unwrap_or("").to_string(),
                    )
                }
                Err(_) => continue,
            };

            if !merged {
                continue;
            }

            self.enqueue_pr(
                db,
                number,
                TaskKind::Extract,
                title,
                PrMetadata {
                    head_branch,
                    base_branch,
                    review_comment: None,
                    source_issue_number: None,
                    review_iteration: 0,
                },
            );
            tracing::info!("queued knowledge extraction for merged PR #{number}");
        }

        Ok(())
    }

    // ─── Recovery ───

    /// Orphan `autodev:wip` 라벨 정리.
    ///
    /// pre-fetched issues/pulls 중 wip 라벨이 있지만 큐에 없는 항목을 복구.
    ///
    /// - Issue: wip 라벨 제거 → 다음 scan에서 재발견
    /// - PR: Pending 큐에 재적재 (Label-Positive 모델에서는 라벨 제거 시 재발견 불가)
    pub async fn recover_orphan_wip(&mut self, gh: &dyn Gh, db: &dyn QueueRepository) -> u64 {
        let mut recovered = 0u64;
        let gh_host = self.gh_host.as_deref();

        for issue in self.issues.iter().filter(|i| i.is_wip()) {
            let work_id = make_work_id(QueueType::Issue, &self.name, issue.number);
            if !self.contains(&work_id)
                && gh
                    .label_remove(&self.name, issue.number, labels::WIP, gh_host)
                    .await
            {
                recovered += 1;
                tracing::info!(
                    "recovered orphan issue #{} in {} (removed autodev:wip)",
                    issue.number,
                    self.name
                );
            }
        }

        // PR: Label-Positive — wip 라벨 유지, Pending 큐에 재적재
        let repo = self.repo_ref();
        let orphan_pulls: Vec<_> = self
            .pulls
            .iter()
            .filter(|p| p.is_wip())
            .filter(|p| {
                let work_id = make_work_id(QueueType::Pr, &self.name, p.number);
                !self.queue.contains(&work_id)
            })
            .map(|p| QueueItem::from_pull(&repo, p, TaskKind::Review))
            .collect();

        for item in orphan_pulls {
            tracing::info!(
                "recovered orphan pr #{} in {} (re-queued to Pending)",
                item.github_number,
                self.name
            );
            if self.queue.push(QueuePhase::Pending, item.clone()) {
                if let Err(e) = db.queue_upsert(&item.to_row(QueuePhase::Pending)) {
                    tracing::error!("queue_upsert failed for {}: {e}", item.work_id);
                }
            }
            recovered += 1;
        }

        recovered
    }

    /// Orphan `autodev:implementing` 이슈 복구.
    ///
    /// implementing 라벨이 있지만 큐에 없는 이슈를 찾아:
    /// - pr-link 마커 있고 PR closed/merged → done 전이
    /// - pr-link 마커 없음 → implementing 제거 (다음 scan에서 재시도)
    pub async fn recover_orphan_implementing(&self, gh: &dyn Gh) -> u64 {
        let mut recovered = 0u64;
        let gh_host = self.gh_host.as_deref();

        for issue in self.issues.iter().filter(|i| i.is_implementing()) {
            let work_id = make_work_id(QueueType::Issue, &self.name, issue.number);
            if self.contains(&work_id) {
                continue;
            }

            match extract_pr_link_from_comments(gh, &self.name, issue.number, gh_host).await {
                Some(pr_num) => {
                    let pr_state = get_pr_state(gh, &self.name, pr_num, gh_host).await;
                    match pr_state.as_deref() {
                        Some("closed") | Some("merged") => {
                            // add-first: add DONE before removing IMPLEMENTING
                            gh.label_add(&self.name, issue.number, labels::DONE, gh_host)
                                .await;
                            gh.label_remove(
                                &self.name,
                                issue.number,
                                labels::IMPLEMENTING,
                                gh_host,
                            )
                            .await;
                            recovered += 1;
                            tracing::info!(
                                "recovered implementing issue #{} in {} (PR #{pr_num} {})",
                                issue.number,
                                self.name,
                                pr_state.as_deref().unwrap_or("unknown")
                            );
                        }
                        Some("open") => {
                            // PR is still open but not in queue — ensure wip label
                            // so scan_pulls or recover_orphan_wip can pick it up.
                            let pr_work_id = make_work_id(QueueType::Pr, &self.name, pr_num);
                            if !self.contains(&pr_work_id) {
                                gh.label_add(&self.name, pr_num, labels::WIP, gh_host).await;
                            }
                            // Always remove implementing from the issue to prevent
                            // infinite recovery loops on subsequent polls.
                            gh.label_remove(
                                &self.name,
                                issue.number,
                                labels::IMPLEMENTING,
                                gh_host,
                            )
                            .await;
                            recovered += 1;
                            tracing::info!(
                                "recovered implementing issue #{} in {} (PR #{pr_num} open, removed implementing)",
                                issue.number,
                                self.name,
                            );
                        }
                        _ => {
                            tracing::warn!(
                                "issue #{}: pr-link references PR #{pr_num} in unhandled state {:?}, skipping",
                                issue.number,
                                pr_state,
                            );
                        }
                    }
                }
                None => {
                    gh.label_remove(&self.name, issue.number, labels::IMPLEMENTING, gh_host)
                        .await;
                    recovered += 1;
                    tracing::info!(
                        "recovered orphan implementing issue #{} in {} (no pr-link marker)",
                        issue.number,
                        self.name
                    );
                }
            }
        }

        recovered
    }

    /// 재시작 시 pre-fetched 상태 기반 큐 복구.
    ///
    /// issues/pulls의 라벨 상태에 따라 적절한 큐에 적재한다.
    pub async fn startup_reconcile(&mut self, gh: &dyn Gh, db: &dyn QueueRepository) -> u64 {
        let mut recovered = 0u64;
        let gh_host = self.gh_host.as_deref();
        let repo = self.repo_ref();

        // ── Issues 복구 ──
        for issue in &self.issues {
            if issue.is_terminal() {
                continue;
            }
            if issue.is_analyze() {
                continue;
            }
            if issue.is_analyzed() {
                continue;
            }
            if issue.is_implementing() {
                continue;
            }

            let work_id = make_work_id(QueueType::Issue, &self.name, issue.number);
            if self.contains(&work_id) {
                continue;
            }

            if issue.is_approved() {
                gh.label_remove(&self.name, issue.number, labels::APPROVED_ANALYSIS, gh_host)
                    .await;
                gh.label_remove(&self.name, issue.number, labels::ANALYZED, gh_host)
                    .await;
                gh.label_add(&self.name, issue.number, labels::IMPLEMENTING, gh_host)
                    .await;

                let item = QueueItem::from_issue(&repo, issue, TaskKind::Implement);
                if self.queue.push(QueuePhase::Pending, item.clone()) {
                    let _ = db.queue_upsert(&item.to_row(QueuePhase::Pending));
                }
                recovered += 1;
                continue;
            }

            if issue.is_wip() {
                let item = QueueItem::from_issue(&repo, issue, TaskKind::Analyze);
                if self.queue.push(QueuePhase::Pending, item.clone()) {
                    let _ = db.queue_upsert(&item.to_row(QueuePhase::Pending));
                }
                recovered += 1;
                continue;
            }
        }

        // ── PRs 복구: wip → Pending (리뷰 재개) ──
        for pull in self.pulls.iter().filter(|p| p.is_wip()) {
            if pull.is_terminal() {
                continue;
            }

            let work_id = make_work_id(QueueType::Pr, &self.name, pull.number);
            if self.queue.contains(&work_id) {
                continue;
            }

            let item = QueueItem::from_pull(&repo, pull, TaskKind::Review);
            if self.queue.push(QueuePhase::Pending, item.clone()) {
                let _ = db.queue_upsert(&item.to_row(QueuePhase::Pending));
            }
            recovered += 1;
        }

        // ── PRs 복구: changes-requested → Pending (피드백 반영 재개) ──
        for pull in self.pulls.iter().filter(|p| p.is_changes_requested()) {
            if pull.is_terminal() {
                continue;
            }

            let work_id = make_work_id(QueueType::Pr, &self.name, pull.number);
            if self.queue.contains(&work_id) {
                continue;
            }

            let item = QueueItem::from_pull(&repo, pull, TaskKind::Improve);
            if self.queue.push(QueuePhase::Pending, item.clone()) {
                let _ = db.queue_upsert(&item.to_row(QueuePhase::Pending));
            }
            recovered += 1;
        }

        recovered
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

// ─── Recovery Helpers ───

/// 이슈 코멘트에서 `<!-- autodev:pr-link:{N} -->` 마커를 추출하여 PR 번호 반환
async fn extract_pr_link_from_comments(
    gh: &dyn Gh,
    repo_name: &str,
    number: i64,
    gh_host: Option<&str>,
) -> Option<i64> {
    let jq = r#"[.[] | select(.body | contains("<!-- autodev:pr-link:")) | .body] | last"#;
    let body = gh
        .api_get_field(repo_name, &format!("issues/{number}/comments"), jq, gh_host)
        .await?;
    let start = body.find("<!-- autodev:pr-link:")? + "<!-- autodev:pr-link:".len();
    let end = body[start..].find(" -->").map(|i| start + i)?;
    body[start..end].trim().parse().ok()
}

/// PR의 state를 조회 ("open", "closed", "merged" 등)
async fn get_pr_state(
    gh: &dyn Gh,
    repo_name: &str,
    pr_number: i64,
    gh_host: Option<&str>,
) -> Option<String> {
    let merged = gh
        .api_get_field(repo_name, &format!("pulls/{pr_number}"), ".merged", gh_host)
        .await;
    if merged.as_deref() == Some("true") {
        return Some("merged".to_string());
    }

    gh.api_get_field(repo_name, &format!("pulls/{pr_number}"), ".state", gh_host)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::core::models::QueuePhase;
    use crate::core::phase::TaskKind;
    use crate::core::queue_item::testing::test_repo_named;
    use crate::infra::gh::mock::MockGh;

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

    impl QueueRepository for MockCursorRepo {
        fn queue_get_phase(&self, _: &str) -> Result<Option<QueuePhase>> {
            Ok(None)
        }
        fn queue_advance(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn queue_skip(&self, _: &str, _: Option<&str>) -> Result<()> {
            Ok(())
        }
        fn queue_list_items(
            &self,
            _: Option<&str>,
        ) -> Result<Vec<crate::core::models::QueueItemRow>> {
            Ok(vec![])
        }
        fn queue_upsert(&self, _: &crate::core::models::QueueItemRow) -> Result<()> {
            Ok(())
        }
        fn queue_remove(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn queue_load_active(&self, _: &str) -> Result<Vec<crate::core::models::QueueItemRow>> {
            Ok(vec![])
        }
        fn queue_transit(&self, _: &str, _: QueuePhase, _: QueuePhase) -> Result<bool> {
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

    fn issue_item(repo_name: &str, number: i64) -> QueueItem {
        let repo = test_repo_named(repo_name);
        QueueItem::new_issue(
            &repo,
            number,
            TaskKind::Analyze,
            format!("Issue #{number}"),
            None,
            vec![],
            "user".into(),
        )
    }

    fn pr_item(repo_name: &str, number: i64) -> QueueItem {
        let repo = test_repo_named(repo_name);
        QueueItem::new_pr(
            &repo,
            number,
            TaskKind::Review,
            format!("PR #{number}"),
            PrMetadata {
                head_branch: "feature".into(),
                base_branch: "main".into(),
                review_comment: None,
                source_issue_number: None,
                review_iteration: 0,
            },
        )
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
    fn contains_checks_queue() {
        let mut repo = make_repo();

        let i = issue_item("org/repo", 42);
        let p = pr_item("org/repo", 10);

        repo.queue.push(QueuePhase::Pending, i);
        repo.queue.push(QueuePhase::Pending, p);

        assert!(repo.contains("issue:org/repo:42"));
        assert!(repo.contains("pr:org/repo:10"));
        assert!(!repo.contains("issue:org/repo:99"));
    }

    #[test]
    fn total_items_sums_all_queues() {
        let mut repo = make_repo();
        assert_eq!(repo.total_items(), 0);

        repo.queue
            .push(QueuePhase::Pending, issue_item("org/repo", 1));
        repo.queue
            .push(QueuePhase::Pending, issue_item("org/repo", 2));
        repo.queue.push(QueuePhase::Pending, pr_item("org/repo", 3));

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
        assert_eq!(repo.queue.len(QueuePhase::Pending), 1);
        let item = repo.queue.pop(QueuePhase::Pending).unwrap();
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

        assert_eq!(repo.queue.len(QueuePhase::Pending), 0);
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
        repo.queue
            .push(QueuePhase::Pending, issue_item("org/repo", 1));

        repo.scan_issues(&gh, &db, &[], &None).await.unwrap();

        // Still only 1 item (no duplicate)
        assert_eq!(repo.queue.len(QueuePhase::Pending), 1);
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
        assert_eq!(repo.queue.len(QueuePhase::Pending), 1);
        let item = repo.queue.pop(QueuePhase::Pending).unwrap();
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
        repo.scan_approved_issues(&gh, &MockCursorRepo::new())
            .await
            .unwrap();

        assert_eq!(repo.queue.len(QueuePhase::Pending), 1);
        let item = repo.queue.pop(QueuePhase::Pending).unwrap();
        assert_eq!(item.github_number, 5);
        assert_eq!(item.task_kind, TaskKind::Implement);

        // Label transitions: implementing added first, then old labels removed
        let removed = gh.removed_labels.lock().unwrap();
        assert_eq!(removed.len(), 2);
        assert!(removed.iter().any(|r| r.2 == "autodev:approved-analysis"));
        assert!(removed.iter().any(|r| r.2 == "autodev:analyzed"));

        let added = gh.added_labels.lock().unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].2, "autodev:implementing");

        // DESIGN-v3: add-first 순서 검증
        gh.assert_add_before_remove(5, labels::IMPLEMENTING, labels::APPROVED_ANALYSIS);
        gh.assert_add_before_remove(5, labels::IMPLEMENTING, labels::ANALYZED);
    }

    #[tokio::test]
    async fn scan_pulls_queues_wip_labeled_prs() {
        let gh = MockGh::new();

        // issues endpoint (labels=autodev:wip) returns PR with pull_request field
        let issues_json = serde_json::json!([
            {
                "number": 10,
                "title": "fix bug",
                "user": {"login": "alice"},
                "labels": [{"name": "autodev:wip"}],
                "pull_request": {}
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        // PR detail endpoint
        let pr_detail = serde_json::json!({
            "head": {"ref": "fix-bug"},
            "base": {"ref": "main"},
            "title": "fix bug"
        });
        gh.set_paginate(
            "org/repo",
            "pulls/10",
            serde_json::to_vec(&pr_detail).unwrap(),
        );

        let mut repo = make_repo();
        repo.scan_pulls(&gh, &MockCursorRepo::new(), &[])
            .await
            .unwrap();

        assert_eq!(repo.queue.len(QueuePhase::Pending), 1);
        let item = repo.queue.pop(QueuePhase::Pending).unwrap();
        assert_eq!(item.github_number, 10);
        assert_eq!(item.head_branch(), Some("fix-bug"));
        assert_eq!(item.task_kind, TaskKind::Review);

        // No wip label added (already has it — Label-Positive)
        let added = gh.added_labels.lock().unwrap();
        assert!(added.is_empty());
    }

    #[tokio::test]
    async fn scan_pulls_skips_already_queued_prs() {
        let gh = MockGh::new();

        let issues_json = serde_json::json!([
            {
                "number": 10,
                "title": "already queued",
                "user": {"login": "alice"},
                "labels": [{"name": "autodev:wip"}],
                "pull_request": {}
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        let mut repo = make_repo();
        // Pre-fill queue to simulate already-queued state
        repo.queue.push(
            QueuePhase::Running,
            QueueItem::new_pr(
                &test_repo_named("org/repo"),
                10,
                TaskKind::Review,
                "already queued".to_string(),
                PrMetadata {
                    head_branch: "fix-bug".to_string(),
                    base_branch: "main".to_string(),
                    review_comment: None,
                    source_issue_number: None,
                    review_iteration: 0,
                },
            ),
        );

        repo.scan_pulls(&gh, &MockCursorRepo::new(), &[])
            .await
            .unwrap();

        // Should not add a second copy
        assert_eq!(repo.queue.len(QueuePhase::Pending), 0);
    }

    #[tokio::test]
    async fn scan_pulls_skips_ignored_authors() {
        let gh = MockGh::new();

        let issues_json = serde_json::json!([
            {
                "number": 10,
                "title": "renovate update",
                "user": {"login": "renovate"},
                "labels": [{"name": "autodev:wip"}],
                "pull_request": {}
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        let mut repo = make_repo();
        repo.scan_pulls(&gh, &MockCursorRepo::new(), &["renovate".to_string()])
            .await
            .unwrap();

        assert_eq!(repo.queue.len(QueuePhase::Pending), 0);
    }

    // ═══════════════════════════════════════════════════
    // Recovery Tests
    // ═══════════════════════════════════════════════════

    fn make_repo_with_state(issues: Vec<RepoIssue>, pulls: Vec<RepoPull>) -> GitRepository {
        let mut repo = make_repo();
        repo.set_github_state(issues, pulls);
        repo
    }

    fn issue_from_json(v: serde_json::Value) -> RepoIssue {
        RepoIssue::from_json(&v).expect("valid issue JSON")
    }

    fn pull_from_json(v: serde_json::Value) -> RepoPull {
        RepoPull::from_json(&v).expect("valid pull JSON")
    }

    #[tokio::test]
    async fn recover_orphan_wip_removes_label_from_unqueued_issues() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![issue_from_json(serde_json::json!({
                "number": 1, "title": "Orphan WIP",
                "labels": [{"name": "autodev:wip"}],
                "user": {"login": "alice"}
            }))],
            vec![],
        );

        let recovered = repo.recover_orphan_wip(&gh, &MockCursorRepo::new()).await;

        assert_eq!(recovered, 1);
        let removed = gh.removed_labels.lock().unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(
            removed[0],
            ("org/repo".to_string(), 1, "autodev:wip".to_string())
        );
    }

    #[tokio::test]
    async fn recover_orphan_wip_keeps_queued_items() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![issue_from_json(serde_json::json!({
                "number": 1, "title": "Queued WIP",
                "labels": [{"name": "autodev:wip"}],
                "user": {"login": "alice"}
            }))],
            vec![],
        );

        // Pre-populate queue
        repo.queue
            .push(QueuePhase::Pending, issue_item("org/repo", 1));

        let recovered = repo.recover_orphan_wip(&gh, &MockCursorRepo::new()).await;

        assert_eq!(recovered, 0);
        assert!(gh.removed_labels.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn recover_orphan_wip_requeues_prs_to_pending() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![],
            vec![pull_from_json(serde_json::json!({
                "number": 10, "title": "Orphan PR",
                "labels": [{"name": "autodev:wip"}],
                "head": {"ref": "fix"}, "base": {"ref": "main"},
                "user": {"login": "bob"}
            }))],
        );

        let recovered = repo.recover_orphan_wip(&gh, &MockCursorRepo::new()).await;

        assert_eq!(recovered, 1);
        // PR은 라벨 제거 대신 Pending 큐에 재적재 (Label-Positive)
        assert!(repo.contains("pr:org/repo:10"));
        assert_eq!(repo.queue.len(QueuePhase::Pending), 1);
        // wip 라벨은 유지
        assert!(gh.removed_labels.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn recover_orphan_implementing_no_pr_link_removes_label() {
        let gh = MockGh::new();
        let repo = make_repo_with_state(
            vec![issue_from_json(serde_json::json!({
                "number": 5, "title": "Implementing",
                "labels": [{"name": "autodev:implementing"}],
                "user": {"login": "alice"}
            }))],
            vec![],
        );
        // No pr-link comment set → extract_pr_link_from_comments returns None

        let recovered = repo.recover_orphan_implementing(&gh).await;

        assert_eq!(recovered, 1);
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed.iter().any(|r| r.2 == "autodev:implementing"));
    }

    #[tokio::test]
    async fn recover_orphan_implementing_with_merged_pr_transitions_to_done() {
        let gh = MockGh::new();
        let repo = make_repo_with_state(
            vec![issue_from_json(serde_json::json!({
                "number": 5, "title": "Implementing",
                "labels": [{"name": "autodev:implementing"}],
                "user": {"login": "alice"}
            }))],
            vec![],
        );

        // Set up pr-link comment
        gh.set_field(
            "org/repo",
            "issues/5/comments",
            r#"[.[] | select(.body | contains("<!-- autodev:pr-link:")) | .body] | last"#,
            "some text <!-- autodev:pr-link:42 --> more text",
        );
        // PR is merged
        gh.set_field("org/repo", "pulls/42", ".merged", "true");

        let recovered = repo.recover_orphan_implementing(&gh).await;

        assert_eq!(recovered, 1);
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed.iter().any(|r| r.2 == "autodev:implementing"));
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|r| r.2 == "autodev:done"));

        // DESIGN-v3: add-first 순서 검증
        gh.assert_add_before_remove(5, labels::DONE, labels::IMPLEMENTING);
    }

    #[tokio::test]
    async fn recover_orphan_implementing_with_open_pr_adds_wip_and_removes_implementing() {
        let gh = MockGh::new();
        let repo = make_repo_with_state(
            vec![issue_from_json(serde_json::json!({
                "number": 5, "title": "Implementing",
                "labels": [{"name": "autodev:implementing"}],
                "user": {"login": "alice"}
            }))],
            vec![],
        );

        // Set up pr-link comment
        gh.set_field(
            "org/repo",
            "issues/5/comments",
            r#"[.[] | select(.body | contains("<!-- autodev:pr-link:")) | .body] | last"#,
            "some text <!-- autodev:pr-link:42 --> more text",
        );
        // PR is open (not merged)
        gh.set_field("org/repo", "pulls/42", ".merged", "false");
        gh.set_field("org/repo", "pulls/42", ".state", "open");

        let recovered = repo.recover_orphan_implementing(&gh).await;

        assert_eq!(recovered, 1);
        // wip label added to PR
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|r| r.1 == 42 && r.2 == "autodev:wip"));
        // implementing label removed from issue (prevents infinite recovery loop)
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|r| r.1 == 5 && r.2 == "autodev:implementing"));
    }

    // ═══════════════════════════════════════════════════
    // startup_reconcile Tests
    // ═══════════════════════════════════════════════════

    #[tokio::test]
    async fn reconcile_skips_unlabeled_issues() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![issue_from_json(serde_json::json!({
                "number": 10, "title": "No label",
                "labels": [], "user": {"login": "alice"}
            }))],
            vec![],
        );

        let result = repo.startup_reconcile(&gh, &MockCursorRepo::new()).await;
        assert_eq!(result, 0);
        assert!(!repo.contains("issue:org/repo:10"));
    }

    #[tokio::test]
    async fn reconcile_skips_terminal_issues() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![
                issue_from_json(serde_json::json!({
                    "number": 1, "title": "Done",
                    "labels": [{"name": "autodev:done"}], "user": {"login": "a"}
                })),
                issue_from_json(serde_json::json!({
                    "number": 2, "title": "Skip",
                    "labels": [{"name": "autodev:skip"}], "user": {"login": "a"}
                })),
            ],
            vec![],
        );

        let result = repo.startup_reconcile(&gh, &MockCursorRepo::new()).await;
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn reconcile_recovers_wip_issue_to_pending() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![issue_from_json(serde_json::json!({
                "number": 42, "title": "Orphan WIP",
                "labels": [{"name": "autodev:wip"}], "user": {"login": "alice"}
            }))],
            vec![],
        );

        let result = repo.startup_reconcile(&gh, &MockCursorRepo::new()).await;

        assert_eq!(result, 1);
        assert!(repo.contains("issue:org/repo:42"));
        assert_eq!(repo.queue.len(QueuePhase::Pending), 1);

        // wip label not touched (kept as-is)
        assert!(gh.removed_labels.lock().unwrap().is_empty());
        assert!(gh.added_labels.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reconcile_recovers_approved_to_ready() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![issue_from_json(serde_json::json!({
                "number": 3, "title": "Approved",
                "labels": [{"name": "autodev:approved-analysis"}],
                "user": {"login": "a"}
            }))],
            vec![],
        );

        let result = repo.startup_reconcile(&gh, &MockCursorRepo::new()).await;

        assert_eq!(result, 1);
        assert!(repo.contains("issue:org/repo:3"));
        assert_eq!(repo.queue.len(QueuePhase::Pending), 1);

        let added = gh.added_labels.lock().unwrap();
        assert!(added
            .iter()
            .any(|(_, n, l)| *n == 3 && l == "autodev:implementing"));

        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, n, l)| *n == 3 && l == "autodev:approved-analysis"));
    }

    #[tokio::test]
    async fn reconcile_recovers_wip_pr_to_pending() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![],
            vec![pull_from_json(serde_json::json!({
                "number": 20, "title": "WIP PR",
                "labels": [{"name": "autodev:wip"}],
                "head": {"ref": "feat/test"}, "base": {"ref": "main"},
                "user": {"login": "bob"}
            }))],
        );

        let result = repo.startup_reconcile(&gh, &MockCursorRepo::new()).await;

        assert_eq!(result, 1);
        assert!(repo.contains("pr:org/repo:20"));
        assert_eq!(repo.queue.len(QueuePhase::Pending), 1);
    }

    #[tokio::test]
    async fn reconcile_skips_unlabeled_prs() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![],
            vec![pull_from_json(serde_json::json!({
                "number": 20, "title": "No label PR",
                "labels": [],
                "head": {"ref": "feat/test"}, "base": {"ref": "main"},
                "user": {"login": "bob"}
            }))],
        );

        let result = repo.startup_reconcile(&gh, &MockCursorRepo::new()).await;
        assert_eq!(result, 0);
        assert!(!repo.contains("pr:org/repo:20"));
    }

    #[tokio::test]
    async fn reconcile_skips_already_queued() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![issue_from_json(serde_json::json!({
                "number": 10, "title": "Already queued",
                "labels": [{"name": "autodev:wip"}], "user": {"login": "a"}
            }))],
            vec![],
        );

        repo.queue
            .push(QueuePhase::Pending, issue_item("org/repo", 10));

        let result = repo.startup_reconcile(&gh, &MockCursorRepo::new()).await;
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn reconcile_skips_analyzed_and_implementing() {
        let gh = MockGh::new();
        let mut repo = make_repo_with_state(
            vec![
                issue_from_json(serde_json::json!({
                    "number": 1, "title": "Analyzed",
                    "labels": [{"name": "autodev:analyzed"}], "user": {"login": "a"}
                })),
                issue_from_json(serde_json::json!({
                    "number": 2, "title": "Implementing",
                    "labels": [{"name": "autodev:implementing"}], "user": {"login": "a"}
                })),
            ],
            vec![],
        );

        let result = repo.startup_reconcile(&gh, &MockCursorRepo::new()).await;
        assert_eq!(result, 0);
    }
}
