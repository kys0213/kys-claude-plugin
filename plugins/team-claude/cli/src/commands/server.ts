/**
 * tc server - 서버 라이프사이클 관리 커맨드
 *
 * 글로벌 서버의 시작, 중지, 상태 확인 등을 처리합니다.
 */

import { Command } from "commander";
import { existsSync, readFileSync, writeFileSync, mkdirSync } from "fs";
import { join, dirname } from "path";
import { homedir } from "os";
import { execSync, spawn } from "child_process";
import { $ } from "bun";

// ============================================================================
// 상수
// ============================================================================

const CLAUDE_DIR = join(homedir(), ".claude");
const SERVER_BINARY = join(CLAUDE_DIR, "team-claude-server");
const PID_FILE = join(CLAUDE_DIR, "team-claude-server.pid");
const LOG_FILE = join(CLAUDE_DIR, "team-claude-server.log");
const DEFAULT_PORT = 7890;

// ============================================================================
// 타입 정의
// ============================================================================

interface ServerStatus {
  running: boolean;
  healthy: boolean;
  pid?: number;
  port: number;
  binaryExists: boolean;
  uptime?: string;
}

interface CLIOutput<T> {
  success: boolean;
  data?: T;
  error?: {
    code: string;
    message: string;
  };
  meta?: {
    timestamp: string;
    duration_ms: number;
  };
}

// ============================================================================
// 유틸리티 함수
// ============================================================================

function ensureDir(dir: string): void {
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
}

function timestamp(): string {
  return new Date().toISOString();
}

function getServerSourceDir(): string {
  // CLI가 위치한 디렉토리 기준으로 server 디렉토리 찾기
  return join(dirname(dirname(dirname(import.meta.dir))), "server");
}

function getPort(): number {
  // TODO: 설정 파일에서 포트 읽기
  return DEFAULT_PORT;
}

function getPid(): number | null {
  if (!existsSync(PID_FILE)) {
    return null;
  }
  try {
    const pid = parseInt(readFileSync(PID_FILE, "utf-8").trim(), 10);
    return isNaN(pid) ? null : pid;
  } catch {
    return null;
  }
}

