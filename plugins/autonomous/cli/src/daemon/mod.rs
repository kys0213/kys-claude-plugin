pub mod pid;
pub mod socket;

use std::path::Path;

use anyhow::{bail, Result};
use tracing::info;

use crate::config;
use crate::queue::Database;
use crate::scanner;
use crate::consumer;

/// 데몬을 백그라운드로 시작
pub async fn start(home: &Path) -> Result<()> {
    if pid::is_running(home) {
        bail!("daemon is already running (pid: {})", pid::read_pid(home).unwrap_or(0));
    }

    info!("starting autonomous daemon...");

    // PID 기록
    pid::write_pid(home)?;

    // DB 열기
    let db_path = home.join("autonomous.db");
    let db = Database::open(&db_path)?;
    db.initialize()?;

    // Unix socket 서버 시작
    let socket_path = config::socket_path();
    let socket_handle = tokio::spawn(async move {
        if let Err(e) = socket::listen(&socket_path).await {
            tracing::error!("socket server error: {e}");
        }
    });

    println!("autonomous daemon started (pid: {})", std::process::id());

    // 메인 루프: scanner + consumer (inline, not spawned - rusqlite is not Sync)
    tokio::select! {
        _ = async {
            loop {
                // 등록된 레포 스캔
                if let Err(e) = scanner::scan_all(&db).await {
                    tracing::error!("scan error: {e}");
                }

                // Consumer 실행
                if let Err(e) = consumer::process_all(&db).await {
                    tracing::error!("consumer error: {e}");
                }

                // 최소 스캔 간격 대기
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        } => {},
        _ = socket_handle => {},
        _ = tokio::signal::ctrl_c() => {
            info!("received SIGINT, shutting down...");
        }
    }

    pid::remove_pid(home);
    Ok(())
}

/// 데몬 중지
pub fn stop(home: &Path) -> Result<()> {
    let pid = pid::read_pid(home).ok_or_else(|| anyhow::anyhow!("daemon is not running"))?;

    // SIGTERM 전송 via kill command
    std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()?;

    pid::remove_pid(home);
    println!("autonomous daemon stopped (pid: {pid})");
    Ok(())
}
