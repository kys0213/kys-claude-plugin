import { describe, test } from 'bun:test';

// ============================================================
// Installer — Black-box Test Spec
// ============================================================
// InstallerDeps를 mock 주입하여 테스트합니다.
//
// 판정 로직:
//   설치 안 됨         → 빌드 + 설치 (action: "installed")
//   구버전 설치됨       → 빌드 + 덮어쓰기 (action: "updated")
//   동일/신버전 설치됨  → 스킵 (action: "skipped")
// ============================================================

describe('Installer', () => {
  describe('신규 설치', () => {
    test.todo('git-utils 미설치 (null) → build + install, action: "installed"');
    test.todo('설치 후 binaryPath가 ~/.local/bin/git-utils');
  });

  describe('업데이트', () => {
    test.todo('설치된 버전 < 플러그인 버전 → build + install, action: "updated"');
    test.todo('previousVersion에 기존 버전 포함');
  });

  describe('스킵', () => {
    test.todo('설치된 버전 = 플러그인 버전 → action: "skipped"');
    test.todo('설치된 버전 > 플러그인 버전 → action: "skipped"');
    test.todo('스킵 시 build/install 호출하지 않음');
  });

  describe('PATH 설정', () => {
    test.todo('~/.local/bin이 PATH에 있으면 → pathConfigured: true');
    test.todo('~/.local/bin이 PATH에 없으면 → addToPath 호출');
    test.todo('bash 사용자 → ~/.bashrc에 PATH 추가');
    test.todo('zsh 사용자 → ~/.zshrc에 PATH 추가');
  });

  describe('버전 비교 로직', () => {
    test.todo('3.0.0 vs 3.0.0 → 같음 (스킵)');
    test.todo('2.9.0 vs 3.0.0 → 낮음 (업데이트)');
    test.todo('3.0.0-alpha.0 vs 3.0.0 → 낮음 (업데이트)');
    test.todo('3.1.0 vs 3.0.0 → 높음 (스킵)');
  });

  describe('에러 처리', () => {
    test.todo('빌드 실패 → ok: false, 에러 전파');
    test.todo('파일 복사 실패 (권한) → ok: false, 에러 전파');
    test.todo('플러그인 버전 읽기 실패 → ok: false');
  });
});
