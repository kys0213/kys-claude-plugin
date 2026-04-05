#![allow(dead_code)]

use anyhow::{bail, Result};
use autopilot::git::GitOps;
use std::collections::{HashMap, HashSet};

/// A mock GitOps that returns predefined responses.
pub struct MockGit {
    head: String,
    diffs: HashMap<String, Vec<String>>,
    existing_commits: HashSet<String>,
    remote: Result<String, String>,
    repo_name: String,
}

impl MockGit {
    pub fn new() -> Self {
        Self {
            head: "abc1234".to_string(),
            diffs: HashMap::new(),
            existing_commits: HashSet::new(),
            remote: Ok("https://github.com/test/repo.git".to_string()),
            repo_name: "repo".to_string(),
        }
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
}
