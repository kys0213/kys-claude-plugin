/**
 * tc flow - 통합 워크플로우 오케스트레이터
 */

import { Command } from "commander";
import chalk from "chalk";
import { join } from "path";
import {
  getSessionsDir,
  getStateDir,
  ensureDir,
  generateId,
  timestamp,
  parseMagicKeyword,
  readJsonFile,
  writeJsonFile,
  type ImplStrategy,
} from "../lib/common";
import { log, printSection, printKV } from "../lib/utils";

// ============================================================================
// 타입 정의
// ============================================================================

interface FlowState {
  sessionId: string;
  mode: string;
  implStrategy: ImplStrategy;
  requirement: string;
  status: string;
  currentPhase: string;
  phases: {
    spec: PhaseState;
    impl: PhaseState;
    merge: PhaseState;
  };
  escalations: Escalation[];
  createdAt: string;
  updatedAt: string;
}

interface PhaseState {
  status: string;
  strategy?: string;
  iterations: number;
  startedAt: string | null;
  completedAt: string | null;
}

interface Escalation {
  phase: string;
  reason: string;
  timestamp: string;
}

interface WorkflowState {
  currentSession: string | null;
  phase: string;
}

// ============================================================================
// 헬퍼 함수
// ============================================================================

function getFlowStatePath(sessionId: string): string {
  return join(getSessionsDir(), sessionId, "flow-state.json");
}

function getWorkflowStatePath(): string {
  return join(getStateDir(), "workflow.json");
}

function initFlowState(
  sessionId: string,
  mode: string,
  requirement: string,
  implStrategy: ImplStrategy
): FlowState {
  const now = timestamp();

  const state: FlowState = {
    sessionId,
    mode,
    implStrategy,
    requirement,
    status: "started",
    currentPhase: "spec",
    phases: {
      spec: {
        status: "pending",
        iterations: 0,
        startedAt: null,
        completedAt: null,
      },
      impl: {
        status: "pending",
        strategy: implStrategy,
        iterations: 0,
        startedAt: null,
        completedAt: null,
      },
      merge: {
        status: "pending",
        startedAt: null,
        completedAt: null,
      },
    },
    escalations: [],
    createdAt: now,
    updatedAt: now,
  };

  const flowPath = getFlowStatePath(sessionId);
  ensureDir(join(getSessionsDir(), sessionId));
  writeJsonFile(flowPath, state);

  return state;
}

function updateWorkflowState(sessionId: string): void {
  const statePath = getWorkflowStatePath();
  ensureDir(getStateDir());

  const state: WorkflowState = {
    currentSession: sessionId,
    phase: "flow_started",
  };

  writeJsonFile(statePath, state);
}

// ============================================================================
// 명령어: start
// ============================================================================

