pub mod mock;
pub mod real;

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;

pub use real::RealGit;

/// Git CLI 추상화
#[async_trait]
pub trait Git: Send + Sync {
    /// `git clone {url} {dest}`
    async fn clone(&self, url: &str, dest: &Path) -> Result<()>;

    /// `git pull --ff-only` in repo_dir
    /// 성공 시 true, 실패 시 false (에러가 아닌 실패)
    async fn pull_ff_only(&self, repo_dir: &Path) -> Result<bool>;

    /// `git worktree add {dest} [branch]` from base_dir
    async fn worktree_add(&self, base_dir: &Path, dest: &Path, branch: Option<&str>) -> Result<()>;

    /// `git worktree remove --force {worktree}`
    async fn worktree_remove(&self, base_dir: &Path, worktree: &Path) -> Result<()>;

    /// `git checkout -b {branch}` in repo_dir
    async fn checkout_new_branch(&self, repo_dir: &Path, branch: &str) -> Result<()>;

    /// `git add {files} && git commit -m {message} && git push origin {branch}`
    async fn add_commit_push(
        &self,
        repo_dir: &Path,
        files: &[&str],
        message: &str,
        branch: &str,
    ) -> Result<()>;
}
