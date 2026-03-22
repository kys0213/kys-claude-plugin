use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;

/// Worktree 관리 trait.
///
/// v4 Git trait를 래핑하여 v5에서 사용.
/// create_or_reuse: Running 진입 시 worktree 생성 (retry 시 기존 재사용).
/// cleanup: Done 전이 시 worktree 삭제.
#[async_trait]
pub trait WorktreeManager: Send + Sync {
    /// Worktree를 생성하거나 기존 것을 재사용한다.
    ///
    /// retry 시에는 기존 worktree를 보존하여 이전 작업을 이어갈 수 있다.
    async fn create_or_reuse(&self, workspace_name: &str, source_id: &str) -> Result<PathBuf>;

    /// Worktree를 삭제한다 (Done 전이 시).
    async fn cleanup(&self, worktree_path: &Path) -> Result<()>;

    /// Worktree가 존재하는지 확인한다.
    fn exists(&self, worktree_path: &Path) -> bool;
}

/// 테스트용 MockWorktreeManager.
///
/// 실제 git worktree 대신 임시 디렉토리를 생성한다.
pub struct MockWorktreeManager {
    base_dir: PathBuf,
}

impl MockWorktreeManager {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl WorktreeManager for MockWorktreeManager {
    async fn create_or_reuse(&self, workspace_name: &str, source_id: &str) -> Result<PathBuf> {
        // source_id에서 안전한 디렉토리 이름 생성
        let safe_name = source_id.replace([':', '/', '#'], "-");
        let path = self.base_dir.join(format!("{workspace_name}-{safe_name}"));
        if !path.exists() {
            std::fs::create_dir_all(&path)?;
        }
        Ok(path)
    }

    async fn cleanup(&self, worktree_path: &Path) -> Result<()> {
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path)?;
        }
        Ok(())
    }

    fn exists(&self, worktree_path: &Path) -> bool {
        worktree_path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn create_and_cleanup() {
        let tmp = TempDir::new().unwrap();
        let mgr = MockWorktreeManager::new(tmp.path());

        let path = mgr
            .create_or_reuse("auth-project", "github:org/repo#42")
            .await
            .unwrap();
        assert!(path.exists());
        assert!(mgr.exists(&path));

        mgr.cleanup(&path).await.unwrap();
        assert!(!path.exists());
        assert!(!mgr.exists(&path));
    }

    #[tokio::test]
    async fn reuse_existing() {
        let tmp = TempDir::new().unwrap();
        let mgr = MockWorktreeManager::new(tmp.path());

        let path1 = mgr
            .create_or_reuse("ws", "github:org/repo#42")
            .await
            .unwrap();
        // 파일 생성하여 "이전 작업"을 시뮬레이션
        std::fs::write(path1.join("work.txt"), "previous work").unwrap();

        let path2 = mgr
            .create_or_reuse("ws", "github:org/repo#42")
            .await
            .unwrap();
        assert_eq!(path1, path2);
        // 기존 파일이 보존됨
        assert!(path2.join("work.txt").exists());
    }

    #[tokio::test]
    async fn cleanup_nonexistent_is_ok() {
        let tmp = TempDir::new().unwrap();
        let mgr = MockWorktreeManager::new(tmp.path());
        let path = tmp.path().join("nonexistent");
        // 존재하지 않는 경로 cleanup → 에러 없음
        mgr.cleanup(&path).await.unwrap();
    }

    #[tokio::test]
    async fn different_source_ids_get_different_paths() {
        let tmp = TempDir::new().unwrap();
        let mgr = MockWorktreeManager::new(tmp.path());

        let p1 = mgr
            .create_or_reuse("ws", "github:org/repo#1")
            .await
            .unwrap();
        let p2 = mgr
            .create_or_reuse("ws", "github:org/repo#2")
            .await
            .unwrap();
        assert_ne!(p1, p2);
    }
}
