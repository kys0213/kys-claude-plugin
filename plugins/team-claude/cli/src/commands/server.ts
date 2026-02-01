/**
 * server 명령어 - Team Claude MCP 서버 관리
 */

import { Command } from "commander";
import { log, printSection, printStatus, icon } from "../lib/utils";
import { ProjectContext } from "../lib/context";
import { existsSync } from "fs";
import { join, dirname } from "path";
import { homedir } from "os";

// Cross-platform: os.homedir() 사용 (Windows 호환)
const SERVER_BINARY = join(homedir(), ".claude", "team-claude-server");
const PID_FILE = join(homedir(), ".claude", "team-claude-server.pid");
const LOG_FILE = join(homedir(), ".claude", "team-claude-server.log");
const DEFAULT_PORT = 7890;

interface ServerStatus {
  binaryExists: boolean;
  processRunning: boolean;
  healthy: boolean;
  pid?: number;
  port: number;
}

async function getPort(): Promise<number> {
  try {
    await ProjectContext.getInstance();
    // TODO: Read from team-claude.yaml config
    return DEFAULT_PORT;
  } catch {
    return DEFAULT_PORT;
  }
}

async function getPid(): Promise<number | null> {
  try {
    const file = Bun.file(PID_FILE);
    if (!(await file.exists())) return null;
    const content = await file.text();
    const pid = parseInt(content.trim(), 10);
    return isNaN(pid) ? null : pid;
  } catch {
    return null;
  }
}

