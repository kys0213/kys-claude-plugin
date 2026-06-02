//! `reviews` command — port of `git-utils/src/commands/reviews.ts`. Resolves
//! a PR number (explicit or auto-detected) and returns its review threads.

use crate::git::core::github::GitHubService;
use crate::git::types::{CmdResult, ReviewsInput, ReviewsOutput};

pub struct ReviewsDeps<'a> {
    pub github: &'a dyn GitHubService,
}

/// Runs the reviews command.
pub fn run(deps: &ReviewsDeps, input: &ReviewsInput) -> CmdResult<ReviewsOutput> {
    let pr_number = match input.pr_number {
        Some(n) => n,
        None => match deps.github.detect_current_pr_number() {
            Ok(Some(n)) => n,
            _ => {
                return CmdResult::Err(
                    "No PR found. Provide a PR number or checkout a PR branch.".to_string(),
                )
            }
        },
    };

    match deps.github.get_review_threads(pr_number) {
        Ok(result) => CmdResult::Ok(ReviewsOutput {
            pr_title: result.pr_title,
            pr_url: result.pr_url,
            threads: result.threads,
        }),
        Err(e) => CmdResult::Err(e),
    }
}
