use anyhow::Result;
use serde::Deserialize;

use crate::infrastructure::gh::Gh;
use crate::queue::repository::*;
use crate::queue::task_queues::{labels, make_work_id, pr_phase, PrItem, TaskQueues};
use crate::queue::Database;

#[derive(Debug, Deserialize)]
struct GitHubPR {
    number: i64,
    title: String,
    #[allow(dead_code)]
    body: Option<String>,
    user: GitHubUser,
    head: GitHubBranch,
    base: GitHubBranch,
    updated_at: String,
    #[serde(default)]
    labels: Vec<GitHubLabel>,
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    login: String,
}

#[derive(Debug, Deserialize)]
struct GitHubBranch {
    #[serde(rename = "ref")]
    ref_name: String,
}

#[derive(Debug, Deserialize)]
struct GitHubLabel {
    name: String,
}

/// autodev: 라벨이 있는지 확인 (done/skip/wip)
fn has_autodev_label(pr_labels: &[GitHubLabel]) -> bool {
    pr_labels.iter().any(|l| l.name.starts_with("autodev:"))
}

/// GitHub PRs를 스캔하여 TaskQueues에 추가 + autodev:wip 라벨 설정
#[allow(clippy::too_many_arguments)]
pub async fn scan(
    db: &Database,
    gh: &dyn Gh,
    repo_id: &str,
    repo_name: &str,
    repo_url: &str,
    ignore_authors: &[String],
    gh_host: Option<&str>,
    queues: &mut TaskQueues,
) -> Result<()> {
    let since = db.cursor_get_last_seen(repo_id, "pulls")?;

    let params: Vec<(&str, &str)> = vec![
        ("state", "open"),
        ("sort", "updated"),
        ("direction", "desc"),
        ("per_page", "30"),
    ];

    let stdout = gh
        .api_paginate(repo_name, "pulls", &params, gh_host)
        .await?;

    let prs: Vec<GitHubPR> = serde_json::from_slice(&stdout)?;
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

        // autodev: 라벨이 이미 있으면 skip (done/skip/wip 모두)
        if has_autodev_label(&pr.labels) {
            if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
                latest_updated = Some(pr.updated_at.clone());
            }
            continue;
        }

        let work_id = make_work_id("pr", repo_name, pr.number);

        // 이미 큐에 있으면 skip (O(1) dedup)
        if queues.contains(&work_id) {
            if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
                latest_updated = Some(pr.updated_at.clone());
            }
            continue;
        }

        let item = PrItem {
            work_id,
            repo_id: repo_id.to_string(),
            repo_name: repo_name.to_string(),
            repo_url: repo_url.to_string(),
            github_number: pr.number,
            title: pr.title.clone(),
            head_branch: pr.head.ref_name.clone(),
            base_branch: pr.base.ref_name.clone(),
            review_comment: None,
        };

        // autodev:wip 라벨 추가 + 큐에 push
        gh.label_add(repo_name, pr.number, labels::WIP, gh_host)
            .await;
        queues.prs.push(pr_phase::PENDING, item);
        tracing::info!("queued PR #{}: {}", pr.number, pr.title);

        if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
            latest_updated = Some(pr.updated_at.clone());
        }
    }

    if let Some(last_seen) = latest_updated {
        db.cursor_upsert(repo_id, "pulls", &last_seen)?;
    }

    Ok(())
}
