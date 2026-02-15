// ============================================================
// git-utils Installer
// ============================================================
// /setup slash command에서 호출되어 git-utils CLI를 설치/업데이트합니다.
//
// Usage (slash command 내부에서):
//   bun run src/installer.ts
//
// 동작:
//   1. plugin.json에서 플러그인 버전 읽기
//   2. 기존 설치 확인: ~/.local/bin/git-utils --version
//   3. 버전 비교 → 신규 설치 / 업데이트 / 스킵
//   4. bun build --compile → standalone binary
//   5. ~/.local/bin/git-utils 에 설치
//   6. PATH 검증 및 안내
//
// 설치 경로:
//   ~/.local/bin/git-utils    (XDG Base Directory 표준)
// ============================================================

import type { Result } from './types';

// -- Constants --

export const INSTALL_DIR = `${process.env.HOME}/.local/bin`;
export const BINARY_NAME = 'git-utils';
export const BINARY_PATH = `${INSTALL_DIR}/${BINARY_NAME}`;

// -- Types --

export type InstallAction = 'installed' | 'updated' | 'skipped';

export interface InstallResult {
  action: InstallAction;
  version: string;
  previousVersion?: string;
  binaryPath: string;
  pathConfigured: boolean;
}

export interface InstallerDeps {
  /** plugin.json 에서 버전 읽기 */
  getPluginVersion(): Promise<string>;

  /** 기존 설치된 바이너리 버전 확인 (없으면 null) */
  getInstalledVersion(): Promise<string | null>;

  /** bun build --compile 실행 */
  buildBinary(outfile: string): Promise<void>;

  /** 파일 복사 + chmod +x */
  installBinary(src: string, dest: string): Promise<void>;

  /** PATH에 디렉토리가 포함되어 있는지 확인 */
  isInPath(dir: string): boolean;

  /** shell rc 파일에 PATH 추가 */
  addToPath(dir: string): Promise<{ shell: string; rcFile: string }>;
}

export interface Installer {
  run(): Promise<Result<InstallResult>>;
}
