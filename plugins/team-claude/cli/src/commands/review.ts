/**
 * tc review - 자동 리뷰 커맨드
 */

import { Command } from "commander";
import { existsSync, mkdirSync, writeFileSync, readFileSync } from "fs";
import { join, dirname } from "path";
import { ProjectContext } from "../lib/context";

// ============================================================================
// 타입 정의
// ============================================================================

interface ReviewResult {
  type: string;
  target: string;
  iteration: number;
  result: "pass" | "warn" | "fail" | "pending";
  details?: unknown;
  timestamp: string;
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
// spec 핸들러
// ============================================================================

async function handleSpec(
  sessionId: string,
  options: { maxIterations?: string; autoFix?: boolean; strict?: boolean; json?: boolean }
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;
  const maxIterations = options.maxIterations ? parseInt(options.maxIterations, 10) : 5;

  if (!sessionId) {
    if (json) {
      outputError("MISSING_SESSION_ID", "세션 ID를 지정하세요.");
    } else {
      console.error("[ERR] 세션 ID를 지정하세요.");
    }
    process.exit(1);
  }

  const ctx = await ProjectContext.getInstance();
  const sessionDir = join(ctx.sessionsDir, sessionId);

  if (!existsSync(sessionDir)) {
    if (json) {
      outputError("SESSION_NOT_FOUND", `세션을 찾을 수 없습니다: ${sessionId}`);
    } else {
      console.error(`[ERR] 세션을 찾을 수 없습니다: ${sessionId}`);
    }
    process.exit(1);
  }

  // 리뷰 결과 저장
  const reviewDir = join(sessionDir, "reviews");
  mkdirSync(reviewDir, { recursive: true });

  const result: ReviewResult = {
    type: "spec",
    target: sessionId,
    iteration: 1,
    result: "pending",
    timestamp: timestamp(),
  };

  writeFileSync(
    join(reviewDir, `spec-review-${Date.now()}.json`),
    JSON.stringify(result, null, 2)
  );

  if (json) {
    outputJson(
      {
        sessionId,
        type: "spec",
        maxIterations,
        autoFix: options.autoFix ?? false,
        strict: options.strict ?? false,
        status: "initiated",
      },
      startTime
    );
  } else {
    console.log(`[INFO] 스펙 리뷰 시작: ${sessionId}`);
    console.log(`  최대 반복: ${maxIterations}`);
    console.log(`  자동 수정: ${options.autoFix ?? false}`);
    console.log(`  엄격 모드: ${options.strict ?? false}`);
    console.log("");
    console.log("[INFO] 리뷰 에이전트를 호출하세요: /team-claude:review spec");
  }
}

// ============================================================================
// code 핸들러
// ============================================================================

async function handleCode(
  checkpointId: string,
  options: { maxIterations?: string; autoFix?: boolean; strict?: boolean; json?: boolean }
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;
  const maxIterations = options.maxIterations ? parseInt(options.maxIterations, 10) : 5;

  if (!checkpointId) {
    if (json) {
      outputError("MISSING_CHECKPOINT_ID", "체크포인트 ID를 지정하세요.");
    } else {
      console.error("[ERR] 체크포인트 ID를 지정하세요.");
    }
    process.exit(1);
  }

  if (json) {
    outputJson(
      {
        checkpointId,
        type: "code",
        maxIterations,
        autoFix: options.autoFix ?? false,
        strict: options.strict ?? false,
        status: "initiated",
      },
      startTime
    );
  } else {
    console.log(`[INFO] 코드 리뷰 시작: ${checkpointId}`);
    console.log(`  최대 반복: ${maxIterations}`);
    console.log(`  자동 수정: ${options.autoFix ?? false}`);
    console.log(`  엄격 모드: ${options.strict ?? false}`);
    console.log("");
    console.log("[INFO] 리뷰 에이전트를 호출하세요: /team-claude:review code");
  }
}

// ============================================================================
// 커맨드 생성
// ============================================================================

export function createReviewCommand(): Command {
  const review = new Command("review")
    .description("자동 리뷰")
    .addHelpText(
      "after",
      `
Examples:
  tc review spec abc12345
  tc review code coupon-service --auto-fix
  tc review spec abc12345 --strict --max-iterations 3
`
    );

  review
    .command("spec <session-id>")
    .description("스펙 리뷰")
    .option("--max-iterations <n>", "최대 반복 횟수", "5")
    .option("--auto-fix", "자동 수정 적용")
    .option("--strict", "엄격 모드 (WARN도 FAIL로 처리)")
    .option("--json", "JSON 형식으로 출력")
    .action(handleSpec);

  review
    .command("code <checkpoint-id>")
    .description("코드 리뷰")
    .option("--max-iterations <n>", "최대 반복 횟수", "5")
    .option("--auto-fix", "자동 수정 적용")
    .option("--strict", "엄격 모드")
    .option("--json", "JSON 형식으로 출력")
    .action(handleCode);

  return review;
}
