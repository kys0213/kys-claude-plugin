pub mod models;

use std::path::PathBuf;

/// ~/.autodev 경로 반환
pub fn autodev_home() -> PathBuf {
    let home = std::env::var("AUTODEV_HOME")
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME not set");
            format!("{home}/.autodev")
        });
    PathBuf::from(home)
}

/// 워크스페이스 기본 경로
pub fn workspaces_path() -> PathBuf {
    autodev_home().join("workspaces")
}
