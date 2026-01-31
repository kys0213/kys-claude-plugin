/**
 * PSM Hooks 병합 로직 테스트
 * 실행: bun test src/test/psm-hooks.test.ts
 */

import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import {
  existsSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
} from "fs";
import { join } from "path";
import { execSync } from "child_process";

// ============================================================================
// 테스트용 PSM hooks 로직 (psm.ts에서 추출)
// ============================================================================

const PSM_HOOK_FILES = [
  "on-worker-complete.sh",
  "on-worker-idle.sh",
  "on-worker-question.sh",
  "on-validation-complete.sh",
];

function getPsmHooksConfig(): Record<string, unknown[]> {
  return {
    Stop: [
      {
        type: "command",
        command: ".claude/hooks/on-worker-complete.sh",
      },
    ],
    PreToolUse: [
      {
        matcher: "Task",
        hooks: [
          {
            type: "command",
            command: ".claude/hooks/on-worker-question.sh",
          },
        ],
      },
    ],
    Notification: [
      {
        matcher: ".*",
        hooks: [
          {
            type: "command",
            command: ".claude/hooks/on-worker-idle.sh",
          },
        ],
      },
    ],
  };
}

/**
 * settings.local.json 병합 로직
 * 기존 설정을 보존하면서 PSM hooks를 추가
 */
function mergeSettingsWithPsmHooks(
  existingContent: string | null
): Record<string, unknown> {
  let existingSettings: Record<string, unknown> = {};

  if (existingContent) {
    try {
      existingSettings = JSON.parse(existingContent) as Record<string, unknown>;
    } catch {
      existingSettings = {};
    }
  }

  const existingHooks = (existingSettings.hooks || {}) as Record<
    string,
    unknown[]
  >;
  const psmHooks = getPsmHooksConfig();

  for (const [hookType, psmHookEntries] of Object.entries(psmHooks)) {
    const existingEntries = existingHooks[hookType] || [];

    const filteredPsmEntries = psmHookEntries.filter((entry) => {
      const e = entry as Record<string, unknown>;
      const cmd =
        e.command || (e.hooks as Array<{ command: string }>)?.[0]?.command;

      return !existingEntries.some((existing) => {
        const ex = existing as Record<string, unknown>;
        const existingCmd =
          ex.command || (ex.hooks as Array<{ command: string }>)?.[0]?.command;
        return existingCmd === cmd;
      });
    });

    if (filteredPsmEntries.length > 0) {
      existingHooks[hookType] = [...existingEntries, ...filteredPsmEntries];
    } else if (!existingHooks[hookType] && existingEntries.length === 0) {
      existingHooks[hookType] = psmHookEntries;
    }
  }

  existingSettings.hooks = existingHooks;
  return existingSettings;
}

/**
 * Hook 파일 복사 시뮬레이션 (기존 파일 보존)
 */
function simulateHookFileCopy(
  existingFiles: string[],
  psmFiles: string[]
): string[] {
  const result = [...existingFiles];
  for (const file of psmFiles) {
    if (!result.includes(file)) {
      result.push(file);
    }
  }
  return result;
}

// ============================================================================
// 테스트 케이스: settings.local.json 병합
// ============================================================================

