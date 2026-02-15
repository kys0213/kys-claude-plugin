import { describe, test, expect } from 'bun:test';
import { createGuardService } from '../../src/core/guard';
import type { GitService } from '../../src/core/git';
import type { GuardInput } from '../../src/types';

// ============================================================
// GuardService.check — Mock 기반 Unit Test
// ============================================================

function mockGit(overrides: Partial<GitService> = {}): GitService {
  return {
    isInsideWorkTree: async () => true,
    getCurrentBranch: async () => 'main',
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

const baseInput: GuardInput = {
  target: 'write',
  projectDir: '/tmp/test',
  createBranchScript: './create-branch.sh',
};

describe('GuardService.check', () => {
  describe('공통 guard 로직 (write & commit)', () => {
    test('git repo가 아니면 → allowed: true (패스)', async () => {
      const guard = createGuardService(mockGit({ isInsideWorkTree: async () => false }));
      const result = await guard.check(baseInput);
      expect(result.allowed).toBe(true);
    });

    test('rebase 진행 중이면 → allowed: true (패스)', async () => {
      const guard = createGuardService(mockGit({
        getSpecialState: async () => ({ rebase: true, merge: false, detached: false }),
      }));
      const result = await guard.check(baseInput);
      expect(result.allowed).toBe(true);
    });

    test('merge 진행 중이면 → allowed: true (패스)', async () => {
      const guard = createGuardService(mockGit({
        getSpecialState: async () => ({ rebase: false, merge: true, detached: false }),
      }));
      const result = await guard.check(baseInput);
      expect(result.allowed).toBe(true);
    });

    test('detached HEAD이면 → allowed: true (패스)', async () => {
      const guard = createGuardService(mockGit({
        getSpecialState: async () => ({ rebase: false, merge: false, detached: true }),
      }));
      const result = await guard.check(baseInput);
      expect(result.allowed).toBe(true);
    });

    test('기본 브랜치가 아닌 브랜치에서 → allowed: true (패스)', async () => {
      const guard = createGuardService(mockGit({
        getCurrentBranch: async () => 'feat/something',
      }));
      const result = await guard.check(baseInput);
      expect(result.allowed).toBe(true);
    });

    test('기본 브랜치(main)에서 → allowed: false (차단)', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check(baseInput);
      expect(result.allowed).toBe(false);
    });

    test('기본 브랜치(master)에서 → allowed: false (차단)', async () => {
      const guard = createGuardService(mockGit({
        getCurrentBranch: async () => 'master',
        detectDefaultBranch: async () => 'master',
      }));
      const result = await guard.check(baseInput);
      expect(result.allowed).toBe(false);
    });

    test('기본 브랜치(develop)에서 → allowed: false (차단)', async () => {
      const guard = createGuardService(mockGit({
        getCurrentBranch: async () => 'develop',
        detectDefaultBranch: async () => 'develop',
      }));
      const result = await guard.check(baseInput);
      expect(result.allowed).toBe(false);
    });
  });

  describe('default branch 감지 fallback', () => {
    test('input.defaultBranch가 지정되면 해당 값 사용', async () => {
      const guard = createGuardService(mockGit({
        getCurrentBranch: async () => 'custom-default',
      }));
      const result = await guard.check({ ...baseInput, defaultBranch: 'custom-default' });
      expect(result.allowed).toBe(false);
      expect(result.defaultBranch).toBe('custom-default');
    });

    test('input.defaultBranch 미지정 → GitService.detectDefaultBranch() 호출', async () => {
      let called = false;
      const guard = createGuardService(mockGit({
        detectDefaultBranch: async () => { called = true; return 'main'; },
      }));
      await guard.check(baseInput);
      expect(called).toBe(true);
    });

    test('감지 실패 시 → allowed: true (패스, 안전 모드)', async () => {
      const guard = createGuardService(mockGit({
        detectDefaultBranch: async () => { throw new Error('no remote'); },
      }));
      const result = await guard.check(baseInput);
      expect(result.allowed).toBe(true);
    });
  });

  describe('target: write', () => {
    test('toolCommand 없이도 guard 판정 수행', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check(baseInput);
      expect(result.allowed).toBe(false);
    });

    test('기본 브랜치에서 차단 시 reason에 브랜치 생성 안내 포함', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check(baseInput);
      expect(result.reason).toContain('파일을 수정하려 합니다');
      expect(result.reason).toContain(baseInput.createBranchScript);
    });
  });

  describe('target: commit', () => {
    const commitInput: GuardInput = { ...baseInput, target: 'commit' };

    test('toolCommand에 "git commit"이 없으면 → allowed: true (패스)', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check({ ...commitInput, toolCommand: 'git push origin main' });
      expect(result.allowed).toBe(true);
    });

    test('toolCommand에 "git commit -m msg"이 있고 기본 브랜치면 → allowed: false', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check({ ...commitInput, toolCommand: 'git commit -m "test"' });
      expect(result.allowed).toBe(false);
    });

    test('toolCommand에 "git add && git commit"이 있고 기본 브랜치면 → allowed: false', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check({ ...commitInput, toolCommand: 'git add . && git commit -m "test"' });
      expect(result.allowed).toBe(false);
    });

    test('toolCommand에 "git log"만 있으면 → allowed: true (commit 아님)', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check({ ...commitInput, toolCommand: 'git log --oneline' });
      expect(result.allowed).toBe(true);
    });

    test('toolCommand가 빈 문자열이면 → allowed: true (패스)', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check({ ...commitInput, toolCommand: '' });
      expect(result.allowed).toBe(true);
    });

    test('toolCommand가 undefined이면 → allowed: true (패스)', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check(commitInput);
      expect(result.allowed).toBe(true);
    });
  });

  describe('차단 메시지 포맷', () => {
    test('차단 시 reason에 현재 브랜치 이름 포함', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check(baseInput);
      expect(result.reason).toContain('main');
      expect(result.currentBranch).toBe('main');
    });

    test('차단 시 reason에 createBranchScript 경로 포함', async () => {
      const guard = createGuardService(mockGit());
      const result = await guard.check(baseInput);
      expect(result.reason).toContain('./create-branch.sh');
    });
  });
});
