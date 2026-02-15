import { describe, test } from 'bun:test';

// ============================================================
// GitService — Black-box Test Spec
// ============================================================
// 실제 git 명령어를 실행하므로, 격리된 temp git repo에서 테스트합니다.
//
// 테스트 전략:
//   - beforeEach: temp 디렉토리에 git init + initial commit 생성
//   - afterEach: temp 디렉토리 삭제
//   - 실제 git 바이너리를 사용하는 통합 테스트
// ============================================================

describe('GitService', () => {
  describe('detectDefaultBranch', () => {
    test.todo('origin/HEAD가 설정된 repo → 해당 브랜치 반환');
    test.todo('origin/HEAD 미설정, origin/main 존재 → "main" 반환');
    test.todo('origin/HEAD 미설정, origin/master만 존재 → "master" 반환');
    test.todo('origin/HEAD 미설정, origin/develop만 존재 → "develop" 반환');
    test.todo('remote 없는 repo → 에러 반환');
  });

  describe('getCurrentBranch', () => {
    test.todo('일반 브랜치에서 → 브랜치 이름 반환');
    test.todo('detached HEAD → 빈 문자열 반환');
  });

  describe('branchExists', () => {
    test.todo('로컬에 존재하는 브랜치 → true (local)');
    test.todo('로컬에 없는 브랜치 → false (local)');
    test.todo('리모트에 존재하는 브랜치 → true (remote)');
    test.todo('어디에도 없는 브랜치 → false (any)');
  });

  describe('isInsideWorkTree', () => {
    test.todo('git repo 내부 → true');
    test.todo('git repo 외부 → false');
  });

  describe('hasUncommittedChanges', () => {
    test.todo('변경 없음 → false');
    test.todo('unstaged 변경 있음 → true');
    test.todo('staged 변경 있음 → true');
    test.todo('untracked 파일만 있음 → true');
  });

  describe('getSpecialState', () => {
    test.todo('정상 상태 → { rebase: false, merge: false, detached: false }');
    // rebase/merge 상태는 실제로 만들기 복잡하므로 통합 테스트에서 다룰 수 있음
    test.todo('detached HEAD → { detached: true }');
  });

  describe('addTracked', () => {
    test.todo('tracked 파일의 변경사항만 staging');
    test.todo('untracked 파일은 staging하지 않음');
  });
});
