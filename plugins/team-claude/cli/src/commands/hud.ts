/**
 * tc hud - HUD (Heads-Up Display)
 * 워크플로우 상태를 statusline에 표시
 */

import { Command } from "commander";
import { join } from "path";
import { existsSync } from "fs";
import {
  getProjectDataDir,
  getSessionsDir,
  getStateDir,
  readJsonFile,
  progressBar,
  formatDuration,
} from "../lib/common";

// ============================================================================
// 타입 정의
// ============================================================================

interface FlowState {
  sessionId: string;
  mode: string;
  implStrategy: string;
  currentPhase: string;
  status: string;
  phases: {
    [key: string]: {
      status: string;
      iterations?: number;
    };
  };
  createdAt: string;
}

interface WorkflowState {
  currentSession: string | null;
}

interface PsmIndex {
  sessions: Array<{
    name: string;
    status: string;
    progress: string;
  }>;
}

// ============================================================================
// 아이콘
// ============================================================================

const ICONS: Record<string, string> = {
  autopilot: "🚀",
  assisted: "👤",
  manual: "✋",
  spec: "📋",
  impl: "🔧",
  merge: "🔀",
  psm: "🌳",
  swarm: "🐝",
  review: "🔍",
  ralph: "🔄",
  pass: "✅",
  fail: "❌",
  progress: "🔄",
  pending: "⏸️",
  time: "⏱️",
};

const SEPARATOR = " │ ";

// ============================================================================
// 헬퍼 함수
// ============================================================================

function getFlowState(): FlowState | null {
  const stateDir = getStateDir();
  const workflowPath = join(stateDir, "workflow.json");

  const workflow = readJsonFile<WorkflowState>(workflowPath);
  if (!workflow?.currentSession) {
    return null;
  }

  const flowPath = join(
    getSessionsDir(),
    workflow.currentSession,
    "flow-state.json"
  );
  return readJsonFile<FlowState>(flowPath);
}

function getPsmState(): PsmIndex | null {
  const psmPath = join(getProjectDataDir(), "psm-index.json");
  return readJsonFile<PsmIndex>(psmPath);
}

// ============================================================================
// 렌더링 함수
// ============================================================================

function renderMode(state: FlowState): string {
  const icon = ICONS[state.mode] || "";
  const shortMode = state.mode.substring(0, 4);
  return `${icon} ${shortMode}`;
}

function renderPhase(state: FlowState): string {
  const icon = ICONS[state.currentPhase] || "";
  const phaseState = state.phases[state.currentPhase];

  let percent = 0;
  switch (phaseState?.status) {
    case "pending":
      percent = 0;
      break;
    case "in_progress":
      percent = 50;
      break;
    case "complete":
      percent = 100;
      break;
  }

  const bar = progressBar(percent, 8);
  return `${icon} ${state.currentPhase} ${bar} ${percent}%`;
}

function renderImplStrategy(state: FlowState): string {
  switch (state.implStrategy) {
    case "psm":
      return ICONS.psm;
    case "swarm":
      return ICONS.swarm;
    default:
      return "";
  }
}

function renderPsm(psmState: PsmIndex): string {
  const total = psmState.sessions.length;
  const complete = psmState.sessions.filter((s) => s.status === "complete")
    .length;

  if (total > 0) {
    return `${ICONS.psm} ${complete}/${total}`;
  }
  return "";
}

function renderReview(state: FlowState): string {
  const phaseState = state.phases[state.currentPhase];
  const iterations = phaseState?.iterations || 0;

  if (iterations > 0) {
    return `${ICONS.review} ${iterations}/5`;
  }
  return "";
}

function renderDuration(state: FlowState): string {
  if (!state.createdAt) {
    return "";
  }

  try {
    const startTs = new Date(state.createdAt).getTime();
    const nowTs = Date.now();
    const elapsed = Math.floor((nowTs - startTs) / 1000);

    return `${ICONS.time} ${formatDuration(elapsed)}`;
  } catch {
    return "";
  }
}

// ============================================================================
// 메인 출력
// ============================================================================

function generateHud(): string {
  const parts: string[] = [];

  const flowState = getFlowState();
  const psmState = getPsmState();

  // Flow가 없으면 빈 출력
  if (!flowState && (!psmState || psmState.sessions.length === 0)) {
    return "";
  }

  if (flowState) {
    // 모드
    const modeOutput = renderMode(flowState);
    if (modeOutput) {
      parts.push(modeOutput);
    }

    // 단계
    const phaseOutput = renderPhase(flowState);
    if (phaseOutput) {
      parts.push(phaseOutput);
    }

    // 구현 전략
    const strategyOutput = renderImplStrategy(flowState);
    if (strategyOutput) {
      parts.push(strategyOutput);
    }

    // 리뷰 상태
    const reviewOutput = renderReview(flowState);
    if (reviewOutput) {
      parts.push(reviewOutput);
    }

    // 경과 시간
    const durationOutput = renderDuration(flowState);
    if (durationOutput) {
      parts.push(durationOutput);
    }
  }

  // PSM 상태
  if (psmState && psmState.sessions.length > 0) {
    const psmOutput = renderPsm(psmState);
    if (psmOutput) {
      parts.push(psmOutput);
    }
  }

  return parts.join(SEPARATOR);
}

// ============================================================================
// 명령어
// ============================================================================

async function cmdOutput(): Promise<void> {
  const output = generateHud();
  if (output) {
    console.log(output);
  }
}

async function cmdSetup(): Promise<void> {
  console.log(`
━━━ Team Claude HUD Setup ━━━

HUD는 Claude Code의 statusline에 워크플로우 상태를 표시합니다.

설치 방법:

1. 스크립트 복사:
   cp plugins/team-claude/scripts/tc-hud.sh ~/.claude/tc-hud.sh
   chmod +x ~/.claude/tc-hud.sh

2. 또는 TypeScript 버전 사용:
   tc hud output

3. Claude Code 설정 (~/.claude/settings.json):
   {
     "statusLine": {
       "type": "command",
       "command": "tc hud output",
       "padding": 0
     }
   }

4. 기존 statusline과 통합:
   ~/.claude/statusline.sh에서:

   #!/bin/bash
   existing=\$(your_existing_statusline)
   tc_hud=\$(tc hud output 2>/dev/null)
   echo "\${existing} │ \${tc_hud}"
`);
}

async function cmdReset(): Promise<void> {
  console.log("HUD 설정이 초기화되었습니다.");
  console.log("다시 설정하려면: tc hud setup");
}

// ============================================================================
// 명령어 등록
// ============================================================================

export function createHudCommand(): Command {
  const hud = new Command("hud").description(
    "HUD (Heads-Up Display) - statusline 워크플로우 상태 표시"
  );

  hud
    .command("output")
    .description("HUD 출력 생성 (statusline용)")
    .action(cmdOutput);

  hud.command("setup").description("HUD 설정 안내").action(cmdSetup);

  hud.command("reset").description("HUD 설정 초기화").action(cmdReset);

  // 기본 동작: output
  hud.action(cmdOutput);

  return hud;
}
