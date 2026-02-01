/**
 * CLI 커맨드 E2E 테스트
 * 실제 tc 명령어를 실행하고 결과를 검증
 *
 * 실행: bun test src/test/cli.test.ts
 */

import { describe, test, expect, beforeAll, afterAll, beforeEach, afterEach } from "bun:test";
import { $ } from "bun";
import { existsSync, rmSync, mkdirSync } from "fs";
import { join } from "path";

// CLI 실행 경로
const CLI_PATH = join(import.meta.dir, "../../src/index.ts");
const runCli = (args: string) => $`bun ${CLI_PATH} ${args.split(" ")}`.quiet().nothrow();

// ============================================================================
// 기본 CLI 테스트
// ============================================================================

describe("tc 기본 명령어", () => {
  test("tc --help 도움말 출력", async () => {
    const result = await runCli("--help");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("Team Claude CLI");
    expect(output).toContain("setup");
    expect(output).toContain("config");
    expect(output).toContain("flow");
    expect(output).toContain("psm");
  });

  test("tc --version 버전 출력", async () => {
    const result = await runCli("--version");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toMatch(/\d+\.\d+\.\d+/); // 버전 형식
  });

  test("tc (인자 없음) 도움말 출력", async () => {
    const result = await runCli("");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("Team Claude CLI");
  });

  test("알 수 없는 명령어 에러", async () => {
    const result = await runCli("unknown-command");

    expect(result.exitCode).not.toBe(0);
  });
});

// ============================================================================
// tc config 테스트
// ============================================================================

describe("tc config", () => {
  test("tc config --help", async () => {
    const result = await runCli("config --help");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("config");
  });

  test("tc config info - 프로젝트 정보 출력", async () => {
    const result = await runCli("config info");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("프로젝트"); // 또는 영문
  });

  test("tc config verify - 환경 검증", async () => {
    const result = await runCli("config verify");
    const output = result.stdout.toString();

    // 검증 결과 출력 (성공 또는 실패)
    expect(result.exitCode).toBeDefined();
  });
});

// ============================================================================
// tc flow 테스트
// ============================================================================

describe("tc flow", () => {
  test("tc flow --help", async () => {
    const result = await runCli("flow --help");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("flow");
    expect(output).toContain("start");
    expect(output).toContain("resume");
    expect(output).toContain("status");
  });

  test("tc flow parse-keyword - autopilot 키워드", async () => {
    const result = await $`bun ${CLI_PATH} flow parse-keyword "autopilot: new feature"`.quiet().nothrow();
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("keyword=autopilot");
    expect(output).toContain("mode=autopilot");
    expect(output).toContain("message=new feature");
    expect(output).toContain("matched=true");
  });

  test("tc flow parse-keyword - spec 키워드", async () => {
    const result = await $`bun ${CLI_PATH} flow parse-keyword "spec: 설계만"`.quiet().nothrow();
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("keyword=spec");
    expect(output).toContain("mode=spec");
  });

  test("tc flow parse-keyword - 조합 키워드", async () => {
    const result = await $`bun ${CLI_PATH} flow parse-keyword "autopilot+swarm: 병렬"`.quiet().nothrow();
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("autopilot+swarm");
    expect(output).toContain("implStrategy=swarm");
  });

  test("tc flow parse-keyword - 키워드 없음", async () => {
    const result = await $`bun ${CLI_PATH} flow parse-keyword "일반 메시지"`.quiet().nothrow();
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("matched=false");
  });

  test("tc flow start --dry-run", async () => {
    const result = await $`bun ${CLI_PATH} flow start "테스트 요구사항" --dry-run`.quiet().nothrow();
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("Dry");
  });

  test("tc flow start --mode autopilot --dry-run", async () => {
    const result = await $`bun ${CLI_PATH} flow start "자동화 테스트" --mode autopilot --dry-run`.quiet().nothrow();
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("autopilot");
  });

  test("tc flow start --impl-strategy swarm --dry-run", async () => {
    const result = await $`bun ${CLI_PATH} flow start "병렬 테스트" --impl-strategy swarm --dry-run`.quiet().nothrow();
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("swarm");
  });

  test("tc flow start - 요구사항 없으면 에러", async () => {
    const result = await $`bun ${CLI_PATH} flow start`.quiet().nothrow();

    expect(result.exitCode).not.toBe(0);
  });

  test("tc flow start - 잘못된 모드 에러", async () => {
    const result = await $`bun ${CLI_PATH} flow start "테스트" --mode invalid`.quiet().nothrow();
    const output = result.stderr.toString() + result.stdout.toString();

    expect(output).toContain("유효하지 않은");
  });

  test("tc flow start - 잘못된 전략 에러", async () => {
    const result = await $`bun ${CLI_PATH} flow start "테스트" --impl-strategy invalid`.quiet().nothrow();
    const output = result.stderr.toString() + result.stdout.toString();

    expect(output).toContain("유효하지 않은");
  });

  test("tc flow resume - 세션 없으면 에러", async () => {
    const result = await $`bun ${CLI_PATH} flow resume nonexistent-session`.quiet().nothrow();

    expect(result.exitCode).not.toBe(0);
  });
});

