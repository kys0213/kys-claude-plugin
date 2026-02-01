/**
 * CLI Commands Test Specification
 *
 * TDD 접근: 먼저 테스트를 작성하고 구현을 진행
 * 각 커맨드의 예상 동작을 정의
 */

import { describe, test, expect } from "bun:test";
import { $ } from "bun";
import { join } from "path";

// ============================================================================
// 테스트 유틸리티
// ============================================================================

// CLI 디렉토리 (패키지 해석을 위해 필요)
const CLI_DIR = join(import.meta.dir, "../..");
const TC_CLI = join(CLI_DIR, "src/index.ts");

interface CLIResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

async function runTC(args: string): Promise<CLIResult> {
  // CLI 디렉토리에서 실행하여 패키지 해석이 작동하도록 함
  const cmd = `bun run ${TC_CLI} ${args}`;
  const result = await $`bash -c ${cmd}`
    .cwd(CLI_DIR)
    .quiet()
    .nothrow();

  return {
    stdout: result.stdout.toString(),
    stderr: result.stderr.toString(),
    exitCode: result.exitCode,
  };
}

async function runTCJson<T>(args: string): Promise<T> {
  const result = await runTC(`${args} --json`);
  return JSON.parse(result.stdout);
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
    test("서버 상태 반환 (running 또는 stopped)", async () => {
      const result = await runTC("server status");
      // 서버가 없어도 상태 출력은 성공 (exit code는 상태에 따라 다름)
      expect(result.stdout).toMatch(/running|stopped/i);
    });

    test("--json 출력 형식", async () => {
      const result = await runTC("server status --json");
      const parsed = JSON.parse(result.stdout);
      expect(parsed.success).toBe(true);
      expect(typeof parsed.data.running).toBe("boolean");
      expect(typeof parsed.data.port).toBe("number");
    });
  });

  describe("start", () => {
    test("서버 시작 도움말", async () => {
      const result = await runTC("server start --help");
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("port");
    });
  });

  describe("stop", () => {
    test("서버 중지 (미실행 시에도 성공)", async () => {
      const result = await runTC("server stop");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("logs", () => {
    test("로그 출력 (파일 없어도 성공)", async () => {
      const result = await runTC("server logs --lines 10");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("build", () => {
    test("서버 빌드 명령 실행", async () => {
      const result = await runTC("server build");
      // 빌드 성공 또는 소스 없음 에러
      expect([0, 1]).toContain(result.exitCode);
    });
  });
});

// ============================================================================
// tc session - 세션 관리 커맨드 테스트
// ============================================================================

describe("tc session", () => {
  describe("help", () => {
    test("session 도움말 출력", async () => {
      const result = await runTC("session --help");
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("create");
      expect(result.stdout).toContain("list");
      expect(result.stdout).toContain("show");
      expect(result.stdout).toContain("delete");
    });
  });

  describe("create", () => {
    test("제목 필수", async () => {
      const result = await runTC("session create");
      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("list", () => {
    test("세션 목록 조회 (현재 프로젝트)", async () => {
      const result = await runTC("session list");
      expect(result.exitCode).toBe(0);
    });

    test("--json 옵션 지원", async () => {
      const result = await runTC("session list --json");
      expect(result.exitCode).toBe(0);
      expect(() => JSON.parse(result.stdout)).not.toThrow();
    });
  });

  describe("show", () => {
    test("존재하지 않는 세션", async () => {
      const result = await runTC("session show nonexistent");
      expect(result.exitCode).not.toBe(0);
    });
  });
});

// ============================================================================
// tc state - 워크플로우 상태 관리 테스트
// ============================================================================

describe("tc state", () => {
  describe("help", () => {
    test("state 도움말 출력", async () => {
      const result = await runTC("state --help");
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("check");
      expect(result.stdout).toContain("get");
      expect(result.stdout).toContain("transition");
      expect(result.stdout).toContain("reset");
    });
  });

  describe("check", () => {
    test("현재 상태 표시", async () => {
      const result = await runTC("state check");
      expect(result.exitCode).toBe(0);
    });

    test("--json 옵션 지원", async () => {
      const result = await runTC("state check --json");
      expect(result.exitCode).toBe(0);
      expect(() => JSON.parse(result.stdout)).not.toThrow();
    });
  });

  describe("get", () => {
    test("키 필수", async () => {
      const result = await runTC("state get");
      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("transition", () => {
    test("유효하지 않은 phase", async () => {
      const result = await runTC("state transition invalid_phase");
      expect(result.exitCode).not.toBe(0);
    });
  });
});

// ============================================================================
// tc review - 자동 리뷰 테스트
// ============================================================================

describe("tc review", () => {
  describe("help", () => {
    test("review 도움말 출력", async () => {
      const result = await runTC("review --help");
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("spec");
      expect(result.stdout).toContain("code");
    });
  });

  describe("spec", () => {
    test("세션 ID 필수", async () => {
      const result = await runTC("review spec");
      expect(result.exitCode).not.toBe(0);
    });

    test("--max-iterations 옵션", async () => {
      const result = await runTC("review spec test123 --max-iterations 3");
      // 세션이 없으므로 실패하지만 파싱은 성공해야 함
      expect(result.stderr).not.toContain("Unknown option");
    });

    test("--auto-fix 옵션", async () => {
      const result = await runTC("review spec test123 --auto-fix");
      expect(result.stderr).not.toContain("Unknown option");
    });

    test("--strict 옵션", async () => {
      const result = await runTC("review spec test123 --strict");
      expect(result.stderr).not.toContain("Unknown option");
    });
  });

  describe("code", () => {
    test("checkpoint ID 필수", async () => {
      const result = await runTC("review code");
      expect(result.exitCode).not.toBe(0);
    });
  });
});

// ============================================================================
// tc worktree - Git Worktree 관리 테스트
// ============================================================================

describe("tc worktree", () => {
  describe("help", () => {
    test("worktree 도움말 출력", async () => {
      const result = await runTC("worktree --help");
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("create");
      expect(result.stdout).toContain("list");
      expect(result.stdout).toContain("path");
      expect(result.stdout).toContain("delete");
      expect(result.stdout).toContain("cleanup");
    });
  });

  describe("create", () => {
    test("ID 필수", async () => {
      const result = await runTC("worktree create");
      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("list", () => {
    test("worktree 목록", async () => {
      const result = await runTC("worktree list");
      expect(result.exitCode).toBe(0);
    });

    test("--json 옵션 지원", async () => {
      const result = await runTC("worktree list --json");
      expect(result.exitCode).toBe(0);
      expect(() => JSON.parse(result.stdout)).not.toThrow();
    });
  });

  describe("cleanup", () => {
    test("전체 정리", async () => {
      const result = await runTC("worktree cleanup");
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
      const result = await runTC("agent list --json");
      const parsed = JSON.parse(result.stdout);
      expect(parsed.success).toBe(true);
      expect(Array.isArray(parsed.data)).toBe(true);
    });
  });

  describe("info", () => {
    test("에이전트 info --help", async () => {
      const result = await runTC("agent info --help");
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
    test("JSON 지원 커맨드에서 JSON 출력", async () => {
      const commands = [
        "server status",
        "agent list",
        "state check",
      ];

      for (const cmd of commands) {
        const result = await runTC(`${cmd} --json`);
        expect(() => JSON.parse(result.stdout)).not.toThrow();
      }
    });

    test("JSON 출력 구조 준수", async () => {
      const result = await runTC("agent list --json");
      const parsed = JSON.parse(result.stdout);
      expect(typeof parsed.success).toBe("boolean");
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
    test("setup --help 동작", async () => {
      const result = await runTC("setup --help");
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("setup");
    });
  });

  describe("tc config", () => {
    test("config --help 동작", async () => {
      const result = await runTC("config --help");
      expect(result.exitCode).toBe(0);
    });

    test("config info 동작", async () => {
      const result = await runTC("config info");
      // 설정 파일이 없어도 정보 출력은 시도
      expect([0, 1]).toContain(result.exitCode);
    });
  });

  describe("tc flow", () => {
    test("flow --help 동작", async () => {
      const result = await runTC("flow --help");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("tc psm", () => {
    test("psm --help 동작", async () => {
      const result = await runTC("psm --help");
      expect(result.exitCode).toBe(0);
    });
  });

  describe("tc hud", () => {
    test("hud --help 동작", async () => {
      const result = await runTC("hud --help");
      expect(result.exitCode).toBe(0);
    });
  });
});
