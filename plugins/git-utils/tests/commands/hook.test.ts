import { describe, test, expect, beforeEach } from 'bun:test';
import { createHookCommand, type HookCommandDeps } from '../../src/commands/hook';

// ============================================================
// hook command — In-memory FS Mock Test
// ============================================================

function createMockFs(): HookCommandDeps['fs'] & { files: Map<string, string> } {
  const files = new Map<string, string>();
  return {
    files,
    async readFile(path: string) {
      const content = files.get(path);
      if (content === undefined) throw new Error(`File not found: ${path}`);
      return content;
    },
    async writeFile(path: string, content: string) {
      files.set(path, content);
    },
    async exists(path: string) {
      // 디렉토리 체크도 포함: files에 해당 prefix로 시작하는 key가 있으면 true
      if (files.has(path)) return true;
      for (const key of files.keys()) {
        if (key.startsWith(path + '/')) return true;
      }
      return false;
    },
    async mkdir(_path: string) {
      // no-op in memory
    },
  };
}

const PROJECT_DIR = '/tmp/test-project';
const SETTINGS_PATH = `${PROJECT_DIR}/.claude/settings.json`;

describe('hook command', () => {
  let fs: ReturnType<typeof createMockFs>;
  let hook: ReturnType<typeof createHookCommand>;

  beforeEach(() => {
    fs = createMockFs();
    hook = createHookCommand({ fs });
  });

  describe('register', () => {
    test('settings.json 없으면 → 새로 생성하고 hook 등록', async () => {
      const result = await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      });
      expect(result.ok).toBe(true);
      expect(fs.files.has(SETTINGS_PATH)).toBe(true);
    });

    test('settings.json 있고 hooks 비어있으면 → hook 추가', async () => {
      fs.files.set(SETTINGS_PATH, JSON.stringify({ hooks: {} }));
      const result = await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      });
      expect(result.ok).toBe(true);
      const settings = JSON.parse(fs.files.get(SETTINGS_PATH)!);
      expect(settings.hooks.Stop).toHaveLength(1);
    });

    test('동일 command가 이미 있으면 → 기존 hook 업데이트 (action: "updated")', async () => {
      fs.files.set(SETTINGS_PATH, JSON.stringify({
        hooks: { Stop: [{ matcher: '*', hooks: [{ type: 'command', command: 'bash hook.sh' }] }] },
      }));
      const result = await hook.register({
        hookType: 'Stop', matcher: 'Write|Edit', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      });
      expect(result.ok && result.data.action).toBe('updated');
      const settings = JSON.parse(fs.files.get(SETTINGS_PATH)!);
      expect(settings.hooks.Stop).toHaveLength(1);
      expect(settings.hooks.Stop[0].matcher).toBe('Write|Edit');
    });

    test('다른 command가 있으면 → 새 hook 추가 (action: "created")', async () => {
      fs.files.set(SETTINGS_PATH, JSON.stringify({
        hooks: { Stop: [{ matcher: '*', hooks: [{ type: 'command', command: 'bash old.sh' }] }] },
      }));
      const result = await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash new.sh', projectDir: PROJECT_DIR,
      });
      expect(result.ok && result.data.action).toBe('created');
      const settings = JSON.parse(fs.files.get(SETTINGS_PATH)!);
      expect(settings.hooks.Stop).toHaveLength(2);
    });

    test('timeout 지정 → hookEntry에 timeout 포함', async () => {
      await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash hook.sh', timeout: 10, projectDir: PROJECT_DIR,
      });
      const settings = JSON.parse(fs.files.get(SETTINGS_PATH)!);
      expect(settings.hooks.Stop[0].hooks[0].timeout).toBe(10);
    });

    test('timeout 미지정 → hookEntry에 timeout 없음', async () => {
      await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      });
      const settings = JSON.parse(fs.files.get(SETTINGS_PATH)!);
      expect(settings.hooks.Stop[0].hooks[0].timeout).toBeUndefined();
    });

    test('.claude 디렉토리 없으면 → 자동 생성', async () => {
      // fs.mkdir is called (no-op in mock but verifies no error)
      const result = await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      });
      expect(result.ok).toBe(true);
    });

    test('projectDir 지정 → 해당 경로의 settings.json 사용', async () => {
      const customDir = '/tmp/custom-project';
      await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash hook.sh', projectDir: customDir,
      });
      expect(fs.files.has(`${customDir}/.claude/settings.json`)).toBe(true);
    });

    test('projectDir 미지정 → cwd 기준', async () => {
      // register without explicit projectDir uses process.cwd()
      const result = await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash hook.sh',
      });
      expect(result.ok).toBe(true);
    });
  });

  describe('unregister', () => {
    test('존재하는 hook 삭제 → ok: true', async () => {
      fs.files.set(SETTINGS_PATH, JSON.stringify({
        hooks: { Stop: [{ matcher: '*', hooks: [{ type: 'command', command: 'bash hook.sh' }] }] },
      }));
      const result = await hook.unregister({
        hookType: 'Stop', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      });
      expect(result.ok).toBe(true);
    });

    test('존재하지 않는 hook → ok: false, "not found" 메시지', async () => {
      fs.files.set(SETTINGS_PATH, JSON.stringify({
        hooks: { Stop: [{ matcher: '*', hooks: [{ type: 'command', command: 'bash other.sh' }] }] },
      }));
      const result = await hook.unregister({
        hookType: 'Stop', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      });
      expect(result.ok).toBe(false);
      if (!result.ok) expect(result.error).toContain('not found');
    });

    test('삭제 후 hookType 배열 비면 → hookType 키 자체 삭제', async () => {
      fs.files.set(SETTINGS_PATH, JSON.stringify({
        hooks: { Stop: [{ matcher: '*', hooks: [{ type: 'command', command: 'bash hook.sh' }] }] },
      }));
      await hook.unregister({ hookType: 'Stop', command: 'bash hook.sh', projectDir: PROJECT_DIR });
      const settings = JSON.parse(fs.files.get(SETTINGS_PATH)!);
      expect(settings.hooks).toBeUndefined(); // hooks 자체도 삭제됨
    });

    test('삭제 후 hooks 객체 비면 → hooks 키 자체 삭제', async () => {
      fs.files.set(SETTINGS_PATH, JSON.stringify({
        hooks: { Stop: [{ matcher: '*', hooks: [{ type: 'command', command: 'bash hook.sh' }] }] },
      }));
      await hook.unregister({ hookType: 'Stop', command: 'bash hook.sh', projectDir: PROJECT_DIR });
      const settings = JSON.parse(fs.files.get(SETTINGS_PATH)!);
      expect(settings.hooks).toBeUndefined();
    });

    test('settings.json 없으면 → ok: false', async () => {
      const result = await hook.unregister({
        hookType: 'Stop', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      });
      expect(result.ok).toBe(false);
    });
  });

  describe('list', () => {
    test('hookType 지정 → 해당 타입의 hook 배열만 반환', async () => {
      fs.files.set(SETTINGS_PATH, JSON.stringify({
        hooks: {
          Stop: [{ matcher: '*', hooks: [{ type: 'command', command: 'bash stop.sh' }] }],
          PreToolUse: [{ matcher: 'Write', hooks: [{ type: 'command', command: 'bash pre.sh' }] }],
        },
      }));
      const result = await hook.list({ hookType: 'Stop', projectDir: PROJECT_DIR });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(Object.keys(result.data)).toEqual(['Stop']);
        expect(result.data.Stop).toHaveLength(1);
      }
    });

    test('hookType 미지정 → 전체 hooks 객체 반환', async () => {
      fs.files.set(SETTINGS_PATH, JSON.stringify({
        hooks: {
          Stop: [{ matcher: '*', hooks: [{ type: 'command', command: 'bash stop.sh' }] }],
          PreToolUse: [{ matcher: 'Write', hooks: [{ type: 'command', command: 'bash pre.sh' }] }],
        },
      }));
      const result = await hook.list({ projectDir: PROJECT_DIR });
      expect(result.ok).toBe(true);
      if (result.ok) expect(Object.keys(result.data)).toHaveLength(2);
    });

    test('등록된 hook 없으면 → 빈 결과', async () => {
      const result = await hook.list({ projectDir: PROJECT_DIR });
      expect(result.ok).toBe(true);
      if (result.ok) expect(Object.keys(result.data)).toHaveLength(0);
    });
  });

  describe('settings.json 무결성', () => {
    test('기존 settings.json의 다른 필드 보존', async () => {
      fs.files.set(SETTINGS_PATH, JSON.stringify({ customField: 'preserved', hooks: {} }));
      await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      });
      const settings = JSON.parse(fs.files.get(SETTINGS_PATH)!);
      expect(settings.customField).toBe('preserved');
    });

    test('JSON 포맷: 2-space indent + trailing newline', async () => {
      await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      });
      const raw = fs.files.get(SETTINGS_PATH)!;
      expect(raw).toContain('  '); // 2-space indent
      expect(raw.endsWith('\n')).toBe(true);
    });

    test('깨진 JSON → 에러 반환 (덮어쓰지 않음)', async () => {
      fs.files.set(SETTINGS_PATH, '{broken json');
      const result = await hook.register({
        hookType: 'Stop', matcher: '*', command: 'bash hook.sh', projectDir: PROJECT_DIR,
      }).catch((e) => ({ ok: false as const, error: (e as Error).message }));
      expect(result.ok).toBe(false);
    });
  });
});
