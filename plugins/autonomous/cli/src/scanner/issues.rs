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
    gh_host: Option<&str>,
) -> Result<()> {
    // 마지막 스캔 시점 이후의 이슈만
    let since = db.cursor_get_last_seen(repo_id, "issues")?;

    let mut args = vec![
        "api".to_string(),
        format!("repos/{repo_name}/issues"),
        "--paginate".to_string(),
        "--method".to_string(), "GET".to_string(),
        "-f".to_string(), "state=open".to_string(),
        "-f".to_string(), "sort=updated".to_string(),
        "-f".to_string(), "direction=desc".to_string(),
        "-f".to_string(), "per_page=30".to_string(),
    ];

    if let Some(ref s) = since {
        args.push("-f".to_string());
        args.push(format!("since={s}"));
    }

    // GitHub Enterprise: --hostname 추가
    if let Some(host) = gh_host {
        args.push("--hostname".to_string());
        args.push(host.to_string());
    }

    let output = tokio::process::Command::new("gh")
        .args(&args)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh api error: {stderr}");
    }

    let issues: Vec<GitHubIssue> = serde_json::from_slice(&output.stdout)?;
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
