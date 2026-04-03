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

import { resolve, join } from 'node:path';
import { chmod, readFile, appendFile, access, rename, stat } from 'node:fs/promises';
import { mkdirSync } from 'node:fs';
import { exec } from './core/shell';
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

  /** 소스 파일이 바이너리보다 최신인지 확인 */
  isSourceNewer(): Promise<boolean>;
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
        } else if (await deps.isSourceNewer()) {
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

// -- Real dependencies (system calls) --

const PLUGIN_ROOT = resolve(import.meta.dir, '..');

export function createRealDeps(): InstallerDeps {
  return {
    async getPluginVersion(): Promise<string> {
      const pkgPath = join(PLUGIN_ROOT, 'package.json');
      const content = await readFile(pkgPath, 'utf-8');
      const pkg = JSON.parse(content) as { version: string };
      return pkg.version;
    },

    async getInstalledVersion(): Promise<string | null> {
      try {
        await access(BINARY_PATH);
      } catch {
        return null;
      }
      const result = await exec([BINARY_PATH, '--version']);
      if (result.exitCode !== 0) return null;
      const match = result.stdout.match(/\d+\.\d+\.\d+[\w.-]*/);
      return match ? match[0] : null;
    },

    async buildBinary(outfile: string): Promise<void> {
      const result = await exec(
        ['bun', 'build', '--compile', 'src/cli.ts', '--outfile', outfile],
        { cwd: PLUGIN_ROOT },
      );
      if (result.exitCode !== 0) {
        throw new Error(result.stderr || result.stdout);
      }
    },

    async installBinary(src: string, dest: string): Promise<void> {
      await chmod(src, 0o755);
      await rename(src, dest);
    },

    isInPath(dir: string): boolean {
      const pathEnv = process.env.PATH ?? '';
      return pathEnv.split(':').includes(dir);
    },

    async isSourceNewer(): Promise<boolean> {
      try {
        const binaryMtime = (await stat(BINARY_PATH)).mtimeMs;
        const glob = new Bun.Glob('src/**/*.ts');
        for (const file of glob.scanSync({ cwd: PLUGIN_ROOT })) {
          const mtime = (await stat(join(PLUGIN_ROOT, file))).mtimeMs;
          if (mtime > binaryMtime) return true;
        }
        return false;
      } catch {
        return true;
      }
    },

    async addToPath(dir: string): Promise<{ shell: string; rcFile: string }> {
      const home = process.env.HOME!;
      const shell = process.env.SHELL?.includes('zsh') ? 'zsh' : 'bash';
      const rcFile = shell === 'zsh' ? join(home, '.zshrc') : join(home, '.bashrc');
      // Avoid duplicate entries on re-runs
      const existing = await readFile(rcFile, 'utf-8').catch(() => '');
      if (existing.includes(dir)) return { shell, rcFile };
      const exportLine = `\nexport PATH="${dir}:$PATH"  # added by git-utils installer\n`;
      await appendFile(rcFile, exportLine);
      return { shell, rcFile };
    },
  };
}

// -- Entrypoint --

async function main(): Promise<void> {
  mkdirSync(INSTALL_DIR, { recursive: true });

  const installer = createInstaller(createRealDeps());
  const result = await installer.run();

  if (!result.ok) {
    console.error(`Install failed: ${result.error}`);
    process.exit(1);
  }

  const { action, version, previousVersion, binaryPath, pathConfigured } = result.data;

  switch (action) {
    case 'installed':
      console.log(`git-utils v${version} installed → ${binaryPath}`);
      break;
    case 'updated':
      if (previousVersion === version) {
        console.log(`git-utils v${version} rebuilt (stale binary detected) → ${binaryPath}`);
      } else {
        console.log(`git-utils updated: v${previousVersion} → v${version} (${binaryPath})`);
      }
      break;
    case 'skipped':
      console.log(`git-utils v${version} is already up to date`);
      break;
  }

  if (!pathConfigured) {
    console.warn(`\nNote: ${INSTALL_DIR} is not in your PATH.`);
    console.warn(`Add this to your shell config:\n  export PATH="${INSTALL_DIR}:$PATH"`);
  }
}

if (import.meta.main) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
