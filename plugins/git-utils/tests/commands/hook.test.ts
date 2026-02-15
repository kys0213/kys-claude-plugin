import { describe, test } from 'bun:test';

// ============================================================
// hook command — Black-box Test Spec
// ============================================================
// 파일시스템(fs)을 mock 주입하여 테스트합니다.
// settings.json 읽기/쓰기를 추상화된 fs 인터페이스로 제어합니다.
//
// 서브커맨드: register, unregister, list
// ============================================================

describe('hook command', () => {
  describe('register', () => {
    test.todo('settings.json 없으면 → 새로 생성하고 hook 등록');
    test.todo('settings.json 있고 hooks 비어있으면 → hook 추가');
    test.todo('동일 command가 이미 있으면 → 기존 hook 업데이트 (action: "updated")');
    test.todo('다른 command가 있으면 → 새 hook 추가 (action: "created")');
    test.todo('timeout 지정 → hookEntry에 timeout 포함');
    test.todo('timeout 미지정 → hookEntry에 timeout 없음');
    test.todo('.claude 디렉토리 없으면 → 자동 생성');
    test.todo('projectDir 지정 → 해당 경로의 settings.json 사용');
    test.todo('projectDir 미지정 → cwd 기준');
  });

  describe('unregister', () => {
    test.todo('존재하는 hook 삭제 → ok: true');
    test.todo('존재하지 않는 hook → ok: false, "not found" 메시지');
    test.todo('삭제 후 hookType 배열 비면 → hookType 키 자체 삭제');
    test.todo('삭제 후 hooks 객체 비면 → hooks 키 자체 삭제');
    test.todo('settings.json 없으면 → ok: false');
  });

  describe('list', () => {
    test.todo('hookType 지정 → 해당 타입의 hook 배열만 반환');
    test.todo('hookType 미지정 → 전체 hooks 객체 반환');
    test.todo('등록된 hook 없으면 → 빈 결과');
  });

  describe('settings.json 무결성', () => {
    test.todo('기존 settings.json의 다른 필드 보존');
    test.todo('JSON 포맷: 2-space indent + trailing newline');
    test.todo('깨진 JSON → 에러 반환 (덮어쓰지 않음)');
  });
});