async function cmdStart(
  requirement: string,
  options: {
    mode?: string;
    phase?: string;
    implStrategy?: string;
    dryRun?: boolean;
  }
): Promise<void> {
  let mode = options.mode || "assisted";
  let implStrategy: ImplStrategy = (options.implStrategy as ImplStrategy) || "psm";
  let cleanRequirement = requirement;

  // Magic Keyword 처리
  const parsed = parseMagicKeyword(requirement);
  if (parsed.keyword) {
    if (parsed.mode) {
      mode = parsed.mode;
    }
    if (parsed.implStrategy) {
      implStrategy = parsed.implStrategy;
    }
    cleanRequirement = parsed.cleanMessage;
    log.info(`Magic Keyword 감지: mode=${mode}, impl_strategy=${implStrategy}`);
  }

  if (!cleanRequirement.trim()) {
    log.err("요구사항을 입력하세요.");
    log.err('사용법: tc flow start "요구사항" --mode <mode>');
    process.exit(1);
  }

  // 모드 검증
  const validModes = [
    "autopilot",
    "assisted",
    "manual",
    "spec",
    "impl",
    "review",
    "parallel",
    "ralph",
  ];
  if (!validModes.includes(mode)) {
    log.err(`유효하지 않은 모드: ${mode}`);
    log.err(`사용 가능: ${validModes.join(", ")}`);
    process.exit(1);
  }

  // 구현 전략 검증
  const validStrategies: ImplStrategy[] = ["psm", "swarm", "sequential"];
  if (!validStrategies.includes(implStrategy)) {
    log.err(`유효하지 않은 구현 전략: ${implStrategy}`);
    log.err(`사용 가능: ${validStrategies.join(", ")}`);
    process.exit(1);
  }

  console.log();
  console.log(chalk.bold("🚀 Automated Workflow 시작"));
  console.log();
  printKV("모드", mode);
  printKV("구현 전략", implStrategy);
  printKV("요구사항", cleanRequirement);

  if (options.phase) {
    printKV("단계", options.phase);
  }

  if (options.dryRun) {
    console.log(chalk.yellow("  (Dry Run - 시뮬레이션만)"));
    console.log();
    log.info("Dry run 모드입니다. 실제 실행하지 않습니다.");
    return;
  }

  console.log();

  // 세션 생성
  const sessionId = generateId();
  log.ok(`세션 생성됨: ${sessionId}`);

  // Flow 상태 초기화
  initFlowState(sessionId, mode, cleanRequirement, implStrategy);

  // 워크플로우 상태 업데이트
  updateWorkflowState(sessionId);

  console.log();
  console.log("━".repeat(70));
  console.log();

  // 구현 전략 안내
  console.log(chalk.bold(`🔧 구현 전략: ${implStrategy.toUpperCase()}`));
  switch (implStrategy) {
    case "psm":
      console.log("   → git worktree 기반 격리 환경에서 병렬 실행");
      break;
    case "swarm":
      console.log("   → 내부 서브에이전트를 통한 병렬 실행 (같은 코드베이스)");
      break;
    case "sequential":
      console.log("   → 순차적으로 하나씩 실행");
      break;
  }
  console.log();

  // 모드에 따른 안내
  switch (mode) {
    case "autopilot":
      console.log(chalk.bold("📋 AUTOPILOT 모드: 전체 자동화"));
      console.log();
      console.log("  1. 스펙 자동 설계 + 자동 리뷰");
      console.log("  2. 자동 구현 (RALPH loop)");
      console.log("  3. 자동 코드 리뷰");
      console.log("  4. 자동 머지");
      console.log();
      console.log("  에스컬레이션 시에만 사용자 개입을 요청합니다.");
      break;
    case "assisted":
      console.log(chalk.bold("📋 ASSISTED 모드: 단계별 확인"));
      console.log();
      console.log("  1. 스펙 자동 설계 + 자동 리뷰 → 승인 요청");
      console.log("  2. 자동 구현 + 자동 리뷰 → 승인 요청");
      console.log("  3. 머지 → 확인 요청");
      break;
    case "spec":
      console.log(chalk.bold("📋 SPEC 모드: 스펙 설계만"));
      console.log();
      console.log("  스펙 설계 + 자동 리뷰까지 진행합니다.");
      break;
    case "impl":
      console.log(chalk.bold("📋 IMPL 모드: 구현만"));
      console.log();
      console.log("  기존 스펙을 기반으로 구현을 진행합니다.");
      break;
    default:
      console.log(chalk.bold(`📋 ${mode.toUpperCase()} 모드`));
      break;
  }

  console.log();
  console.log("━".repeat(70));
  console.log();

  // 결과 출력
  printKV("세션 ID", sessionId);
  console.log();
  console.log("  다음 단계:");
  console.log(`    /team-claude:architect "${cleanRequirement}"`);
  console.log();
  console.log("  또는 flow 재개:");
  console.log(`    tc flow resume ${sessionId}`);
  console.log();

  // JSON 출력
  console.log("---");
  console.log(
    JSON.stringify(
      {
        sessionId,
        mode,
        implStrategy,
        status: "started",
      },
      null,
      2
    )
  );
}

// ============================================================================
// 명령어: resume
// ============================================================================

