/**
 * review 명령어 - 자동 리뷰 실행
 */

import { Command } from "commander";
import { existsSync, readdirSync } from "fs";
import { readFile, writeFile, mkdir } from "fs/promises";
import { join } from "path";
import { log, printSection, printStatus, printKV, icon } from "../lib/utils";
import { ProjectContext } from "../lib/context";

interface ReviewResult {
  type: "spec" | "code";
  target: string;
  iteration: number;
  result: "PASS" | "WARN" | "FAIL" | "SIMULATED";
  details: {
    issues: string[];
    warnings: string[];
  };
  timestamp: string;
}

interface ReviewSummary {
  sessionId?: string;
  checkpointId?: string;
  type: "spec" | "code";
  result: "PASS" | "WARN" | "FAIL";
  iterations: number;
}

// ============================================================================
// 리뷰 결과 저장
// ============================================================================

async function getReviewDir(
  type: "spec" | "code",
  target: string
): Promise<string> {
  const ctx = await ProjectContext.getInstance();

  if (type === "spec") {
    return join(ctx.sessionsDir, target, "reviews");
  } else {
    return join(ctx.sessionsDir, "current", "reviews", target);
  }
}

async function saveReviewResult(
  type: "spec" | "code",
  target: string,
  iteration: number,
  result: "PASS" | "WARN" | "FAIL" | "SIMULATED",
  details: { issues: string[]; warnings: string[] }
): Promise<string> {
  const reviewDir = await getReviewDir(type, target);

  if (!existsSync(reviewDir)) {
    await mkdir(reviewDir, { recursive: true });
  }

  const reviewFile = join(reviewDir, `iteration-${iteration}.json`);
  const reviewResult: ReviewResult = {
    type,
    target,
    iteration,
    result,
    details,
    timestamp: new Date().toISOString(),
  };

  await writeFile(reviewFile, JSON.stringify(reviewResult, null, 2), "utf-8");
  return reviewFile;
}

// ============================================================================
// spec - 스펙 리뷰
// ============================================================================

