// ============================================================
// reviews command (← unresolved-reviews.sh)
// ============================================================

import type { Result, ReviewsInput, ReviewsOutput } from '../types';
import type { GitHubService } from '../core/github';

export interface ReviewsDeps {
  github: GitHubService;
}

export function createReviewsCommand(deps: ReviewsDeps) {
  return {
    name: 'reviews',
    description: 'Query unresolved PR review threads',

    async run(input: ReviewsInput): Promise<Result<ReviewsOutput>> {
      let prNumber = input.prNumber;

      if (!prNumber) {
        prNumber = await deps.github.detectCurrentPrNumber() ?? undefined;
        if (!prNumber) {
          return { ok: false, error: 'No PR found. Provide a PR number or checkout a PR branch.' };
        }
      }

      try {
        const result = await deps.github.getReviewThreads(prNumber);
        return {
          ok: true,
          data: {
            prTitle: result.prTitle,
            prUrl: result.prUrl,
            threads: result.threads,
          },
        };
      } catch (e) {
        return { ok: false, error: (e as Error).message };
      }
    },
  };
}