async function cmdResume(sessionId: string): Promise<void> {
  const flowPath = getFlowStatePath(sessionId);
  const state = readJsonFile<FlowState>(flowPath);

  if (!state) {
    log.err(`Flow 상태 파일이 없습니다: ${sessionId}`);
    process.exit(1);
  }

  console.log();
  log.ok(`워크플로우 재개: ${sessionId}`);
  console.log();
  printKV("모드", state.mode);
  printKV("현재 단계", state.currentPhase);
  printKV("상태", state.status);
  console.log();

  // 단계별 안내
  switch (state.currentPhase) {
    case "spec":
      console.log("  다음 단계:");
      console.log(`    /team-claude:architect --resume ${sessionId}`);
      break;
    case "impl":
      console.log("  다음 단계:");
      console.log(`    /team-claude:delegate --session ${sessionId} --all`);
      break;
    case "merge":
      console.log("  다음 단계:");
      console.log(`    /team-claude:merge --session ${sessionId}`);
      break;
  }
  console.log();
}

// ============================================================================
// 명령어: status
// ============================================================================

async function cmdStatus(sessionId?: string): Promise<void> {
  let targetSessionId = sessionId;

  if (!targetSessionId) {
    // 현재 활성 세션
    const workflowState = readJsonFile<WorkflowState>(getWorkflowStatePath());
    targetSessionId = workflowState?.currentSession || undefined;

    if (!targetSessionId) {
      log.err("활성 세션이 없습니다.");
      process.exit(1);
    }
  }

  const flowPath = getFlowStatePath(targetSessionId);
  const state = readJsonFile<FlowState>(flowPath);

  if (!state) {
    log.err(`Flow 상태 파일이 없습니다: ${targetSessionId}`);
    process.exit(1);
  }

  printSection(`Flow Status: ${targetSessionId}`);

  printKV("모드", state.mode);
  printKV("상태", state.status);
  printKV("현재 단계", state.currentPhase);
  printKV("요구사항", state.requirement);
  console.log();

  printSection("Phases");

  for (const [phase, phaseState] of Object.entries(state.phases)) {
    let icon = "❓";
    switch (phaseState.status) {
      case "complete":
        icon = "✅";
        break;
      case "in_progress":
        icon = "🔄";
        break;
      case "pending":
        icon = "⏸️";
        break;
      case "error":
        icon = "❌";
        break;
    }

    console.log(`  ${icon} ${phase}: ${phaseState.status}`);
    if (phaseState.iterations > 0) {
      console.log(`      반복: ${phaseState.iterations}회`);
    }
  }

  console.log();

  // 에스컬레이션 정보
  if (state.escalations.length > 0) {
    printSection("Escalations");
    for (const esc of state.escalations) {
      console.log(`  ⚠️ ${esc.phase}: ${esc.reason}`);
    }
    console.log();
  }
}

// ============================================================================
// 명령어: parse-keyword
// ============================================================================

function cmdParseKeyword(message: string): void {
  const result = parseMagicKeyword(message);

  console.log(`keyword=${result.keyword || ""}`);
  console.log(`mode=${result.mode || ""}`);
  console.log(`implStrategy=${result.implStrategy || ""}`);
  console.log(`message=${result.cleanMessage}`);
  console.log(`matched=${result.keyword ? "true" : "false"}`);
}

// ============================================================================
// 명령어 등록
// ============================================================================

export function createFlowCommand(): Command {
  const flow = new Command("flow").description(
    "통합 워크플로우 오케스트레이터"
  );

  flow
    .command("start <requirement>")
    .description("새 워크플로우 시작")
    .option("--mode <mode>", "실행 모드 (autopilot|assisted|manual)", "assisted")
    .option("--phase <phase>", "특정 단계만 (spec|impl|merge)")
    .option(
      "--impl-strategy <strategy>",
      "구현 전략 (psm|swarm|sequential)",
      "psm"
    )
    .option("--dry-run", "시뮬레이션만")
    .action(cmdStart);

  flow
    .command("resume <session-id>")
    .description("기존 워크플로우 재개")
    .action(cmdResume);

  flow
    .command("status [session-id]")
    .description("워크플로우 상태 확인")
    .action(cmdStatus);

  flow
    .command("parse-keyword <message>")
    .description("Magic Keyword 파싱")
    .action(cmdParseKeyword);

  return flow;
}
