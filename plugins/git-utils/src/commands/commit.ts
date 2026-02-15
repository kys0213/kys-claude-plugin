// ============================================================
// commit command (← commit.sh)
// ============================================================
// CLI: bun run src/cli.ts commit <type> <description> [--scope=<s>] [--body=<b>] [--skip-add]
//
// 동작:
//   1. 현재 브랜치에서 Jira 티켓 감지
//   2. 티켓 있으면: [WAD-0212] feat: description
//      없으면:     feat(scope): description
//   3. --skip-add 아니면 git add -u
//   4. git commit 실행
//
// 기존 commit.sh 대비 개선:
//   - 인자 순서 의존 → named flag로 전환 (--scope, --body)
//   - JiraService 주입으로 테스트 용이
// ============================================================

import type { Command, CommitInput, CommitOutput } from '../types';
import type { GitService, JiraService } from '../core';

export interface CommitDeps {
  git: GitService;
  jira: JiraService;
}

export type CommitCommand = Command<CommitInput, CommitOutput>;
