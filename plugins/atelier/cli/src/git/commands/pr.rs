//! `pr` command — port of `git-utils/src/commands/pr.ts`. Pushes the current
//! branch and opens a PR against the default branch, prefixing the title with
//! a detected Jira ticket. All failure paths are returned as handled errors.

use crate::git::core::git::{GitService, PushOptions};
use crate::git::core::github::{CreatePrOptions, GitHubService};
use crate::git::core::jira::JiraService;
use crate::git::types::{CmdResult, PrInput, PrOutput};

pub struct PrDeps<'a> {
    pub git: &'a dyn GitService,
    pub jira: &'a dyn JiraService,
    pub github: &'a dyn GitHubService,
}

/// Runs the pr command.
pub fn run(deps: &PrDeps, input: &PrInput) -> CmdResult<PrOutput> {
    if input.title.trim().is_empty() {
        return CmdResult::Err("Title is required".to_string());
    }

    let default_branch = match deps.git.detect_default_branch() {
        Ok(b) => b,
        Err(e) => return CmdResult::Err(e),
    };

    let current = deps.git.get_current_branch();
    if current == default_branch {
        return CmdResult::Err(format!(
            "Cannot create PR from default branch ({default_branch})"
        ));
    }

    if !deps.github.is_authenticated() {
        return CmdResult::Err("GitHub CLI is not authenticated. Run: gh auth login".to_string());
    }

    let ticket = deps.jira.detect_ticket(&current);
    let title = match &ticket {
        Some(t) => format!("[{}] {}", t.normalized, input.title),
        None => input.title.clone(),
    };

    if let Err(e) = deps
        .git
        .push(&current, Some(&PushOptions { set_upstream: true }))
    {
        return CmdResult::Err(e);
    }

    let body = input.description.clone().unwrap_or_default();
    let url = match deps.github.create_pr(&CreatePrOptions {
        base: &default_branch,
        title: &title,
        body: &body,
    }) {
        Ok(u) => u,
        Err(e) => return CmdResult::Err(e),
    };

    CmdResult::Ok(PrOutput {
        url,
        title,
        base_branch: default_branch,
        jira_ticket: ticket.map(|t| t.normalized),
    })
}
