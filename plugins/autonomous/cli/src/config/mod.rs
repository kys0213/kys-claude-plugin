pub mod loader;
pub mod models;

use std::path::PathBuf;

/// 환경 변수 접근을 추상화하는 트레이트 (테스트 격리를 위해 사용)
pub trait Env: Send + Sync {
    fn var(&self, key: &str) -> Result<String, std::env::VarError>;
}

/// 실제 환경 변수를 사용하는 구현체
pub struct RealEnv;

impl Env for RealEnv {
    fn var(&self, key: &str) -> Result<String, std::env::VarError> {
        std::env::var(key)
    }
}

/// ~/.autodev 경로 반환
pub fn autodev_home(env: &dyn Env) -> PathBuf {
    let home = env
        .var("AUTODEV_HOME")
        .unwrap_or_else(|_| {
            let home = env.var("HOME").expect("HOME not set");
            format!("{home}/.autodev")
        });
    PathBuf::from(home)
}

/// 워크스페이스 기본 경로
pub fn workspaces_path(env: &dyn Env) -> PathBuf {
    autodev_home(env).join("workspaces")
}
