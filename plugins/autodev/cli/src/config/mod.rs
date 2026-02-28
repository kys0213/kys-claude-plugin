pub mod loader;
pub mod models;

use std::path::{Path, PathBuf};

use self::models::WorkflowConfig;

// ─── ConfigLoader trait ───

/// 설정 로드 추상화 — Task에서 이 trait에만 의존한다.
///
/// 실제 구현체는 `RealConfigLoader`이며, 테스트에서는 MockConfigLoader를 주입한다.
#[allow(dead_code)]
pub trait ConfigLoader: Send + Sync {
    /// 글로벌 + 레포별 설정을 머지하여 최종 설정 반환.
    /// `workspace_path`가 Some이면 해당 디렉토리의 레포별 설정을 오버라이드한다.
    fn load(&self, workspace_path: Option<&Path>) -> WorkflowConfig;
}

/// 실제 설정 로더 — `loader::load_merged`에 위임
#[allow(dead_code)]
pub struct RealConfigLoader {
    env: Box<dyn Env>,
}

#[allow(dead_code)]
impl RealConfigLoader {
    pub fn new(env: Box<dyn Env>) -> Self {
        Self { env }
    }
}

impl ConfigLoader for RealConfigLoader {
    fn load(&self, workspace_path: Option<&Path>) -> WorkflowConfig {
        loader::load_merged(&*self.env, workspace_path)
    }
}

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
    let home = env.var("AUTODEV_HOME").unwrap_or_else(|_| {
        let home = env.var("HOME").expect("HOME not set");
        format!("{home}/.autodev")
    });
    PathBuf::from(home)
}

/// 워크스페이스 기본 경로
pub fn workspaces_path(env: &dyn Env) -> PathBuf {
    autodev_home(env).join("workspaces")
}

/// 레포 이름을 파일시스템 안전한 디렉토리명으로 변환
/// 예: "org/repo" → "org-repo"
pub fn sanitize_repo_name(name: &str) -> String {
    name.replace('/', "-")
}

/// 신뢰할 수 없는 경로를 `base` 안에서만 유효한 경로로 결합.
/// 절대경로, `..` 컴포넌트, base 디렉토리 탈출 시 `Err`를 반환한다.
pub fn safe_join(base: &std::path::Path, untrusted: &str) -> Result<PathBuf, String> {
    use std::path::Component;

    let path = std::path::Path::new(untrusted);

    // 절대경로 거부
    if path.is_absolute() {
        return Err(format!("absolute path not allowed: {untrusted}"));
    }

    // `..` 컴포넌트 거부
    for comp in path.components() {
        if matches!(comp, Component::ParentDir) {
            return Err(format!(
                "parent directory traversal not allowed: {untrusted}"
            ));
        }
    }

    let joined = base.join(path);

    // 최종 경로가 base 안에 있는지 확인
    if !joined.starts_with(base) {
        return Err(format!("path escapes base directory: {untrusted}"));
    }

    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn safe_join_allows_normal_relative_path() {
        let base = Path::new("/worktree");
        let result = safe_join(base, ".claude/rules/test.md").unwrap();
        assert_eq!(result, base.join(".claude/rules/test.md"));
    }

    #[test]
    fn safe_join_allows_simple_filename() {
        let base = Path::new("/worktree");
        let result = safe_join(base, "CLAUDE.md").unwrap();
        assert_eq!(result, base.join("CLAUDE.md"));
    }

    #[test]
    fn safe_join_rejects_absolute_path() {
        let base = Path::new("/worktree");
        assert!(safe_join(base, "/etc/passwd").is_err());
    }

    #[test]
    fn safe_join_rejects_parent_traversal() {
        let base = Path::new("/worktree");
        assert!(safe_join(base, "../../../etc/passwd").is_err());
    }

    #[test]
    fn safe_join_rejects_mixed_traversal() {
        let base = Path::new("/worktree");
        assert!(safe_join(base, "valid/path/../../../escape").is_err());
    }

    #[test]
    fn safe_join_rejects_single_parent() {
        let base = Path::new("/worktree");
        assert!(safe_join(base, "..").is_err());
    }

    #[test]
    fn safe_join_allows_nested_directory() {
        let base = Path::new("/worktree");
        let result = safe_join(base, ".claude/hooks.json").unwrap();
        assert_eq!(result, base.join(".claude/hooks.json"));
    }
}

/// 로그 디렉토리 경로 해석: 절대 경로면 그대로, 상대 경로면 home 기준
pub fn resolve_log_dir(log_dir: &str, home: &std::path::Path) -> PathBuf {
    let path = std::path::Path::new(log_dir);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        home.join(log_dir)
    }
}
