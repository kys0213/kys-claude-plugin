/**
 * state 명령어 - 워크플로우 상태 관리
 */

import { Command } from "commander";
import { existsSync } from "fs";
import { readFile, writeFile, mkdir } from "fs/promises";
import { join } from "path";
import { ProjectContext } from "../lib/context";
import { log, printSection, printStatus, icon } from "../lib/utils";

interface WorkflowState {
  phase:
    | "idle"
    | "setup"
    | "designing"
    | "checkpoints_approved"
    | "delegating"
    | "merging"
    | "completed";
  serverRunning: boolean;
  currentSessionId: string | null;
  prerequisites: {
    setup: boolean;
    architect: boolean;
    checkpointsApproved: boolean;
    serverHealthy: boolean;
  };
  createdAt: string;
  updatedAt: string;
}

const PHASE_ORDER = [
  "idle",
  "setup",
  "designing",
  "checkpoints_approved",
  "delegating",
  "merging",
  "completed",
] as const;

const PHASE_ICONS: Record<string, string> = {
  idle: "⏸️",
  setup: "🔧",
  designing: "🏗️",
  checkpoints_approved: "✅",
  delegating: "🚀",
  merging: "🔀",
  completed: "🎉",
};

async function getStateFilePath(): Promise<string> {
  const ctx = await ProjectContext.getInstance();
  return join(ctx.stateDir, "workflow.json");
}

async function ensureStateDirExists(): Promise<void> {
  const ctx = await ProjectContext.getInstance();
  if (!existsSync(ctx.stateDir)) {
    await mkdir(ctx.stateDir, { recursive: true });
  }
}

async function readState(): Promise<WorkflowState | null> {
  const statePath = await getStateFilePath();
  if (!existsSync(statePath)) {
    return null;
  }
  const content = await readFile(statePath, "utf-8");
  return JSON.parse(content);
}

async function writeState(state: WorkflowState): Promise<void> {
  await ensureStateDirExists();
  const statePath = await getStateFilePath();
  state.updatedAt = new Date().toISOString();
  await writeFile(statePath, JSON.stringify(state, null, 2), "utf-8");
}

function createDefaultState(): WorkflowState {
  const now = new Date().toISOString();
  return {
    phase: "idle",
    serverRunning: false,
    currentSessionId: null,
    prerequisites: {
      setup: false,
      architect: false,
      checkpointsApproved: false,
      serverHealthy: false,
    },
    createdAt: now,
    updatedAt: now,
  };
}

function getPhaseIndex(phase: WorkflowState["phase"]): number {
  return PHASE_ORDER.indexOf(phase);
}

// ============================================================================
// init - 상태 파일 초기화
// ============================================================================

async function initCommand(): Promise<void> {
  const statePath = await getStateFilePath();

  if (existsSync(statePath)) {
    log.warn("상태 파일이 이미 존재합니다.");
    log.warn("덮어쓰려면 'tc state reset'을 먼저 실행하세요.");
    return;
  }

  const state = createDefaultState();
  await writeState(state);

  printSection("상태 파일 초기화");
  log.ok(`생성됨: ${statePath}`);
}

// ============================================================================
// check - 현재 상태 표시
// ============================================================================

async function checkCommand(): Promise<void> {
  const state = await readState();

  if (!state) {
    log.err("상태 파일이 없습니다.");
    log.err("'tc state init'을 먼저 실행하세요.");
    process.exit(1);
  }

  console.log();
  printSection("Team Claude Workflow State");
  console.log();

  const phaseIcon = PHASE_ICONS[state.phase] || "❓";
  console.log(`  Phase: ${phaseIcon} ${state.phase}`);
  console.log(`  Session: ${state.currentSessionId || "없음"}`);
  console.log(
    `  Server: ${state.serverRunning ? "🟢 실행 중" : "🔴 중지"}`
  );
  console.log();

  printSection("Prerequisites");
  console.log();

  const prereqs = state.prerequisites;
  console.log(`  ${prereqs.setup ? "✅" : "⬜"} setup`);
  console.log(`  ${prereqs.architect ? "✅" : "⬜"} architect`);
  console.log(`  ${prereqs.checkpointsApproved ? "✅" : "⬜"} checkpointsApproved`);
  console.log(`  ${prereqs.serverHealthy ? "✅" : "⬜"} serverHealthy`);
  console.log();
}

// ============================================================================
// get - 특정 값 조회
// ============================================================================

async function getCommand(key: string): Promise<void> {
  const state = await readState();

  if (!state) {
    log.err("상태 파일이 없습니다.");
    process.exit(1);
  }

  // 중첩 키 지원 (e.g., prerequisites.setup)
  const keys = key.split(".");
  let value: unknown = state;

  for (const k of keys) {
    if (value && typeof value === "object" && k in (value as object)) {
      value = (value as Record<string, unknown>)[k];
    } else {
      log.err(`키를 찾을 수 없습니다: ${key}`);
      process.exit(1);
    }
  }

  if (typeof value === "object") {
    console.log(JSON.stringify(value, null, 2));
  } else {
    console.log(value);
  }
}

// ============================================================================
// require - 필요한 단계가 아니면 실패
// ============================================================================

