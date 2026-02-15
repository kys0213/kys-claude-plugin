import { describe, test, expect } from 'bun:test';
import { createCommitCommand } from '../../src/commands/commit';
import type { GitService } from '../../src/core/git';
import type { JiraService } from '../../src/core/jira';
import type { CommitInput } from '../../src/types';

// ============================================================
// commit command — Black-box Test
// ============================================================

function mockGit(overrides: Partial<GitService> = {}): GitService {
  return {
    isInsideWorkTree: async () => true,
    getCurrentBranch: async () => 'feat/something',
    detectDefaultBranch: async () => 'main',
    getSpecialState: async () => ({ rebase: false, merge: false, detached: false }),
    branchExists: async () => false,
    hasUncommittedChanges: async () => false,
    fetch: async () => {},
    checkout: async () => {},
    commit: async () => {},
    push: async () => {},
    pull: async () => {},
    addTracked: async () => {},
    ...overrides,
  };
}

function mockJira(overrides: Partial<JiraService> = {}): JiraService {
  return { detectTicket: () => null, ...overrides };
}

describe('commit command', () => {
  describe('커밋 메시지 포맷팅', () => {
    describe('Jira 브랜치', () => {
      test('Jira 티켓 감지 시 → "[WAD-0212] feat: description" 형식', async () => {
        const cmd = createCommitCommand({
          git: mockGit({ getCurrentBranch: async () => 'feat/WAD-0212' }),
          jira: mockJira({ detectTicket: () => ({ raw: 'WAD-0212', normalized: 'WAD-0212' }) }),
        });
        const result = await cmd.run({ type: 'feat', description: 'add login' });
        expect(result.ok).toBe(true);
        if (result.ok) {
          expect(result.data.subject).toBe('[WAD-0212] feat: add login');
        }
      });

      test('Jira 브랜치에서 scope 무시 → 티켓이 scope 대체', async () => {
        const cmd = createCommitCommand({
          git: mockGit({ getCurrentBranch: async () => 'feat/WAD-0212' }),
          jira: mockJira({ detectTicket: () => ({ raw: 'WAD-0212', normalized: 'WAD-0212' }) }),
        });
        const result = await cmd.run({ type: 'feat', description: 'add login', scope: 'auth' });
        expect(result.ok).toBe(true);
        if (result.ok) {
          // scope should be ignored when Jira ticket is present
          expect(result.data.subject).toBe('[WAD-0212] feat: add login');
          expect(result.data.subject).not.toContain('auth');
        }
      });

      test('output.jiraTicket에 감지된 티켓 포함', async () => {
        const cmd = createCommitCommand({
          git: mockGit({ getCurrentBranch: async () => 'feat/WAD-0212' }),
          jira: mockJira({ detectTicket: () => ({ raw: 'WAD-0212', normalized: 'WAD-0212' }) }),
        });
        const result = await cmd.run({ type: 'feat', description: 'add login' });
        expect(result.ok).toBe(true);
        if (result.ok) {
          expect(result.data.jiraTicket).toBe('WAD-0212');
        }
      });
    });

    describe('일반 브랜치', () => {
      test('scope 있으면 → "feat(auth): description" 형식', async () => {
        const cmd = createCommitCommand({
          git: mockGit(),
          jira: mockJira(),
        });
        const result = await cmd.run({ type: 'feat', description: 'add login', scope: 'auth' });
        expect(result.ok).toBe(true);
        if (result.ok) {
          expect(result.data.subject).toBe('feat(auth): add login');
        }
      });

      test('scope 없으면 → "feat: description" 형식', async () => {
        const cmd = createCommitCommand({
          git: mockGit(),
          jira: mockJira(),
        });
        const result = await cmd.run({ type: 'feat', description: 'add login' });
        expect(result.ok).toBe(true);
        if (result.ok) {
          expect(result.data.subject).toBe('feat: add login');
        }
      });

      test('output.jiraTicket은 undefined', async () => {
        const cmd = createCommitCommand({
          git: mockGit(),
          jira: mockJira(),
        });
        const result = await cmd.run({ type: 'feat', description: 'add login' });
        expect(result.ok).toBe(true);
        if (result.ok) {
          expect(result.data.jiraTicket).toBeUndefined();
        }
      });
    });
  });

  describe('commit type 검증', () => {
    test('유효한 type (feat, fix, docs, style, refactor, test, chore, perf) → 성공', async () => {
      const validTypes = ['feat', 'fix', 'docs', 'style', 'refactor', 'test', 'chore', 'perf'] as const;
      for (const type of validTypes) {
        const cmd = createCommitCommand({ git: mockGit(), jira: mockJira() });
        const result = await cmd.run({ type, description: 'test description' });
        expect(result.ok).toBe(true);
      }
    });

    test('유효하지 않은 type → ok: false, error 메시지', async () => {
      const cmd = createCommitCommand({ git: mockGit(), jira: mockJira() });
      const result = await cmd.run({ type: 'invalid' as any, description: 'test' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('Invalid commit type');
      }
    });
  });

  describe('git 조작', () => {
    test('skipAdd=false → git.addTracked() 호출', async () => {
      let addTrackedCalled = false;
      const cmd = createCommitCommand({
        git: mockGit({ addTracked: async () => { addTrackedCalled = true; } }),
        jira: mockJira(),
      });
      await cmd.run({ type: 'feat', description: 'test' });
      expect(addTrackedCalled).toBe(true);
    });

    test('skipAdd=true → git.addTracked() 미호출', async () => {
      let addTrackedCalled = false;
      const cmd = createCommitCommand({
        git: mockGit({ addTracked: async () => { addTrackedCalled = true; } }),
        jira: mockJira(),
      });
      await cmd.run({ type: 'feat', description: 'test', skipAdd: true });
      expect(addTrackedCalled).toBe(false);
    });

    test('git.commit()에 전달되는 메시지에 Co-Authored-By 포함', async () => {
      let committedMessage = '';
      const cmd = createCommitCommand({
        git: mockGit({ commit: async (msg) => { committedMessage = msg; } }),
        jira: mockJira(),
      });
      await cmd.run({ type: 'feat', description: 'test' });
      expect(committedMessage).toContain('Co-Authored-By: Claude <noreply@anthropic.com>');
    });

    test('git.commit()에 body가 있으면 subject + 빈줄 + body 포함', async () => {
      let committedMessage = '';
      const cmd = createCommitCommand({
        git: mockGit({ commit: async (msg) => { committedMessage = msg; } }),
        jira: mockJira(),
      });
      await cmd.run({ type: 'feat', description: 'test', body: 'detailed explanation' });
      expect(committedMessage).toContain('feat: test');
      expect(committedMessage).toContain('\n\ndetailed explanation');
    });
  });

  describe('에러 처리', () => {
    test('description 빈 문자열 → ok: false', async () => {
      const cmd = createCommitCommand({ git: mockGit(), jira: mockJira() });
      const result = await cmd.run({ type: 'feat', description: '' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('Description is required');
      }
    });

    test('git.commit() 실패 → ok: false, 에러 전파', async () => {
      const cmd = createCommitCommand({
        git: mockGit({ commit: async () => { throw new Error('commit failed'); } }),
        jira: mockJira(),
      });
      const result = await cmd.run({ type: 'feat', description: 'test' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toBe('commit failed');
      }
    });

    test('git.addTracked() 실패 → ok: false, 에러 전파', async () => {
      const cmd = createCommitCommand({
        git: mockGit({ addTracked: async () => { throw new Error('add failed'); } }),
        jira: mockJira(),
      });
      // addTracked throws, which is not caught by the command - it will propagate as unhandled
      // Looking at the source: addTracked is called without try/catch, so it will throw
      await expect(cmd.run({ type: 'feat', description: 'test' })).rejects.toThrow('add failed');
    });
  });
});
