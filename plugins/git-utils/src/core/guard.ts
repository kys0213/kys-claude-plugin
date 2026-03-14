// ============================================================
// GuardService — Default Branch Guard 구현
// ============================================================
// 두 hook 스크립트(~30줄 중복)를 단일 서비스로 통합합니다.
// ============================================================

import { resolve, dirname } from 'node:path';
import { existsSync } from 'node:fs';
import type { GuardInput, GuardOutput } from '../types';
import type { GitService } from './git';

export interface GuardService {
  check(input: GuardInput): Promise<GuardOutput>;
}

const GIT_COMMIT_PATTERN = /\bgit\b.*\bcommit\b/;

/**
 * 파일 경로가 프로젝트 디렉토리 내부에 있는지 확인합니다.
 * 양쪽 경로를 resolve한 뒤 접두사 비교를 수행합니다.
 */
export function isInsideProjectDir(filePath: string, projectDir: string): boolean {
  const resolvedFile = resolve(filePath);
  const resolvedProject = resolve(projectDir);
  // 디렉토리 구분자를 붙여 /tmp/test-extra 가 /tmp/test 내부로 판정되지 않도록 합니다.
  return resolvedFile === resolvedProject || resolvedFile.startsWith(resolvedProject + '/');
}

/**
 * 파일 경로가 어떤 git 저장소 안에 있는지 확인합니다.
 * 부모 디렉토리를 순회하며 .git 디렉토리 존재 여부를 검사합니다.
 */
export function isInsideAnyGitRepo(filePath: string): boolean {
  let dir = resolve(dirname(filePath));
  const root = resolve('/');
  while (dir !== root) {
    if (existsSync(`${dir}/.git`)) {
      return true;
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return false;
}

export function createGuardService(git: GitService): GuardService {
  return {
    async check(input: GuardInput): Promise<GuardOutput> {
      const pass = (reason?: string): GuardOutput => ({
        allowed: true,
        reason,
      });

      // write guard: 프로젝트 외부 파일 판정
      if (input.target === 'write' && input.toolFilePath && !isInsideProjectDir(input.toolFilePath, input.projectDir)) {
        // 다른 git repo 안에 있으면 차단 유지 (기존 guard 로직 계속 진행)
        // git repo 밖이면 허용 (설정 파일 등)
        if (!isInsideAnyGitRepo(input.toolFilePath)) {
          return pass('file is outside any git repository');
        }
      }

      // commit guard: git commit 패턴이 아니면 패스
      if (input.target === 'commit' && (!input.toolCommand || !GIT_COMMIT_PATTERN.test(input.toolCommand))) {
        return pass('not a git commit command');
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
