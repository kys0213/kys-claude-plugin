// ============================================================
// guard command (← default-branch-guard-hook.sh, guard-commit-hook.sh)
// ============================================================
// CLI (hook으로 호출됨):
//   bun run src/cli.ts guard write --project-dir=<p> --create-branch-script=<s> [--default-branch=<b>]
//   bun run src/cli.ts guard commit --project-dir=<p> --create-branch-script=<s> [--default-branch=<b>]
//
// 동작:
//   1. stdin에서 Claude hook JSON 읽기
//   2. commit guard: tool_input.command에서 git commit 패턴 확인
//   3. 공통 guard 로직 실행 (GuardService.check)
//   4. 차단 시 exit 2, 통과 시 exit 0
//
// 기존 대비 핵심 개선:
//   - 두 스크립트의 ~30줄 중복 guard 로직 → GuardService 단일화
//   - default branch 감지 fallback → GitService.detectDefaultBranch() 재사용
// ============================================================

import type { Command, GuardInput, GuardOutput } from '../types';
import type { GuardService } from '../core';

export interface GuardCommandDeps {
  guard: GuardService;
}

export type GuardCommand = Command<GuardInput, GuardOutput>;
