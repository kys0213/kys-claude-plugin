// ============================================================
// hook command (← register-hook.js)
// ============================================================
// CLI:
//   bun run src/cli.ts hook register <hookType> <matcher> <command> [--timeout=<n>] [--project-dir=<p>]
//   bun run src/cli.ts hook unregister <hookType> <command> [--project-dir=<p>]
//   bun run src/cli.ts hook list [hookType] [--project-dir=<p>]
//
// 동작:
//   - .claude/settings.json에 hook을 안전하게 등록/삭제
//   - 기존 hook과 병합, 중복 방지
//
// 기존 register-hook.js 대비 개선:
//   - TypeScript 타입 안전성
//   - timeout 단위 문서 일관성 (seconds)
// ============================================================

import type {
  Command,
  Result,
  HookRegisterInput,
  HookRegisterOutput,
  HookUnregisterInput,
  HookUnregisterOutput,
  HookListInput,
  HookMatcher,
} from '../types';

export interface HookCommandDeps {
  /** settings.json 읽기/쓰기를 위한 파일시스템 추상화 */
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
