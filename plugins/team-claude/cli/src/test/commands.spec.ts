/**
 * CLI Commands Test Specification
 *
 * TDD 접근: 먼저 테스트를 작성하고 구현을 진행
 * 각 커맨드의 예상 동작을 정의
 */

import { describe, test, expect, beforeAll, afterAll } from "bun:test";
import { $ } from "bun";
import { mkdirSync, rmSync, existsSync, writeFileSync, readFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

// ============================================================================
// 테스트 유틸리티
// ============================================================================

const TC_CLI = join(import.meta.dir, "../../src/index.ts");

interface CLIResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

async function runTC(args: string, cwd?: string): Promise<CLIResult> {
  // 셸을 통해 실행하여 따옴표가 포함된 인자를 올바르게 처리
  const cmd = `bun run ${TC_CLI} ${args}`;
  const result = await $`bash -c ${cmd}`
    .cwd(cwd || process.cwd())
    .quiet()
    .nothrow();

  return {
    stdout: result.stdout.toString(),
    stderr: result.stderr.toString(),
    exitCode: result.exitCode,
  };
}

async function runTCJson<T>(args: string, cwd?: string): Promise<T> {
  const result = await runTC(`${args} --json`, cwd);
  return JSON.parse(result.stdout);
}

// 테스트용 임시 Git 저장소 생성
async function createTestRepo(): Promise<string> {
  const testDir = join(tmpdir(), `tc-test-${Date.now()}`);
  mkdirSync(testDir, { recursive: true });

  await $`git init`.cwd(testDir).quiet();
  await $`git config user.email "test@test.com"`.cwd(testDir).quiet();
  await $`git config user.name "Test"`.cwd(testDir).quiet();

  writeFileSync(join(testDir, "README.md"), "# Test Project");
  await $`git add . && git commit -m "Initial commit"`.cwd(testDir).quiet();

  return testDir;
}

function cleanupTestRepo(testDir: string): void {
  if (existsSync(testDir)) {
    rmSync(testDir, { recursive: true, force: true });
  }
}

// ============================================================================
// tc hook - Hook 통합 커맨드 테스트
// ============================================================================

describe("tc hook", () => {
  describe("validation-complete", () => {
    test("성공 시 상태를 completed로 업데이트", async () => {
      const result = await runTC("hook validation-complete --exit-code 0");
      expect(result.exitCode).toBe(0);
    });

    test("실패 시 재시도 트리거 (iteration < max)", async () => {
      const result = await runTC(
        "hook validation-complete --exit-code 1 --iteration 1 --max-iterations 5"
      );
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("retry");
    });

    test("최대 반복 초과 시 에스컬레이션", async () => {
      const result = await runTC(
        "hook validation-complete --exit-code 1 --iteration 5 --max-iterations 5"
      );
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("escalat");
    });

    test("--json 출력 형식 준수", async () => {
      const result = await runTCJson<{
        success: boolean;
        data: { action: string };
      }>("hook validation-complete --exit-code 0");

      expect(result.success).toBe(true);
      expect(result.data.action).toBeDefined();
    });
  });

  describe("worker-complete", () => {
    test("task-id 필수", async () => {
      const result = await runTC("hook worker-complete");
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("task-id");
    });

    test("유효한 task-id로 검증 트리거", async () => {
      const result = await runTC("hook worker-complete --task-id test-123");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("worker-idle", () => {
    test("context 사용률 전달", async () => {
      const result = await runTC("hook worker-idle --percent 80");
      expect(result.exitCode).toBe(0);
    });

    test("80% 이상 시 체크포인트 저장 권장", async () => {
      const result = await runTCJson<{
        data: { recommendation: string };
      }>("hook worker-idle --percent 85");

      expect(result.data.recommendation).toContain("checkpoint");
    });
  });

  describe("worker-question", () => {
    test("질문 내용 전달", async () => {
      const result = await runTC('hook worker-question --question "API 키가 필요합니다"');
      expect(result.exitCode).toBe(0);
    });
  });
});

// ============================================================================
// tc server - 서버 관리 커맨드 테스트
// ============================================================================

describe("tc server", () => {
  describe("status", () => {
    test("서버 상태 반환", async () => {
      const result = await runTC("server status");
      expect(result.exitCode).toBe(0);
      // running 또는 stopped 상태
      expect(result.stdout).toMatch(/running|stopped/i);
    });

    test("--json 출력 형식", async () => {
      const result = await runTCJson<{
        success: boolean;
        data: { running: boolean; port: number };
      }>("server status");

      expect(result.success).toBe(true);
      expect(typeof result.data.running).toBe("boolean");
      expect(typeof result.data.port).toBe("number");
    });
  });

  describe("start", () => {
    test("서버 시작 (이미 실행 중이면 스킵)", async () => {
      const result = await runTC("server start");
      expect(result.exitCode).toBe(0);
    });

    test("커스텀 포트 지정", async () => {
      const result = await runTC("server start --port 7899");
      // 성공 또는 이미 실행 중
      expect([0, 1]).toContain(result.exitCode);
    });
  });

  describe("stop", () => {
    test("서버 중지", async () => {
      const result = await runTC("server stop");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("logs", () => {
    test("로그 출력", async () => {
      const result = await runTC("server logs --lines 10");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("build", () => {
    test("서버 빌드", async () => {
      const result = await runTC("server build");
      expect(result.exitCode).toBe(0);
    });
  });
});

// ============================================================================
// tc session - 세션 관리 커맨드 테스트
// ============================================================================

describe("tc session", () => {
  let testDir: string;

  beforeAll(async () => {
    testDir = await createTestRepo();
  });

  afterAll(() => {
    cleanupTestRepo(testDir);
  });

  describe("create", () => {
    test("새 세션 생성", async () => {
      const result = await runTC('session create "테스트 기능"', testDir);
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/[a-z0-9]{8}/); // 세션 ID
    });

    test("--json으로 세션 ID 반환", async () => {
      const result = await runTCJson<{
        success: boolean;
        data: { sessionId: string; path: string };
      }>('session create "JSON 테스트"', testDir);

      expect(result.success).toBe(true);
      expect(result.data.sessionId).toMatch(/^[a-z0-9]{8}$/);
      expect(result.data.path).toContain("sessions");
    });

    test("제목 필수", async () => {
      const result = await runTC("session create", testDir);
      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("list", () => {
    test("세션 목록 조회", async () => {
      const result = await runTC("session list", testDir);
      expect(result.exitCode).toBe(0);
    });

    test("--json으로 배열 반환", async () => {
      const result = await runTCJson<{
        success: boolean;
        data: Array<{ sessionId: string; title: string; status: string }>;
      }>("session list", testDir);

      expect(result.success).toBe(true);
      expect(Array.isArray(result.data)).toBe(true);
    });
  });

  describe("show", () => {
    test("세션 상세 정보", async () => {
      // 먼저 세션 생성
      const createResult = await runTCJson<{
        data: { sessionId: string };
      }>('session create "Show 테스트"', testDir);

      const result = await runTC(`session show ${createResult.data.sessionId}`, testDir);
      expect(result.exitCode).toBe(0);
    });

    test("존재하지 않는 세션", async () => {
      const result = await runTC("session show nonexistent", testDir);
      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("delete", () => {
    test("세션 삭제", async () => {
      const createResult = await runTCJson<{
        data: { sessionId: string };
      }>('session create "Delete 테스트"', testDir);

      const result = await runTC(`session delete ${createResult.data.sessionId}`, testDir);
      expect(result.exitCode).toBe(0);
    });
  });
});

// ============================================================================
// tc state - 워크플로우 상태 관리 테스트
// ============================================================================

describe("tc state", () => {
  let testDir: string;

  beforeAll(async () => {
    testDir = await createTestRepo();
  });

  afterAll(() => {
    cleanupTestRepo(testDir);
  });

  describe("check", () => {
    test("현재 상태 표시", async () => {
      const result = await runTC("state check", testDir);
      expect(result.exitCode).toBe(0);
    });

    test("--json 형식", async () => {
      const result = await runTCJson<{
        success: boolean;
        data: { phase: string; sessionId?: string };
      }>("state check", testDir);

      expect(result.success).toBe(true);
      expect(result.data.phase).toBeDefined();
    });
  });

  describe("get", () => {
    test("특정 키 조회", async () => {
      const result = await runTC("state get phase", testDir);
      expect(result.exitCode).toBe(0);
    });
  });

  describe("transition", () => {
    test("유효한 상태 전이", async () => {
      const result = await runTC("state transition designing", testDir);
      expect(result.exitCode).toBe(0);
    });

    test("유효하지 않은 phase", async () => {
      const result = await runTC("state transition invalid_phase", testDir);
      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("reset", () => {
    test("상태 초기화", async () => {
      const result = await runTC("state reset", testDir);
      expect(result.exitCode).toBe(0);
    });

    test("초기화 후 phase는 idle", async () => {
      await runTC("state reset", testDir);
      const result = await runTCJson<{
        data: { phase: string };
      }>("state check", testDir);

      expect(result.data.phase).toBe("idle");
    });
  });
});

// ============================================================================
// tc review - 자동 리뷰 테스트
// ============================================================================

describe("tc review", () => {
  let testDir: string;

  beforeAll(async () => {
    testDir = await createTestRepo();
  });

  afterAll(() => {
    cleanupTestRepo(testDir);
  });

  describe("spec", () => {
    test("세션 ID 필수", async () => {
      const result = await runTC("review spec", testDir);
      expect(result.exitCode).not.toBe(0);
    });

    test("--max-iterations 옵션", async () => {
      const result = await runTC("review spec test123 --max-iterations 3", testDir);
      // 세션이 없으므로 실패하지만 파싱은 성공해야 함
      expect(result.stderr).not.toContain("Unknown option");
    });

    test("--auto-fix 옵션", async () => {
      const result = await runTC("review spec test123 --auto-fix", testDir);
      expect(result.stderr).not.toContain("Unknown option");
    });

    test("--strict 옵션", async () => {
      const result = await runTC("review spec test123 --strict", testDir);
      expect(result.stderr).not.toContain("Unknown option");
    });
  });

  describe("code", () => {
    test("checkpoint ID 필수", async () => {
      const result = await runTC("review code", testDir);
      expect(result.exitCode).not.toBe(0);
    });
  });
});

// ============================================================================
// tc worktree - Git Worktree 관리 테스트
// ============================================================================

describe("tc worktree", () => {
  let testDir: string;

  beforeAll(async () => {
    testDir = await createTestRepo();
  });

  afterAll(() => {
    cleanupTestRepo(testDir);
  });

  describe("create", () => {
    test("worktree 생성", async () => {
      const result = await runTC("worktree create test-feature", testDir);
      expect(result.exitCode).toBe(0);
    });

    test("--json으로 경로 반환", async () => {
      const result = await runTCJson<{
        success: boolean;
        data: { path: string; branch: string };
      }>("worktree create json-feature", testDir);

      expect(result.success).toBe(true);
      expect(result.data.path).toBeDefined();
      expect(result.data.branch).toContain("team-claude");
    });

    test("ID 필수", async () => {
      const result = await runTC("worktree create", testDir);
      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("list", () => {
    test("worktree 목록", async () => {
      const result = await runTC("worktree list", testDir);
      expect(result.exitCode).toBe(0);
    });
  });

  describe("path", () => {
    test("특정 worktree 경로 출력", async () => {
      await runTC("worktree create path-test", testDir);
      const result = await runTC("worktree path path-test", testDir);
      expect(result.exitCode).toBe(0);
      expect(result.stdout.trim()).toContain("worktrees");
    });
  });

  describe("delete", () => {
    test("worktree 삭제", async () => {
      await runTC("worktree create delete-test", testDir);
      const result = await runTC("worktree delete delete-test", testDir);
      expect(result.exitCode).toBe(0);
    });
  });

  describe("cleanup", () => {
    test("전체 정리", async () => {
      const result = await runTC("worktree cleanup", testDir);
      expect(result.exitCode).toBe(0);
    });
  });
});

// ============================================================================
// tc agent - 에이전트 관리 테스트
// ============================================================================

describe("tc agent", () => {
  describe("list", () => {
    test("에이전트 목록", async () => {
      const result = await runTC("agent list");
      expect(result.exitCode).toBe(0);
    });

    test("--json 배열 반환", async () => {
      const result = await runTCJson<{
        success: boolean;
        data: Array<{ name: string; source: string }>;
      }>("agent list");

      expect(result.success).toBe(true);
      expect(Array.isArray(result.data)).toBe(true);
    });
  });

  describe("info", () => {
    test("특정 에이전트 정보", async () => {
      const result = await runTC("agent info spec-reviewer");
      expect(result.exitCode).toBe(0);
    });

    test("존재하지 않는 에이전트", async () => {
      const result = await runTC("agent info nonexistent-agent");
      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("validate", () => {
    test("충돌 검사 실행", async () => {
      const result = await runTC("agent validate");
      expect(result.exitCode).toBe(0);
    });
  });
});

// ============================================================================
// 글로벌 옵션 테스트
// ============================================================================

describe("Global Options", () => {
  describe("--json", () => {
    test("모든 커맨드에서 JSON 출력", async () => {
      const commands = [
        "config info",
        "server status",
        "psm list",
        "agent list",
      ];

      for (const cmd of commands) {
        const result = await runTC(`${cmd} --json`);
        if (result.exitCode === 0) {
          expect(() => JSON.parse(result.stdout)).not.toThrow();
        }
      }
    });

    test("JSON 출력 구조 준수", async () => {
      const result = await runTCJson<{
        success: boolean;
        meta?: { timestamp: string };
      }>("config info");

      expect(typeof result.success).toBe("boolean");
    });
  });

  describe("--quiet", () => {
    test("최소 출력", async () => {
      const normal = await runTC("config info");
      const quiet = await runTC("config info --quiet");

      expect(quiet.stdout.length).toBeLessThanOrEqual(normal.stdout.length);
    });
  });

  describe("--help", () => {
    test("도움말 출력", async () => {
      const result = await runTC("--help");
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Usage");
    });

    test("서브커맨드 도움말", async () => {
      const result = await runTC("server --help");
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("server");
    });
  });

  describe("--version", () => {
    test("버전 출력", async () => {
      const result = await runTC("--version");
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/\d+\.\d+\.\d+/);
    });
  });
});

// ============================================================================
// 기존 커맨드 회귀 테스트
// ============================================================================

describe("Regression - Existing Commands", () => {
  describe("tc setup", () => {
    test("setup status 동작", async () => {
      const result = await runTC("setup status");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("tc config", () => {
    test("config show 동작", async () => {
      const result = await runTC("config show");
      expect(result.exitCode).toBe(0);
    });

    test("config info 동작", async () => {
      const result = await runTC("config info");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("tc flow", () => {
    test("flow status 동작", async () => {
      const result = await runTC("flow status");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("tc psm", () => {
    test("psm list 동작", async () => {
      const result = await runTC("psm list");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("tc hud", () => {
    test("hud output 동작", async () => {
      const result = await runTC("hud output");
      expect(result.exitCode).toBe(0);
    });
  });
});
