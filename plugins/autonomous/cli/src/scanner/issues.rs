use anyhow::Result;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

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
    let since = get_last_seen(db, repo_id, "issues")?;
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
    let conn = db.conn();
    let now = Utc::now().to_rfc3339();
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
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM issue_queue WHERE repo_id = ?1 AND github_number = ?2",
            rusqlite::params![repo_id, issue.number],
            |row| row.get(0),
        )?;

        if !exists {
            let id = Uuid::new_v4().to_string();
            let labels_json = serde_json::to_string(
                &issue.labels.iter().map(|l| &l.name).collect::<Vec<_>>(),
            )?;

            conn.execute(
                "INSERT INTO issue_queue (id, repo_id, github_number, title, body, labels, author, status, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, ?8)",
                rusqlite::params![
                    id,
                    repo_id,
                    issue.number,
                    issue.title,
                    issue.body,
                    labels_json,
                    issue.user.login,
                    now,
                ],
            )?;

            tracing::info!("queued issue #{}: {}", issue.number, issue.title);
        }

        // 최신 updated_at 추적
        if latest_updated.as_ref().map_or(true, |l| issue.updated_at > *l) {
            latest_updated = Some(issue.updated_at.clone());
        }
    }

    // 스캔 커서 업데이트
    if let Some(last_seen) = latest_updated {
        conn.execute(
            "INSERT OR REPLACE INTO scan_cursors (repo_id, target, last_seen, last_scan) VALUES (?1, 'issues', ?2, ?3)",
            rusqlite::params![repo_id, last_seen, now],
        )?;
    }

    Ok(())
}

fn get_last_seen(db: &Database, repo_id: &str, target: &str) -> Result<Option<String>> {
    let result = db.conn().query_row(
        "SELECT last_seen FROM scan_cursors WHERE repo_id = ?1 AND target = ?2",
        rusqlite::params![repo_id, target],
        |row| row.get(0),
    );
    Ok(result.ok())
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
