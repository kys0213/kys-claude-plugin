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

  hook
    .command("generate-config")
    .description("hooks.json 설정 생성")
    .action(cmdGenerateConfig);

  return hook;
}
