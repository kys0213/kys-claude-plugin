pub mod pid;
pub mod recovery;

use std::path::Path;

use anyhow::{bail, Result};
use tracing::info;

use crate::active::ActiveItems;
use crate::components::notifier::Notifier;
use crate::components::workspace::Workspace;
use crate::config::{self, Env};
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::pipeline;
use crate::queue::repository::{QueueAdmin, RepoRepository};
use crate::queue::Database;
use crate::scanner;

/// 데몬을 포그라운드로 시작
pub async fn start(
    home: &Path,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Claude,
) -> Result<()> {
    if pid::is_running(home) {
        bail!(
            "daemon is already running (pid: {})",
            pid::read_pid(home).unwrap_or(0)
        );
    }

    info!("starting autodev daemon...");

    pid::write_pid(home)?;

    let cfg = config::loader::load_merged(env, None);
    let stuck_threshold = cfg.consumer.stuck_threshold_secs as i64;

    let db_path = home.join("autodev.db");
    let db = Database::open(&db_path)?;
    db.initialize()?;

    // stuck 상태 복구
    match db.queue_reset_stuck(stuck_threshold) {
        Ok(n) if n > 0 => info!("recovered {n} stuck items → pending"),
        Err(e) => tracing::error!("stuck recovery failed: {e}"),
        _ => {}
    }

    // failed 항목 자동 재시도
    match db.queue_auto_retry_failed(3) {
        Ok(n) if n > 0 => info!("auto-retrying {n} failed items"),
        Err(e) => tracing::error!("auto-retry failed: {e}"),
        _ => {}
    }

    println!("autodev daemon started (pid: {})", std::process::id());

    let mut active = ActiveItems::new();
    let workspace = Workspace::new(git, env);
    let notifier = Notifier::new(gh);

    let gh_host = cfg.consumer.gh_host.clone();

    // 메인 루프: recovery → scanner → pipeline
    tokio::select! {
        _ = async {
            loop {
                // 1. Recovery: orphan autodev:wip 라벨 정리
                match db.repo_find_enabled() {
                    Ok(repos) => {
                        match recovery::recover_orphan_wip(&repos, gh, &active, gh_host.as_deref()).await {
                            Ok(n) if n > 0 => info!("recovered {n} orphan wip items"),
                            Err(e) => tracing::error!("recovery error: {e}"),
                            _ => {}
                        }
                    }
                    Err(e) => tracing::error!("recovery repo lookup failed: {e}"),
                }

                // 2. Scan
                if let Err(e) = scanner::scan_all(&db, env, gh, &mut active).await {
                    tracing::error!("scan error: {e}");
                }

                // 3. Pipeline
                if let Err(e) = pipeline::process_all(&db, env, &workspace, &notifier, claude, &mut active).await {
                    tracing::error!("pipeline error: {e}");
                }

                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        } => {},
        _ = tokio::signal::ctrl_c() => {
            info!("received SIGINT, shutting down...");
        }
    }

    pid::remove_pid(home);
    Ok(())
}

/// 데몬 중지 (PID → SIGTERM)
pub fn stop(home: &Path) -> Result<()> {
    let pid =
        pid::read_pid(home).ok_or_else(|| anyhow::anyhow!("daemon is not running"))?;

    std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()?;

    pid::remove_pid(home);
    println!("autodev daemon stopped (pid: {pid})");
    Ok(())
}
