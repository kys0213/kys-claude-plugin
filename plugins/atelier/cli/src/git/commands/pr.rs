//! `pr` command, ported from git-utils `commands/pr.ts` (originally
//! `create-pr.sh`). Pushes the current branch and opens a PR against the
//! default branch, prefixing the title with a detected Jira ticket.

use crate::git::core::git::GitService;
use crate::git::core::github::GitHubService;
use crate::git::core::jira::detect_ticket;
use crate::git::types::{PrInput, PrOutput};

/// The `pr` command, backed by injected [`GitService`] and [`GitHubService`].
pub struct PrCommand<'a> {
    git: &'a dyn GitService,
    github: &'a dyn GitHubService,
}

impl<'a> PrCommand<'a> {
    pub fn new(git: &'a dyn GitService, github: &'a dyn GitHubService) -> Self {
        Self { git, github }
    }

    pub fn run(&self, input: &PrInput) -> Result<PrOutput, String> {
        if input.title.trim().is_empty() {
            return Err("Title is required".to_string());
        }

        let default_branch = self
            .git
            .detect_default_branch()
            .map_err(|e| e.to_string())?;

        let current = self.git.get_current_branch().map_err(|e| e.to_string())?;
        if current == default_branch {
            return Err(format!(
                "Cannot create PR from default branch ({default_branch})"
            ));
        }

        if !self.github.is_authenticated().map_err(|e| e.to_string())? {
            return Err("GitHub CLI is not authenticated. Run: gh auth login".to_string());
        }

        let ticket = detect_ticket(&current);
        let title = match &ticket {
            Some(t) => format!("[{}] {}", t.normalized, input.title),
            None => input.title.clone(),
        };

        self.git.push(&current, true).map_err(|e| e.to_string())?;

        let body = input.description.clone().unwrap_or_default();
        let url = self
            .github
            .create_pr(&default_branch, &title, &body)
            .map_err(|e| e.to_string())?;

        Ok(PrOutput {
            url,
            title,
            base_branch: default_branch,
            jira_ticket: ticket.map(|t| t.normalized),
        })
    }
}
