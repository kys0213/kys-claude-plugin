use std::path::PathBuf;

use anyhow::Result;

use crate::config;

/// 레포의 base clone 경로
pub fn repo_base_path(repo_name: &str) -> PathBuf {
    let sanitized = repo_name.replace('/', "-");
    config::workspaces_path().join(&sanitized).join("main")
}

/// 작업별 worktree 경로
pub fn worktree_path(repo_name: &str, task_id: &str) -> PathBuf {
    let sanitized = repo_name.replace('/', "-");
    config::workspaces_path().join(&sanitized).join(task_id)
}

/// 레포가 아직 클론되지 않았으면 클론
pub async fn ensure_cloned(repo_url: &str, repo_name: &str) -> Result<PathBuf> {
    let base = repo_base_path(repo_name);

    if !base.exists() {
        std::fs::create_dir_all(base.parent().unwrap())?;
        let status = tokio::process::Command::new("git")
            .args(["clone", repo_url, base.to_str().unwrap()])
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("git clone failed for {repo_url}");
        }
    } else {
        // 기존 클론 업데이트
        let status = tokio::process::Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(&base)
            .status()
            .await?;

        if !status.success() {
            tracing::warn!("git pull failed for {repo_name}, continuing with existing state");
        }
    }

    Ok(base)
}

/// 작업용 worktree 생성
pub async fn create_worktree(
    repo_name: &str,
    task_id: &str,
    branch: Option<&str>,
) -> Result<PathBuf> {
    let base = repo_base_path(repo_name);
    let wt_path = worktree_path(repo_name, task_id);

    if wt_path.exists() {
        return Ok(wt_path);
    }

    std::fs::create_dir_all(wt_path.parent().unwrap())?;

    let mut args = vec![
        "worktree".to_string(),
        "add".to_string(),
        wt_path.to_str().unwrap().to_string(),
    ];

    if let Some(b) = branch {
        args.push(b.to_string());
    }

    let status = tokio::process::Command::new("git")
        .args(&args)
        .current_dir(&base)
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("git worktree add failed for {task_id}");
    }

    Ok(wt_path)
}

/// worktree 제거
pub async fn remove_worktree(repo_name: &str, task_id: &str) -> Result<()> {
    let base = repo_base_path(repo_name);
    let wt_path = worktree_path(repo_name, task_id);

    if wt_path.exists() {
        tokio::process::Command::new("git")
            .args(["worktree", "remove", "--force", wt_path.to_str().unwrap()])
            .current_dir(&base)
            .status()
            .await?;
    }

    Ok(())
}
