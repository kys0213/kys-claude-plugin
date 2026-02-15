// ============================================================
// pr command (← create-pr.sh)
// ============================================================
// CLI: bun run src/cli.ts pr <title> [--description=<d>]
//
// 동작:
//   1. default branch 감지
//   2. 현재 브랜치가 default가 아닌지 확인
//   3. gh auth 확인
//   4. Jira 티켓 감지 → PR 타이틀에 반영
//   5. git push → gh pr create
//
// 기존 create-pr.sh 대비 개선:
//   - push 에러 핸들링 개선 (stderr 무시 제거)
//   - GitHubService 추상화로 테스트 가능
// ============================================================

import type { Command, PrInput, PrOutput } from '../types';
import type { GitService, JiraService, GitHubService } from '../core';

export interface PrDeps {
  git: GitService;
  jira: JiraService;
  github: GitHubService;
}

export type PrCommand = Command<PrInput, PrOutput>;
