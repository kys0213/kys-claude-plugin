import { describe, test, expect } from 'bun:test';
import {
  createInstaller,
  compareSemVer,
  BINARY_PATH,
  type InstallerDeps,
} from '../src/installer';

// ============================================================
// Installer — Black-box Test
// ============================================================

function mockDeps(overrides: Partial<InstallerDeps> = {}): InstallerDeps {
  return {
    getPluginVersion: async () => '3.0.0',
    getInstalledVersion: async () => null,
    buildBinary: async () => {},
    installBinary: async () => {},
    isInPath: () => true,
    addToPath: async () => ({ shell: 'bash', rcFile: '~/.bashrc' }),
    ...overrides,
  };
}

describe('Installer', () => {
  describe('신규 설치', () => {
    test('git-utils 미설치 (null) → build + install, action: "installed"', async () => {
      let buildCalled = false;
      let installCalled = false;
      const installer = createInstaller(mockDeps({
        getInstalledVersion: async () => null,
        buildBinary: async () => { buildCalled = true; },
        installBinary: async () => { installCalled = true; },
      }));
      const result = await installer.run();
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.action).toBe('installed');
        expect(buildCalled).toBe(true);
        expect(installCalled).toBe(true);
      }
    });

    test('설치 후 binaryPath가 ~/.local/bin/git-utils', async () => {
      const installer = createInstaller(mockDeps());
      const result = await installer.run();
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.binaryPath).toBe(BINARY_PATH);
      }
    });
  });

  describe('업데이트', () => {
    test('설치된 버전 < 플러그인 버전 → build + install, action: "updated"', async () => {
      let buildCalled = false;
      let installCalled = false;
      const installer = createInstaller(mockDeps({
        getPluginVersion: async () => '3.0.0',
        getInstalledVersion: async () => '2.9.0',
        buildBinary: async () => { buildCalled = true; },
        installBinary: async () => { installCalled = true; },
      }));
      const result = await installer.run();
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.action).toBe('updated');
        expect(buildCalled).toBe(true);
        expect(installCalled).toBe(true);
      }
    });

    test('previousVersion에 기존 버전 포함', async () => {
      const installer = createInstaller(mockDeps({
        getPluginVersion: async () => '3.0.0',
        getInstalledVersion: async () => '2.9.0',
      }));
      const result = await installer.run();
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.previousVersion).toBe('2.9.0');
      }
    });
  });

  describe('스킵', () => {
    test('설치된 버전 = 플러그인 버전 → action: "skipped"', async () => {
      const installer = createInstaller(mockDeps({
        getPluginVersion: async () => '3.0.0',
        getInstalledVersion: async () => '3.0.0',
      }));
      const result = await installer.run();
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.action).toBe('skipped');
      }
    });

    test('설치된 버전 > 플러그인 버전 → action: "skipped"', async () => {
      const installer = createInstaller(mockDeps({
        getPluginVersion: async () => '3.0.0',
        getInstalledVersion: async () => '3.1.0',
      }));
      const result = await installer.run();
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.action).toBe('skipped');
      }
    });

    test('스킵 시 build/install 호출하지 않음', async () => {
      let buildCalled = false;
      let installCalled = false;
      const installer = createInstaller(mockDeps({
        getPluginVersion: async () => '3.0.0',
        getInstalledVersion: async () => '3.0.0',
        buildBinary: async () => { buildCalled = true; },
        installBinary: async () => { installCalled = true; },
      }));
      await installer.run();
      expect(buildCalled).toBe(false);
      expect(installCalled).toBe(false);
    });
  });

  describe('PATH 설정', () => {
    test('~/.local/bin이 PATH에 있으면 → pathConfigured: true', async () => {
      const installer = createInstaller(mockDeps({
        isInPath: () => true,
      }));
      const result = await installer.run();
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.pathConfigured).toBe(true);
      }
    });

    test('~/.local/bin이 PATH에 없으면 → addToPath 호출', async () => {
      let addToPathCalled = false;
      const installer = createInstaller(mockDeps({
        isInPath: () => false,
        addToPath: async () => { addToPathCalled = true; return { shell: 'bash', rcFile: '~/.bashrc' }; },
      }));
      const result = await installer.run();
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(addToPathCalled).toBe(true);
        expect(result.data.pathConfigured).toBe(true);
      }
    });

    test('addToPath 실패 → pathConfigured: false (비치명적)', async () => {
      const installer = createInstaller(mockDeps({
        isInPath: () => false,
        addToPath: async () => { throw new Error('permission denied'); },
      }));
      const result = await installer.run();
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.data.pathConfigured).toBe(false);
      }
    });
  });

  describe('버전 비교 로직', () => {
    test('3.0.0 vs 3.0.0 → 같음 (0)', () => {
      expect(compareSemVer('3.0.0', '3.0.0')).toBe(0);
    });

    test('2.9.0 vs 3.0.0 → 낮음 (-1)', () => {
      expect(compareSemVer('2.9.0', '3.0.0')).toBe(-1);
    });

    test('3.0.0-alpha.0 vs 3.0.0 → 낮음 (-1)', () => {
      expect(compareSemVer('3.0.0-alpha.0', '3.0.0')).toBe(-1);
    });

    test('3.1.0 vs 3.0.0 → 높음 (1)', () => {
      expect(compareSemVer('3.1.0', '3.0.0')).toBe(1);
    });
  });

  describe('에러 처리', () => {
    test('빌드 실패 → ok: false, 에러 전파', async () => {
      const installer = createInstaller(mockDeps({
        buildBinary: async () => { throw new Error('bun build failed'); },
      }));
      const result = await installer.run();
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('Build failed');
        expect(result.error).toContain('bun build failed');
      }
    });

    test('파일 복사 실패 (권한) → ok: false, 에러 전파', async () => {
      const installer = createInstaller(mockDeps({
        installBinary: async () => { throw new Error('EACCES: permission denied'); },
      }));
      const result = await installer.run();
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('Install failed');
        expect(result.error).toContain('permission denied');
      }
    });

    test('플러그인 버전 읽기 실패 → ok: false', async () => {
      const installer = createInstaller(mockDeps({
        getPluginVersion: async () => { throw new Error('plugin.json not found'); },
      }));
      const result = await installer.run();
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).toContain('plugin version');
      }
    });
  });
});
