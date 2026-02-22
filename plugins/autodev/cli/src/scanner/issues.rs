use anyhow::Result;
use serde::Deserialize;

use crate::active::ActiveItems;
use crate::infrastructure::gh::Gh;
use crate::queue::models::*;
use crate::queue::repository::*;
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

/// GitHub Issues를 스캔하여 큐에 추가
pub async fn scan(
    db: &Database,
    gh: &dyn Gh,
    repo_id: &str,
    repo_name: &str,
    ignore_authors: &[String],
    filter_labels: &Option<Vec<String>>,
    gh_host: Option<&str>,
    active: &mut ActiveItems,
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
        if issue.pull_request.is_some() {
            continue;
        }

        if ignore_authors.contains(&issue.user.login) {
            continue;
        }

        if let Some(labels) = filter_labels {
            let issue_labels: Vec<&str> = issue.labels.iter().map(|l| l.name.as_str()).collect();
            if !labels.iter().any(|l| issue_labels.contains(&l.as_str())) {
                continue;
            }
        }

        if active.contains("issue", repo_id, issue.number) {
            if latest_updated
                .as_ref()
                .map_or(true, |l| issue.updated_at > *l)
            {
                latest_updated = Some(issue.updated_at.clone());
            }
            continue;
        }

        let exists = db.issue_exists(repo_id, issue.number)?;

        if exists {
            active.insert("issue", repo_id, issue.number);
        } else {
            let labels_json = serde_json::to_string(
                &issue.labels.iter().map(|l| &l.name).collect::<Vec<_>>(),
            )?;

            let item = NewIssueItem {
                repo_id: repo_id.to_string(),
                github_number: issue.number,
                title: issue.title.clone(),
                body: issue.body.clone(),
                labels: labels_json,
                author: issue.user.login.clone(),
            };

            db.issue_insert(&item)?;
            active.insert("issue", repo_id, issue.number);
            tracing::info!("queued issue #{}: {}", issue.number, issue.title);
        }

        if latest_updated
            .as_ref()
            .map_or(true, |l| issue.updated_at > *l)
        {
            latest_updated = Some(issue.updated_at.clone());
        }
    }

    if let Some(last_seen) = latest_updated {
        db.cursor_upsert(repo_id, "issues", &last_seen)?;
    }

    Ok(())
}
