// ============================================================
// pr command (← create-pr.sh)
// ============================================================

import type { Result, PrInput, PrOutput } from '../types';
import type { GitService } from '../core/git';
import type { JiraService } from '../core/jira';
import type { GitHubService } from '../core/github';

export interface PrDeps {
  git: GitService;
  jira: JiraService;
  github: GitHubService;
}

export function createPrCommand(deps: PrDeps) {
  return {
    name: 'pr',
    description: 'Create a Pull Request',

    async run(input: PrInput): Promise<Result<PrOutput>> {
      if (!input.title || input.title.trim() === '') {
        return { ok: false, error: 'Title is required' };
      }

      // Detect default branch
      let defaultBranch: string;
      try {
        defaultBranch = await deps.git.detectDefaultBranch();
      } catch (e) {
        return { ok: false, error: (e as Error).message };
      }

      // Check not on default branch
      const current = await deps.git.getCurrentBranch();
      if (current === defaultBranch) {
        return { ok: false, error: `Cannot create PR from default branch (${defaultBranch})` };
      }

      // Check gh auth
      if (!(await deps.github.isAuthenticated())) {
        return { ok: false, error: 'GitHub CLI is not authenticated. Run: gh auth login' };
      }

      // Jira ticket
      const ticket = deps.jira.detectTicket(current);
      const title = ticket ? `[${ticket.normalized}] ${input.title}` : input.title;

      // Push
      try {
        await deps.git.push(current, { setUpstream: true });
      } catch (e) {
        return { ok: false, error: (e as Error).message };
      }

      // Create PR
      let url: string;
      try {
        url = await deps.github.createPr({
          base: defaultBranch,
          title,
          body: input.description || '',
        });
      } catch (e) {
        return { ok: false, error: (e as Error).message };
      }

      return { ok: true, data: { url, title, baseBranch: defaultBranch, jiraTicket: ticket?.normalized } };
    },
  };
}
