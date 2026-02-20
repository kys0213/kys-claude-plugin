use anyhow::Result;
use serde::Deserialize;

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
    repo_id: &str,
    repo_name: &str,
    ignore_authors: &[String],
    gh_host: Option<&str>,
) -> Result<()> {
    let since = db.cursor_get_last_seen(repo_id, "pulls")?;

    let mut args = vec![
        "api".to_string(),
        format!("repos/{repo_name}/pulls"),
        "--method".to_string(), "GET".to_string(),
        "--paginate".to_string(),
        "-f".to_string(), "state=open".to_string(),
        "-f".to_string(), "sort=updated".to_string(),
        "-f".to_string(), "direction=desc".to_string(),
        "-f".to_string(), "per_page=30".to_string(),
    ];

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

    let prs: Vec<GitHubPR> = serde_json::from_slice(&output.stdout)?;
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
        let exists = db.pr_exists(repo_id, pr.number)?;

        if !exists {
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
            tracing::info!("queued PR #{}: {}", pr.number, pr.title);
        }

        if latest_updated.as_ref().map_or(true, |l| pr.updated_at > *l) {
            latest_updated = Some(pr.updated_at.clone());
        }
    }

    // 스캔 커서 업데이트
    if let Some(last_seen) = latest_updated {
        db.cursor_upsert(repo_id, "pulls", &last_seen)?;
    }

    Ok(())
}
