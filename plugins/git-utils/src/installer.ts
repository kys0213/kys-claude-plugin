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

// -- Version comparison --

interface SemVer {
  major: number;
  minor: number;
  patch: number;
  prerelease: string | null;
}

export function parseSemVer(version: string): SemVer {
  const clean = version.replace(/^v/, '');
  const [core, prerelease] = clean.split('-', 2);
  const [major, minor, patch] = core.split('.').map(Number);
  return { major: major ?? 0, minor: minor ?? 0, patch: patch ?? 0, prerelease: prerelease ?? null };
}

/** Returns -1 if a < b, 0 if equal, 1 if a > b */
export function compareSemVer(a: string, b: string): -1 | 0 | 1 {
  const va = parseSemVer(a);
  const vb = parseSemVer(b);

  for (const key of ['major', 'minor', 'patch'] as const) {
    if (va[key] < vb[key]) return -1;
    if (va[key] > vb[key]) return 1;
  }

  // prerelease < release (e.g. 3.0.0-alpha.0 < 3.0.0)
  if (va.prerelease && !vb.prerelease) return -1;
  if (!va.prerelease && vb.prerelease) return 1;

  // Both have prerelease: lexicographic compare
  if (va.prerelease && vb.prerelease) {
    if (va.prerelease < vb.prerelease) return -1;
    if (va.prerelease > vb.prerelease) return 1;
  }

  return 0;
}

// -- Installer factory --

export function createInstaller(deps: InstallerDeps): Installer {
  return {
    async run(): Promise<Result<InstallResult>> {
      let pluginVersion: string;
      try {
        pluginVersion = await deps.getPluginVersion();
      } catch (e) {
        return { ok: false, error: `Failed to read plugin version: ${(e as Error).message}` };
      }

      const installedVersion = await deps.getInstalledVersion();

      // Decide action
      let action: InstallAction;
      if (!installedVersion) {
        action = 'installed';
      } else {
        const cmp = compareSemVer(installedVersion, pluginVersion);
        if (cmp < 0) {
          action = 'updated';
        } else {
          action = 'skipped';
        }
      }

      if (action !== 'skipped') {
        const tmpOut = `${INSTALL_DIR}/${BINARY_NAME}.tmp`;
        try {
          await deps.buildBinary(tmpOut);
        } catch (e) {
          return { ok: false, error: `Build failed: ${(e as Error).message}` };
        }

        try {
          await deps.installBinary(tmpOut, BINARY_PATH);
        } catch (e) {
          return { ok: false, error: `Install failed: ${(e as Error).message}` };
        }
      }

      // PATH configuration
      let pathConfigured = deps.isInPath(INSTALL_DIR);
      if (!pathConfigured) {
        try {
          await deps.addToPath(INSTALL_DIR);
          pathConfigured = true;
        } catch {
          // PATH addition failed but install succeeded — non-fatal
          pathConfigured = false;
        }
      }

      return {
        ok: true,
        data: {
          action,
          version: pluginVersion,
          previousVersion: installedVersion ?? undefined,
          binaryPath: BINARY_PATH,
          pathConfigured,
        },
      };
    },
  };
}