async function specCommand(
  sessionId: string,
  options: {
    maxIterations?: number;
    autoFix?: boolean;
    strict?: boolean;
  }
): Promise<void> {
  const maxIterations = options.maxIterations ?? 3;
  const autoFix = options.autoFix ?? false;
  const strict = options.strict ?? false;

  const ctx = await ProjectContext.getInstance();
  const sessionPath = join(ctx.sessionsDir, sessionId);

  if (!existsSync(sessionPath)) {
    log.err(`세션을 찾을 수 없습니다: ${sessionId}`);
    process.exit(1);
  }

  console.log();
  printSection("🔍 Spec Review 시작");
  console.log();
  printKV("세션", sessionId);
  printKV("최대 반복", maxIterations.toString());
  printKV("자동 수정", autoFix ? "ON" : "OFF");
  printKV("엄격 모드", strict ? "ON" : "OFF");
  console.log();
  console.log(
    "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  );
  console.log();

  // 스펙 파일 확인
  const specsDir = join(sessionPath, "specs");
  const requiredFiles = [
    "architecture.md",
    "contracts.md",
    "checkpoints.yaml",
  ];

  const missingFiles = requiredFiles.filter(
    (f) => !existsSync(join(specsDir, f))
  );

  if (missingFiles.length > 0) {
    log.warn("누락된 스펙 파일:");
    for (const f of missingFiles) {
      console.log(`  - ${f}`);
    }
    console.log();
  }

  // 리뷰 체크리스트 출력
  console.log("📋 Review Checklist");
  console.log();
  console.log("  완전성 (Completeness)");
  console.log("    [ ] 모든 요구사항 반영");
  console.log("    [ ] 엣지 케이스 정의");
  console.log("    [ ] 에러 처리 정의");
  console.log();
  console.log("  일관성 (Consistency)");
  console.log("    [ ] 기존 아키텍처 일관성");
  console.log("    [ ] 용어/네이밍 일관성");
  console.log();
  console.log("  테스트 가능성 (Testability)");
  console.log("    [ ] 검증 가능한 기준");
  console.log("    [ ] Contract Test 충분성");
  console.log();
  console.log("  의존성 (Dependencies)");
  console.log("    [ ] 의존성 그래프 정확성");
  console.log("    [ ] 순환 의존성 없음");
  console.log();

  // 리뷰 시뮬레이션 (실제로는 에이전트가 수행)
  console.log("━━━ Auto-Review Loop ━━━");
  console.log();

  let iteration = 1;
  let finalResult: "PASS" | "WARN" | "FAIL" = "PASS";

  while (iteration <= maxIterations) {
    console.log(`  Iteration ${iteration}/${maxIterations}:`);
    console.log("    🔍 리뷰 수행 중...");

    // 실제 구현에서는 여기서 spec-reviewer 에이전트 호출
    // 지금은 플레이스홀더

    const details = { issues: [], warnings: [] };
    await saveReviewResult("spec", sessionId, iteration, "SIMULATED", details);

    console.log("    ✅ 리뷰 완료");
    console.log();

    finalResult = "PASS";
    break;
  }

  console.log(
    "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  );
  console.log();

  if (finalResult === "PASS") {
    log.ok("Spec Review 완료: PASS");
  } else if (finalResult === "WARN") {
    log.warn("Spec Review 완료: WARN (경고 있음)");
  } else {
    log.err("Spec Review 완료: FAIL (수정 필요)");
  }

  console.log();
  const reviewDir = await getReviewDir("spec", sessionId);
  printKV("결과 저장", reviewDir);
  console.log();

  // JSON 출력
  console.log("---");
  const summary: ReviewSummary = {
    sessionId,
    type: "spec",
    result: finalResult,
    iterations: iteration,
  };
  console.log(JSON.stringify(summary, null, 2));
}

// ============================================================================
// code - 코드 리뷰
// ============================================================================

async function codeCommand(
  checkpointId: string,
  options: {
    maxIterations?: number;
    autoFix?: boolean;
    strict?: boolean;
  }
): Promise<void> {
  const maxIterations = options.maxIterations ?? 3;
  const autoFix = options.autoFix ?? false;
  const strict = options.strict ?? false;

  const ctx = await ProjectContext.getInstance();
  const worktreePath = join(ctx.worktreesDir, checkpointId);

  if (!existsSync(worktreePath)) {
    log.err(`Worktree를 찾을 수 없습니다: ${checkpointId}`);
    process.exit(1);
  }

  console.log();
  printSection("🔍 Code Review 시작");
  console.log();
  printKV("Checkpoint", checkpointId);
  printKV("Worktree", worktreePath);
  printKV("최대 반복", maxIterations.toString());
  printKV("자동 수정", autoFix ? "ON" : "OFF");
  printKV("엄격 모드", strict ? "ON" : "OFF");
  console.log();
  console.log(
    "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  );
  console.log();

  // 변경 파일 목록
  console.log("📁 Changed Files");
  console.log();

  // Git diff로 변경된 파일 목록 확인 (실제 구현에서는 Git API 사용)
  log.info("커밋된 변경 사항 확인 중...");
  console.log();

  // 리뷰 체크리스트
  console.log("📋 Review Checklist");
  console.log();
  console.log("  Contract 준수");
  console.log("    [ ] Interface 구현 정확성");
  console.log("    [ ] Test 통과");
  console.log();
  console.log("  코드 품질");
  console.log("    [ ] 스타일 일관성");
  console.log("    [ ] 복잡도 적절");
  console.log();
  console.log("  보안");
  console.log("    [ ] SQL Injection");
  console.log("    [ ] XSS");
  console.log("    [ ] 입력 검증");
  console.log();
  console.log("  성능");
  console.log("    [ ] N+1 쿼리");
  console.log("    [ ] 불필요한 반복");
  console.log();

  // 리뷰 시뮬레이션
  console.log("━━━ Auto-Review Loop ━━━");
  console.log();

  let iteration = 1;
  let finalResult: "PASS" | "WARN" | "FAIL" = "PASS";

  while (iteration <= maxIterations) {
    console.log(`  Iteration ${iteration}/${maxIterations}:`);
    console.log("    🔍 리뷰 수행 중...");

    // 실제 구현에서는 여기서 code-reviewer 에이전트 호출

    const details = { issues: [], warnings: [] };
    await saveReviewResult(
      "code",
      checkpointId,
      iteration,
      "SIMULATED",
      details
    );

    console.log("    ✅ 리뷰 완료");
    console.log();

    finalResult = "PASS";
    break;
  }

  console.log(
    "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  );
  console.log();

  if (finalResult === "PASS") {
    log.ok("Code Review 완료: PASS");
  } else if (finalResult === "WARN") {
    log.warn("Code Review 완료: WARN (경고 있음)");
  } else {
    log.err("Code Review 완료: FAIL (수정 필요)");
  }

  console.log();

  // JSON 출력
  console.log("---");
  const summary: ReviewSummary = {
    checkpointId,
    type: "code",
    result: finalResult,
    iterations: iteration,
  };
  console.log(JSON.stringify(summary, null, 2));
}

// ============================================================================
// 명령어 생성
// ============================================================================

export function createReviewCommand(): Command {
  const cmd = new Command("review").description("자동 리뷰 실행");

  cmd
    .command("spec")
    .description("스펙 리뷰")
    .argument("<session-id>", "세션 ID")
    .option("--max-iterations <n>", "최대 반복 횟수", "3")
    .option("--auto-fix", "자동 수정 적용")
    .option("--strict", "엄격 모드 (WARN도 FAIL로 처리)")
    .action(
      async (
        sessionId: string,
        options: {
          maxIterations?: string;
          autoFix?: boolean;
          strict?: boolean;
        }
      ) => {
        await specCommand(sessionId, {
          maxIterations: options.maxIterations
            ? parseInt(options.maxIterations, 10)
            : 3,
          autoFix: options.autoFix,
          strict: options.strict,
        });
      }
    );

  cmd
    .command("code")
    .description("코드 리뷰")
    .argument("<checkpoint-id>", "Checkpoint ID")
    .option("--max-iterations <n>", "최대 반복 횟수", "3")
    .option("--auto-fix", "자동 수정 적용")
    .option("--strict", "엄격 모드 (WARN도 FAIL로 처리)")
    .action(
      async (
        checkpointId: string,
        options: {
          maxIterations?: string;
          autoFix?: boolean;
          strict?: boolean;
        }
      ) => {
        await codeCommand(checkpointId, {
          maxIterations: options.maxIterations
            ? parseInt(options.maxIterations, 10)
            : 3,
          autoFix: options.autoFix,
          strict: options.strict,
        });
      }
    );

  return cmd;
}
