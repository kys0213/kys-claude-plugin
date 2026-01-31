/**
 * tc hook - Hook 이벤트 처리 커맨드
 *
 * Claude hooks에서 호출되어 이벤트를 처리합니다.
 * 기존 bash 스크립트들의 로직을 TypeScript로 통합.
 */

import { Command } from "commander";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { join, dirname } from "path";
import { execSync } from "child_process";

// ============================================================================
// 타입 정의
// ============================================================================

interface DelegationState {
  sessionId: string;
  currentCheckpoint: string;
  iteration: number;
  maxIterations: number;
  status: string;
  result?: string;
  lastIdleAt?: string;
  lastIdleNotified?: number;
}

interface HookResult {
  success: boolean;
  action: string;
  message: string;
  data?: Record<string, unknown>;
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

const STATE_FILE = ".team-claude/state/current-delegation.json";
const DEFAULT_SERVER_PORT = 7890;

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

function loadState(): DelegationState | null {
  const statePath = getStatePath();
  if (!existsSync(statePath)) {
    return null;
  }
  try {
    return JSON.parse(readFileSync(statePath, "utf-8"));
  } catch {
    return null;
  }
}

function saveState(state: DelegationState): void {
  const statePath = getStatePath();
  const dir = dirname(statePath);
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
  writeFileSync(statePath, JSON.stringify(state, null, 2));
}

function getServerUrl(): string {
  // TODO: 설정 파일에서 포트 읽기
  return `http://localhost:${DEFAULT_SERVER_PORT}`;
}

function timestamp(): string {
  return new Date().toISOString();
}

function ensureDir(dir: string): void {
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
}

async function notifyServer(
  endpoint: string,
  data: Record<string, unknown>
): Promise<boolean> {
  const serverUrl = getServerUrl();
  try {
    const response = await fetch(`${serverUrl}${endpoint}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    });
    return response.ok;
  } catch {
    return false;
  }
}

function sendDesktopNotification(title: string, message: string, urgent = false): void {
  try {
    if (process.platform === "darwin") {
      const sound = urgent ? 'sound name "Sosumi"' : 'sound name "Glass"';
      execSync(
        `osascript -e 'display notification "${message}" with title "${title}" ${sound}'`,
        { stdio: "ignore" }
      );
    } else if (process.platform === "linux") {
      const urgency = urgent ? "-u critical" : "";
      execSync(`notify-send ${urgency} "${title}" "${message}"`, {
        stdio: "ignore",
      });
    }
  } catch {
    // 알림 실패는 무시
  }
}

function outputResult(result: HookResult, json: boolean): void {
  if (json) {
    const output: CLIOutput<HookResult> = {
      success: result.success,
      data: result,
      meta: {
        timestamp: timestamp(),
        duration_ms: 0,
      },
    };
    console.log(JSON.stringify(output, null, 2));
  } else {
    const icon = result.success ? "✅" : "❌";
    console.log(`${icon} ${result.message}`);
  }
}

// ============================================================================
// validation-complete 핸들러
// ============================================================================

interface ValidationCompleteOptions {
  exitCode: string;
  output?: string;
  sessionId?: string;
  checkpointId?: string;
  iteration?: string;
  maxIterations?: string;
  json?: boolean;
}

async function handleValidationComplete(
  options: ValidationCompleteOptions
): Promise<void> {
  const startTime = Date.now();
  const exitCode = parseInt(options.exitCode, 10);
  const json = options.json ?? false;

  // 상태 로드 또는 옵션에서 가져오기
  let state = loadState();
  const sessionId = options.sessionId || state?.sessionId || "unknown";
  const checkpointId = options.checkpointId || state?.currentCheckpoint || "unknown";
  const iteration = options.iteration
    ? parseInt(options.iteration, 10)
    : state?.iteration || 1;
  const maxIterations = options.maxIterations
    ? parseInt(options.maxIterations, 10)
    : state?.maxIterations || 5;

  // 결과 저장
  const gitRoot = getGitRoot();
  const resultDir = join(
    gitRoot,
    `.team-claude/sessions/${sessionId}/delegations/${checkpointId}/iterations/${iteration}`
  );
  ensureDir(resultDir);

  const validationOutput = options.output || "";
  const resultFile = join(resultDir, "result.json");
  writeFileSync(
    resultFile,
    JSON.stringify(
      {
        iteration,
        timestamp: timestamp(),
        exitCode,
        output: validationOutput,
      },
      null,
      2
    )
  );

  let result: HookResult;

  if (exitCode === 0) {
    // 성공
    if (state) {
      state.status = "completed";
      state.result = "pass";
      saveState(state);
    }

    sendDesktopNotification(
      "Team Claude: 검증 성공",
      `Checkpoint ${checkpointId} 통과!`
    );

    await notifyServer("/checkpoint-passed", {
      sessionId,
      checkpoint: checkpointId,
    });

    result = {
      success: true,
      action: "completed",
      message: `Validation PASSED for ${checkpointId}`,
      data: { sessionId, checkpointId, iteration },
    };
  } else if (iteration < maxIterations) {
    // 재시도
    if (state) {
      state.status = "retrying";
      state.iteration = iteration + 1;
      saveState(state);
    }

    await notifyServer("/generate-feedback", {
      sessionId,
      checkpoint: checkpointId,
      iteration,
      output: validationOutput,
    });

    result = {
      success: true,
      action: "retry",
      message: `Validation FAILED, triggering retry (${iteration + 1}/${maxIterations})`,
      data: { sessionId, checkpointId, iteration: iteration + 1, maxIterations },
    };
  } else {
    // 에스컬레이션
    if (state) {
      state.status = "escalated";
      state.result = "max_retry_exceeded";
      saveState(state);
    }

    sendDesktopNotification(
      "Team Claude: 개입 필요",
      `Checkpoint ${checkpointId} 에스컬레이션 필요`,
      true
    );

    result = {
      success: true,
      action: "escalated",
      message: `Max iterations reached (${maxIterations}), escalating to human`,
      data: { sessionId, checkpointId, iteration, maxIterations },
    };
  }

  if (json) {
    const output: CLIOutput<HookResult> = {
      success: true,
      data: result,
      meta: {
        timestamp: timestamp(),
        duration_ms: Date.now() - startTime,
      },
    };
    console.log(JSON.stringify(output, null, 2));
  } else {
    outputResult(result, false);
  }
}

// ============================================================================
// worker-complete 핸들러
// ============================================================================

interface WorkerCompleteOptions {
  taskId: string;
  exitCode?: string;
  json?: boolean;
}

async function handleWorkerComplete(
  options: WorkerCompleteOptions
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  if (!options.taskId) {
    const error = {
      success: false,
      error: {
        code: "MISSING_TASK_ID",
        message: "--task-id is required",
      },
    };
    if (json) {
      console.log(JSON.stringify(error, null, 2));
    } else {
      console.error("Error: --task-id is required");
    }
    process.exit(1);
  }

  const state = loadState();
  if (!state) {
    const result: HookResult = {
      success: true,
      action: "skipped",
      message: "No active delegation found",
      data: { taskId: options.taskId },
    };
    outputResult(result, json);
    return;
  }

  // 상태 업데이트
  state.status = "validating";
  saveState(state);

  // 검증 트리거
  const notified = await notifyServer("/validate", {
    sessionId: state.sessionId,
    checkpoint: state.currentCheckpoint,
    iteration: state.iteration,
    taskId: options.taskId,
  });

  sendDesktopNotification(
    "Team Claude: Worker 완료",
    `Checkpoint ${state.currentCheckpoint} 검증 시작`
  );

  const result: HookResult = {
    success: true,
    action: "validation_triggered",
    message: `Worker completed, validation triggered for ${state.currentCheckpoint}`,
    data: {
      taskId: options.taskId,
      sessionId: state.sessionId,
      checkpointId: state.currentCheckpoint,
      iteration: state.iteration,
      serverNotified: notified,
    },
  };

  if (json) {
    const output: CLIOutput<HookResult> = {
      success: true,
      data: result,
      meta: {
        timestamp: timestamp(),
        duration_ms: Date.now() - startTime,
      },
    };
    console.log(JSON.stringify(output, null, 2));
  } else {
    outputResult(result, false);
  }
}

// ============================================================================
// worker-idle 핸들러
// ============================================================================

interface WorkerIdleOptions {
  percent: string;
  json?: boolean;
}

async function handleWorkerIdle(options: WorkerIdleOptions): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;
  const percent = parseInt(options.percent, 10);

  const state = loadState();
  const sessionId = state?.sessionId || "unknown";
  const checkpointId = state?.currentCheckpoint || "unknown";

  // 상태 업데이트
  if (state) {
    state.lastIdleAt = timestamp();
    saveState(state);
  }

  // 5분에 한 번만 알림
  const currentTime = Math.floor(Date.now() / 1000);
  const shouldNotify =
    !state?.lastIdleNotified || currentTime - state.lastIdleNotified > 300;

  if (shouldNotify) {
    sendDesktopNotification("Team Claude: Worker Idle", "Worker가 대기 중입니다");

    if (state) {
      state.lastIdleNotified = currentTime;
      saveState(state);
    }
  }

  await notifyServer("/worker-idle", {
    sessionId,
    checkpoint: checkpointId,
    percent,
  });

  // 80% 이상이면 체크포인트 권장
  const recommendation =
    percent >= 80
      ? "Consider saving a checkpoint to preserve context"
      : "Context usage is within normal range";

  const result: HookResult = {
    success: true,
    action: "idle_recorded",
    message: `Worker idle at ${percent}% context usage`,
    data: {
      sessionId,
      checkpointId,
      percent,
      recommendation,
      notificationSent: shouldNotify,
    },
  };

  if (json) {
    const output: CLIOutput<{ recommendation: string } & HookResult> = {
      success: true,
      data: { ...result, recommendation },
      meta: {
        timestamp: timestamp(),
        duration_ms: Date.now() - startTime,
      },
    };
    console.log(JSON.stringify(output, null, 2));
  } else {
    outputResult(result, false);
    if (percent >= 80) {
      console.log(`  Recommendation: ${recommendation}`);
    }
  }
}

// ============================================================================
// worker-question 핸들러
// ============================================================================

interface WorkerQuestionOptions {
  question: string;
  context?: string;
  json?: boolean;
}

async function handleWorkerQuestion(
  options: WorkerQuestionOptions
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  const state = loadState();
  const sessionId = state?.sessionId || "unknown";
  const checkpointId = state?.currentCheckpoint || "unknown";

  // 상태 업데이트
  if (state) {
    state.status = "waiting_for_human";
    saveState(state);
  }

  // 질문 저장
  const gitRoot = getGitRoot();
  const questionFile = join(
    gitRoot,
    `.team-claude/sessions/${sessionId}/delegations/${checkpointId}/pending-question.json`
  );
  ensureDir(dirname(questionFile));
  writeFileSync(
    questionFile,
    JSON.stringify(
      {
        timestamp: timestamp(),
        checkpoint: checkpointId,
        question: options.question,
        context: options.context,
      },
      null,
      2
    )
  );

  // 긴급 알림
  sendDesktopNotification(
    "Team Claude: 인간 개입 필요",
    "Worker가 질문을 하고 있습니다",
    true
  );

  await notifyServer("/worker-question", {
    sessionId,
    checkpoint: checkpointId,
    question: options.question,
    context: options.context,
  });

  const result: HookResult = {
    success: true,
    action: "question_escalated",
    message: `Question escalated to human for ${checkpointId}`,
    data: {
      sessionId,
      checkpointId,
      question: options.question,
      questionFile,
    },
  };

  if (json) {
    const output: CLIOutput<HookResult> = {
      success: true,
      data: result,
      meta: {
        timestamp: timestamp(),
        duration_ms: Date.now() - startTime,
      },
    };
    console.log(JSON.stringify(output, null, 2));
  } else {
    outputResult(result, false);
  }
}

// ============================================================================
// 커맨드 생성
// ============================================================================

export function createHookCommand(): Command {
  const hook = new Command("hook")
    .description("Hook 이벤트 처리 (내부용)")
    .addHelpText(
      "after",
      `
Examples:
  tc hook validation-complete --exit-code 0
  tc hook validation-complete --exit-code 1 --iteration 2 --max-iterations 5
  tc hook worker-complete --task-id abc123
  tc hook worker-idle --percent 80
  tc hook worker-question --question "API 키가 필요합니다"
`
    );

  // validation-complete
  hook
    .command("validation-complete")
    .description("검증 완료 이벤트 처리")
    .requiredOption("--exit-code <code>", "검증 명령 종료 코드")
    .option("--output <text>", "검증 출력 내용")
    .option("--session-id <id>", "세션 ID (자동 감지)")
    .option("--checkpoint-id <id>", "체크포인트 ID (자동 감지)")
    .option("--iteration <n>", "현재 반복 횟수")
    .option("--max-iterations <n>", "최대 반복 횟수")
    .option("--json", "JSON 형식으로 출력")
    .action(async (options) => {
      await handleValidationComplete(options);
    });

  // worker-complete
  hook
    .command("worker-complete")
    .description("Worker 완료 이벤트 처리")
    .requiredOption("--task-id <id>", "태스크 ID")
    .option("--exit-code <code>", "종료 코드")
    .option("--json", "JSON 형식으로 출력")
    .action(async (options) => {
      await handleWorkerComplete(options);
    });

  // worker-idle
  hook
    .command("worker-idle")
    .description("Worker 대기 상태 이벤트 처리")
    .requiredOption("--percent <n>", "Context 사용률 (%)")
    .option("--json", "JSON 형식으로 출력")
    .action(async (options) => {
      await handleWorkerIdle(options);
    });

  // worker-question
  hook
    .command("worker-question")
    .description("Worker 질문 에스컬레이션")
    .requiredOption("--question <text>", "질문 내용")
    .option("--context <text>", "추가 컨텍스트")
    .option("--json", "JSON 형식으로 출력")
    .action(async (options) => {
      await handleWorkerQuestion(options);
    });

  return hook;
}
