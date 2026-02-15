import { describe, test } from 'bun:test';

// ============================================================
// reviews command — Black-box Test Spec
// ============================================================
// GitHubService를 mock 주입하여 테스트합니다.
//
// 입력: ReviewsInput { prNumber? }
// 출력: Result<ReviewsOutput> { prTitle, prUrl, threads[] }
// ============================================================

describe('reviews command', () => {
  describe('PR 번호 결정', () => {
    test.todo('prNumber 직접 지정 → 해당 번호 사용');
    test.todo('prNumber 미지정 → detectCurrentPrNumber()로 자동 감지');
    test.todo('자동 감지 실패 → ok: false, PR 번호 필요 안내');
  });

  describe('정상 동작', () => {
    test.todo('리뷰 쓰레드가 있으면 → threads 배열에 포함');
    test.todo('리뷰 쓰레드가 없으면 → 빈 threads 배열');
    test.todo('output에 prTitle, prUrl 포함');
  });

  describe('쓰레드 데이터 매핑', () => {
    test.todo('isResolved, isOutdated 필드 매핑');
    test.todo('path, line 필드 매핑');
    test.todo('comments 배열에 author, body, createdAt, url 포함');
  });

  describe('에러 처리', () => {
    test.todo('gh API 호출 실패 → ok: false, 에러 전파');
    test.todo('존재하지 않는 PR 번호 → ok: false');
  });
});
