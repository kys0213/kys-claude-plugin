/**
 * tc hook - Claude Code Hook 이벤트 핸들러
 * Shell 스크립트 대신 CLI로 hook 처리
 */

import { Command } from "commander";
import { existsSync } from "fs";
import { join } from "path";
import {
  findGitRoot,
  readJsonFile,
  writeJsonFile,
  timestamp,
  TC_SERVER_DEFAULT_PORT,
} from "../lib/common";

// ============================================================================
// 타입 정의
// ============================================================================

interface DelegationState {
  sessionId: string;
  currentCheckpoint: string;
  iteration: number;
  status: string;
  lastIdleAt?: string;
  lastIdleNotified?: number;
}

// ============================================================================
// 헬퍼 함수
// ============================================================================

function getDelegationStatePath(): string {
  return join(findGitRoot(), ".team-claude", "state", "current-delegation.json");
}

function getDelegationState(): DelegationState | null {
  const statePath = getDelegationStatePath();
  if (!existsSync(statePath)) {
    return null;
  }
  return readJsonFile<DelegationState>(statePath);
}

function updateDelegationState(updates: Partial<DelegationState>): void {
  const statePath = getDelegationStatePath();
  const current = getDelegationState();
  if (!current) return;

  writeJsonFile(statePath, { ...current, ...updates });
}

function getServerUrl(): string {
  return `http://localhost:${TC_SERVER_DEFAULT_PORT}`;
}

async function postToServer(endpoint: string, data: Record<string, unknown>): Promise<boolean> {
  const url = `${getServerUrl()}${endpoint}`;
  try {
    const healthCheck = await fetch(`${getServerUrl()}/health`);
    if (!healthCheck.ok) return false;

    await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    });
    return true;
  } catch {
    return false;
  }
}

function sendNotification(title: string, message: string): void {
  // macOS
  if (process.platform === "darwin") {
    try {
      Bun.spawnSync(["osascript", "-e", `display notification "${message}" with title "${title}"`]);
    } catch {
      // 알림 실패 무시
    }
  }
  // Linux
  else if (process.platform === "linux") {
    try {
      Bun.spawnSync(["notify-send", title, message]);
    } catch {
      // 알림 실패 무시
    }
  }
}

// ============================================================================
// worker-complete: Worker 완료 시 검증 트리거
// ============================================================================

async function cmdWorkerComplete(): Promise<void> {
  const state = getDelegationState();

  if (!state) {
    console.log("No active delegation found");
    return;
  }

  const { sessionId, currentCheckpoint, iteration } = state;
  console.log(`Worker completed: ${currentCheckpoint} (iteration ${iteration})`);

  // 상태 업데이트
  updateDelegationState({ status: "validating" });

  // 서버에 알림
  await postToServer("/validate", {
    sessionId,
    checkpoint: currentCheckpoint,
    iteration,
  });

  // OS 알림
  sendNotification(
    "Team Claude: Worker 완료",
    `Checkpoint ${currentCheckpoint} 검증 시작`
  );

  console.log("Validation triggered successfully");
}

// ============================================================================
// worker-idle: Worker 대기 상태 감지
// ============================================================================

async function cmdWorkerIdle(): Promise<void> {
  const state = getDelegationState();

  const sessionId = state?.sessionId || "unknown";
  const checkpoint = state?.currentCheckpoint || "unknown";

  console.log(`Worker idle detected: ${checkpoint}`);

  if (state) {
    // 마지막 idle 시간 업데이트
    updateDelegationState({ lastIdleAt: timestamp() });

    // 5분에 한 번만 알림
    const currentTime = Math.floor(Date.now() / 1000);
    const lastNotified = state.lastIdleNotified || 0;

    if (currentTime - lastNotified > 300) {
      sendNotification(
        "Team Claude: Worker Idle",
        `Worker가 대기 중: ${checkpoint}`
      );
      updateDelegationState({ lastIdleNotified: currentTime });
    }
  }

  // 서버 상태 업데이트
  await postToServer("/worker-idle", { sessionId, checkpoint });

  console.log("Idle notification sent");
}

