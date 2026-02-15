// ============================================================
// branch command (← create-branch.sh)
// ============================================================
// CLI: bun run src/cli.ts branch <branch-name> [--base=<branch>]
//
// 동작:
//   1. uncommitted 변경 체크
//   2. base branch 감지 (미지정 시 default branch)
//   3. git fetch → checkout base → pull → checkout -b
//
// 기존 create-branch.sh 대비 개선:
//   - GitService로 git 조작 추상화
//   - base branch를 positional arg → --base flag로 전환
// ============================================================

import type { Command, BranchInput, BranchOutput } from '../types';
import type { GitService } from '../core';

export interface BranchDeps {
  git: GitService;
}

export type BranchCommand = Command<BranchInput, BranchOutput>;
