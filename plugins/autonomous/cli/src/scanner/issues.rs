use anyhow::Result;
use serde::Deserialize;

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
    pull_request: Option<serde_json::Value>, // PR이면 Some
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
    repo_id: &str,
    repo_name: &str,
    ignore_authors: &[String],
    filter_labels: &Option<Vec<String>>,
) -> Result<()> {
    let token = std::env::var("GITHUB_TOKEN")
        .or_else(|_| get_gh_token())
        .map_err(|_| anyhow::anyhow!("GITHUB_TOKEN not set and gh auth not available"))?;

    // 마지막 스캔 시점 이후의 이슈만
    let since = db.cursor_get_last_seen(repo_id, "issues")?;
    let url = format!(
        "https://api.github.com/repos/{repo_name}/issues?state=open&sort=updated&direction=desc&per_page=30{}",
        since.as_ref().map(|s| format!("&since={s}")).unwrap_or_default()
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "autonomous-cli")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("GitHub API error: {}", response.status());
    }

    let issues: Vec<GitHubIssue> = response.json().await?;
    let mut latest_updated = since;

    for issue in &issues {
        // PR은 건너뜀
        if issue.pull_request.is_some() {
            continue;
        }

        // 작성자 필터
        if ignore_authors.contains(&issue.user.login) {
            continue;
        }

        // 라벨 필터
        if let Some(labels) = filter_labels {
            let issue_labels: Vec<&str> = issue.labels.iter().map(|l| l.name.as_str()).collect();
            if !labels.iter().any(|l| issue_labels.contains(&l.as_str())) {
                continue;
            }
        }

        // 이미 큐에 있는지 확인
        let exists = db.issue_exists(repo_id, issue.number)?;

        if !exists {
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
            tracing::info!("queued issue #{}: {}", issue.number, issue.title);
        }

        // 최신 updated_at 추적
        if latest_updated.as_ref().map_or(true, |l| issue.updated_at > *l) {
            latest_updated = Some(issue.updated_at.clone());
        }
    }

    // 스캔 커서 업데이트
    if let Some(last_seen) = latest_updated {
        db.cursor_upsert(repo_id, "issues", &last_seen)?;
    }

    Ok(())
}

/// `gh auth token`으로 토큰 획득
fn get_gh_token() -> Result<String, anyhow::Error> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    } else {
        anyhow::bail!("gh auth token failed")
    }
}
