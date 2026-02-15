import { describe, test, expect } from 'bun:test';
import { createBranchCommand } from '../../src/commands/branch';
import type { GitService } from '../../src/core/git';

// ============================================================
// branch command — Black-box Test
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

describe('branch command', () => {
  describe('정상 동작', () => {
    test('baseBranch 미지정 → detectDefaultBranch()로 감지하여 사용', async () => {
      let detectedDefault = false;
      const cmd = createBranchCommand({
        git: mockGit({
          detectDefaultBranch: async () => { detectedDefault = true; return 'main'; },
          branchExists: async (name, location) => {
            if (name === 'main' && location === 'any') return true;
            return false;
          },
        }),
      });
      const result = await cmd.run({ branchName: 'feat/new-feature' });
      expect(detectedDefault).toBe(true);
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.baseBranch).toBe('main');
      }
    });

    test('baseBranch 지정 → 해당 브랜치를 base로 사용', async () => {
      const cmd = createBranchCommand({
        git: mockGit({
          branchExists: async (name, location) => {
            if (name === 'develop' && location === 'any') return true;
            if (name === 'develop' && location === 'local') return true;
            return false;
          },
        }),
      });
      const result = await cmd.run({ branchName: 'feat/new-feature', baseBranch: 'develop' });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.baseBranch).toBe('develop');
      }
    });

    test('output에 생성된 branchName과 baseBranch 반환', async () => {
      const cmd = createBranchCommand({
        git: mockGit({
          branchExists: async (name, location) => {
            if (name === 'main' && location === 'any') return true;
            if (name === 'main' && location === 'local') return true;
            return false;
          },
        }),
      });
      const result = await cmd.run({ branchName: 'feat/login' });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.branchName).toBe('feat/login');
        expect(result.data.baseBranch).toBe('main');
      }
    });
  });

  describe('git 조작 순서', () => {
    test('fetch → checkout base → pull → checkout -b 순서로 호출', async () => {
      const calls: string[] = [];
      const cmd = createBranchCommand({
        git: mockGit({
          branchExists: async (name, location) => {
            if (name === 'main' && location === 'any') return true;
            if (name === 'main' && location === 'local') return true;
            if (name === 'feat/new' && location === 'local') return false;
            return false;
          },
          fetch: async () => { calls.push('fetch'); },
          checkout: async (branch, opts) => {
            if (opts?.create) {
              calls.push(`checkout-create:${branch}`);
            } else {
              calls.push(`checkout:${branch}`);
            }
          },
          pull: async (branch) => { calls.push(`pull:${branch}`); },
        }),
      });
      await cmd.run({ branchName: 'feat/new' });
      expect(calls).toEqual([
        'fetch',
        'checkout:main',
        'pull:main',
        'checkout-create:feat/new',
      ]);
    });

    test('base가 local에만 있으면 checkout → pull', async () => {
      const calls: string[] = [];
      const cmd = createBranchCommand({
        git: mockGit({
          branchExists: async (name, location) => {
            if (name === 'main' && location === 'any') return true;
            if (name === 'main' && location === 'local') return true;
            return false;
          },
          fetch: async () => { calls.push('fetch'); },
          checkout: async (branch, opts) => {
            if (opts?.create) {
              calls.push(`checkout-create:${branch}`);
            } else {
              calls.push(`checkout:${branch}`);
            }
          },
          pull: async (branch) => { calls.push(`pull:${branch}`); },
        }),
      });
      await cmd.run({ branchName: 'feat/new' });
      // local exists → checkout (no create) + pull
      expect(calls).toContain('checkout:main');
      expect(calls).toContain('pull:main');
    });

    test('base가 remote에만 있으면 checkout -b --track', async () => {
      const calls: { branch: string; opts?: any }[] = [];
      const cmd = createBranchCommand({
        git: mockGit({
          branchExists: async (name, location) => {
            if (name === 'main' && location === 'any') return true;
            if (name === 'main' && location === 'local') return false; // not local
            return false;
          },
          fetch: async () => {},
          checkout: async (branch, opts) => {
            calls.push({ branch, opts });
          },
          pull: async () => {},
        }),
      });
      await cmd.run({ branchName: 'feat/new' });
      // First checkout should be create with track for the base branch
      const baseCheckout = calls.find(c => c.branch === 'main');
      expect(baseCheckout).toBeDefined();
      expect(baseCheckout!.opts?.create).toBe(true);
      expect(baseCheckout!.opts?.track).toBe('origin/main');
    });
  });

  describe('사전 조건 검증', () => {
    test('uncommitted 변경 있으면 → ok: false, 변경사항 안내', async () => {
      const cmd = createBranchCommand({
        git: mockGit({ hasUncommittedChanges: async () => true }),
      });
      const result = await cmd.run({ branchName: 'feat/new' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('Uncommitted changes');
      }
    });

    test('base 브랜치가 local/remote 모두 없으면 → ok: false', async () => {
      const cmd = createBranchCommand({
        git: mockGit({
          branchExists: async () => false,
        }),
      });
      const result = await cmd.run({ branchName: 'feat/new' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('does not exist');
      }
    });

    test('이미 같은 이름의 브랜치 존재 → ok: false', async () => {
      const cmd = createBranchCommand({
        git: mockGit({
          branchExists: async (name, location) => {
            if (name === 'main' && location === 'any') return true;
            if (name === 'feat/existing' && location === 'local') return true;
            return false;
          },
        }),
      });
      const result = await cmd.run({ branchName: 'feat/existing' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('already exists');
      }
    });
  });

  describe('에러 처리', () => {
    test('git fetch 실패 → 무시하고 계속 진행 (기존 동작 유지)', async () => {
      const cmd = createBranchCommand({
        git: mockGit({
          fetch: async () => { throw new Error('network error'); },
          branchExists: async (name, location) => {
            if (name === 'main' && location === 'any') return true;
            if (name === 'main' && location === 'local') return true;
            return false;
          },
        }),
      });
      const result = await cmd.run({ branchName: 'feat/new' });
      // fetch failure should be ignored
      expect(result.ok).toBe(true);
    });

    test('git checkout -b 실패 → ok: false, 에러 전파', async () => {
      const cmd = createBranchCommand({
        git: mockGit({
          branchExists: async (name, location) => {
            if (name === 'main' && location === 'any') return true;
            if (name === 'main' && location === 'local') return true;
            return false;
          },
          checkout: async (_branch, opts) => {
            if (opts?.create) throw new Error('checkout failed');
          },
        }),
      });
      const result = await cmd.run({ branchName: 'feat/new' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toBe('checkout failed');
      }
    });

    test('branchName 빈 문자열 → ok: false', async () => {
      const cmd = createBranchCommand({ git: mockGit() });
      const result = await cmd.run({ branchName: '' });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('Branch name is required');
      }
    });
  });
});