describe("PSM Hooks - settings.local.json 병합", () => {
  test("빈 설정에 PSM hooks 추가", () => {
    const result = mergeSettingsWithPsmHooks(null);

    expect(result.hooks).toBeDefined();
    const hooks = result.hooks as Record<string, unknown[]>;

    expect(hooks.Stop).toHaveLength(1);
    expect(hooks.PreToolUse).toHaveLength(1);
    expect(hooks.Notification).toHaveLength(1);
  });

  test("빈 JSON 객체에 PSM hooks 추가", () => {
    const result = mergeSettingsWithPsmHooks("{}");

    const hooks = result.hooks as Record<string, unknown[]>;
    expect(hooks.Stop).toHaveLength(1);
    expect(hooks.PreToolUse).toHaveLength(1);
    expect(hooks.Notification).toHaveLength(1);
  });

  test("기존 커스텀 설정 보존", () => {
    const existing = JSON.stringify({
      customSetting: "should be preserved",
      anotherSetting: { nested: true },
    });

    const result = mergeSettingsWithPsmHooks(existing);

    expect(result.customSetting).toBe("should be preserved");
    expect((result.anotherSetting as { nested: boolean }).nested).toBe(true);
    expect(result.hooks).toBeDefined();
  });

  test("기존 커스텀 hooks 보존하면서 PSM hooks 추가", () => {
    const existing = JSON.stringify({
      hooks: {
        Stop: [{ type: "command", command: ".claude/hooks/custom-stop.sh" }],
        PreToolUse: [
          {
            matcher: "Write",
            hooks: [
              { type: "command", command: ".claude/hooks/write-guard.sh" },
            ],
          },
        ],
      },
    });

    const result = mergeSettingsWithPsmHooks(existing);
    const hooks = result.hooks as Record<string, unknown[]>;

    // Stop: 기존 1개 + PSM 1개 = 2개
    expect(hooks.Stop).toHaveLength(2);
    expect(
      hooks.Stop.some(
        (h) => (h as { command: string }).command === ".claude/hooks/custom-stop.sh"
      )
    ).toBe(true);
    expect(
      hooks.Stop.some(
        (h) =>
          (h as { command: string }).command ===
          ".claude/hooks/on-worker-complete.sh"
      )
    ).toBe(true);

    // PreToolUse: 기존 1개 + PSM 1개 = 2개
    expect(hooks.PreToolUse).toHaveLength(2);

    // Notification: PSM 1개만 추가
    expect(hooks.Notification).toHaveLength(1);
  });

  test("PSM hooks 중복 추가 방지", () => {
    const existing = JSON.stringify({
      hooks: {
        Stop: [
          {
            type: "command",
            command: ".claude/hooks/on-worker-complete.sh",
          },
        ],
        PreToolUse: [
          {
            matcher: "Task",
            hooks: [
              {
                type: "command",
                command: ".claude/hooks/on-worker-question.sh",
              },
            ],
          },
        ],
      },
    });

    const result = mergeSettingsWithPsmHooks(existing);
    const hooks = result.hooks as Record<string, unknown[]>;

    // 중복 추가되지 않아야 함
    expect(hooks.Stop).toHaveLength(1);
    expect(hooks.PreToolUse).toHaveLength(1);
    // Notification은 기존에 없으므로 추가됨
    expect(hooks.Notification).toHaveLength(1);
  });

  test("잘못된 JSON 입력 처리", () => {
    const result = mergeSettingsWithPsmHooks("invalid json {{{");

    // 파싱 실패해도 PSM hooks는 추가되어야 함
    expect(result.hooks).toBeDefined();
    const hooks = result.hooks as Record<string, unknown[]>;
    expect(hooks.Stop).toHaveLength(1);
  });

  test("부분적으로 PSM hooks가 있는 경우", () => {
    const existing = JSON.stringify({
      hooks: {
        Stop: [
          {
            type: "command",
            command: ".claude/hooks/on-worker-complete.sh",
          },
        ],
        // PreToolUse와 Notification은 없음
      },
    });

    const result = mergeSettingsWithPsmHooks(existing);
    const hooks = result.hooks as Record<string, unknown[]>;

    // Stop은 이미 있으므로 그대로
    expect(hooks.Stop).toHaveLength(1);
    // 나머지는 추가됨
    expect(hooks.PreToolUse).toHaveLength(1);
    expect(hooks.Notification).toHaveLength(1);
  });

  test("다른 hook 타입 보존", () => {
    const existing = JSON.stringify({
      hooks: {
        PostToolUse: [{ type: "command", command: ".claude/hooks/post-tool.sh" }],
        CustomHook: [{ type: "command", command: ".claude/hooks/custom.sh" }],
      },
    });

    const result = mergeSettingsWithPsmHooks(existing);
    const hooks = result.hooks as Record<string, unknown[]>;

    // 기존 다른 hook 타입 보존
    expect(hooks.PostToolUse).toHaveLength(1);
    expect(hooks.CustomHook).toHaveLength(1);

    // PSM hooks 추가
    expect(hooks.Stop).toHaveLength(1);
    expect(hooks.PreToolUse).toHaveLength(1);
    expect(hooks.Notification).toHaveLength(1);
  });
});

