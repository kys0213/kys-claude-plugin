/**
 * tc state - 워크플로우 상태 관리 커맨드
 */

import { Command } from "commander";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { join, dirname } from "path";
import { execSync } from "child_process";

// ============================================================================
// 상수
// ============================================================================

const STATE_FILE = ".team-claude/state/workflow.json";
const VALID_PHASES = [
  "idle",
  "setup",
  "designing",
  "checkpoints_approved",
  "delegating",
  "merging",
  "completed",
] as const;

type Phase = (typeof VALID_PHASES)[number];

// ============================================================================
// 타입 정의
// ============================================================================

interface WorkflowState {
  phase: Phase;
  sessionId?: string;
  serverRunning: boolean;
  lastUpdated: string;
}

interface CLIOutput<T> {
  success: boolean;
  data?: T;
  error?: { code: string; message: string };
  meta?: { timestamp: string; duration_ms: number };
}

// ============================================================================
// 유틸리티
// ============================================================================

function timestamp(): string {
  return new Date().toISOString();
}

function getGitRoot(): string {
  try {
    return execSync("git rev-parse --show-toplevel", {
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    }).trim();
  } catch {
    return process.cwd();
  }
}

function getStatePath(): string {
  return join(getGitRoot(), STATE_FILE);
}

function loadState(): WorkflowState {
  const statePath = getStatePath();
  if (!existsSync(statePath)) {
    return {
      phase: "idle",
      serverRunning: false,
      lastUpdated: timestamp(),
    };
  }
  try {
    return JSON.parse(readFileSync(statePath, "utf-8"));
  } catch {
    return {
      phase: "idle",
      serverRunning: false,
      lastUpdated: timestamp(),
    };
  }
}

function saveState(state: WorkflowState): void {
  const statePath = getStatePath();
  const dir = dirname(statePath);
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
  state.lastUpdated = timestamp();
  writeFileSync(statePath, JSON.stringify(state, null, 2));
}

function outputJson<T>(data: T, startTime: number): void {
  const output: CLIOutput<T> = {
    success: true,
    data,
    meta: { timestamp: timestamp(), duration_ms: Date.now() - startTime },
  };
  console.log(JSON.stringify(output, null, 2));
}

function outputError(code: string, message: string): void {
  console.log(JSON.stringify({ success: false, error: { code, message } }, null, 2));
}

// ============================================================================
// check 핸들러
// ============================================================================

async function handleCheck(options: { json?: boolean }): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;
  const state = loadState();

  if (json) {
    outputJson(state, startTime);
  } else {
    console.log("\n━━━ 워크플로우 상태 ━━━\n");
    console.log(`  Phase: ${state.phase}`);
    if (state.sessionId) {
      console.log(`  Session: ${state.sessionId}`);
    }
    console.log(`  Server: ${state.serverRunning ? "running" : "stopped"}`);
    console.log(`  Updated: ${state.lastUpdated}`);
    console.log("");
  }
}

// ============================================================================
// get 핸들러
// ============================================================================

async function handleGet(
  key: string,
  options: { json?: boolean }
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;
  const state = loadState();

  const value = (state as Record<string, unknown>)[key];

  if (json) {
    outputJson({ key, value }, startTime);
  } else {
    console.log(value ?? "");
  }
}

// ============================================================================
// transition 핸들러
// ============================================================================

async function handleTransition(
  phase: string,
  options: { json?: boolean }
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  if (!VALID_PHASES.includes(phase as Phase)) {
    if (json) {
      outputError("INVALID_PHASE", `유효하지 않은 phase: ${phase}. 가능한 값: ${VALID_PHASES.join(", ")}`);
    } else {
      console.error(`[ERR] 유효하지 않은 phase: ${phase}`);
      console.error(`가능한 값: ${VALID_PHASES.join(", ")}`);
    }
    process.exit(1);
  }

  const state = loadState();
  const oldPhase = state.phase;
  state.phase = phase as Phase;
  saveState(state);

  if (json) {
    outputJson({ oldPhase, newPhase: phase }, startTime);
  } else {
    console.log(`[OK] 상태 전이: ${oldPhase} → ${phase}`);
  }
}

// ============================================================================
// reset 핸들러
// ============================================================================

async function handleReset(options: { json?: boolean }): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  const state: WorkflowState = {
    phase: "idle",
    serverRunning: false,
    lastUpdated: timestamp(),
  };
  saveState(state);

  if (json) {
    outputJson(state, startTime);
  } else {
    console.log("[OK] 워크플로우 상태 초기화됨");
  }
}

// ============================================================================
// 커맨드 생성
// ============================================================================

export function createStateCommand(): Command {
  const state = new Command("state")
    .description("워크플로우 상태 관리")
    .addHelpText(
      "after",
      `
Examples:
  tc state check              현재 상태 표시
  tc state get phase          특정 값 조회
  tc state transition designing  상태 전이
  tc state reset              상태 초기화
`
    );

  state
    .command("check")
    .description("현재 워크플로우 상태 표시")
    .option("--json", "JSON 형식으로 출력")
    .action(handleCheck);

  state
    .command("get <key>")
    .description("특정 값 조회")
    .option("--json", "JSON 형식으로 출력")
    .action(handleGet);

  state
    .command("transition <phase>")
    .description("상태 전이")
    .option("--json", "JSON 형식으로 출력")
    .action(handleTransition);

  state
    .command("reset")
    .description("상태 초기화")
    .option("--json", "JSON 형식으로 출력")
    .action(handleReset);

  return state;
}
