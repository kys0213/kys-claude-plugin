use anyhow::Result;
use serde::Deserialize;

use crate::active::ActiveItems;
use crate::infrastructure::gh::Gh;
use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::Database;

#[derive(Debug, Deserialize)]
struct GitHubPR {
    number: i64,
    title: String,
    body: Option<String>,
    user: GitHubUser,
    head: GitHubBranch,
    base: GitHubBranch,
    updated_at: String,
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

/// GitHub PRs를 스캔하여 큐에 추가
pub async fn scan(
    db: &Database,
    gh: &dyn Gh,
    repo_id: &str,
    repo_name: &str,
    ignore_authors: &[String],
    gh_host: Option<&str>,
    active: &mut ActiveItems,
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

        if active.contains("pr", repo_id, pr.number) {
            if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
                latest_updated = Some(pr.updated_at.clone());
            }
            continue;
        }

        let exists = db.pr_exists(repo_id, pr.number)?;

        if exists {
            active.insert("pr", repo_id, pr.number);
        } else {
            let item = NewPrItem {
                repo_id: repo_id.to_string(),
                github_number: pr.number,
                title: pr.title.clone(),
                body: pr.body.clone(),
                author: pr.user.login.clone(),
                head_branch: pr.head.ref_name.clone(),
                base_branch: pr.base.ref_name.clone(),
            };

            db.pr_insert(&item)?;
            active.insert("pr", repo_id, pr.number);
            tracing::info!("queued PR #{}: {}", pr.number, pr.title);
        }

        if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
            latest_updated = Some(pr.updated_at.clone());
        }
    }

    if let Some(last_seen) = latest_updated {
        db.cursor_upsert(repo_id, "pulls", &last_seen)?;
    }

    Ok(())
}
