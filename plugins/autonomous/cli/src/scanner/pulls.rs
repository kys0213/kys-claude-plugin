use anyhow::Result;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

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
    repo_id: &str,
    repo_name: &str,
    ignore_authors: &[String],
) -> Result<()> {
    let token = std::env::var("GITHUB_TOKEN")
        .or_else(|_| get_gh_token())
        .map_err(|_| anyhow::anyhow!("GITHUB_TOKEN not set"))?;

    let since = get_last_seen(db, repo_id)?;
    let url = format!(
        "https://api.github.com/repos/{repo_name}/pulls?state=open&sort=updated&direction=desc&per_page=30"
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

    let prs: Vec<GitHubPR> = response.json().await?;
    let conn = db.conn();
    let now = Utc::now().to_rfc3339();
    let mut latest_updated = since;

    for pr in &prs {
        // since 이전 PR은 건너뜀
        if let Some(ref s) = latest_updated {
            if pr.updated_at <= *s {
                continue;
            }
        }

        // 작성자 필터
        if ignore_authors.contains(&pr.user.login) {
            continue;
        }

        // 이미 큐에 있는지 확인
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pr_queue WHERE repo_id = ?1 AND github_number = ?2",
            rusqlite::params![repo_id, pr.number],
            |row| row.get(0),
        )?;

        if !exists {
            let id = Uuid::new_v4().to_string();

            conn.execute(
                "INSERT INTO pr_queue (id, repo_id, github_number, title, body, author, head_branch, base_branch, status, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9, ?9)",
                rusqlite::params![
                    id,
                    repo_id,
                    pr.number,
                    pr.title,
                    pr.body,
                    pr.user.login,
                    pr.head.ref_name,
                    pr.base.ref_name,
                    now,
                ],
            )?;

            tracing::info!("queued PR #{}: {}", pr.number, pr.title);
        }

        if latest_updated.as_ref().map_or(true, |l| pr.updated_at > *l) {
            latest_updated = Some(pr.updated_at.clone());
        }
    }

    // 스캔 커서 업데이트
    if let Some(last_seen) = latest_updated {
        conn.execute(
            "INSERT OR REPLACE INTO scan_cursors (repo_id, target, last_seen, last_scan) VALUES (?1, 'pulls', ?2, ?3)",
            rusqlite::params![repo_id, last_seen, now],
        )?;
    }

    Ok(())
}

fn get_last_seen(db: &Database, repo_id: &str) -> Result<Option<String>> {
    let result = db.conn().query_row(
        "SELECT last_seen FROM scan_cursors WHERE repo_id = ?1 AND target = 'pulls'",
        rusqlite::params![repo_id],
        |row| row.get(0),
    );
    Ok(result.ok())
}

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
