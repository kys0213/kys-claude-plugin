// ============================================================
// GitHubService — GitHub CLI(gh) 래퍼 인터페이스
// ============================================================
// create-pr.sh, unresolved-reviews.sh 에서 사용하는
// gh CLI 호출을 타입 안전한 인터페이스로 추상화합니다.
// ============================================================

import type { ReviewThread } from '../types';

export interface GitHubService {
  /** gh auth status 확인 */
  isAuthenticated(): Promise<boolean>;

  /** PR 생성 — gh pr create */
  createPr(options: {
    base: string;
    title: string;
    body: string;
  }): Promise<string>; // returns PR URL

  /** PR 리뷰 쓰레드 조회 — gh api graphql */
  getReviewThreads(prNumber: number): Promise<{
    prTitle: string;
    prUrl: string;
    threads: ReviewThread[];
  }>;

  /** 현재 브랜치의 PR 번호 자동 감지 */
  detectCurrentPrNumber(): Promise<number | null>;
}
