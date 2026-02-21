pub mod issues;
pub mod pulls;

use anyhow::Result;

use crate::config;
use crate::queue::repository::*;
use crate::queue::Database;

/// 등록된 모든 레포를 스캔
pub async fn scan_all(db: &Database) -> Result<()> {
    let repos = db.repo_find_enabled()?;

    for repo in repos {
        // 워크스페이스 경로에서 레포별 YAML 설정 로드
        let ws_path = config::workspaces_path().join(&repo.name);
        let cfg = config::loader::load_merged(
            if ws_path.exists() { Some(ws_path.as_path()) } else { None }
        );

        let should_scan = db.cursor_should_scan(&repo.id, cfg.consumer.scan_interval_secs as i64)?;
        if !should_scan {
            continue;
        }

        tracing::info!("scanning {}...", repo.name);

        let gh_host = cfg.consumer.gh_host.as_deref();

        for target in &cfg.consumer.scan_targets {
            match target.as_str() {
                "issues" => {
                    if let Err(e) = issues::scan(
                        db, &repo.id, &repo.name,
                        &cfg.consumer.ignore_authors,
                        &cfg.consumer.filter_labels,
                        gh_host,
                    ).await {
                        tracing::error!("issue scan error for {}: {e}", repo.name);
                    }
                }
                "pulls" => {
                    if let Err(e) = pulls::scan(
                        db, &repo.id, &repo.name,
                        &cfg.consumer.ignore_authors,
                        gh_host,
                    ).await {
                        tracing::error!("PR scan error for {}: {e}", repo.name);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}
