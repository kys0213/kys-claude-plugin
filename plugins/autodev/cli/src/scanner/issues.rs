use anyhow::Result;
use serde::Deserialize;

use crate::infrastructure::gh::Gh;
use crate::queue::repository::*;
use crate::queue::task_queues::TaskQueues;
use crate::queue::task_queues::{issue_phase, labels, make_work_id, IssueItem};
use crate::queue::Database;

#[derive(Debug, Deserialize)]
struct GitHubIssue {
    number: i64,
    title: String,
    body: Option<String>,
    labels: Vec<GitHubLabel>,
    user: GitHubUser,
    pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GitHubLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    login: String,
}

/// Label-Positive: `autodev:analyze` 트리거 라벨이 있는 이슈만 스캔하여
/// TaskQueues에 추가 + `analyze→wip` 라벨 전이
#[allow(clippy::too_many_arguments)]
pub async fn scan(
    db: &Database,
    gh: &dyn Gh,
    repo_id: &str,
    repo_name: &str,
    repo_url: &str,
    ignore_authors: &[String],
    filter_labels: &Option<Vec<String>>,
    gh_host: Option<&str>,
    queues: &mut TaskQueues,
) -> Result<()> {
    // Label-Positive: autodev:analyze 라벨이 있는 이슈만 조회
    let params: Vec<(&str, &str)> = vec![
        ("state", "open"),
        ("labels", labels::ANALYZE),
        ("per_page", "30"),
    ];

    let stdout = gh
        .api_paginate(repo_name, "issues", &params, gh_host)
        .await?;

    let issues: Vec<GitHubIssue> = serde_json::from_slice(&stdout)?;

    for issue in &issues {
        // PR은 issues API에 포함되므로 제외
        if issue.pull_request.is_some() {
            continue;
        }

        if ignore_authors.contains(&issue.user.login) {
            continue;
        }

        // filter_labels 추가 필터 (트리거 라벨과 별개의 안전장치)
        if let Some(labels) = filter_labels {
            let issue_labels: Vec<&str> = issue.labels.iter().map(|l| l.name.as_str()).collect();
            if !labels.iter().any(|l| issue_labels.contains(&l.as_str())) {
                continue;
            }
        }

        let work_id = make_work_id("issue", repo_name, issue.number);

        // 이미 큐에 있으면 skip (O(1) dedup)
        if queues.contains(&work_id) {
            continue;
        }

        let label_names: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

        let item = IssueItem {
            work_id,
            repo_id: repo_id.to_string(),
            repo_name: repo_name.to_string(),
            repo_url: repo_url.to_string(),
            github_number: issue.number,
            title: issue.title.clone(),
            body: issue.body.clone(),
            labels: label_names,
            author: issue.user.login.clone(),
            analysis_report: None,
        };

        // 라벨 전이: analyze 제거 → wip 추가 (트리거 소비)
        gh.label_remove(repo_name, issue.number, labels::ANALYZE, gh_host)
            .await;
        gh.label_add(repo_name, issue.number, labels::WIP, gh_host)
            .await;
        queues.issues.push(issue_phase::PENDING, item);
        tracing::info!("issue #{}: autodev:analyze → wip (Pending)", issue.number);
    }

    // cursor 업데이트 (scan interval 제어용)
    let now = chrono::Utc::now().to_rfc3339();
    db.cursor_upsert(repo_id, "issues", &now)?;

    Ok(())
}

/// v2: `autodev:approved-analysis` 라벨이 있는 이슈를 스캔하여 Ready 큐에 적재
///
/// 사람이 분석 리뷰를 승인하면(`approved-analysis` 라벨 추가),
/// 이 함수가 해당 이슈를 감지하여 구현 큐(Ready)에 넣는다.
///
/// 1. `autodev:approved-analysis` 라벨 제거
/// 2. `autodev:implementing` 라벨 추가
/// 3. `IssueItem` 생성 → Ready 큐 push
#[allow(clippy::too_many_arguments)]
pub async fn scan_approved(
    gh: &dyn Gh,
    repo_id: &str,
    repo_name: &str,
    repo_url: &str,
    gh_host: Option<&str>,
    queues: &mut TaskQueues,
) -> Result<()> {
    // approved-analysis 라벨이 있는 open 이슈 조회
    let params: Vec<(&str, &str)> = vec![
        ("state", "open"),
        ("labels", labels::APPROVED_ANALYSIS),
        ("per_page", "30"),
    ];

    let stdout = gh
        .api_paginate(repo_name, "issues", &params, gh_host)
        .await?;

    let issues: Vec<GitHubIssue> = serde_json::from_slice(&stdout)?;

    for issue in &issues {
        // PR 제외
        if issue.pull_request.is_some() {
            continue;
        }

        let work_id = make_work_id("issue", repo_name, issue.number);

        // 이미 큐에 있으면 skip
        if queues.contains(&work_id) {
            continue;
        }

        // approved-analysis 라벨 제거 + implementing 라벨 추가
        gh.label_remove(repo_name, issue.number, labels::APPROVED_ANALYSIS, gh_host)
            .await;
        gh.label_remove(repo_name, issue.number, labels::ANALYZED, gh_host)
            .await;
        gh.label_add(repo_name, issue.number, labels::IMPLEMENTING, gh_host)
            .await;

        // 분석 리포트는 에이전트가 gh CLI로 직접 코멘트를 확인
        let analysis_report = None;

        let label_names: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

        let item = IssueItem {
            work_id,
            repo_id: repo_id.to_string(),
            repo_name: repo_name.to_string(),
            repo_url: repo_url.to_string(),
            github_number: issue.number,
            title: issue.title.clone(),
            body: issue.body.clone(),
            labels: label_names,
            author: issue.user.login.clone(),
            analysis_report,
        };

        queues.issues.push(issue_phase::READY, item);
        tracing::info!(
            "queued approved issue #{}: {} → Ready",
            issue.number,
            issue.title
        );
    }

    Ok(())
}
