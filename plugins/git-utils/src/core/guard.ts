// ============================================================
// GuardService — Default Branch Guard 구현
// ============================================================
// 두 hook 스크립트(~30줄 중복)를 단일 서비스로 통합합니다.
// ============================================================

import { resolve, dirname, relative, isAbsolute } from 'node:path';
import { existsSync } from 'node:fs';
import type { GuardInput, GuardOutput } from '../types';
import type { GitService } from './git';

export interface GuardService {
  check(input: GuardInput): Promise<GuardOutput>;
}

/** `commit` 서브커맨드 다음에 오기 전 값을 받는(=다음 토큰을 소비하는) git 글로벌 옵션 */
const VALUE_TAKING_GLOBAL_OPTS = new Set([
  '-C', '-c', '--git-dir', '--work-tree', '--namespace', '--super-prefix', '--exec-path',
]);

/**
 * 명령어가 실제로 `git commit`을 **실행**하는지 토큰 기반으로 판정합니다.
 *
 * 기존 `/\bgit\b.*\bcommit\b/` 정규식은 substring 매칭이라
 * `gh issue create --body "...git commit..."` 처럼 본문/변수 안에 'git'과 'commit'이
 * 들어간 명령까지 false positive로 차단했습니다 (#754).
 *
 * 이 함수는 셸 구분자(`&&`, `||`, `;`, `|`, 개행)로 세그먼트를 나눈 뒤,
 * 각 세그먼트의 **실제 명령 토큰**이 `git`이고 그 서브커맨드가 `commit`일 때만 true를 반환합니다.
 * (가드 목적상 따옴표 내부까지 엄밀히 파싱하지는 않습니다.)
 */
export function isGitCommitCommand(command: string): boolean {
  const segments = command.split(/&&|\|\||[;\n|]/);
  for (const seg of segments) {
    const tokens = seg.trim().split(/\s+/).filter(Boolean);

    // 선행 환경변수 할당 제거 (예: `GIT_AUTHOR_NAME=x git commit`)
    let i = 0;
    while (i < tokens.length && /^[A-Za-z_][A-Za-z0-9_]*=/.test(tokens[i])) i++;
    if (i >= tokens.length) continue;

    // 명령 토큰이 git(또는 /usr/bin/git 같은 경로 형태)인지 확인
    const cmd = tokens[i];
    if (cmd !== 'git' && !cmd.endsWith('/git')) continue;

    // git 다음의 첫 비옵션 토큰 = 서브커맨드. 값을 받는 글로벌 옵션은 값 토큰까지 건너뜀.
    let j = i + 1;
    while (j < tokens.length && tokens[j].startsWith('-')) {
      j += VALUE_TAKING_GLOBAL_OPTS.has(tokens[j]) ? 2 : 1;
    }
    if (j < tokens.length && tokens[j] === 'commit') return true;
  }
  return false;
}

/**
 * 파일 경로가 프로젝트 디렉토리 내부에 있는지 확인합니다.
 * path.relative()를 사용하여 상대 경로 기반으로 판정합니다.
 */
export function isInsideProjectDir(filePath: string, projectDir: string): boolean {
  const rel = relative(resolve(projectDir), resolve(filePath));
  // relative path가 '..'로 시작하지 않고 절대 경로가 아니면 내부
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel));
}

/**
 * 파일 경로가 어떤 git 저장소 안에 있는지 확인합니다.
 * 존재하는 최상위 디렉토리부터 .git 디렉토리 존재 여부를 검사합니다.
 */
export function isInsideAnyGitRepo(filePath: string): boolean {
  // 존재하는 최상위 디렉토리부터 검색하여 불필요한 existsSync 호출 감소
  let dir = resolve(dirname(filePath));
  const root = resolve('/');
  while (dir !== root && !existsSync(dir)) {
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
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

      // commit guard: 실제 git commit 명령이 아니면 패스 (substring false positive 방지 — #754)
      if (input.target === 'commit' && (!input.toolCommand || !isGitCommitCommand(input.toolCommand))) {
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

      // 보호 브랜치 목록 구성: default branch + 추가 보호 브랜치 + 기본 보호 대상(develop)
      const protectedSet = new Set<string>([defaultBranch, 'develop']);
      if (input.protectedBranches) {
        for (const b of input.protectedBranches) {
          protectedSet.add(b);
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

      // 보호 브랜치가 아니면 패스
      if (!protectedSet.has(currentBranch)) {
        return { allowed: true, currentBranch, defaultBranch };
      }

      // 보호 브랜치에서 작업 → 차단
      const action = input.target === 'commit' ? '커밋할 수 없습니다' : '파일을 수정하려 합니다';
      return {
        allowed: false,
        currentBranch,
        defaultBranch,
        reason: [
          `[Branch Guard] 보호 브랜치(${currentBranch})에서 ${action}.`,
          // 진단 정보: 어느 디렉토리/브랜치를 기준으로 판정했는지 노출 (worktree 디버깅 용이성 — #754)
          `  평가 디렉토리: ${input.projectDir}`,
          `  감지된 브랜치: ${currentBranch} (기본 브랜치: ${defaultBranch})`,
          `먼저 새 브랜치를 생성해주세요:`,
          `  ${input.createBranchScript} <branch-name>`,
        ].join('\n'),
      };
    },
  };
}
