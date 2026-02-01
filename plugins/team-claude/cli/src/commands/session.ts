/**
 * tc session - 세션 관리 커맨드
 */

import { Command } from "commander";
import { existsSync, mkdirSync, readFileSync, writeFileSync, readdirSync, rmSync } from "fs";
import { join } from "path";
import { ProjectContext } from "../lib/context";

// ============================================================================
// 타입 정의
// ============================================================================

interface SessionMeta {
  sessionId: string;
  title: string;
  status: string;
  phase: string;
  createdAt: string;
  updatedAt: string;
  decisions: unknown[];
  checkpointsApproved: boolean;
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

function generateId(): string {
  const chars = "abcdefghijklmnopqrstuvwxyz0123456789";
  let result = "";
  for (let i = 0; i < 8; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
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
// create 핸들러
// ============================================================================

async function handleCreate(
  title: string,
  options: { json?: boolean }
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  if (!title) {
    if (json) {
      outputError("MISSING_TITLE", "세션 제목을 지정하세요.");
    } else {
      console.error("[ERR] 세션 제목을 지정하세요.");
      console.error("사용법: tc session create <title>");
    }
    process.exit(1);
  }

  const ctx = await ProjectContext.getInstance();
  const sessionsDir = ctx.sessionsDir;

  if (!existsSync(sessionsDir)) {
    mkdirSync(sessionsDir, { recursive: true });
  }

  const sessionId = generateId();
  const sessionDir = join(sessionsDir, sessionId);

  // 세션 디렉토리 구조 생성
  mkdirSync(join(sessionDir, "specs"), { recursive: true });
  mkdirSync(join(sessionDir, "checkpoints"), { recursive: true });
  mkdirSync(join(sessionDir, "contracts"), { recursive: true });
  mkdirSync(join(sessionDir, "delegations"), { recursive: true });

  const now = timestamp();
  const meta: SessionMeta = {
    sessionId,
    title,
    status: "designing",
    phase: "initial",
    createdAt: now,
    updatedAt: now,
    decisions: [],
    checkpointsApproved: false,
  };

  writeFileSync(join(sessionDir, "meta.json"), JSON.stringify(meta, null, 2));

  if (json) {
    outputJson({ sessionId, path: sessionDir }, startTime);
  } else {
    console.log(`[OK] 세션 생성됨: ${sessionId}`);
    console.log(`  경로: ${sessionDir}`);
  }
}

// ============================================================================
// list 핸들러
// ============================================================================

async function handleList(options: { json?: boolean }): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  const ctx = await ProjectContext.getInstance();
  const sessionsDir = ctx.sessionsDir;

  if (!existsSync(sessionsDir)) {
    if (json) {
      outputJson([], startTime);
    } else {
      console.log("[INFO] 세션이 없습니다.");
    }
    return;
  }

  const sessions: Array<{ sessionId: string; title: string; status: string; createdAt: string }> = [];

  const dirs = readdirSync(sessionsDir, { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => d.name);

  for (const dir of dirs) {
    const metaPath = join(sessionsDir, dir, "meta.json");
    if (existsSync(metaPath)) {
      try {
        const meta = JSON.parse(readFileSync(metaPath, "utf-8")) as SessionMeta;
        sessions.push({
          sessionId: meta.sessionId,
          title: meta.title,
          status: meta.status,
          createdAt: meta.createdAt,
        });
      } catch {
        // 무시
      }
    }
  }

  if (json) {
    outputJson(sessions, startTime);
  } else {
    if (sessions.length === 0) {
      console.log("[INFO] 세션이 없습니다.");
    } else {
      console.log("\n━━━ 세션 목록 ━━━\n");
      for (const s of sessions) {
        console.log(`  ${s.sessionId}  ${s.title}  [${s.status}]`);
      }
      console.log("");
    }
  }
}

// ============================================================================
// show 핸들러
// ============================================================================

async function handleShow(
  id: string,
  options: { json?: boolean }
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  if (!id) {
    if (json) {
      outputError("MISSING_ID", "세션 ID를 지정하세요.");
    } else {
      console.error("[ERR] 세션 ID를 지정하세요.");
    }
    process.exit(1);
  }

  const ctx = await ProjectContext.getInstance();
  const metaPath = join(ctx.sessionsDir, id, "meta.json");

  if (!existsSync(metaPath)) {
    if (json) {
      outputError("NOT_FOUND", `세션을 찾을 수 없습니다: ${id}`);
    } else {
      console.error(`[ERR] 세션을 찾을 수 없습니다: ${id}`);
    }
    process.exit(1);
  }

  const meta = JSON.parse(readFileSync(metaPath, "utf-8")) as SessionMeta;

  if (json) {
    outputJson(meta, startTime);
  } else {
    console.log("\n━━━ 세션 상세 ━━━\n");
    console.log(`  ID: ${meta.sessionId}`);
    console.log(`  제목: ${meta.title}`);
    console.log(`  상태: ${meta.status}`);
    console.log(`  단계: ${meta.phase}`);
    console.log(`  생성: ${meta.createdAt}`);
    console.log(`  수정: ${meta.updatedAt}`);
    console.log("");
  }
}

// ============================================================================
// delete 핸들러
// ============================================================================

async function handleDelete(
  id: string,
  options: { json?: boolean }
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  if (!id) {
    if (json) {
      outputError("MISSING_ID", "세션 ID를 지정하세요.");
    } else {
      console.error("[ERR] 세션 ID를 지정하세요.");
    }
    process.exit(1);
  }

  const ctx = await ProjectContext.getInstance();
  const sessionDir = join(ctx.sessionsDir, id);

  if (!existsSync(sessionDir)) {
    if (json) {
      outputError("NOT_FOUND", `세션을 찾을 수 없습니다: ${id}`);
    } else {
      console.error(`[ERR] 세션을 찾을 수 없습니다: ${id}`);
    }
    process.exit(1);
  }

  rmSync(sessionDir, { recursive: true, force: true });

  if (json) {
    outputJson({ deleted: id }, startTime);
  } else {
    console.log(`[OK] 세션 삭제됨: ${id}`);
  }
}

// ============================================================================
// 커맨드 생성
// ============================================================================

export function createSessionCommand(): Command {
  const session = new Command("session")
    .description("세션 관리")
    .addHelpText(
      "after",
      `
Examples:
  tc session create "쿠폰 할인 기능"
  tc session list
  tc session show abc12345
  tc session delete abc12345
`
    );

  session
    .command("create <title>")
    .description("새 세션 생성")
    .option("--json", "JSON 형식으로 출력")
    .action(handleCreate);

  session
    .command("list")
    .description("세션 목록")
    .option("--json", "JSON 형식으로 출력")
    .action(handleList);

  session
    .command("show <id>")
    .description("세션 상세 정보")
    .option("--json", "JSON 형식으로 출력")
    .action(handleShow);

  session
    .command("delete <id>")
    .description("세션 삭제")
    .option("--json", "JSON 형식으로 출력")
    .action(handleDelete);

  return session;
}
