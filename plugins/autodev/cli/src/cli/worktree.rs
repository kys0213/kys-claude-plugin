use anyhow::Result;

use crate::core::config::{self, Env};

/// List preserved worktrees across all repos.
///
/// Scans `~/.autodev/workspaces/<repo>/` for subdirectories (each is a worktree).
pub fn list(env: &dyn Env, repo_filter: Option<&str>) -> Result<String> {
    let ws_root = config::workspaces_path(env);
    if !ws_root.exists() {
        return Ok("No workspaces directory found.\n".to_string());
    }

    let mut entries = Vec::new();

    for repo_entry in std::fs::read_dir(&ws_root)? {
        let repo_entry = repo_entry?;
        if !repo_entry.file_type()?.is_dir() {
            continue;
        }

        let repo_name = repo_entry.file_name().to_string_lossy().to_string();

        // Apply repo filter (matches sanitized name)
        if let Some(filter) = repo_filter {
            let sanitized = config::sanitize_workspace_name(filter);
            if repo_name != sanitized {
                continue;
            }
        }

        let repo_dir = repo_entry.path();
        for wt_entry in std::fs::read_dir(&repo_dir)? {
            let wt_entry = wt_entry?;
            if !wt_entry.file_type()?.is_dir() {
                continue;
            }

            // Skip known non-worktree dirs (base clone, claw, dotfiles)
            let wt_name = wt_entry.file_name().to_string_lossy().to_string();
            if matches!(wt_name.as_str(), "main" | "claw") || wt_name.starts_with('.') {
                continue;
            }

            entries.push((repo_name.clone(), wt_name, wt_entry.path()));
        }
    }

    entries.sort();

    if entries.is_empty() {
        return Ok("No preserved worktrees found.\n".to_string());
    }

    let mut output = format!("{} preserved worktree(s):\n\n", entries.len());
    for (repo, task_id, path) in &entries {
        output.push_str(&format!("  {repo}/{task_id}\n    {}\n", path.display()));
    }
    Ok(output)
}

/// Remove a preserved worktree by task ID or full path.
pub fn remove(env: &dyn Env, id: &str) -> Result<String> {
    let ws_root = config::workspaces_path(env);

    // Try to find the worktree by scanning repos (constrained to ws_root)
    for repo_entry in std::fs::read_dir(&ws_root)? {
        let repo_entry = repo_entry?;
        if !repo_entry.file_type()?.is_dir() {
            continue;
        }

        let candidate = repo_entry.path().join(id);
        if candidate.is_dir() {
            std::fs::remove_dir_all(&candidate)?;
            return Ok(format!("Removed worktree: {}\n", candidate.display()));
        }
    }

    anyhow::bail!(
        "worktree not found: {id}\nHint: use task ID (e.g., 'issue-42'), not a full path."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::VarError;

    struct TestEnv {
        home: String,
    }

    impl Env for TestEnv {
        fn var(&self, key: &str) -> Result<String, VarError> {
            match key {
                "HOME" | "AUTODEV_HOME" => Ok(self.home.clone()),
                _ => Err(VarError::NotPresent),
            }
        }
    }

    #[test]
    fn list_empty_when_no_workspaces() {
        let tmp = tempfile::tempdir().unwrap();
        let env = TestEnv {
            home: tmp.path().to_string_lossy().to_string(),
        };
        let output = list(&env, None).unwrap();
        assert!(output.contains("No workspaces directory"));
    }

    #[test]
    fn list_shows_preserved_worktrees() {
        let tmp = tempfile::tempdir().unwrap();
        let env = TestEnv {
            home: tmp.path().to_string_lossy().to_string(),
        };

        // Create fake worktree directories
        let ws = config::workspaces_path(&env);
        let wt_dir = ws.join("org-repo").join("issue-42");
        std::fs::create_dir_all(&wt_dir).unwrap();

        let output = list(&env, None).unwrap();
        assert!(output.contains("1 preserved worktree(s)"));
        assert!(output.contains("org-repo/issue-42"));
    }

    #[test]
    fn list_filters_by_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let env = TestEnv {
            home: tmp.path().to_string_lossy().to_string(),
        };

        let ws = config::workspaces_path(&env);
        std::fs::create_dir_all(ws.join("org-repo").join("issue-1")).unwrap();
        std::fs::create_dir_all(ws.join("other-repo").join("issue-2")).unwrap();

        let output = list(&env, Some("org/repo")).unwrap();
        assert!(output.contains("org-repo/issue-1"));
        assert!(!output.contains("other-repo"));
    }

    #[test]
    fn remove_deletes_worktree_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let env = TestEnv {
            home: tmp.path().to_string_lossy().to_string(),
        };

        let ws = config::workspaces_path(&env);
        let wt_dir = ws.join("org-repo").join("issue-42");
        std::fs::create_dir_all(&wt_dir).unwrap();

        let output = remove(&env, "issue-42").unwrap();
        assert!(output.contains("Removed"));
        assert!(!wt_dir.exists());
    }
}
