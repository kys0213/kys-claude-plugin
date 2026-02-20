pub mod issues;
pub mod pulls;

use anyhow::Result;

use crate::queue::models::*;
use crate::queue::repository::*;
use crate::queue::Database;

/// 등록된 모든 레포를 스캔
pub async fn scan_all(db: &Database) -> Result<()> {
    let repos: Vec<EnabledRepo> = db.repo_find_enabled()?;

    for repo in repos {
        let should_scan = db.cursor_should_scan(&repo.id, repo.scan_interval_secs)?;
        if !should_scan {
            continue;
        }

        let targets: Vec<String> = serde_json::from_str(&repo.scan_targets)?;
        let ignore: Vec<String> = serde_json::from_str(&repo.ignore_authors)?;
        let labels: Option<Vec<String>> =
            repo.filter_labels.and_then(|l| serde_json::from_str(&l).ok());

        tracing::info!("scanning {}...", repo.name);

        let gh_host = repo.gh_host.as_deref();

        for target in &targets {
            match target.as_str() {
                "issues" => {
                    if let Err(e) = issues::scan(db, &repo.id, &repo.name, &ignore, &labels, gh_host).await
                    {
                        tracing::error!("issue scan error for {}: {e}", repo.name);
                    }
                }
                "pulls" => {
                    if let Err(e) = pulls::scan(db, &repo.id, &repo.name, &ignore, gh_host).await {
                        tracing::error!("PR scan error for {}: {e}", repo.name);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}