async function requireCommand(requiredPhase: string): Promise<void> {
  const state = await readState();

  if (!state) {
    log.err("상태 파일이 없습니다.");
    log.err("'/team-claude:setup'을 먼저 실행하세요.");
    process.exit(1);
  }

  if (!PHASE_ORDER.includes(requiredPhase as WorkflowState["phase"])) {
    log.err(`유효하지 않은 phase: ${requiredPhase}`);
    log.info(`유효한 phases: ${PHASE_ORDER.join(", ")}`);
    process.exit(1);
  }

  const requiredIndex = getPhaseIndex(requiredPhase as WorkflowState["phase"]);
  const currentIndex = getPhaseIndex(state.phase);

  if (currentIndex < requiredIndex) {
    log.err(`필요한 단계: ${requiredPhase}`);
    log.err(`현재 단계: ${state.phase}`);
    console.log();

    // 다음 단계 안내
    switch (requiredPhase) {
      case "setup":
        log.err("'/team-claude:setup'을 먼저 실행하세요.");
        break;
      case "designing":
        log.err("'/team-claude:architect'로 설계를 시작하세요.");
        break;
      case "checkpoints_approved":
        log.err("'/team-claude:architect'에서 Checkpoint를 승인하세요.");
        break;
      case "delegating":
        log.err("'/team-claude:delegate'로 구현을 위임하세요.");
        break;
      case "merging":
        log.err("'/team-claude:merge'로 병합을 시작하세요.");
        break;
    }

    process.exit(1);
  }

  log.ok(`Phase 확인됨: ${state.phase} >= ${requiredPhase}`);
}

// ============================================================================
// transition - 상태 전이
// ============================================================================

async function transitionCommand(toPhase: string): Promise<void> {
  const state = await readState();

  if (!state) {
    log.err("상태 파일이 없습니다.");
    log.err("'tc state init'을 먼저 실행하세요.");
    process.exit(1);
  }

  if (!PHASE_ORDER.includes(toPhase as WorkflowState["phase"])) {
    log.err(`유효하지 않은 phase: ${toPhase}`);
    log.info(`유효한 phases: ${PHASE_ORDER.join(", ")}`);
    process.exit(1);
  }

  const targetPhase = toPhase as WorkflowState["phase"];
  const fromPhase = state.phase;

  state.phase = targetPhase;

  // prerequisites 업데이트
  switch (targetPhase) {
    case "setup":
      state.prerequisites.setup = true;
      break;
    case "designing":
      state.prerequisites.architect = true;
      break;
    case "checkpoints_approved":
      state.prerequisites.checkpointsApproved = true;
      break;
  }

  await writeState(state);

  log.ok(`상태 전이: ${fromPhase} → ${targetPhase}`);
}

// ============================================================================
// set-session - 현재 세션 ID 설정
// ============================================================================

async function setSessionCommand(sessionId: string): Promise<void> {
  const state = await readState();

  if (!state) {
    log.err("상태 파일이 없습니다.");
    process.exit(1);
  }

  state.currentSessionId = sessionId;
  await writeState(state);

  log.ok(`현재 세션 설정됨: ${sessionId}`);
}

// ============================================================================
// set-server - 서버 실행 상태 설정
// ============================================================================

async function setServerCommand(running: string): Promise<void> {
  if (running !== "true" && running !== "false") {
    log.err("true 또는 false를 지정하세요.");
    process.exit(1);
  }

  const state = await readState();

  if (!state) {
    log.err("상태 파일이 없습니다.");
    process.exit(1);
  }

  const isRunning = running === "true";
  state.serverRunning = isRunning;
  state.prerequisites.serverHealthy = isRunning;

  await writeState(state);

  log.ok(`서버 상태 설정됨: ${running}`);
}

// ============================================================================
// reset - 상태 초기화
// ============================================================================

async function resetCommand(): Promise<void> {
  const statePath = await getStateFilePath();

  const state = createDefaultState();
  await writeState(state);

  log.ok("상태 파일 초기화됨");
}

// ============================================================================
// 명령어 생성
// ============================================================================

export function createStateCommand(): Command {
  const cmd = new Command("state").description("워크플로우 상태 관리");

  cmd
    .command("init")
    .description("상태 파일 초기화")
    .action(async () => {
      await initCommand();
    });

  cmd
    .command("check")
    .description("현재 워크플로우 상태 표시")
    .action(async () => {
      await checkCommand();
    });

  cmd
    .command("get")
    .description("특정 값 조회")
    .argument("<key>", "조회할 키 (예: phase, prerequisites.setup)")
    .action(async (key: string) => {
      await getCommand(key);
    });

  cmd
    .command("require")
    .description("필요한 단계가 아니면 실패")
    .argument("<phase>", "필요한 phase")
    .action(async (phase: string) => {
      await requireCommand(phase);
    });

  cmd
    .command("transition")
    .description("상태 전이")
    .argument("<to>", "전이할 phase")
    .action(async (to: string) => {
      await transitionCommand(to);
    });

  cmd
    .command("set-session")
    .description("현재 세션 ID 설정")
    .argument("<id>", "세션 ID")
    .action(async (id: string) => {
      await setSessionCommand(id);
    });

  cmd
    .command("set-server")
    .description("서버 실행 상태 설정")
    .argument("<running>", "true 또는 false")
    .action(async (running: string) => {
      await setServerCommand(running);
    });

  cmd
    .command("reset")
    .description("상태 파일 초기화")
    .action(async () => {
      await resetCommand();
    });

  return cmd;
}
