pub mod pid;

use std::path::Path;

use anyhow::{bail, Result};
use tracing::info;

use crate::queue::Database;
use crate::scanner;
use crate::consumer;

/// 데몬을 포그라운드로 시작
pub async fn start(home: &Path) -> Result<()> {
    if pid::is_running(home) {
        bail!("daemon is already running (pid: {})", pid::read_pid(home).unwrap_or(0));
    }

    info!("starting autodev daemon...");

    // PID 기록
    pid::write_pid(home)?;

    // DB 열기
    let db_path = home.join("autodev.db");
    let db = Database::open(&db_path)?;
    db.initialize()?;

    println!("autodev daemon started (pid: {})", std::process::id());

    // 메인 루프: scanner + consumer (inline - rusqlite is not Sync)
    tokio::select! {
        _ = async {
            loop {
                if let Err(e) = scanner::scan_all(&db).await {
                    tracing::error!("scan error: {e}");
                }

                if let Err(e) = consumer::process_all(&db).await {
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
