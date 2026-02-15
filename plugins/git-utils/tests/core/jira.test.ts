import { describe, test } from 'bun:test';

// ============================================================
// JiraService.detectTicket — Black-box Test Spec
// ============================================================
// 순수 함수: 브랜치 이름 문자열 → JiraTicket | null
// git 호출 없음, 외부 의존 없음 → mock 불필요
// ============================================================

describe('JiraService.detectTicket', () => {
  describe('Jira 티켓 감지 성공', () => {
    test.todo('대문자 직접 티켓: WAD-0212 → { raw: "WAD-0212", normalized: "WAD-0212" }');
    test.todo('prefix/대문자: feat/WAD-0212 → WAD-0212');
    test.todo('prefix/소문자: feat/wad-0212 → WAD-0212 (대문자 정규화)');
    test.todo('fix prefix: fix/wad-2223 → WAD-2223');
    test.todo('숫자가 긴 티켓: PROJ-12345 → PROJ-12345');
    test.todo('prefix/ticket/description: feat/WAD-0212/add-login → WAD-0212');
    test.todo('하이픈 prefix: feat-WAD-0212 → WAD-0212');
  });

  describe('Jira 티켓 미감지 (null 반환)', () => {
    test.todo('일반 feature 브랜치: feature/user-auth → null');
    test.todo('main 브랜치: main → null');
    test.todo('숫자만 있는 브랜치: 12345 → null');
    test.todo('빈 문자열 → null');
  });

  describe('엣지 케이스', () => {
    test.todo('프로젝트 키가 1글자: A-123 → 매칭 여부 결정');
    test.todo('숫자 뒤 추가 문자: feat/WAD-0212abc → WAD-0212 만 추출');
    test.todo('여러 티켓 패턴 존재: WAD-001-FIX-002 → 첫 번째 매칭');
  });
});
