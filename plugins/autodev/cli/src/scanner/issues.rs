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
    updated_at: String,
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

/// autodev: 라벨이 있는지 확인 (done/skip/wip)
fn has_autodev_label(issue_labels: &[GitHubLabel]) -> bool {
    issue_labels.iter().any(|l| l.name.starts_with("autodev:"))
}

/// GitHub Issues를 스캔하여 TaskQueues에 추가 + autodev:wip 라벨 설정
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
    let since = db.cursor_get_last_seen(repo_id, "issues")?;

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
        .api_paginate(repo_name, "issues", &params, gh_host)
        .await?;

    let issues: Vec<GitHubIssue> = serde_json::from_slice(&stdout)?;
    let mut latest_updated = since;

    for issue in &issues {
        // PR은 issues API에 포함되므로 제외
        if issue.pull_request.is_some() {
            continue;
        }

        if ignore_authors.contains(&issue.user.login) {
            continue;
        }

        // autodev: 라벨이 이미 있으면 skip (done/skip/wip 모두)
        if has_autodev_label(&issue.labels) {
            if latest_updated
                .as_ref()
                .is_none_or(|l| issue.updated_at > *l)
            {
                latest_updated = Some(issue.updated_at.clone());
            }
            continue;
        }

        if let Some(labels) = filter_labels {
            let issue_labels: Vec<&str> = issue.labels.iter().map(|l| l.name.as_str()).collect();
            if !labels.iter().any(|l| issue_labels.contains(&l.as_str())) {
                continue;
            }
        }

        let work_id = make_work_id("issue", repo_name, issue.number);

        // 이미 큐에 있으면 skip (O(1) dedup)
        if queues.contains(&work_id) {
            if latest_updated
                .as_ref()
                .is_none_or(|l| issue.updated_at > *l)
            {
                latest_updated = Some(issue.updated_at.clone());
            }
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

        // autodev:wip 라벨 추가 + 큐에 push
        gh.label_add(repo_name, issue.number, labels::WIP, gh_host)
            .await;
        queues.issues.push(issue_phase::PENDING, item);
        tracing::info!("queued issue #{}: {}", issue.number, issue.title);

        if latest_updated
            .as_ref()
            .is_none_or(|l| issue.updated_at > *l)
        {
            latest_updated = Some(issue.updated_at.clone());
        }
    }

    if let Some(last_seen) = latest_updated {
        db.cursor_upsert(repo_id, "issues", &last_seen)?;
    }

    Ok(())
}
