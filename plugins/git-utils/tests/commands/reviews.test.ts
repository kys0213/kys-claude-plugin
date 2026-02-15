import { describe, test, expect } from 'bun:test';
import { createReviewsCommand } from '../../src/commands/reviews';
import type { GitHubService } from '../../src/core/github';
import type { ReviewThread } from '../../src/types';

// ============================================================
// reviews command — Black-box Test
// ============================================================

function mockGitHub(overrides: Partial<GitHubService> = {}): GitHubService {
  return {
    isAuthenticated: async () => true,
    createPr: async () => 'https://github.com/org/repo/pull/1',
    getReviewThreads: async () => ({ prTitle: 'test', prUrl: 'https://github.com/org/repo/pull/1', threads: [] }),
    detectCurrentPrNumber: async () => null,
    ...overrides,
  };
}

const sampleThread: ReviewThread = {
  isResolved: false,
  isOutdated: false,
  path: 'src/index.ts',
  line: 42,
  comments: [
    {
      author: 'reviewer1',
      body: 'Please fix this.',
      createdAt: '2024-01-15T10:00:00Z',
      url: 'https://github.com/org/repo/pull/1#discussion_r1',
    },
  ],
};

const resolvedThread: ReviewThread = {
  isResolved: true,
  isOutdated: true,
  path: 'src/utils.ts',
  line: 10,
  comments: [
    {
      author: 'reviewer2',
      body: 'Looks good now.',
      createdAt: '2024-01-16T12:00:00Z',
      url: 'https://github.com/org/repo/pull/1#discussion_r2',
    },
  ],
};

describe('reviews command', () => {
  describe('PR 번호 결정', () => {
    test('prNumber 직접 지정 → 해당 번호 사용', async () => {
      let receivedPrNumber: number | undefined;
      const cmd = createReviewsCommand({
        github: mockGitHub({
          getReviewThreads: async (prNumber) => {
            receivedPrNumber = prNumber;
            return { prTitle: 'test', prUrl: 'url', threads: [] };
          },
        }),
      });
      await cmd.run({ prNumber: 42 });
      expect(receivedPrNumber).toBe(42);
    });

    test('prNumber 미지정 → detectCurrentPrNumber()로 자동 감지', async () => {
      let receivedPrNumber: number | undefined;
      const cmd = createReviewsCommand({
        github: mockGitHub({
          detectCurrentPrNumber: async () => 99,
          getReviewThreads: async (prNumber) => {
            receivedPrNumber = prNumber;
            return { prTitle: 'test', prUrl: 'url', threads: [] };
          },
        }),
      });
      await cmd.run({});
      expect(receivedPrNumber).toBe(99);
    });

    test('자동 감지 실패 → ok: false, PR 번호 필요 안내', async () => {
      const cmd = createReviewsCommand({
        github: mockGitHub({
          detectCurrentPrNumber: async () => null,
        }),
      });
      const result = await cmd.run({});
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('No PR found');
      }
    });
  });

  describe('정상 동작', () => {
    test('리뷰 쓰레드가 있으면 → threads 배열에 포함', async () => {
      const cmd = createReviewsCommand({
        github: mockGitHub({
          getReviewThreads: async () => ({
            prTitle: 'feat: add auth',
            prUrl: 'https://github.com/org/repo/pull/1',
            threads: [sampleThread],
          }),
        }),
      });
      const result = await cmd.run({ prNumber: 1 });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.threads).toHaveLength(1);
        expect(result.data.threads[0]).toEqual(sampleThread);
      }
    });

    test('리뷰 쓰레드가 없으면 → 빈 threads 배열', async () => {
      const cmd = createReviewsCommand({
        github: mockGitHub({
          getReviewThreads: async () => ({
            prTitle: 'feat: add auth',
            prUrl: 'https://github.com/org/repo/pull/1',
            threads: [],
          }),
        }),
      });
      const result = await cmd.run({ prNumber: 1 });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.threads).toEqual([]);
      }
    });

    test('output에 prTitle, prUrl 포함', async () => {
      const cmd = createReviewsCommand({
        github: mockGitHub({
          getReviewThreads: async () => ({
            prTitle: 'feat: add auth',
            prUrl: 'https://github.com/org/repo/pull/1',
            threads: [],
          }),
        }),
      });
      const result = await cmd.run({ prNumber: 1 });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.prTitle).toBe('feat: add auth');
        expect(result.data.prUrl).toBe('https://github.com/org/repo/pull/1');
      }
    });
  });

  describe('쓰레드 데이터 매핑', () => {
    test('isResolved, isOutdated 필드 매핑', async () => {
      const cmd = createReviewsCommand({
        github: mockGitHub({
          getReviewThreads: async () => ({
            prTitle: 'test',
            prUrl: 'url',
            threads: [sampleThread, resolvedThread],
          }),
        }),
      });
      const result = await cmd.run({ prNumber: 1 });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.threads[0].isResolved).toBe(false);
        expect(result.data.threads[0].isOutdated).toBe(false);
        expect(result.data.threads[1].isResolved).toBe(true);
        expect(result.data.threads[1].isOutdated).toBe(true);
      }
    });

    test('path, line 필드 매핑', async () => {
      const cmd = createReviewsCommand({
        github: mockGitHub({
          getReviewThreads: async () => ({
            prTitle: 'test',
            prUrl: 'url',
            threads: [sampleThread],
          }),
        }),
      });
      const result = await cmd.run({ prNumber: 1 });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.threads[0].path).toBe('src/index.ts');
        expect(result.data.threads[0].line).toBe(42);
      }
    });

    test('comments 배열에 author, body, createdAt, url 포함', async () => {
      const cmd = createReviewsCommand({
        github: mockGitHub({
          getReviewThreads: async () => ({
            prTitle: 'test',
            prUrl: 'url',
            threads: [sampleThread],
          }),
        }),
      });
      const result = await cmd.run({ prNumber: 1 });
      expect(result.ok).toBe(true);
      if (result.ok) {
        const comment = result.data.threads[0].comments[0];
        expect(comment.author).toBe('reviewer1');
        expect(comment.body).toBe('Please fix this.');
        expect(comment.createdAt).toBe('2024-01-15T10:00:00Z');
        expect(comment.url).toBe('https://github.com/org/repo/pull/1#discussion_r1');
      }
    });
  });

  describe('에러 처리', () => {
    test('gh API 호출 실패 → ok: false, 에러 전파', async () => {
      const cmd = createReviewsCommand({
        github: mockGitHub({
          getReviewThreads: async () => { throw new Error('API rate limit exceeded'); },
        }),
      });
      const result = await cmd.run({ prNumber: 1 });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toBe('API rate limit exceeded');
      }
    });

    test('존재하지 않는 PR 번호 → ok: false', async () => {
      const cmd = createReviewsCommand({
        github: mockGitHub({
          getReviewThreads: async () => { throw new Error('Could not resolve to a PullRequest with the number of 9999'); },
        }),
      });
      const result = await cmd.run({ prNumber: 9999 });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('9999');
      }
    });
  });
});
