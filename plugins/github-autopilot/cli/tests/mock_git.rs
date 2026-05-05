#![allow(dead_code)]

use anyhow::{bail, Result};
use autopilot::git::{GitOps, WorktreeEntry};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// A mock GitOps that returns predefined responses.
pub struct MockGit {
    head: String,
    diffs: HashMap<String, Vec<String>>,
    existing_commits: HashSet<String>,
    remote: Result<String, String>,
    repo_name: String,
    refs: HashMap<String, String>,
    rev_list_counts: HashMap<String, u64>,
    worktrees: Vec<WorktreeEntry>,
    removed_worktrees: Mutex<Vec<String>>,
    deleted_branches: Mutex<Vec<String>>,
    pruned: Mutex<bool>,
    fail_worktree_list: bool,
    fail_worktree_remove: bool,
    fail_branch_delete: bool,
    uncommitted_worktrees: HashSet<String>,
    committed_worktrees: Mutex<Vec<(String, String)>>,
    fail_commit: bool,
}

impl Default for MockGit {
    fn default() -> Self {
        Self::new()
    }
}

impl MockGit {
    pub fn new() -> Self {
        Self {
            head: "abc1234".to_string(),
            diffs: HashMap::new(),
            existing_commits: HashSet::new(),
            remote: Ok("https://github.com/test/repo.git".to_string()),
            repo_name: "repo".to_string(),
            refs: HashMap::new(),
            rev_list_counts: HashMap::new(),
            worktrees: Vec::new(),
            removed_worktrees: Mutex::new(Vec::new()),
            deleted_branches: Mutex::new(Vec::new()),
            pruned: Mutex::new(false),
            fail_worktree_list: false,
            fail_worktree_remove: false,
            fail_branch_delete: false,
            uncommitted_worktrees: HashSet::new(),
            committed_worktrees: Mutex::new(Vec::new()),
            fail_commit: false,
        }
    }

    pub fn with_fail_worktree_list(mut self) -> Self {
        self.fail_worktree_list = true;
        self
    }

    pub fn with_fail_worktree_remove(mut self) -> Self {
        self.fail_worktree_remove = true;
        self
    }

    pub fn with_fail_branch_delete(mut self) -> Self {
        self.fail_branch_delete = true;
        self
    }

    pub fn with_uncommitted_worktree(mut self, path: &str) -> Self {
        self.uncommitted_worktrees.insert(path.to_string());
        self
    }

    pub fn with_fail_commit(mut self) -> Self {
        self.fail_commit = true;
        self
    }

    pub fn committed_worktrees(&self) -> Vec<(String, String)> {
        self.committed_worktrees.lock().unwrap().clone()
    }

    pub fn with_worktree(mut self, path: &str, branch: Option<&str>) -> Self {
        self.worktrees.push(WorktreeEntry {
            path: path.to_string(),
            branch: branch.map(String::from),
        });
        self
    }

    pub fn removed_worktrees(&self) -> Vec<String> {
        self.removed_worktrees.lock().unwrap().clone()
    }

    pub fn deleted_branches(&self) -> Vec<String> {
        self.deleted_branches.lock().unwrap().clone()
    }

    pub fn was_pruned(&self) -> bool {
        *self.pruned.lock().unwrap()
    }

    pub fn with_head(mut self, hash: &str) -> Self {
        self.head = hash.to_string();
        self.existing_commits.insert(hash.to_string());
        self
    }

    pub fn with_diff(mut self, from: &str, to: &str, files: Vec<&str>) -> Self {
        let key = format!("{from}..{to}");
        self.diffs
            .insert(key, files.into_iter().map(String::from).collect());
        self
    }

    pub fn with_commit(mut self, hash: &str) -> Self {
        self.existing_commits.insert(hash.to_string());
        self
    }

    pub fn with_remote_err(mut self, msg: &str) -> Self {
        self.remote = Err(msg.to_string());
        self
    }

    pub fn with_repo_name(mut self, name: &str) -> Self {
        self.repo_name = name.to_string();
        self
    }

    pub fn with_ref(mut self, refname: &str, sha: &str) -> Self {
        self.refs.insert(refname.to_string(), sha.to_string());
        self
    }

    pub fn with_rev_list_count(mut self, from: &str, to: &str, count: u64) -> Self {
        let key = format!("{from}..{to}");
        self.rev_list_counts.insert(key, count);
        self
    }
}

impl GitOps for MockGit {
    fn rev_parse_head(&self) -> Result<String> {
        Ok(self.head.clone())
    }

    fn diff_name_only(&self, from: &str, to: &str) -> Result<Vec<String>> {
        let key = format!("{from}..{to}");
        Ok(self.diffs.get(&key).cloned().unwrap_or_default())
    }

    fn commit_exists(&self, hash: &str) -> Result<bool> {
        Ok(self.existing_commits.contains(hash))
    }

    fn remote_url(&self, _name: &str) -> Result<String> {
        match &self.remote {
            Ok(url) => Ok(url.clone()),
            Err(msg) => bail!("{msg}"),
        }
    }

    fn repo_name(&self) -> Result<String> {
        Ok(self.repo_name.clone())
    }

    fn fetch_remote(&self, _remote: &str, _branch: &str) -> Result<()> {
        Ok(())
    }

    fn rev_parse_ref(&self, refname: &str) -> Result<String> {
        self.refs
            .get(refname)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown ref: {refname}"))
    }

    fn rev_list_count(&self, from: &str, to: &str) -> Result<u64> {
        let key = format!("{from}..{to}");
        Ok(self.rev_list_counts.get(&key).copied().unwrap_or(0))
    }

    fn worktree_list(&self) -> Result<Vec<WorktreeEntry>> {
        if self.fail_worktree_list {
            bail!("worktree list failed");
        }
        Ok(self.worktrees.clone())
    }

    fn worktree_remove(&self, path: &str) -> Result<()> {
        if self.fail_worktree_remove {
            bail!("worktree remove failed");
        }
        self.removed_worktrees
            .lock()
            .unwrap()
            .push(path.to_string());
        Ok(())
    }

    fn worktree_prune(&self) -> Result<()> {
        *self.pruned.lock().unwrap() = true;
        Ok(())
    }

    fn branch_delete(&self, name: &str) -> Result<()> {
        if self.fail_branch_delete {
            bail!("branch delete failed");
        }
        self.deleted_branches.lock().unwrap().push(name.to_string());
        Ok(())
    }

    fn has_uncommitted_changes(&self, worktree_path: &str) -> Result<bool> {
        Ok(self.uncommitted_worktrees.contains(worktree_path))
    }

    fn commit_all_in_worktree(&self, worktree_path: &str, message: &str) -> Result<()> {
        if self.fail_commit {
            bail!("commit failed");
        }
        self.committed_worktrees
            .lock()
            .unwrap()
            .push((worktree_path.to_string(), message.to_string()));
        Ok(())
    }
}
