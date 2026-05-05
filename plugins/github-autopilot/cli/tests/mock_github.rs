#![allow(dead_code)]

use anyhow::{bail, Result};
use autopilot::github::{CompletedRun, GitHub, OpenIssue};
use std::sync::Mutex;

pub struct MockGitHub {
    default_branch: String,
    completed_runs: Vec<CompletedRun>,
    open_issues: Vec<OpenIssue>,
    /// Remaining number of times `list_open_issues` should return Err.
    /// Decremented on each call; reaches 0 → returns the configured Ok.
    fail_issues_remaining: Mutex<u64>,
    /// Remaining number of times `list_completed_runs` should return Err.
    fail_runs_remaining: Mutex<u64>,
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
            fail_issues_remaining: Mutex::new(0),
            fail_runs_remaining: Mutex::new(0),
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

    /// Configures `list_open_issues` to return `Err(...)` for the next
    /// `n` calls; subsequent calls behave normally and return the
    /// configured issues. Used by resilience scenarios that simulate a
    /// transient `gh` failure followed by recovery.
    pub fn with_fail_issues_first_n(self, n: u64) -> Self {
        *self.fail_issues_remaining.lock().unwrap() = n;
        self
    }

    /// Same as `with_fail_issues_first_n` but for `list_completed_runs`.
    pub fn with_fail_runs_first_n(self, n: u64) -> Self {
        *self.fail_runs_remaining.lock().unwrap() = n;
        self
    }
}

impl GitHub for MockGitHub {
    fn default_branch(&self) -> Result<String> {
        Ok(self.default_branch.clone())
    }

    fn list_completed_runs(&self, _limit: u64) -> Result<Vec<CompletedRun>> {
        let mut remaining = self.fail_runs_remaining.lock().unwrap();
        if *remaining > 0 {
            *remaining -= 1;
            bail!("simulated transient gh failure (list_completed_runs)");
        }
        Ok(self.completed_runs.clone())
    }

    fn list_open_issues(&self, _limit: u64) -> Result<Vec<OpenIssue>> {
        let mut remaining = self.fail_issues_remaining.lock().unwrap();
        if *remaining > 0 {
            *remaining -= 1;
            bail!("simulated transient gh failure (list_open_issues)");
        }
        Ok(self.open_issues.clone())
    }
}
