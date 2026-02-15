import { describe, test } from 'bun:test';

// ============================================================
// GuardService.check — Black-box Test Spec
// ============================================================
// GitService를 mock으로 주입하여 guard 판정 로직만 테스트합니다.
//
// 기존 중복 제거의 핵심 — 두 hook 스크립트가 공유하는 로직:
//   1. git repo 확인
//   2. default branch 감지
//   3. 특수 상태(rebase/merge) 패스
//   4. detached HEAD 패스
//   5. 현재 브랜치 vs 기본 브랜치 비교
//   6. (commit만) git commit 패턴 매칭
// ============================================================

describe('GuardService.check', () => {
  describe('공통 guard 로직 (write & commit)', () => {
    test.todo('git repo가 아니면 → allowed: true (패스)');
    test.todo('rebase 진행 중이면 → allowed: true (패스)');
    test.todo('merge 진행 중이면 → allowed: true (패스)');
    test.todo('detached HEAD이면 → allowed: true (패스)');
    test.todo('기본 브랜치가 아닌 브랜치에서 → allowed: true (패스)');
    test.todo('기본 브랜치(main)에서 → allowed: false (차단)');
    test.todo('기본 브랜치(master)에서 → allowed: false (차단)');
    test.todo('기본 브랜치(develop)에서 → allowed: false (차단)');
  });

  describe('default branch 감지 fallback', () => {
    test.todo('input.defaultBranch가 지정되면 해당 값 사용');
    test.todo('input.defaultBranch 미지정 → GitService.detectDefaultBranch() 호출');
    test.todo('감지 실패 시 → allowed: true (패스, 안전 모드)');
  });

  describe('target: write', () => {
    test.todo('toolCommand 없이도 guard 판정 수행');
    test.todo('기본 브랜치에서 차단 시 reason에 브랜치 생성 안내 포함');
  });

  describe('target: commit', () => {
    test.todo('toolCommand에 "git commit"이 없으면 → allowed: true (패스)');
    test.todo('toolCommand에 "git commit -m msg"이 있고 기본 브랜치면 → allowed: false');
    test.todo('toolCommand에 "git add && git commit"이 있고 기본 브랜치면 → allowed: false');
    test.todo('toolCommand에 "git log"만 있으면 → allowed: true (commit 아님)');
    test.todo('toolCommand가 빈 문자열이면 → allowed: true (패스)');
    test.todo('toolCommand가 undefined이면 → allowed: true (패스)');
  });

  describe('차단 메시지 포맷', () => {
    test.todo('차단 시 reason에 현재 브랜치 이름 포함');
    test.todo('차단 시 reason에 createBranchScript 경로 포함');
  });
});
