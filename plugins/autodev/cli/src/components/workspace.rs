use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;

use crate::config::{self, Env};
use crate::infrastructure::git::Git;

// ─── WorkspaceOps trait ───

/// Workspace 조작 추상화 — Task에서 이 trait에만 의존한다.
///
/// 실제 구현체는 `Workspace` struct이며, 테스트에서는 MockWorkspace를 주입한다.
#[allow(dead_code)]
#[async_trait]
pub trait WorkspaceOps: Send + Sync {
    /// 레포가 아직 클론되지 않았으면 클론, 있으면 pull.
    /// 반환값: base clone 경로.
    async fn ensure_cloned(&self, repo_url: &str, repo_name: &str) -> Result<PathBuf>;

    /// 작업용 worktree 생성.
    /// 반환값: worktree 경로.
    async fn create_worktree(
        &self,
        repo_name: &str,
        task_id: &str,
        branch: Option<&str>,
    ) -> Result<PathBuf>;

    /// worktree 제거.
    async fn remove_worktree(&self, repo_name: &str, task_id: &str) -> Result<()>;

    /// 레포의 base clone 경로 반환 (read-only 용도).
    fn repo_base_path(&self, repo_name: &str) -> PathBuf;

    /// 작업별 worktree 경로 반환 (read-only 용도).
    fn worktree_path(&self, repo_name: &str, task_id: &str) -> PathBuf;
}

// ─── Workspace struct ───

/// Workspace 관리 — Git trait 주입받아 worktree 생명주기 관리
pub struct Workspace<'a> {
    git: &'a dyn Git,
    env: &'a dyn Env,
}

impl<'a> Workspace<'a> {
    pub fn new(git: &'a dyn Git, env: &'a dyn Env) -> Self {
        Self { git, env }
    }

    /// 내부 Git 인프라 참조 반환
    pub fn git(&self) -> &'a dyn Git {
        self.git
    }

    /// 레포의 base clone 경로
    pub fn repo_base_path(&self, repo_name: &str) -> PathBuf {
        let sanitized = config::sanitize_repo_name(repo_name);
        config::workspaces_path(self.env)
            .join(&sanitized)
            .join("main")
    }

    /// 작업별 worktree 경로
    pub fn worktree_path(&self, repo_name: &str, task_id: &str) -> PathBuf {
        let sanitized = config::sanitize_repo_name(repo_name);
        config::workspaces_path(self.env)
            .join(&sanitized)
            .join(task_id)
    }

    /// 레포가 아직 클론되지 않았으면 클론, 있으면 pull
    pub async fn ensure_cloned(&self, repo_url: &str, repo_name: &str) -> Result<PathBuf> {
        let base = self.repo_base_path(repo_name);

        if !base.exists() {
            std::fs::create_dir_all(base.parent().unwrap())?;
            self.git.clone(repo_url, &base).await?;
        } else if !self.git.pull_ff_only(&base).await? {
            tracing::warn!("git pull failed for {repo_name}, continuing with existing state");
        }

        Ok(base)
    }

    /// 작업용 worktree 생성
    pub async fn create_worktree(
        &self,
        repo_name: &str,
        task_id: &str,
        branch: Option<&str>,
    ) -> Result<PathBuf> {
        let base = self.repo_base_path(repo_name);
        let wt_path = self.worktree_path(repo_name, task_id);

        if wt_path.exists() {
            return Ok(wt_path);
        }

        std::fs::create_dir_all(wt_path.parent().unwrap())?;
        self.git.worktree_add(&base, &wt_path, branch).await?;

        Ok(wt_path)
    }

    /// worktree 제거
    pub async fn remove_worktree(&self, repo_name: &str, task_id: &str) -> Result<()> {
        let base = self.repo_base_path(repo_name);
        let wt_path = self.worktree_path(repo_name, task_id);

        if wt_path.exists() {
            self.git.worktree_remove(&base, &wt_path).await?;
        }

        Ok(())
    }
}

/// WorkspaceOps trait 구현 — inherent 메서드에 위임
#[allow(dead_code)]
#[async_trait]
impl WorkspaceOps for Workspace<'_> {
    async fn ensure_cloned(&self, repo_url: &str, repo_name: &str) -> Result<PathBuf> {
        Workspace::ensure_cloned(self, repo_url, repo_name).await
    }

    async fn create_worktree(
        &self,
        repo_name: &str,
        task_id: &str,
        branch: Option<&str>,
    ) -> Result<PathBuf> {
        Workspace::create_worktree(self, repo_name, task_id, branch).await
    }

    async fn remove_worktree(&self, repo_name: &str, task_id: &str) -> Result<()> {
        Workspace::remove_worktree(self, repo_name, task_id).await
    }

    fn repo_base_path(&self, repo_name: &str) -> PathBuf {
        Workspace::repo_base_path(self, repo_name)
    }

    fn worktree_path(&self, repo_name: &str, task_id: &str) -> PathBuf {
        Workspace::worktree_path(self, repo_name, task_id)
    }
}
