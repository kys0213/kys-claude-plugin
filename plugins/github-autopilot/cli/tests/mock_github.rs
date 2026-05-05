#![allow(dead_code)]

use anyhow::Result;
use autopilot::github::{CompletedRun, GitHub, OpenIssue};

pub struct MockGitHub {
    default_branch: String,
    completed_runs: Vec<CompletedRun>,
    open_issues: Vec<OpenIssue>,
}

impl Default for MockGitHub {
    fn default() -> Self {
        Self::new()
    }
}

impl MockGitHub {
    pub fn new() -> Self {
        Self {
            default_branch: "main".to_string(),
            completed_runs: vec![],
            open_issues: vec![],
        }
    }

    pub fn with_default_branch(mut self, branch: &str) -> Self {
        self.default_branch = branch.to_string();
        self
    }

    pub fn with_runs(mut self, runs: Vec<CompletedRun>) -> Self {
        self.completed_runs = runs;
        self
    }

    pub fn with_issues(mut self, issues: Vec<OpenIssue>) -> Self {
        self.open_issues = issues;
        self
    }
}

impl GitHub for MockGitHub {
    fn default_branch(&self) -> Result<String> {
        Ok(self.default_branch.clone())
    }

    fn list_completed_runs(&self, _limit: u64) -> Result<Vec<CompletedRun>> {
        Ok(self.completed_runs.clone())
    }

    fn list_open_issues(&self, _limit: u64) -> Result<Vec<OpenIssue>> {
        Ok(self.open_issues.clone())
    }
}
