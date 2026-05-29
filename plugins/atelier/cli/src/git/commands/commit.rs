//! `commit` command — port of `git-utils/src/commands/commit.ts`. Formats a
//! conventional-commit subject (with Jira-ticket prefixing), stages tracked
//! changes, and commits. Dependencies (`GitService`, `JiraService`) are
//! injected for unit testing.
//!
//! The outer `Result::Err` models the TS "unhandled throw" path: `addTracked`
//! failures propagate (the original code does not wrap them in try/catch),
//! whereas validation and `commit` failures are returned as `CmdResult::Err`.

use crate::git::core::git::GitService;
use crate::git::core::jira::JiraService;
use crate::git::types::{CmdResult, CommitInput, CommitOutput, COMMIT_TYPES};

pub struct CommitDeps<'a> {
    pub git: &'a dyn GitService,
    pub jira: &'a dyn JiraService,
}

/// Runs the commit command. See module docs for the dual error model.
pub fn run(deps: &CommitDeps, input: &CommitInput) -> Result<CmdResult<CommitOutput>, String> {
    if !COMMIT_TYPES.contains(&input.commit_type.as_str()) {
        return Ok(CmdResult::Err(format!(
            "Invalid commit type: {}",
            input.commit_type
        )));
    }
    if input.description.trim().is_empty() {
        return Ok(CmdResult::Err("Description is required".to_string()));
    }

    let branch = deps.git.get_current_branch();
    let ticket = deps.jira.detect_ticket(&branch);

    let subject = if let Some(t) = &ticket {
        format!(
            "[{}] {}: {}",
            t.normalized, input.commit_type, input.description
        )
    } else if let Some(scope) = &input.scope {
        format!("{}({}): {}", input.commit_type, scope, input.description)
    } else {
        format!("{}: {}", input.commit_type, input.description)
    };

    let mut message = subject.clone();
    if let Some(body) = &input.body {
        message.push_str(&format!("\n\n{body}"));
    }
    message.push_str("\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)");
    message.push_str("\nCo-Authored-By: Claude <noreply@anthropic.com>");

    // addTracked failures propagate (outer Err), matching the TS source which
    // does not wrap this call in try/catch.
    if !input.skip_add {
        deps.git.add_tracked()?;
    }

    if let Err(e) = deps.git.commit(&message) {
        return Ok(CmdResult::Err(e));
    }

    Ok(CmdResult::Ok(CommitOutput {
        subject,
        jira_ticket: ticket.map(|t| t.normalized),
    }))
}