function isRunning(): boolean {
  const pid = getPid();
  if (!pid) return false;

  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

async function isHealthy(): Promise<boolean> {
  const port = getPort();
  try {
    const response = await fetch(`http://localhost:${port}/health`);
    return response.ok;
  } catch {
    return false;
  }
}

function formatUptime(startTime: number): string {
  const seconds = Math.floor((Date.now() - startTime) / 1000);
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  const hours = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${mins}m`;
}

function outputResult<T>(data: T, json: boolean, startTime: number): void {
  if (json) {
    const output: CLIOutput<T> = {
      success: true,
      data,
      meta: {
        timestamp: timestamp(),
        duration_ms: Date.now() - startTime,
      },
    };
    console.log(JSON.stringify(output, null, 2));
  }
}

// ============================================================================
// status 핸들러
// ============================================================================

async function handleStatus(options: { json?: boolean }): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  const port = getPort();
  const pid = getPid();
  const running = isRunning();
  const healthy = await isHealthy();
  const binaryExists = existsSync(SERVER_BINARY);

  const status: ServerStatus = {
    running,
    healthy,
    pid: pid ?? undefined,
    port,
    binaryExists,
  };

  if (json) {
    outputResult(status, true, startTime);
  } else {
    console.log("");
    console.log("━━━ Team Claude Server Status ━━━");
    console.log("");
    console.log(`  Binary: ${SERVER_BINARY}`);
    console.log(`  Port: ${port}`);
    console.log("");

    if (binaryExists) {
      console.log("[OK] Binary: 설치됨");
    } else {
      console.log("[ERR] Binary: 미설치 (tc server build 실행 필요)");
    }

    if (running) {
      console.log(`[OK] Process: 실행 중 (PID: ${pid})`);
    } else {
      console.log("[ERR] Process: 중지됨");
    }

    if (healthy) {
      console.log("[OK] Health: OK");
    } else {
      console.log("[ERR] Health: 응답 없음");
    }

    console.log("");
    console.log(running && healthy ? "running" : "stopped");
  }

  if (!(running && healthy)) {
    process.exitCode = 1;
  }
}

// ============================================================================
// start 핸들러
// ============================================================================

async function handleStart(options: {
  port?: string;
  json?: boolean;
}): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;
  const port = options.port ? parseInt(options.port, 10) : getPort();

  // 이미 실행 중인지 확인
  if (isRunning() && (await isHealthy())) {
    if (json) {
      outputResult({ status: "already_running", port }, true, startTime);
    } else {
      console.log("[INFO] 서버가 이미 실행 중입니다.");
    }
    return;
  }

  // 바이너리 확인
  if (!existsSync(SERVER_BINARY)) {
    if (json) {
      console.log(
        JSON.stringify({
          success: false,
          error: {
            code: "BINARY_NOT_FOUND",
            message: `서버 바이너리가 없습니다: ${SERVER_BINARY}`,
          },
        })
      );
    } else {
      console.error(`[ERR] 서버 바이너리가 없습니다: ${SERVER_BINARY}`);
      console.error("[ERR] 'tc server build'를 먼저 실행하세요.");
    }
    process.exit(1);
  }

  ensureDir(CLAUDE_DIR);

  if (!json) {
    console.log(`[INFO] 서버 시작 중... (port: ${port})`);
  }

  // 백그라운드 실행
  const child = spawn(SERVER_BINARY, [], {
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, TEAM_CLAUDE_PORT: String(port) },
  });

  // 로그 파일에 출력 리다이렉트
  const { appendFileSync } = await import("fs");
  child.stdout?.on("data", (data) => appendFileSync(LOG_FILE, data));
  child.stderr?.on("data", (data) => appendFileSync(LOG_FILE, data));

  child.unref();
  const pid = child.pid!;
  writeFileSync(PID_FILE, String(pid));

  // 시작 대기 (최대 10초)
  let attempts = 0;
  while (attempts < 20) {
    if (await isHealthy()) {
      if (json) {
        outputResult({ status: "started", pid, port }, true, startTime);
      } else {
        console.log(`[OK] 서버 시작됨 (PID: ${pid}, Port: ${port})`);
      }
      return;
    }
    await Bun.sleep(500);
    attempts++;
  }

  if (json) {
    console.log(
      JSON.stringify({
        success: false,
        error: {
          code: "START_TIMEOUT",
          message: "서버 시작 실패 (timeout)",
        },
      })
    );
  } else {
    console.error("[ERR] 서버 시작 실패 (timeout)");
    console.error("[ERR] 로그 확인: tc server logs");
  }
  process.exit(1);
}

// ============================================================================
// stop 핸들러
// ============================================================================

async function handleStop(options: { json?: boolean }): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  if (!isRunning()) {
    if (json) {
      outputResult({ status: "not_running" }, true, startTime);
    } else {
      console.log("[INFO] 서버가 실행 중이지 않습니다.");
    }
    return;
  }

  const pid = getPid()!;

  if (!json) {
    console.log(`[INFO] 서버 중지 중... (PID: ${pid})`);
  }

  // SIGTERM 전송
  try {
    process.kill(pid, "SIGTERM");
  } catch {
    // 이미 종료됨
  }

  // 종료 대기 (최대 5초)
  let attempts = 0;
  while (attempts < 10) {
    try {
      process.kill(pid, 0);
    } catch {
      // 프로세스 종료됨
      try {
        const { unlinkSync } = await import("fs");
        unlinkSync(PID_FILE);
      } catch {}

      if (json) {
        outputResult({ status: "stopped", pid }, true, startTime);
      } else {
        console.log("[OK] 서버 중지됨");
      }
      return;
    }
    await Bun.sleep(500);
    attempts++;
  }

  // 강제 종료
  if (!json) {
    console.log("[WARN] SIGKILL 전송...");
  }

  try {
    process.kill(pid, "SIGKILL");
  } catch {}

  try {
    const { unlinkSync } = await import("fs");
    unlinkSync(PID_FILE);
  } catch {}

  if (json) {
    outputResult({ status: "killed", pid }, true, startTime);
  } else {
    console.log("[OK] 서버 강제 중지됨");
  }
}

// ============================================================================
// restart 핸들러
// ============================================================================

async function handleRestart(options: { json?: boolean }): Promise<void> {
  await handleStop({ json: false });
  await Bun.sleep(1000);
  await handleStart(options);
}

// ============================================================================
// logs 핸들러
// ============================================================================

async function handleLogs(options: {
  follow?: boolean;
  lines?: string;
}): Promise<void> {
  if (!existsSync(LOG_FILE)) {
    console.log("[INFO] 로그 파일이 없습니다.");
    return;
  }

  const lines = options.lines ? parseInt(options.lines, 10) : 100;

  if (options.follow) {
    // tail -f 실행
    const child = spawn("tail", ["-f", LOG_FILE], {
      stdio: "inherit",
    });
    child.on("error", (err) => {
      console.error(`[ERR] 로그 확인 실패: ${err.message}`);
    });
  } else {
    const result = await $`tail -${lines} ${LOG_FILE}`.quiet().nothrow();
    console.log(result.stdout.toString());
  }
}

// ============================================================================
// build 핸들러
// ============================================================================

async function handleBuild(options: { json?: boolean }): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  const serverSourceDir = getServerSourceDir();

  if (!existsSync(serverSourceDir)) {
    if (json) {
      console.log(
        JSON.stringify({
          success: false,
          error: {
            code: "SOURCE_NOT_FOUND",
            message: `서버 소스를 찾을 수 없습니다: ${serverSourceDir}`,
          },
        })
      );
    } else {
      console.error(`[ERR] 서버 소스를 찾을 수 없습니다: ${serverSourceDir}`);
    }
    process.exit(1);
  }

  ensureDir(CLAUDE_DIR);

  if (!json) {
    console.log("[INFO] 서버 빌드 중...");
  }

  try {
    // 의존성 설치 (필요한 경우)
    const nodeModulesPath = join(serverSourceDir, "node_modules");
    if (!existsSync(nodeModulesPath)) {
      if (!json) {
        console.log("[INFO] 의존성 설치 중...");
      }
      await $`bun install`.cwd(serverSourceDir).quiet();
    }

    // 빌드
    await $`bun build src/index.ts --compile --outfile ${SERVER_BINARY}`
      .cwd(serverSourceDir)
      .quiet();

    // 실행 권한 부여
    const { chmodSync } = await import("fs");
    chmodSync(SERVER_BINARY, 0o755);

    if (json) {
      outputResult(
        { status: "built", binary: SERVER_BINARY },
        true,
        startTime
      );
    } else {
      console.log(`[OK] 서버 빌드 완료: ${SERVER_BINARY}`);
    }
  } catch (error) {
    if (json) {
      console.log(
        JSON.stringify({
          success: false,
          error: {
            code: "BUILD_FAILED",
            message: error instanceof Error ? error.message : String(error),
          },
        })
      );
    } else {
      console.error("[ERR] 빌드 실패");
      console.error(error);
    }
    process.exit(1);
  }
}

// ============================================================================
// 커맨드 생성
// ============================================================================

export function createServerCommand(): Command {
  const server = new Command("server")
    .description("서버 라이프사이클 관리")
    .addHelpText(
      "after",
      `
Examples:
  tc server status          서버 상태 확인
  tc server start           서버 시작
  tc server stop            서버 중지
  tc server restart         서버 재시작
  tc server logs            최근 로그 보기
  tc server logs -f         로그 실시간 보기
  tc server build           서버 빌드
`
    );

  server
    .command("status")
    .description("서버 상태 확인")
    .option("--json", "JSON 형식으로 출력")
    .action(handleStatus);

  server
    .command("start")
    .description("서버 시작")
    .option("--port <port>", "포트 지정")
    .option("--json", "JSON 형식으로 출력")
    .action(handleStart);

  server
    .command("stop")
    .description("서버 중지")
    .option("--json", "JSON 형식으로 출력")
    .action(handleStop);

  server
    .command("restart")
    .description("서버 재시작")
    .option("--json", "JSON 형식으로 출력")
    .action(handleRestart);

  server
    .command("logs")
    .description("서버 로그 확인")
    .option("-f, --follow", "실시간 로그 보기")
    .option("--lines <n>", "표시할 줄 수", "100")
    .action(handleLogs);

  server
    .command("build")
    .description("서버 빌드")
    .option("--json", "JSON 형식으로 출력")
    .action(handleBuild);

  return server;
}
