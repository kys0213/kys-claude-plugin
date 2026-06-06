//! `commit` command, ported from git-utils `commands/commit.ts`
//! (originally `commit.sh`). Builds a conventional-commit subject with
//! automatic Jira-ticket detection from the current branch.

use crate::git::core::git::GitService;
use crate::git::core::jira::detect_ticket;
use crate::git::types::{CommitInput, CommitOutput, COMMIT_TYPES};

const COMMIT_FOOTER: &str = "\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\nCo-Authored-By: Claude <noreply@anthropic.com>";

/// The `commit` command, backed by an injected [`GitService`]. Jira detection
/// uses the pure [`detect_ticket`] function (no injection needed — it is a
/// deterministic function of the branch name).
pub struct CommitCommand<'a> {
    git: &'a dyn GitService,
}

impl<'a> CommitCommand<'a> {
    pub fn new(git: &'a dyn GitService) -> Self {
        Self { git }
    }

    pub fn run(&self, input: &CommitInput) -> Result<CommitOutput, String> {
        if !COMMIT_TYPES.contains(&input.commit_type.as_str()) {
            return Err(format!("Invalid commit type: {}", input.commit_type));
        }
        if input.description.trim().is_empty() {
            return Err("Description is required".to_string());
        }

        // Detect a Jira ticket from the current branch.
        let branch = self.git.get_current_branch().map_err(|e| e.to_string())?;
        let ticket = detect_ticket(&branch);

        // Format the subject: ticket > scope > plain.
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

        // Build the full message.
        let mut message = subject.clone();
        if let Some(body) = &input.body {
            message.push_str(&format!("\n\n{body}"));
        }
        message.push_str(COMMIT_FOOTER);

        // Stage (unless skipped), then commit.
        if !input.skip_add {
            self.git.add_tracked().map_err(|e| e.to_string())?;
        }
        self.git.commit(&message).map_err(|e| e.to_string())?;

        Ok(CommitOutput {
            subject,
            jira_ticket: ticket.map(|t| t.normalized),
        })
    }
}