// ============================================================================
// worker-question: Worker 질문 에스컬레이션
// ============================================================================

async function cmdWorkerQuestion(): Promise<void> {
  // stdin에서 hook 데이터 읽기
  let hookData: Record<string, unknown> = {};

  try {
    const stdin = await Bun.stdin.text();
    if (stdin.trim()) {
      hookData = JSON.parse(stdin);
    }
  } catch {
    // stdin 파싱 실패 시 빈 객체
  }

  const state = getDelegationState();
  const sessionId = state?.sessionId || "unknown";
  const checkpoint = state?.currentCheckpoint || "unknown";

  console.log(`Worker question escalation: ${checkpoint}`);

  // 서버에 알림
  await postToServer("/worker-question", {
    sessionId,
    checkpoint,
    hookData,
  });

  // OS 알림
  sendNotification(
    "Team Claude: Worker 질문",
    `Checkpoint ${checkpoint}에서 질문 발생`
  );

  console.log("Question escalation sent");
}

// ============================================================================
// validation-complete: 검증 완료 처리
// ============================================================================

async function cmdValidationComplete(): Promise<void> {
  // stdin에서 hook 데이터 읽기 (Bash 도구 결과)
  let hookData: Record<string, unknown> = {};

  try {
    const stdin = await Bun.stdin.text();
    if (stdin.trim()) {
      hookData = JSON.parse(stdin);
    }
  } catch {
    // stdin 파싱 실패 시 빈 객체
  }

  // test 명령어가 아니면 무시 (condition 대체 로직)
  const toolInput = hookData.tool_input as { command?: string } | undefined;
  const command = toolInput?.command || "";
  if (!command.includes("test")) {
    // test 명령어가 아니면 조용히 종료
    return;
  }

  const state = getDelegationState();
  const sessionId = state?.sessionId || "unknown";
  const checkpoint = state?.currentCheckpoint || "unknown";

  // 검증 결과 분석 (exit_code 기반)
  const toolResult = hookData.tool_result as { exit_code?: number } | undefined;
  const exitCode = toolResult?.exit_code;
  const passed = exitCode === 0;

  console.log(`Validation complete: ${checkpoint} (${passed ? "PASSED" : "FAILED"})`);

  if (state) {
    updateDelegationState({
      status: passed ? "checkpoint_passed" : "checkpoint_failed",
    });
  }

  // 서버에 알림
  const endpoint = passed ? "/checkpoint-passed" : "/generate-feedback";
  await postToServer(endpoint, {
    sessionId,
    checkpoint,
    exitCode,
    hookData,
  });

  // OS 알림
  sendNotification(
    `Team Claude: 검증 ${passed ? "성공" : "실패"}`,
    `Checkpoint ${checkpoint}`
  );

  console.log("Validation result processed");
}

// ============================================================================
// Spec Refine 상태 헬퍼
// ============================================================================

interface RefineState {
  sessionId: string;
  status: string;
  currentIteration: number;
  config: {
    maxIterations: number;
    passThreshold: number;
    warnThreshold: number;
    maxPerspectives: number;
  };
  iterations: Array<{
    iteration: number;
    reviews: Array<{ perspective: string; score: number }>;
    consensusIssues: Array<{ summary: string; level: string; resolved: boolean }>;
    weightedScore: number;
    verdict: string;
    refinementActions: string[];
    perspectives: Array<{ role: string; weight: number }>;
  }>;
  carry: {
    unresolvedIssues: Array<{ summary: string; level: string; resolved: boolean }>;
    resolvedIssues: Array<{ summary: string; resolved: boolean; resolvedAt?: string }>;
    scoreHistory: number[];
    perspectiveHistory: string[][];
  };
  updatedAt: string;
  completedAt: string | null;
}

