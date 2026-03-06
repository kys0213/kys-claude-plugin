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

    async fn sync_default_branch(&self, repo_dir: &Path) -> Result<bool> {
        // 1. fetch origin
        let status = tokio::process::Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(repo_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await?;
        if !status.success() {
            return Ok(false);
        }

        // 2. detect default branch via symbolic-ref
        let output = tokio::process::Command::new("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
            .current_dir(repo_dir)
            .output()
            .await?;
        let default_ref = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let branch = if output.status.success() && !default_ref.is_empty() {
            default_ref.strip_prefix("origin/").unwrap_or(&default_ref)
        } else {
            tracing::warn!(
                "could not detect default branch for {}, falling back to 'main'",
                repo_dir.display()
            );
            "main"
        };

        // 3. checkout default branch (force to discard any local changes)
        let status = tokio::process::Command::new("git")
            .args(["checkout", branch])
            .current_dir(repo_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await?;
        if !status.success() {
            return Ok(false);
        }

        // 4. reset to origin/<branch>
        let status = tokio::process::Command::new("git")
            .args(["reset", "--hard", &format!("origin/{branch}")])
            .current_dir(repo_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await?;

        Ok(status.success())
    }

    async fn worktree_add(&self, base_dir: &Path, dest: &Path, branch: Option<&str>) -> Result<()> {
        let dest_str = path_to_string(dest);

        if let Some(b) = branch {
            // Try creating a new branch first; if it already exists, fall back to checkout.
            // This avoids a separate rev-parse check (TOCTOU) and saves a subprocess in the
            // common case (new branch).
            let create = tokio::process::Command::new("git")
                .args(["worktree", "add", "-b", b, &dest_str])
                .current_dir(base_dir)
                .output()
                .await?;

            if !create.status.success() {
                let stderr = String::from_utf8_lossy(&create.stderr);
                if !stderr.contains("already exists") {
                    anyhow::bail!("git worktree add -b failed: {}", stderr.trim());
                }

                // Branch already exists → checkout into worktree
                let checkout = tokio::process::Command::new("git")
                    .args(["worktree", "add", &dest_str, b])
                    .current_dir(base_dir)
                    .output()
                    .await?;

                if !checkout.status.success() {
                    let err = String::from_utf8_lossy(&checkout.stderr);
                    anyhow::bail!("git worktree add failed: {}", err.trim());
                }
            }
        } else {
            let out = tokio::process::Command::new("git")
                .args(["worktree", "add", &dest_str])
                .current_dir(base_dir)
                .output()
                .await?;

            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                anyhow::bail!("git worktree add failed: {}", err.trim());
            }
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

    async fn checkout_new_branch(&self, repo_dir: &Path, branch: &str) -> Result<()> {
        let status = tokio::process::Command::new("git")
            .args(["checkout", "-b", branch])
            .current_dir(repo_dir)
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("git checkout -b {branch} failed");
        }
        Ok(())
    }

    async fn add_commit_push(
        &self,
        repo_dir: &Path,
        files: &[&str],
        message: &str,
        branch: &str,
    ) -> Result<()> {
        let mut add_args = vec!["add".to_string()];
        for f in files {
            add_args.push(f.to_string());
        }

        let status = tokio::process::Command::new("git")
            .args(&add_args)
            .current_dir(repo_dir)
            .status()
            .await?;
        if !status.success() {
            anyhow::bail!("git add failed");
        }

        let status = tokio::process::Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(repo_dir)
            .status()
            .await?;
        if !status.success() {
            anyhow::bail!("git commit failed");
        }

        let status = tokio::process::Command::new("git")
            .args(["push", "origin", branch])
            .current_dir(repo_dir)
            .status()
            .await?;
        if !status.success() {
            anyhow::bail!("git push origin {branch} failed");
        }

        Ok(())
    }
}
