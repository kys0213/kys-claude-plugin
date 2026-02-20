pub mod models;

use std::path::PathBuf;

/// ~/.autonomous 경로 반환
pub fn autonomous_home() -> PathBuf {
    let home = std::env::var("AUTONOMOUS_HOME")
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME not set");
            format!("{home}/.autonomous")
        });
    PathBuf::from(home)
}

/// PID 파일 경로
pub fn pid_path() -> PathBuf {
    autonomous_home().join("daemon.pid")
}

/// Unix socket 경로
pub fn socket_path() -> PathBuf {
    autonomous_home().join("daemon.sock")
}

/// 워크스페이스 기본 경로
pub fn workspaces_path() -> PathBuf {
    autonomous_home().join("workspaces")
}