function getRefineStatePath(): string {
  const sessionsDir = join(findGitRoot(), ".team-claude", "sessions");
  // 현재 활성 세션에서 refine-state.json 찾기
  const workflowPath = join(findGitRoot(), ".team-claude", "state", "workflow.json");
  const workflow = readJsonFile<{ currentSession: string | null }>(workflowPath);
  const sessionId = workflow?.currentSession || "unknown";
  return join(sessionsDir, sessionId, "refine-state.json");
}

function getRefineState(): RefineState | null {
  const statePath = getRefineStatePath();
  if (!existsSync(statePath)) return null;
  return readJsonFile<RefineState>(statePath);
}

function updateRefineState(updates: Partial<RefineState>): void {
  const statePath = getRefineStatePath();
  const current = getRefineState();
  if (!current) return;
  writeJsonFile(statePath, { ...current, ...updates, updatedAt: timestamp() });
}

// ============================================================================
// refine-review-complete: 리뷰 에이전트/스크립트 완료 감지
// ============================================================================

async function cmdRefineReviewComplete(): Promise<void> {
  let hookData: Record<string, unknown> = {};
  try {
    const stdin = await Bun.stdin.text();
    if (stdin.trim()) hookData = JSON.parse(stdin);
  } catch { /* ignore */ }

  const state = getRefineState();
  if (!state || state.status !== "running") return;

  // Bash hook: call-codex.sh / call-gemini.sh 결과만 처리
  const toolInput = hookData.tool_input as { command?: string } | undefined;
  const command = toolInput?.command || "";
  const isExternalReview = command.includes("call-codex") || command.includes("call-gemini");

  // Task hook: 리뷰 에이전트 결과 (tool_input에 prompt 포함)
  const toolInputTask = hookData.tool_input as { prompt?: string } | undefined;
  const isAgentReview = toolInputTask?.prompt?.includes("리뷰") || toolInputTask?.prompt?.includes("review");

  if (!isExternalReview && !isAgentReview) return;

  const iteration = state.currentIteration;
  const currentIter = state.iterations[iteration - 1];

  if (!currentIter) return;

  const reviewCount = currentIter.reviews.length;
  const expectedCount = currentIter.perspectives.length;

  console.log(`[spec-refine] Review complete: ${reviewCount}/${expectedCount} (iteration ${iteration})`);

  if (reviewCount >= expectedCount) {
    console.log(`[spec-refine] All reviews collected. Proceed to consensus analysis.`);
    sendNotification(
      "Spec Refine: 리뷰 완료",
      `Iteration ${iteration}: 전체 ${expectedCount}개 리뷰 수집 완료`
    );
  }
}

// ============================================================================
// refine-spec-modified: 스펙 파일 수정 감지 → 정제 액션 기록
// ============================================================================

async function cmdRefineSpecModified(): Promise<void> {
  let hookData: Record<string, unknown> = {};
  try {
    const stdin = await Bun.stdin.text();
    if (stdin.trim()) hookData = JSON.parse(stdin);
  } catch { /* ignore */ }

  const state = getRefineState();
  if (!state || state.status !== "running") return;

  // specs/ 디렉토리 파일 수정만 추적
  const toolInput = hookData.tool_input as { file_path?: string } | undefined;
  const filePath = toolInput?.file_path || "";
  if (!filePath.includes("/specs/")) return;

  const iteration = state.currentIteration;
  const currentIter = state.iterations[iteration - 1];
  if (!currentIter) return;

  const action = `${filePath.split("/").pop()} modified`;
  if (!currentIter.refinementActions.includes(action)) {
    currentIter.refinementActions.push(action);
    updateRefineState({ iterations: state.iterations });
  }

  console.log(`[spec-refine] Spec modified: ${filePath}`);
}

// ============================================================================
// refine-iteration-end: iteration 종료 → carry 업데이트 + 에스컬레이션 판단
// ============================================================================

