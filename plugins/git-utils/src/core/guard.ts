// ============================================================
// GuardService — Default Branch Guard 구현
// ============================================================
// 두 hook 스크립트(~30줄 중복)를 단일 서비스로 통합합니다.
// ============================================================

import type { GuardInput, GuardOutput } from '../types';
import type { GitService } from './git';

export interface GuardService {
  check(input: GuardInput): Promise<GuardOutput>;
}

const GIT_COMMIT_PATTERN = /\bgit\b.*\bcommit\b/;

export function createGuardService(git: GitService): GuardService {
  return {
    async check(input: GuardInput): Promise<GuardOutput> {
      const pass = (reason?: string): GuardOutput => ({
        allowed: true,
        reason,
      });

      // commit guard: git commit 패턴이 아니면 패스
      if (input.target === 'commit') {
        if (!input.toolCommand || !GIT_COMMIT_PATTERN.test(input.toolCommand)) {
          return pass('not a git commit command');
        }
      }

      // Guard 1: git repo 확인
      if (!(await git.isInsideWorkTree())) {
        return pass('not a git repository');
      }

      // Default branch 결정
      let defaultBranch = input.defaultBranch;
      if (!defaultBranch) {
        try {
          defaultBranch = await git.detectDefaultBranch();
        } catch {
          return pass('could not detect default branch');
        }
      }

      // Guard 2: 특수 상태 (rebase/merge) → 패스
      const state = await git.getSpecialState();
      if (state.rebase || state.merge) {
        return pass('special git state (rebase/merge)');
      }

      // Guard 3: detached HEAD → 패스
      if (state.detached) {
        return pass('detached HEAD');
      }

      // 현재 브랜치 확인
      const currentBranch = await git.getCurrentBranch();

      // 기본 브랜치가 아니면 패스
      if (currentBranch !== defaultBranch) {
        return { allowed: true, currentBranch, defaultBranch };
      }

      // 기본 브랜치에서 작업 → 차단
      const action = input.target === 'commit' ? '커밋할 수 없습니다' : '파일을 수정하려 합니다';
      return {
        allowed: false,
        currentBranch,
        defaultBranch,
        reason: [
          `[Branch Guard] 기본 브랜치(${defaultBranch})에서 ${action}.`,
          `먼저 새 브랜치를 생성해주세요:`,
          `  ${input.createBranchScript} <branch-name>`,
        ].join('\n'),
      };
    },
  };
}
