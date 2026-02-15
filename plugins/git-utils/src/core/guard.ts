// ============================================================
// GuardService — Default Branch Guard 인터페이스
// ============================================================
// default-branch-guard-hook.sh + default-branch-guard-commit-hook.sh
// 두 스크립트에서 중복된 ~30줄의 guard 로직을 단일 서비스로 통합합니다.
//
// 통합 대상:
//   1. git repo 확인
//   2. default branch 런타임 감지 (fallback)
//   3. 특수 상태(rebase/merge) 패스
//   4. detached HEAD 패스
//   5. 현재 브랜치 vs 기본 브랜치 비교
//   6. commit guard 전용: tool_input.command에서 git commit 패턴 매칭
// ============================================================

import type { GuardInput, GuardOutput } from '../types';

export interface GuardService {
  /**
   * 기본 브랜치 보호 체크를 수행합니다.
   *
   * @param input.target - 'write' (Write/Edit 도구) 또는 'commit' (Bash git commit)
   * @param input.projectDir - 프로젝트 디렉토리
   * @param input.createBranchScript - 브랜치 생성 스크립트 경로 (에러 메시지용)
   * @param input.defaultBranch - 고정된 기본 브랜치 (비어있으면 런타임 감지)
   * @param input.toolCommand - commit guard 전용: 실행하려는 bash 명령어
   */
  check(input: GuardInput): Promise<GuardOutput>;
}
