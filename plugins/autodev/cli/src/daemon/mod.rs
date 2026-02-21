pub mod pid;

use std::path::Path;

use anyhow::{bail, Result};
use tracing::info;

use crate::active::ActiveItems;
use crate::config::{self, Env};
use crate::queue::Database;
use crate::queue::repository::QueueAdmin;
use crate::scanner;
use crate::consumer;

/// 데몬을 포그라운드로 시작
pub async fn start(home: &Path, env: &dyn Env) -> Result<()> {
    if pid::is_running(home) {
        bail!("daemon is already running (pid: {})", pid::read_pid(home).unwrap_or(0));
    }

    info!("starting autodev daemon...");

    // PID 기록
    pid::write_pid(home)?;

    // 설정 로드
    let cfg = config::loader::load_merged(env, None);
    let stuck_threshold = cfg.consumer.stuck_threshold_secs as i64;

    // DB 열기
    let db_path = home.join("autodev.db");
    let db = Database::open(&db_path)?;
    db.initialize()?;

    // 시작 시 stuck 상태 복구: 이전 데몬이 비정상 종료되어 중간 상태에 남은 항목 복구
    match db.queue_reset_stuck(stuck_threshold) {
        Ok(n) if n > 0 => info!("recovered {n} stuck items → pending"),
        Err(e) => tracing::error!("stuck recovery failed: {e}"),
        _ => {}
    }

    // failed 항목 자동 재시도 (최대 3회)
    match db.queue_auto_retry_failed(3) {
        Ok(n) if n > 0 => info!("auto-retrying {n} failed items"),
        Err(e) => tracing::error!("auto-retry failed: {e}"),
        _ => {}
    }

    println!("autodev daemon started (pid: {})", std::process::id());

    // 인메모리 중복 방지: 큐에 존재하는 항목 추적
    let mut active = ActiveItems::new();

    // 메인 루프: scanner + consumer (inline - rusqlite is not Sync)
    tokio::select! {
        _ = async {
            loop {
                if let Err(e) = scanner::scan_all(&db, env, &mut active).await {
                    tracing::error!("scan error: {e}");
                }

                if let Err(e) = consumer::process_all(&db, env, &mut active).await {
                    tracing::error!("consumer error: {e}");
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
    let pid = pid::read_pid(home).ok_or_else(|| anyhow::anyhow!("daemon is not running"))?;

    std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()?;

    pid::remove_pid(home);
    println!("autodev daemon stopped (pid: {pid})");
    Ok(())
}