async function cmdRefineIterationEnd(): Promise<void> {
  const state = getRefineState();
  if (!state || state.status !== "running") return;

  const iteration = state.currentIteration;
  const currentIter = state.iterations[iteration - 1];
  if (!currentIter) return;

  // verdict가 아직 설정 안 되었으면 무시 (아직 진행 중)
  if (!currentIter.verdict) return;

  console.log(`[spec-refine] Iteration ${iteration} ended: ${currentIter.verdict}`);

  // ━━━ carry 업데이트 ━━━
  const carry = state.carry;

  // 1. 점수 기록
  carry.scoreHistory.push(currentIter.weightedScore);

  // 2. 관점 기록
  carry.perspectiveHistory.push(
    currentIter.perspectives.map((p: { role: string }) => p.role)
  );

  // 3. 이슈 분류: resolved vs unresolved
  for (const issue of currentIter.consensusIssues) {
    if (issue.resolved) {
      // unresolvedIssues에서 제거, resolvedIssues에 추가
      carry.unresolvedIssues = carry.unresolvedIssues.filter(
        (u: { summary: string }) => u.summary !== issue.summary
      );
      carry.resolvedIssues.push({
        ...issue,
        resolved: true,
        resolvedAt: `iteration-${iteration}`,
      });
    } else if (!carry.unresolvedIssues.some((u: { summary: string }) => u.summary === issue.summary)) {
      carry.unresolvedIssues.push(issue);
    }
  }

  // ━━━ 에스컬레이션 판단 ━━━
  let shouldEscalate = false;
  let escalationReason = "";

  // 1. 최대 반복 도달
  if (iteration >= state.config.maxIterations) {
    shouldEscalate = true;
    escalationReason = `최대 반복 도달 (${iteration}/${state.config.maxIterations})`;
  }

  // 2. 점수 정체 (최근 2회 차이 < 3점)
  if (carry.scoreHistory.length >= 2) {
    const recent = carry.scoreHistory.slice(-2);
    if (Math.abs(recent[1] - recent[0]) < 3) {
      shouldEscalate = true;
      escalationReason = `점수 정체 (${recent[0].toFixed(1)} → ${recent[1].toFixed(1)})`;
    }
  }

  // 3. 점수 하락
  if (carry.scoreHistory.length >= 2) {
    const recent = carry.scoreHistory.slice(-2);
    if (recent[1] < recent[0]) {
      shouldEscalate = true;
      escalationReason = `점수 하락 (${recent[0].toFixed(1)} → ${recent[1].toFixed(1)})`;
    }
  }

  // 4. 동일 이슈 3회 이상 반복
  for (const issue of carry.unresolvedIssues) {
    const appearances = state.iterations.filter((iter) =>
      iter.consensusIssues.some(
        (ci: { summary: string; resolved: boolean }) => ci.summary === issue.summary && !ci.resolved
      )
    ).length;
    if (appearances >= 3) {
      shouldEscalate = true;
      escalationReason = `동일 이슈 ${appearances}회 반복: "${issue.summary}"`;
      break;
    }
  }

  if (shouldEscalate && currentIter.verdict === "fail") {
    updateRefineState({
      status: "escalated",
      carry,
      completedAt: timestamp(),
    });

    console.log(`[spec-refine] ESCALATED: ${escalationReason}`);
    sendNotification(
      "Spec Refine: 에스컬레이션",
      escalationReason
    );
  } else if (currentIter.verdict === "pass") {
    updateRefineState({
      status: "passed",
      carry,
      completedAt: timestamp(),
    });

    console.log(`[spec-refine] PASSED (score: ${currentIter.weightedScore})`);
    sendNotification(
      "Spec Refine: 통과",
      `점수 ${currentIter.weightedScore.toFixed(1)} (iteration ${iteration})`
    );
  } else if (currentIter.verdict === "warn") {
    updateRefineState({
      status: "warned",
      carry,
    });

    console.log(`[spec-refine] WARN (score: ${currentIter.weightedScore})`);
    sendNotification(
      "Spec Refine: 경고",
      `점수 ${currentIter.weightedScore.toFixed(1)} - 사용자 확인 필요`
    );
  } else {
    // fail but not escalated → continue
    updateRefineState({ carry });
    console.log(`[spec-refine] FAIL → next iteration (score: ${currentIter.weightedScore})`);
  }
}