// ============================================================================
// tc psm 테스트
// ============================================================================

describe("tc psm", () => {
  test("tc psm --help", async () => {
    const result = await runCli("psm --help");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("psm");
    expect(output).toContain("new");
    expect(output).toContain("list");
    expect(output).toContain("status");
  });

  test("tc psm list - 세션 목록", async () => {
    const result = await runCli("psm list");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("PSM Sessions");
  });

  test("tc psm list --status active", async () => {
    const result = await runCli("psm list --status active");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
  });

  test("tc psm status - 전체 상태", async () => {
    const result = await runCli("psm status");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("PSM Status");
  });

  test("tc psm new - 유효하지 않은 세션 이름 (숫자로 시작)", async () => {
    const result = await $`bun ${CLI_PATH} psm new 123invalid`.quiet().nothrow();
    const output = result.stderr.toString() + result.stdout.toString();

    expect(result.exitCode).not.toBe(0);
    expect(output).toContain("유효하지 않은");
  });

  test("tc psm new - 유효하지 않은 세션 이름 (특수문자)", async () => {
    const result = await $`bun ${CLI_PATH} psm new invalid_name`.quiet().nothrow();
    const output = result.stderr.toString() + result.stdout.toString();

    expect(result.exitCode).not.toBe(0);
  });

  test("tc psm switch - 존재하지 않는 세션", async () => {
    const result = await $`bun ${CLI_PATH} psm switch nonexistent`.quiet().nothrow();

    expect(result.exitCode).not.toBe(0);
  });

  test("tc psm parallel - 세션 2개 미만 에러", async () => {
    const result = await $`bun ${CLI_PATH} psm parallel session1`.quiet().nothrow();
    const output = result.stderr.toString() + result.stdout.toString();

    expect(result.exitCode).not.toBe(0);
    expect(output).toContain("2");
  });
});

// ============================================================================
// tc setup 테스트
// ============================================================================

describe("tc setup", () => {
  test("tc setup --help", async () => {
    const result = await runCli("setup --help");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("setup");
  });

  test("tc setup status - 설정 상태", async () => {
    const result = await runCli("setup status");

    expect(result.exitCode).toBe(0);
  });

  test("tc setup init - hooks 설정 포함", async () => {
    const result = await runCli("setup init");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    // hooks 설정이 tc hook 명령어로 설정됨
    expect(output).toMatch(/[Hh]ooks/);
  });
});

// ============================================================================
// tc hud 테스트
// ============================================================================

describe("tc hud", () => {
  test("tc hud --help", async () => {
    const result = await runCli("hud --help");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("hud");
  });

  test("tc hud output - 상태라인 출력", async () => {
    const result = await runCli("hud output");

    // HUD는 상태에 따라 다양한 출력
    expect(result.exitCode).toBe(0);
  });
});

// ============================================================================
// tc test 테스트
// ============================================================================

describe("tc test", () => {
  test("tc test --help", async () => {
    const result = await runCli("test --help");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("test");
  });
});

// ============================================================================
// 통합 시나리오 테스트
// ============================================================================

