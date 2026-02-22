pub mod issues;
pub mod pulls;

use anyhow::Result;

use crate::active::ActiveItems;
use crate::config;
use crate::config::Env;
use crate::infrastructure::gh::Gh;
use crate::queue::repository::*;
use crate::queue::Database;

/// 등록된 모든 레포를 스캔
pub async fn scan_all(
    db: &Database,
    env: &dyn Env,
    gh: &dyn Gh,
    active: &mut ActiveItems,
) -> Result<()> {
    let repos = db.repo_find_enabled()?;

    for repo in repos {
        let ws_path = config::workspaces_path(env).join(&repo.name);
        let cfg = config::loader::load_merged(
            env,
            if ws_path.exists() {
                Some(ws_path.as_path())
            } else {
                None
            },
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
                        db,
                        gh,
                        &repo.id,
                        &repo.name,
                        &cfg.consumer.ignore_authors,
                        &cfg.consumer.filter_labels,
                        gh_host,
                        active,
                    )
                    .await
                    {
                        tracing::error!("issue scan error for {}: {e}", repo.name);
                    }
                }
                "pulls" => {
                    if let Err(e) = pulls::scan(
                        db,
                        gh,
                        &repo.id,
                        &repo.name,
                        &cfg.consumer.ignore_authors,
                        gh_host,
                        active,
                    )
                    .await
                    {
                        tracing::error!("PR scan error for {}: {e}", repo.name);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}
