import { describe, test } from 'bun:test';

// ============================================================
// commit command — Black-box Test Spec
// ============================================================
// GitService, JiraService를 mock 주입하여 테스트합니다.
//
// 입력: CommitInput { type, description, scope?, body?, skipAdd? }
// 출력: Result<CommitOutput> { subject, jiraTicket? }
// ============================================================

describe('commit command', () => {
  describe('커밋 메시지 포맷팅', () => {
    describe('Jira 브랜치', () => {
      test.todo('Jira 티켓 감지 시 → "[WAD-0212] feat: description" 형식');
      test.todo('Jira 브랜치에서 scope 무시 → 티켓이 scope 대체');
      test.todo('output.jiraTicket에 감지된 티켓 포함');
    });

    describe('일반 브랜치', () => {
      test.todo('scope 있으면 → "feat(auth): description" 형식');
      test.todo('scope 없으면 → "feat: description" 형식');
      test.todo('output.jiraTicket은 undefined');
    });
  });

  describe('commit type 검증', () => {
    test.todo('유효한 type (feat, fix, docs, style, refactor, test, chore, perf) → 성공');
    test.todo('유효하지 않은 type → ok: false, error 메시지');
  });

  describe('git 조작', () => {
    test.todo('skipAdd=false → git.addTracked() 호출');
    test.todo('skipAdd=true → git.addTracked() 미호출');
    test.todo('git.commit()에 전달되는 메시지에 Co-Authored-By 포함');
    test.todo('git.commit()에 body가 있으면 subject + 빈줄 + body 포함');
  });

  describe('에러 처리', () => {
    test.todo('description 빈 문자열 → ok: false');
    test.todo('git.commit() 실패 → ok: false, 에러 전파');
    test.todo('git.addTracked() 실패 → ok: false, 에러 전파');
  });
});