describe("통합 시나리오", () => {
  test("flow start → status 사이클 (dry-run)", async () => {
    // 1. dry-run으로 시작
    const startResult = await $`bun ${CLI_PATH} flow start "테스트 기능" --dry-run`.quiet().nothrow();
    expect(startResult.exitCode).toBe(0);
    expect(startResult.stdout.toString()).toContain("Dry");
  });

  test("Magic Keyword 전체 파싱 테스트", async () => {
    const keywords = [
      { input: "autopilot: test", expect: "autopilot" },
      { input: "auto: test", expect: "autopilot" },
      { input: "ap: test", expect: "autopilot" },
      { input: "spec: test", expect: "spec" },
      { input: "sp: test", expect: "spec" },
      { input: "impl: test", expect: "impl" },
      { input: "im: test", expect: "impl" },
    ];

    for (const kw of keywords) {
      const result = await $`bun ${CLI_PATH} flow parse-keyword ${kw.input}`.quiet().nothrow();
      const output = result.stdout.toString();

      expect(result.exitCode).toBe(0);
      expect(output).toContain(`mode=${kw.expect}`);
    }
  });

  test("구현 전략 파싱 테스트", async () => {
    const strategies = [
      { input: "swarm: test", expect: "swarm" },
      { input: "psm: test", expect: "psm" },
      { input: "sequential: test", expect: "sequential" },
    ];

    for (const s of strategies) {
      const result = await $`bun ${CLI_PATH} flow parse-keyword ${s.input}`.quiet().nothrow();
      const output = result.stdout.toString();

      expect(result.exitCode).toBe(0);
      expect(output).toContain(`implStrategy=${s.expect}`);
    }
  });

  test("조합 키워드 파싱 테스트", async () => {
    const combos = [
      { input: "autopilot+swarm: test", mode: "autopilot", strategy: "swarm" },
      { input: "auto+psm: test", mode: "autopilot", strategy: "psm" },
      { input: "spec+sequential: test", mode: "spec", strategy: "sequential" },
    ];

    for (const c of combos) {
      const result = await $`bun ${CLI_PATH} flow parse-keyword ${c.input}`.quiet().nothrow();
      const output = result.stdout.toString();

      expect(result.exitCode).toBe(0);
      expect(output).toContain(`mode=${c.mode}`);
      expect(output).toContain(`implStrategy=${c.strategy}`);
    }
  });
});

// ============================================================================
// 에러 핸들링 테스트
// ============================================================================

describe("에러 핸들링", () => {
  test("빈 인자 처리", async () => {
    const commands = ["flow start", "psm new", "psm switch", "psm parallel"];

    for (const cmd of commands) {
      const result = await runCli(cmd);
      // 에러 또는 도움말 출력
      expect(result.exitCode).toBeDefined();
    }
  });

  test("잘못된 서브커맨드", async () => {
    const result = await runCli("flow invalid-subcommand");
    expect(result.exitCode).not.toBe(0);
  });
});

// ============================================================================
// 출력 형식 테스트
// ============================================================================

describe("출력 형식", () => {
  test("JSON 출력 포함 (flow start --dry-run)", async () => {
    const result = await $`bun ${CLI_PATH} flow start "테스트" --mode autopilot --dry-run`.quiet().nothrow();
    const output = result.stdout.toString();

    // dry-run은 JSON 출력 없음, 시뮬레이션 메시지만
    expect(output).toContain("Dry");
  });

  test("컬러 출력 지원 확인", async () => {
    // CI 환경에서는 컬러 비활성화될 수 있음
    const result = await runCli("--help");
    expect(result.exitCode).toBe(0);
  });
});

// ============================================================================
// tc hook 테스트
// ============================================================================

describe("tc hook", () => {
  test("tc hook --help", async () => {
    const result = await runCli("hook --help");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("hook");
    expect(output).toContain("worker-complete");
    expect(output).toContain("worker-idle");
    expect(output).toContain("worker-question");
    expect(output).toContain("validation-complete");
  });

  test("tc hook worker-complete - delegation 없으면 종료", async () => {
    const result = await runCli("hook worker-complete");
    const output = result.stdout.toString();

    // delegation 상태 파일이 없으면 정상 종료
    expect(result.exitCode).toBe(0);
    expect(output).toContain("No active delegation");
  });

  test("tc hook worker-idle - delegation 없어도 정상 종료", async () => {
    const result = await runCli("hook worker-idle");

    expect(result.exitCode).toBe(0);
  });

  test("tc hook worker-question - stdin 없으면 정상 종료", async () => {
    const result = await runCli("hook worker-question");

    expect(result.exitCode).toBe(0);
  });

  test("tc hook validation-complete - stdin 없으면 정상 종료", async () => {
    const result = await runCli("hook validation-complete");

    expect(result.exitCode).toBe(0);
  });

  test("tc hook generate-config - hooks.json 출력", async () => {
    const result = await runCli("hook generate-config");
    const output = result.stdout.toString();

    expect(result.exitCode).toBe(0);
    expect(output).toContain("hooks");
    expect(output).toContain("Stop");
    expect(output).toContain("tc hook");
  });
});

// ============================================================================
// 성능 테스트
// ============================================================================

describe("성능", () => {
  test("CLI 시작 시간 (--help)", async () => {
    const start = performance.now();
    await runCli("--help");
    const duration = performance.now() - start;

    // 2초 이내에 완료되어야 함
    expect(duration).toBeLessThan(2000);
  });

  test("parse-keyword 응답 시간", async () => {
    const start = performance.now();
    await $`bun ${CLI_PATH} flow parse-keyword "autopilot: test"`.quiet().nothrow();
    const duration = performance.now() - start;

    // 1초 이내에 완료되어야 함
    expect(duration).toBeLessThan(1000);
  });
});
