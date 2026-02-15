import { describe, test, expect } from 'bun:test';
import { createPrCommand } from '../../src/commands/pr';
import type { GitService } from '../../src/core/git';
import type { JiraService } from '../../src/core/jira';
import type { GitHubService } from '../../src/core/github';

// ============================================================
// pr command — Black-box Test
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

function mockGitHub(overrides: Partial<GitHubService> = {}): GitHubService {
  return {
    isAuthenticated: async () => true,
    createPr: async () => 'https://github.com/org/repo/pull/1',
    getReviewThreads: async () => ({ prTitle: 'test', prUrl: 'url', threads: [] }),
    detectCurrentPrNumber: async () => null,
    ...overrides,
  };
}

describe('pr command', () => {
  describe('PR 타이틀 포맷팅', () => {
    test('Jira 티켓 감지 시 → "[WAD-0212] title" 형식', async () => {
      const cmd = createPrCommand({
        git: mockGit({ getCurrentBranch: async () => 'feat/WAD-0212' }),
        jira: mockJira({ detectTicket: () => ({ raw: 'WAD-0212', normalized: 'WAD-0212' }) }),
        github: mockGitHub(),
      });
      const result = await cmd.run({ title: 'add login feature' });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.title).toBe('[WAD-0212] add login feature');
      }
    });

    test('Jira 티켓 미감지 → title 그대로 사용', async () => {
      const cmd = createPrCommand({
        git: mockGit(),
        jira: mockJira(),
        github: mockGitHub(),
      });
      const result = await cmd.run({ title: 'add login feature' });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.title).toBe('add login feature');
      }
    });
  });

  describe('정상 동작', () => {
    test('push → pr create 순서로 호출', async () => {
      const calls: string[] = [];
      const cmd = createPrCommand({
        git: mockGit({
          push: async () => { calls.push('push'); },
        }),
        jira: mockJira(),
        github: mockGitHub({
          createPr: async () => { calls.push('createPr'); return 'https://github.com/org/repo/pull/1'; },
        }),
      });
      await cmd.run({ title: 'test pr' });
      expect(calls).toEqual(['push', 'createPr']);
    });

    test('description 있으면 PR body에 포함', async () => {
      let receivedBody = '';
      const cmd = createPrCommand({
        git: mockGit(),
        jira: mockJira(),
        github: mockGitHub({
          createPr: async (opts) => { receivedBody = opts.body; return 'https://github.com/org/repo/pull/1'; },
        }),
      });
      await cmd.run({ title: 'test pr', description: 'detailed description' });
      expect(receivedBody).toBe('detailed description');
    });

    test('description 없으면 빈 body로 생성', async () => {
      let receivedBody: string | undefined;
      const cmd = createPrCommand({
        git: mockGit(),
        jira: mockJira(),
        github: mockGitHub({
          createPr: async (opts) => { receivedBody = opts.body; return 'https://github.com/org/repo/pull/1'; },
        }),
      });
      await cmd.run({ title: 'test pr' });
      expect(receivedBody).toBe('');
    });

    test('output에 PR URL, 최종 title, baseBranch 반환', async () => {
      const cmd = createPrCommand({
        git: mockGit(),
        jira: mockJira(),
        github: mockGitHub({
          createPr: async () => 'https://github.com/org/repo/pull/42',
        }),
      });
      const result = await cmd.run({ title: 'feat: add auth' });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.url).toBe('https://github.com/org/repo/pull/42');
        expect(result.data.title).toBe('feat: add auth');
        expect(result.data.baseBranch).toBe('main');
      }
    });
  });

  describe('사전 조건 검증', () => {
    test('현재 브랜치가 default 브랜치면 → ok: false', async () => {
      const cmd = createPrCommand({
        git: mockGit({
          getCurrentBranch: async () => 'main',
          detectDefaultBranch: async () => 'main',
        }),
        jira: mockJira(),
        github: mockGitHub(),
      });
      const result = await cmd.run({ title: 'test pr' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('Cannot create PR from default branch');
      }
    });

    test('GitHub CLI 미인증 → ok: false, 인증 안내 메시지', async () => {
      const cmd = createPrCommand({
        git: mockGit(),
        jira: mockJira(),
        github: mockGitHub({ isAuthenticated: async () => false }),
      });
      const result = await cmd.run({ title: 'test pr' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('not authenticated');
        expect(result.error).toContain('gh auth login');
      }
    });
  });

  describe('에러 처리', () => {
    test('git push 실패 → ok: false, 에러 전파', async () => {
      const cmd = createPrCommand({
        git: mockGit({ push: async () => { throw new Error('push rejected'); } }),
        jira: mockJira(),
        github: mockGitHub(),
      });
      const result = await cmd.run({ title: 'test pr' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toBe('push rejected');
      }
    });

    test('gh pr create 실패 → ok: false, 에러 전파', async () => {
      const cmd = createPrCommand({
        git: mockGit(),
        jira: mockJira(),
        github: mockGitHub({
          createPr: async () => { throw new Error('PR already exists'); },
        }),
      });
      const result = await cmd.run({ title: 'test pr' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toBe('PR already exists');
      }
    });

    test('title 빈 문자열 → ok: false', async () => {
      const cmd = createPrCommand({
        git: mockGit(),
        jira: mockJira(),
        github: mockGitHub(),
      });
      const result = await cmd.run({ title: '' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('Title is required');
      }
    });
  });
});
