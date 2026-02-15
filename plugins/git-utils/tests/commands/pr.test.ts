import { describe, test } from 'bun:test';

// ============================================================
// pr command — Black-box Test Spec
// ============================================================
// GitService, JiraService, GitHubService를 mock 주입하여 테스트합니다.
//
// 입력: PrInput { title, description? }
// 출력: Result<PrOutput> { url, title, baseBranch, jiraTicket? }
// ============================================================

describe('pr command', () => {
  describe('PR 타이틀 포맷팅', () => {
    test.todo('Jira 티켓 감지 시 → "[WAD-0212] title" 형식');
    test.todo('Jira 티켓 미감지 → title 그대로 사용');
  });

  describe('정상 동작', () => {
    test.todo('push → pr create 순서로 호출');
    test.todo('description 있으면 PR body에 포함');
    test.todo('description 없으면 빈 body로 생성');
    test.todo('output에 PR URL, 최종 title, baseBranch 반환');
  });

  describe('사전 조건 검증', () => {
    test.todo('현재 브랜치가 default 브랜치면 → ok: false');
    test.todo('GitHub CLI 미인증 → ok: false, 인증 안내 메시지');
  });

  describe('에러 처리', () => {
    test.todo('git push 실패 → ok: false, 에러 전파');
    test.todo('gh pr create 실패 → ok: false, 에러 전파');
    test.todo('title 빈 문자열 → ok: false');
  });
});
