// ============================================================
// commit command (← commit.sh)
// ============================================================

import type { Command, CommitInput, CommitOutput, Result, COMMIT_TYPES } from '../types';
import type { GitService } from '../core/git';
import type { JiraService } from '../core/jira';

export interface CommitDeps {
  git: GitService;
  jira: JiraService;
}

const VALID_TYPES = new Set(['feat', 'fix', 'docs', 'style', 'refactor', 'test', 'chore', 'perf']);

export function createCommitCommand(deps: CommitDeps) {
  return {
    name: 'commit',
    description: 'Smart commit with Jira ticket detection',

    async run(input: CommitInput): Promise<Result<CommitOutput>> {
      // Validate
      if (!VALID_TYPES.has(input.type)) {
        return { ok: false, error: `Invalid commit type: ${input.type}` };
      }
      if (!input.description || input.description.trim() === '') {
        return { ok: false, error: 'Description is required' };
      }

      // Detect Jira ticket
      const branch = await deps.git.getCurrentBranch();
      const ticket = deps.jira.detectTicket(branch);

      // Format subject
      let subject: string;
      if (ticket) {
        subject = `[${ticket.normalized}] ${input.type}: ${input.description}`;
      } else if (input.scope) {
        subject = `${input.type}(${input.scope}): ${input.description}`;
      } else {
        subject = `${input.type}: ${input.description}`;
      }

      // Build full message
      let message = subject;
      if (input.body) {
        message += `\n\n${input.body}`;
      }
      message += '\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)';
      message += '\nCo-Authored-By: Claude <noreply@anthropic.com>';

      // Stage + commit
      if (!input.skipAdd) {
        await deps.git.addTracked();
      }

      try {
        await deps.git.commit(message);
      } catch (e) {
        return { ok: false, error: (e as Error).message };
      }

      return { ok: true, data: { subject, jiraTicket: ticket?.normalized } };
    },
  };
}
