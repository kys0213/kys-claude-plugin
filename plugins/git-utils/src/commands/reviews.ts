// ============================================================
// reviews command (← unresolved-reviews.sh)
// ============================================================
// CLI: bun run src/cli.ts reviews [pr-number]
//
// 동작:
//   1. PR 번호 결정 (인자 or 현재 브랜치에서 자동 감지)
//   2. gh api graphql로 리뷰 쓰레드 조회
//   3. JSON 출력
//
// 기존 unresolved-reviews.sh 대비 개선:
//   - GraphQL 쿼리를 TypeScript 내 관리
//   - GitHubService 주입으로 테스트 가능
// ============================================================

import type { Command, ReviewsInput, ReviewsOutput } from '../types';
import type { GitHubService } from '../core';

export interface ReviewsDeps {
  github: GitHubService;
}

export type ReviewsCommand = Command<ReviewsInput, ReviewsOutput>;
