//! `reviews` command, ported from git-utils `commands/reviews.ts`
//! (originally `unresolved-reviews.sh`). Resolves the PR number (explicit or
//! detected) and fetches its review threads.

use crate::git::core::github::GitHubService;
use crate::git::types::{ReviewsInput, ReviewsOutput};

/// The `reviews` command, backed by an injected [`GitHubService`].
pub struct ReviewsCommand<'a> {
    github: &'a dyn GitHubService,
}

impl<'a> ReviewsCommand<'a> {
    pub fn new(github: &'a dyn GitHubService) -> Self {
        Self { github }
    }

    pub fn run(&self, input: &ReviewsInput) -> Result<ReviewsOutput, String> {
        let pr_number = match input.pr_number {
            Some(n) => n,
            None => match self
                .github
                .detect_current_pr_number()
                .map_err(|e| e.to_string())?
            {
                Some(n) => n,
                None => {
                    return Err(
                        "No PR found. Provide a PR number or checkout a PR branch.".to_string()
                    )
                }
            },
        };

        self.github
            .get_review_threads(pr_number)
            .map_err(|e| e.to_string())
    }
}