// ============================================================================
// 테스트 케이스: Hook 파일 복사
// ============================================================================

describe("PSM Hooks - 파일 복사 로직", () => {
  test("빈 디렉토리에 PSM hook 파일 복사", () => {
    const existingFiles: string[] = [];
    const result = simulateHookFileCopy(existingFiles, PSM_HOOK_FILES);

    expect(result).toHaveLength(4);
    expect(result).toContain("on-worker-complete.sh");
    expect(result).toContain("on-worker-idle.sh");
    expect(result).toContain("on-worker-question.sh");
    expect(result).toContain("on-validation-complete.sh");
  });

  test("기존 파일 보존 (덮어쓰기 방지)", () => {
    const existingFiles = ["on-worker-complete.sh", "custom-hook.sh"];
    const result = simulateHookFileCopy(existingFiles, PSM_HOOK_FILES);

    // 기존 파일 + 새 PSM 파일 (중복 제외)
    expect(result).toHaveLength(5); // 2 existing + 3 new PSM files
    expect(result).toContain("custom-hook.sh");
    expect(result).toContain("on-worker-complete.sh");
  });

  test("모든 PSM hook 파일이 이미 있는 경우", () => {
    const existingFiles = [...PSM_HOOK_FILES, "extra-hook.sh"];
    const result = simulateHookFileCopy(existingFiles, PSM_HOOK_FILES);

    // 변경 없음
    expect(result).toHaveLength(5);
  });
});

// ============================================================================
// 통합 테스트: 실제 파일 시스템
// ============================================================================