// ============================================================================
// generate-config: hooks.json 설정 생성
// ============================================================================

function cmdGenerateConfig(): void {
  // Claude Code 공식 문서 형식에 맞춤
  const config = {
    hooks: {
      Stop: [
        {
          matcher: "",
          description: "Worker 완료 시 자동 검증 트리거",
          hooks: [
            {
              type: "command",
              command: "tc hook worker-complete",
              timeout: 30,
            },
          ],
        },
        {
          matcher: "",
          description: "spec-refine iteration 종료 시 carry 업데이트 및 에스컬레이션 판단",
          hooks: [
            {
              type: "command",
              command: "tc hook refine-iteration-end",
              timeout: 30,
            },
          ],
        },
      ],
      PreToolUse: [
        {
          matcher: "Task",
          description: "Worker 질문 시 에스컬레이션 (Task 도구 사용 시)",
          hooks: [
            {
              type: "command",
              command: "tc hook worker-question",
              timeout: 10,
            },
          ],
        },
      ],
      PostToolUse: [
        {
          matcher: "Bash",
          description: "Bash 실행 후 결과 분석 (test 명령어는 내부에서 필터링)",
          hooks: [
            {
              type: "command",
              command: "tc hook validation-complete",
              timeout: 60,
            },
          ],
        },
        {
          matcher: "Bash",
          description: "spec-refine: 외부 LLM 리뷰 스크립트 완료 감지",
          hooks: [
            {
              type: "command",
              command: "tc hook refine-review-complete",
              timeout: 30,
            },
          ],
        },
        {
          matcher: "Task",
          description: "spec-refine: Claude 리뷰 에이전트 완료 감지",
          hooks: [
            {
              type: "command",
              command: "tc hook refine-review-complete",
              timeout: 30,
            },
          ],
        },
        {
          matcher: "Write",
          description: "spec-refine: 스펙 파일 수정 감지 → 정제 액션 기록",
          hooks: [
            {
              type: "command",
              command: "tc hook refine-spec-modified",
              timeout: 10,
            },
          ],
        },
      ],
      Notification: [
        {
          matcher: "idle_prompt",
          description: "Worker 대기 상태 감지",
          hooks: [
            {
              type: "command",
              command: "tc hook worker-idle",
              timeout: 5,
            },
          ],
        },
      ],
    },
  };

  console.log(JSON.stringify(config, null, 2));
}

// ============================================================================
// 명령어 등록
// ============================================================================

export function createHookCommand(): Command {
  const hook = new Command("hook")
    .description("Claude Code Hook 이벤트 핸들러");

  hook
    .command("worker-complete")
    .description("Worker 완료 시 검증 트리거 (Stop hook)")
    .action(cmdWorkerComplete);

  hook
    .command("worker-idle")
    .description("Worker 대기 상태 감지 (Notification hook)")
    .action(cmdWorkerIdle);

  hook
    .command("worker-question")
    .description("Worker 질문 에스컬레이션 (PreToolUse hook)")
    .action(cmdWorkerQuestion);

  hook
    .command("validation-complete")
    .description("검증 완료 처리 (PostToolUse hook)")
    .action(cmdValidationComplete);

  // Spec Refine hooks
  hook
    .command("refine-review-complete")
    .description("리뷰 에이전트/스크립트 완료 감지 (PostToolUse hook)")
    .action(cmdRefineReviewComplete);

  hook
    .command("refine-spec-modified")
    .description("스펙 파일 수정 감지 → 정제 액션 기록 (PostToolUse hook)")
    .action(cmdRefineSpecModified);

  hook
    .command("refine-iteration-end")
    .description("iteration 종료 → carry 업데이트 + 에스컬레이션 판단 (Stop hook)")
    .action(cmdRefineIterationEnd);

  hook
    .command("generate-config")
    .description("hooks.json 설정 생성")
    .action(cmdGenerateConfig);

  return hook;
}
