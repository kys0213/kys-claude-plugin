use std::path::Path;
use std::process::Output;

use anyhow::Result;
use async_trait::async_trait;

use super::Git;

/// Path → 문자열 변환 (non-UTF-8 경로에서도 안전)
fn path_to_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// git 명령 실행 후 Output 반환 (성공 여부는 호출자가 판단)
async fn run_git(dir: &Path, args: &[&str]) -> Result<Output> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await?;
    Ok(output)
}

/// git 명령 실행, 실패 시 stderr를 포함한 에러 반환
async fn run_git_ok(dir: &Path, args: &[&str]) -> Result<Output> {
    let output = run_git(dir, args).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let cmd = args.join(" ");
        anyhow::bail!("git {cmd} failed: {}", stderr.trim());
    }
    Ok(output)
}

/// 실제 `git` CLI를 호출하는 구현체
pub struct RealGit;

#[async_trait]
impl Git for RealGit {
    async fn clone(&self, url: &str, dest: &Path) -> Result<()> {
        let dest_str = path_to_string(dest);
        run_git_ok(
            dest.parent().unwrap_or(Path::new(".")),
            &["clone", url, &dest_str],
        )
        .await?;
        Ok(())
    }

    async fn sync_default_branch(&self, repo_dir: &Path) -> Result<bool> {
        // 1. fetch origin
        if !run_git(repo_dir, &["fetch", "origin"])
            .await?
            .status
            .success()
        {
            return Ok(false);
        }

        // 2. detect default branch via symbolic-ref
        let output = run_git(
            repo_dir,
            &["symbolic-ref", "refs/remotes/origin/HEAD", "--short"],
        )
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

        // 3. checkout default branch
        if !run_git(repo_dir, &["checkout", branch])
            .await?
            .status
            .success()
        {
            return Ok(false);
        }

        // 4. reset to origin/<branch>
        let reset_ref = format!("origin/{branch}");
        Ok(run_git(repo_dir, &["reset", "--hard", &reset_ref])
            .await?
            .status
            .success())
    }

    async fn worktree_add(&self, base_dir: &Path, dest: &Path, branch: Option<&str>) -> Result<()> {
        let dest_str = path_to_string(dest);

        if let Some(b) = branch {
            // Try checking out an existing branch first (handles local and remote tracking
            // branches). If the branch doesn't exist yet, fall back to creating it with -b.
            // This order is important: ReviewTask/ImproveTask pass existing PR branches that
            // may only exist on the remote, while ImplementTask creates new branches.
            let checkout = run_git(base_dir, &["worktree", "add", &dest_str, b]).await?;
            if !checkout.status.success() {
                // Branch doesn't exist → create new branch from HEAD
                run_git_ok(base_dir, &["worktree", "add", "-b", b, &dest_str]).await?;
            }
        } else {
            run_git_ok(base_dir, &["worktree", "add", &dest_str]).await?;
        }

        Ok(())
    }

    async fn worktree_remove(&self, base_dir: &Path, worktree: &Path) -> Result<()> {
        let wt_str = path_to_string(worktree);
        let output = run_git(base_dir, &["worktree", "remove", "--force", &wt_str]).await?;
        if !output.status.success() {
            tracing::warn!("git worktree remove failed for {}", worktree.display());
        }
        Ok(())
    }

    async fn checkout_new_branch(&self, repo_dir: &Path, branch: &str) -> Result<()> {
        run_git_ok(repo_dir, &["checkout", "-b", branch]).await?;
        Ok(())
    }

    async fn add_commit_push(
        &self,
        repo_dir: &Path,
        files: &[&str],
        message: &str,
        branch: &str,
    ) -> Result<()> {
        let mut add_args: Vec<&str> = vec!["add"];
        add_args.extend_from_slice(files);
        run_git_ok(repo_dir, &add_args).await?;
        run_git_ok(repo_dir, &["commit", "-m", message]).await?;
        run_git_ok(repo_dir, &["push", "origin", branch]).await?;
        Ok(())
    }
}
