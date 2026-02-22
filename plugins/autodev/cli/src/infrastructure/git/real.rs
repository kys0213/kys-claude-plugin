use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;

use super::Git;

/// Path → 문자열 변환 (non-UTF-8 경로에서도 안전)
fn path_to_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// 실제 `git` CLI를 호출하는 구현체
pub struct RealGit;

#[async_trait]
impl Git for RealGit {
    async fn clone(&self, url: &str, dest: &Path) -> Result<()> {
        let status = tokio::process::Command::new("git")
            .args(["clone", url, &path_to_string(dest)])
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("git clone failed for {url}");
        }
        Ok(())
    }

    async fn pull_ff_only(&self, repo_dir: &Path) -> Result<bool> {
        let status = tokio::process::Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(repo_dir)
            .status()
            .await?;

        Ok(status.success())
    }

    async fn worktree_add(&self, base_dir: &Path, dest: &Path, branch: Option<&str>) -> Result<()> {
        let mut args = vec![
            "worktree".to_string(),
            "add".to_string(),
            path_to_string(dest),
        ];

        if let Some(b) = branch {
            args.push(b.to_string());
        }

        let status = tokio::process::Command::new("git")
            .args(&args)
            .current_dir(base_dir)
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("git worktree add failed for {}", dest.display());
        }
        Ok(())
    }

    async fn worktree_remove(&self, base_dir: &Path, worktree: &Path) -> Result<()> {
        let status = tokio::process::Command::new("git")
            .args(["worktree", "remove", "--force", &path_to_string(worktree)])
            .current_dir(base_dir)
            .status()
            .await?;

        if !status.success() {
            tracing::warn!("git worktree remove failed for {}", worktree.display());
        }

        Ok(())
    }
}