describe("PSM Hooks - 파일 시스템 통합 테스트", () => {
  const testDir = "/tmp/psm-hooks-test";

  beforeEach(() => {
    // 테스트 디렉토리 생성
    if (existsSync(testDir)) {
      rmSync(testDir, { recursive: true });
    }
    mkdirSync(join(testDir, ".claude", "hooks"), { recursive: true });
  });

  afterEach(() => {
    // 정리
    if (existsSync(testDir)) {
      rmSync(testDir, { recursive: true });
    }
  });

  test("실제 파일로 settings.local.json 병합", () => {
    const settingsPath = join(testDir, ".claude", "settings.local.json");

    // 기존 설정 파일 생성
    const existingSettings = {
      customKey: "customValue",
      hooks: {
        Stop: [{ type: "command", command: ".claude/hooks/my-hook.sh" }],
      },
    };
    writeFileSync(settingsPath, JSON.stringify(existingSettings, null, 2));

    // 병합 실행
    const content = readFileSync(settingsPath, "utf-8");
    const merged = mergeSettingsWithPsmHooks(content);
    writeFileSync(settingsPath, JSON.stringify(merged, null, 2));

    // 검증
    const finalContent = readFileSync(settingsPath, "utf-8");
    const final = JSON.parse(finalContent);

    expect(final.customKey).toBe("customValue");
    expect(final.hooks.Stop).toHaveLength(2);
  });

  test("settings.local.json이 없는 경우 새로 생성", () => {
    const settingsPath = join(testDir, ".claude", "settings.local.json");

    // 파일 없음 확인
    expect(existsSync(settingsPath)).toBe(false);

    // 병합 실행 (빈 상태)
    const merged = mergeSettingsWithPsmHooks(null);
    writeFileSync(settingsPath, JSON.stringify(merged, null, 2));

    // 검증
    expect(existsSync(settingsPath)).toBe(true);
    const content = readFileSync(settingsPath, "utf-8");
    const settings = JSON.parse(content);

    expect(settings.hooks.Stop).toBeDefined();
    expect(settings.hooks.PreToolUse).toBeDefined();
    expect(settings.hooks.Notification).toBeDefined();
  });

  test("hook 파일 개별 복사 (기존 파일 보존)", () => {
    const hooksDir = join(testDir, ".claude", "hooks");

    // 기존 hook 파일 생성
    writeFileSync(
      join(hooksDir, "on-worker-complete.sh"),
      "#!/bin/bash\n# Custom version\necho 'custom'"
    );
    writeFileSync(
      join(hooksDir, "my-custom-hook.sh"),
      "#!/bin/bash\necho 'my hook'"
    );

    // PSM hook 파일 복사 시뮬레이션
    const srcHooks = ["on-worker-complete.sh", "on-worker-idle.sh"];
    for (const hookFile of srcHooks) {
      const destPath = join(hooksDir, hookFile);
      if (!existsSync(destPath)) {
        writeFileSync(destPath, `#!/bin/bash\n# PSM version of ${hookFile}`);
      }
    }

    // 검증: on-worker-complete.sh는 덮어쓰지 않음
    const content = readFileSync(
      join(hooksDir, "on-worker-complete.sh"),
      "utf-8"
    );
    expect(content).toContain("Custom version");

    // 검증: on-worker-idle.sh는 새로 생성됨
    const idleContent = readFileSync(
      join(hooksDir, "on-worker-idle.sh"),
      "utf-8"
    );
    expect(idleContent).toContain("PSM version");

    // 검증: 기존 커스텀 hook 보존
    expect(existsSync(join(hooksDir, "my-custom-hook.sh"))).toBe(true);
  });
});

// ============================================================================
// Edge Cases
// ============================================================================

describe("PSM Hooks - Edge Cases", () => {
  test("hooks 배열이 빈 배열인 경우", () => {
    const existing = JSON.stringify({
      hooks: {
        Stop: [],
        PreToolUse: [],
      },
    });

    const result = mergeSettingsWithPsmHooks(existing);
    const hooks = result.hooks as Record<string, unknown[]>;

    expect(hooks.Stop).toHaveLength(1);
    expect(hooks.PreToolUse).toHaveLength(1);
  });

  test("hooks가 null인 경우", () => {
    const existing = JSON.stringify({
      hooks: null,
    });

    const result = mergeSettingsWithPsmHooks(existing);
    expect(result.hooks).toBeDefined();
  });

  test("중첩된 hooks 구조 처리", () => {
    const existing = JSON.stringify({
      hooks: {
        PreToolUse: [
          {
            matcher: "Task",
            hooks: [
              { type: "command", command: ".claude/hooks/on-worker-question.sh" },
            ],
          },
          {
            matcher: "Bash",
            hooks: [{ type: "command", command: ".claude/hooks/bash-guard.sh" }],
          },
        ],
      },
    });

    const result = mergeSettingsWithPsmHooks(existing);
    const hooks = result.hooks as Record<string, unknown[]>;

    // Task matcher가 이미 있으므로 중복 추가 안됨, Bash는 유지
    expect(hooks.PreToolUse).toHaveLength(2);
  });

  test("command 경로가 다른 경우 추가됨", () => {
    const existing = JSON.stringify({
      hooks: {
        Stop: [
          { type: "command", command: ".claude/hooks/different-stop.sh" },
        ],
      },
    });

    const result = mergeSettingsWithPsmHooks(existing);
    const hooks = result.hooks as Record<string, unknown[]>;

    // 다른 command이므로 둘 다 존재
    expect(hooks.Stop).toHaveLength(2);
  });
});