async function isProcessRunning(pid: number): Promise<boolean> {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

async function isHealthy(port: number): Promise<boolean> {
  try {
    const response = await fetch(`http://localhost:${port}/health`, {
      signal: AbortSignal.timeout(2000),
    });
    return response.ok;
  } catch {
    return false;
  }
}

async function getServerStatus(): Promise<ServerStatus> {
  const port = await getPort();
  const binaryExists = existsSync(SERVER_BINARY);
  const pid = await getPid();
  const processRunning = pid ? await isProcessRunning(pid) : false;
  const healthy = processRunning ? await isHealthy(port) : false;

  return {
    binaryExists,
    processRunning,
    healthy,
    pid: pid || undefined,
    port,
  };
}

async function getServerSourceDir(): Promise<string> {
  // Find server directory relative to CLI
  const cliDir = dirname(dirname(dirname(__dirname)));
  return join(cliDir, "server");
}

async function startServer(): Promise<void> {
  const status = await getServerStatus();

  if (!status.binaryExists) {
    log.err("서버 바이너리가 없습니다. 'tc server install'을 먼저 실행하세요.");
    process.exit(1);
  }

  if (status.processRunning && status.healthy) {
    log.info("서버가 이미 실행 중입니다.");
    return;
  }

  if (status.processRunning && !status.healthy) {
    log.warn("서버 프로세스가 있지만 응답이 없습니다. 재시작합니다...");
    await stopServer();
    await Bun.sleep(1000);
  }

  printSection("Team Claude 서버 시작");

  // 로그 파일에 append 모드로 열기
  const logFileHandle = Bun.file(LOG_FILE);

  const proc = Bun.spawn([SERVER_BINARY], {
    stdout: logFileHandle,
    stderr: logFileHandle,
    stdin: "ignore",
    env: {
      ...process.env,
      TEAM_CLAUDE_PORT: String(status.port),
    },
  });

  // PID 저장
  await Bun.write(PID_FILE, String(proc.pid));

  // 프로세스 분리
  proc.unref();

  // Health check 대기
  log.info(`서버 시작 중... (포트: ${status.port})`);
  for (let i = 0; i < 20; i++) {
    await Bun.sleep(500);
    if (await isHealthy(status.port)) {
      printStatus("서버 시작됨", true);
      log.info(`PID: ${proc.pid}`);
      log.info(`로그: ${LOG_FILE}`);
      return;
    }
  }

  log.err("서버 시작 실패 (타임아웃)");
  log.err(`로그 확인: tc server logs`);
  process.exit(1);
}

async function stopServer(): Promise<void> {
  const pid = await getPid();

  if (!pid) {
    log.info("PID 파일이 없습니다. 서버가 실행 중이지 않습니다.");
    return;
  }

  if (!(await isProcessRunning(pid))) {
    log.info("서버 프로세스가 실행 중이지 않습니다. PID 파일 정리 중...");
    await Bun.write(PID_FILE, "");
    return;
  }

  printSection("Team Claude 서버 중지");
  log.info(`PID ${pid}에 SIGTERM 전송 중...`);

  try {
    process.kill(pid, "SIGTERM");

    // 최대 5초 대기
    for (let i = 0; i < 10; i++) {
      await Bun.sleep(500);
      if (!(await isProcessRunning(pid))) {
        printStatus("서버 정상 중지됨", true);
        await Bun.write(PID_FILE, "");
        return;
      }
    }

    // 강제 종료
    log.warn("정상 종료 타임아웃. SIGKILL 전송 중...");
    process.kill(pid, "SIGKILL");
    await Bun.sleep(500);

    if (!(await isProcessRunning(pid))) {
      printStatus("서버 강제 종료됨", true);
      await Bun.write(PID_FILE, "");
    } else {
      log.err("서버 중지 실패");
      process.exit(1);
    }
  } catch (error) {
    log.err(`서버 중지 실패: ${error}`);
    process.exit(1);
  }
}

async function restartServer(): Promise<void> {
  await stopServer();
  await Bun.sleep(1000);
  await startServer();
}

async function ensureServer(): Promise<void> {
  const status = await getServerStatus();

  if (status.healthy) {
    log.info(`서버가 이미 healthy 상태입니다 (PID: ${status.pid})`);
    console.log("already_running");
    return;
  }

  if (status.processRunning) {
    log.warn("서버가 실행 중이지만 unhealthy 상태입니다. 재시작합니다...");
    await restartServer();
    return;
  }

  log.info("서버가 실행 중이지 않습니다. 시작합니다...");
  await startServer();
  console.log("started");
}

async function installServer(): Promise<void> {
  printSection("Team Claude 서버 설치");

  const serverDir = await getServerSourceDir();

  if (!existsSync(serverDir)) {
    log.err(`서버 소스를 찾을 수 없습니다: ${serverDir}`);
    process.exit(1);
  }

  log.info("의존성 설치 중...");
  const install = Bun.spawn(["bun", "install"], {
    cwd: serverDir,
    stdout: "inherit",
    stderr: "inherit",
  });
  const installCode = await install.exited;

  if (installCode !== 0) {
    log.err("의존성 설치 실패");
    process.exit(1);
  }

  printStatus("의존성 설치 완료", true);
  await buildServer();
}

async function buildServer(): Promise<void> {
  printSection("Team Claude 서버 빌드");

  const serverDir = await getServerSourceDir();

  if (!existsSync(serverDir)) {
    log.err(`서버 소스를 찾을 수 없습니다: ${serverDir}`);
    process.exit(1);
  }

  log.info("서버 빌드 중...");
  const build = Bun.spawn(
    ["bun", "build", "src/index.ts", "--compile", "--outfile", SERVER_BINARY],
    {
      cwd: serverDir,
      stdout: "inherit",
      stderr: "inherit",
    }
  );
  const buildCode = await build.exited;

  if (buildCode !== 0) {
    log.err("서버 빌드 실패");
    process.exit(1);
  }

  printStatus("서버 빌드 완료", true);
  log.info(`바이너리: ${SERVER_BINARY}`);
}

async function showLogs(follow: boolean): Promise<void> {
  if (!existsSync(LOG_FILE)) {
    log.err(`로그 파일이 없습니다: ${LOG_FILE}`);
    process.exit(1);
  }

  if (follow) {
    log.info(`로그 추적 중 (Ctrl+C로 종료)...`);
    const tail = Bun.spawn(["tail", "-f", LOG_FILE], {
      stdout: "inherit",
      stderr: "inherit",
    });
    await tail.exited;
  } else {
    const content = await Bun.file(LOG_FILE).text();
    console.log(content);
  }
}

async function showStatus(): Promise<void> {
  const status = await getServerStatus();

  printSection("Team Claude 서버 상태");

  console.log(`  바이너리: ${SERVER_BINARY}`);
  console.log(`  포트: ${status.port}`);
  console.log();

  if (status.binaryExists) {
    printStatus("Binary", true);
  } else {
    printStatus("Binary", false, "미설치 (tc server install 필요)");
  }

  if (status.processRunning) {
    printStatus(`Process (PID: ${status.pid})`, true);
  } else {
    printStatus("Process", false, "중지됨");
  }

  if (status.healthy) {
    printStatus("Health", true);
  } else {
    printStatus("Health", false, "응답 없음");
  }

  console.log();

  if (status.healthy) {
    console.log(`${icon.check} 서버가 정상 실행 중입니다.`);
    console.log("running");
  } else if (status.processRunning) {
    console.log(`${icon.cross} 서버가 실행 중이지만 응답하지 않습니다.`);
    console.log("unhealthy");
  } else if (status.binaryExists) {
    console.log(`${icon.warn} 서버가 중지되어 있습니다.`);
    console.log("stopped");
  } else {
    console.log(`${icon.cross} 서버가 설치되어 있지 않습니다.`);
    console.log("not_installed");
  }
}

export function createServerCommand(): Command {
  const cmd = new Command("server").description("Team Claude MCP 서버 관리");

  cmd
    .command("status")
    .description("서버 상태 확인")
    .action(async () => {
      await showStatus();
    });

  cmd
    .command("start")
    .description("서버 백그라운드 시작")
    .action(async () => {
      await startServer();
    });

  cmd
    .command("stop")
    .description("서버 중지")
    .action(async () => {
      await stopServer();
    });

  cmd
    .command("restart")
    .description("서버 재시작")
    .action(async () => {
      await restartServer();
    });

  cmd
    .command("ensure")
    .description("서버 실행 확인 (미실행 시 시작)")
    .action(async () => {
      await ensureServer();
    });

  cmd
    .command("install")
    .description("의존성 설치 및 서버 빌드")
    .action(async () => {
      await installServer();
    });

  cmd
    .command("build")
    .description("서버 빌드만 실행")
    .action(async () => {
      await buildServer();
    });

  cmd
    .command("logs")
    .description("서버 로그 확인")
    .option("-f, --follow", "로그 추적")
    .action(async (opts) => {
      await showLogs(opts.follow || false);
    });

  return cmd;
}
