// ============================================================
// hook command (← register-hook.js)
// ============================================================

import type {
  Result,
  HookRegisterInput,
  HookRegisterOutput,
  HookUnregisterInput,
  HookUnregisterOutput,
  HookListInput,
  HookMatcher,
} from '../types';
import { join } from 'node:path';

export interface HookCommandDeps {
  fs: {
    readFile(path: string): Promise<string>;
    writeFile(path: string, content: string): Promise<void>;
    exists(path: string): Promise<boolean>;
    mkdir(path: string): Promise<void>;
  };
}

export interface HookCommandInterface {
  register(input: HookRegisterInput): Promise<Result<HookRegisterOutput>>;
  unregister(input: HookUnregisterInput): Promise<Result<HookUnregisterOutput>>;
  list(input: HookListInput): Promise<Result<Record<string, HookMatcher[]>>>;
}

interface Settings {
  hooks?: Record<string, HookMatcher[]>;
  [key: string]: unknown;
}

export function createHookCommand(deps: HookCommandDeps): HookCommandInterface {
  function settingsPath(projectDir: string): string {
    return join(projectDir, '.claude', 'settings.json');
  }

  async function readSettings(projectDir: string): Promise<Settings> {
    const path = settingsPath(projectDir);
    if (!(await deps.fs.exists(path))) {
      return { hooks: {} };
    }
    const content = await deps.fs.readFile(path);
    const settings = JSON.parse(content) as Settings;
    settings.hooks = settings.hooks || {};
    return settings;
  }

  async function writeSettings(projectDir: string, settings: Settings): Promise<void> {
    const claudeDir = join(projectDir, '.claude');
    if (!(await deps.fs.exists(claudeDir))) {
      await deps.fs.mkdir(claudeDir);
    }
    await deps.fs.writeFile(
      settingsPath(projectDir),
      JSON.stringify(settings, null, 2) + '\n',
    );
  }

  return {
    async register(input: HookRegisterInput): Promise<Result<HookRegisterOutput>> {
      const projectDir = input.projectDir || process.cwd();
      const settings = await readSettings(projectDir);

      settings.hooks![input.hookType] = settings.hooks![input.hookType] || [];

      const hookEntry: { type: 'command'; command: string; timeout?: number } = {
        type: 'command',
        command: input.command,
      };
      if (input.timeout !== undefined) {
        hookEntry.timeout = input.timeout;
      }

      const newHook: HookMatcher = {
        matcher: input.matcher,
        hooks: [hookEntry],
      };

      const arr = settings.hooks![input.hookType];
      const existingIndex = arr.findIndex(
        (h) => h.hooks?.some((hook) => hook.command === input.command),
      );

      let action: 'created' | 'updated';
      if (existingIndex >= 0) {
        arr[existingIndex] = newHook;
        action = 'updated';
      } else {
        arr.push(newHook);
        action = 'created';
      }

      await writeSettings(projectDir, settings);
      return { ok: true, data: { action, command: input.command } };
    },

    async unregister(input: HookUnregisterInput): Promise<Result<HookUnregisterOutput>> {
      const projectDir = input.projectDir || process.cwd();

      const path = settingsPath(projectDir);
      if (!(await deps.fs.exists(path))) {
        return { ok: false, error: `No hooks found for type: ${input.hookType}` };
      }

      const settings = await readSettings(projectDir);

      if (!settings.hooks?.[input.hookType]) {
        return { ok: false, error: `No hooks found for type: ${input.hookType}` };
      }

      const initial = settings.hooks[input.hookType].length;
      settings.hooks[input.hookType] = settings.hooks[input.hookType].filter(
        (h) => !h.hooks?.some((hook) => hook.command === input.command),
      );

      if (settings.hooks[input.hookType].length === initial) {
        return { ok: false, error: `Hook not found: ${input.command}` };
      }

      // 빈 배열이면 키 삭제
      if (settings.hooks[input.hookType].length === 0) {
        delete settings.hooks[input.hookType];
      }

      // hooks 객체 자체가 비면 삭제
      if (Object.keys(settings.hooks).length === 0) {
        delete settings.hooks;
      }

      await writeSettings(projectDir, settings);
      return { ok: true, data: { command: input.command } };
    },

    async list(input: HookListInput): Promise<Result<Record<string, HookMatcher[]>>> {
      const projectDir = input.projectDir || process.cwd();
      const settings = await readSettings(projectDir);

      if (input.hookType) {
        const hooks = settings.hooks?.[input.hookType] || [];
        return { ok: true, data: { [input.hookType]: hooks } };
      }

      return { ok: true, data: settings.hooks || {} };
    },
  };
}
