// ============================================================
// branch command (← create-branch.sh)
// ============================================================

import type { Result, BranchInput, BranchOutput } from '../types';
import type { GitService } from '../core/git';

export interface BranchDeps {
  git: GitService;
}

export function createBranchCommand(deps: BranchDeps) {
  return {
    name: 'branch',
    description: 'Create a new branch from base branch',

    async run(input: BranchInput): Promise<Result<BranchOutput>> {
      if (!input.branchName || input.branchName.trim() === '') {
        return { ok: false, error: 'Branch name is required' };
      }

      // Check uncommitted changes
      if (await deps.git.hasUncommittedChanges()) {
        return { ok: false, error: 'Uncommitted changes detected. Please commit or stash first.' };
      }

      // Determine base branch
      let baseBranch = input.baseBranch;
      if (!baseBranch) {
        try {
          baseBranch = await deps.git.detectDefaultBranch();
        } catch (e) {
          return { ok: false, error: (e as Error).message };
        }
      }

      // Check base exists
      const baseExists = await deps.git.branchExists(baseBranch, 'any');
      if (!baseExists) {
        return { ok: false, error: `Base branch '${baseBranch}' does not exist locally or remotely.` };
      }

      // Check target doesn't exist
      if (await deps.git.branchExists(input.branchName, 'local')) {
        return { ok: false, error: `Branch '${input.branchName}' already exists.` };
      }

      // Fetch (ignore errors)
      try { await deps.git.fetch(); } catch { /* ignore */ }

      // Checkout base + pull
      const localExists = await deps.git.branchExists(baseBranch, 'local');
      if (localExists) {
        await deps.git.checkout(baseBranch);
        try { await deps.git.pull(baseBranch); } catch { /* ignore */ }
      } else {
        await deps.git.checkout(baseBranch, { create: true, track: `origin/${baseBranch}` });
      }

      // Create new branch
      try {
        await deps.git.checkout(input.branchName, { create: true });
      } catch (e) {
        return { ok: false, error: (e as Error).message };
      }

      return { ok: true, data: { branchName: input.branchName, baseBranch } };
    },
  };
}
