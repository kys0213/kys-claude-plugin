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

/// 워크스페이스 기본 경로
pub fn workspaces_path() -> PathBuf {
    autonomous_home().join("workspaces")
}
