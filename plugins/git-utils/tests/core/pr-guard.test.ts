import { describe, test, expect } from 'bun:test';
import { createPrGuardService } from '../../src/core/pr-guard';
import type { GitHubService } from '../../src/core/github';

// ============================================================
// PrGuardService.check — Mock 기반 Unit Test
// ============================================================

function mockGitHub(overrides: Partial<GitHubService> = {}): GitHubService {
  return {
    isAuthenticated: async () => true,
    createPr: async () => 'https://github.com/org/repo/pull/1',
    getReviewThreads: async () => ({ prTitle: '', prUrl: '', threads: [] }),
    detectCurrentPrNumber: async () => null,
    ...overrides,
  };
}

describe('PrGuardService.check', () => {
  describe('열린 PR이 있을 때', () => {
    test('allowed: false + PR 정보 포함', async () => {
      const guard = createPrGuardService(mockGitHub({
        detectCurrentPrNumber: async () => 123,
      }));
      const result = await guard.check({});

      expect(result.allowed).toBe(false);
      expect(result.prNumber).toBe(123);
      expect(result.reason).toContain('#123');
      expect(result.reason).toContain('열린 PR이 있습니다');
    });

    test('reason에 안내 메시지 포함', async () => {
      const guard = createPrGuardService(mockGitHub({
        detectCurrentPrNumber: async () => 42,
      }));
      const result = await guard.check({});

      expect(result.reason).toContain('기존 PR을 머지하거나 닫기');
      expect(result.reason).toContain('git push만 실행하세요');
    });
  });

  describe('열린 PR이 없을 때', () => {
    test('allowed: true', async () => {
      const guard = createPrGuardService(mockGitHub({
        detectCurrentPrNumber: async () => null,
      }));
      const result = await guard.check({});

      expect(result.allowed).toBe(true);
      expect(result.prNumber).toBeUndefined();
    });

    test('merged PR이 있는 브랜치에서는 차단하지 않아야 함 (OPEN이 아닌 PR은 무시)', async () => {
      // detectCurrentPrNumber()는 OPEN 상태가 아닌 PR에 대해 null을 반환
      const guard = createPrGuardService(mockGitHub({
        detectCurrentPrNumber: async () => null,
      }));
      const result = await guard.check({ toolCommand: 'gh pr create --title "new feature"' });

      expect(result.allowed).toBe(true);
      expect(result.prNumber).toBeUndefined();
    });
  });

  describe('gh 실패 시 (안전 모드)', () => {
    test('detectCurrentPrNumber가 throw하면 allowed: true', async () => {
      const guard = createPrGuardService(mockGitHub({
        detectCurrentPrNumber: async () => { throw new Error('network error'); },
      }));
      const result = await guard.check({});

      expect(result.allowed).toBe(true);
      expect(result.reason).toContain('could not check');
    });
  });

  describe('toolCommand 필터링', () => {
    test('gh pr create 패턴이 아니면 allowed: true (패스)', async () => {
      const guard = createPrGuardService(mockGitHub({
        detectCurrentPrNumber: async () => 123,
      }));
      const result = await guard.check({ toolCommand: 'gh pr view' });

      expect(result.allowed).toBe(true);
      expect(result.reason).toBe('not a gh pr create command');
    });

    test('gh pr create 패턴이면 guard 수행', async () => {
      const guard = createPrGuardService(mockGitHub({
        detectCurrentPrNumber: async () => 123,
      }));
      const result = await guard.check({ toolCommand: 'gh pr create --title "test"' });

      expect(result.allowed).toBe(false);
      expect(result.prNumber).toBe(123);
    });

    test('toolCommand가 undefined이면 guard 수행', async () => {
      const guard = createPrGuardService(mockGitHub({
        detectCurrentPrNumber: async () => 123,
      }));
      const result = await guard.check({});

      expect(result.allowed).toBe(false);
    });

    test('toolCommand가 빈 문자열이면 allowed: true (패스)', async () => {
      const guard = createPrGuardService(mockGitHub({
        detectCurrentPrNumber: async () => 123,
      }));
      const result = await guard.check({ toolCommand: '' });

      expect(result.allowed).toBe(true);
      expect(result.reason).toBe('not a gh pr create command');
    });
  });
});
