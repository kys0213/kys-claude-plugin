/**
 * session 명령어 - 프로젝트 세션 관리
 */

import { Command } from "commander";
import { existsSync, mkdirSync, readdirSync, rmSync } from "fs";
import { readFile, writeFile, mkdir } from "fs/promises";
import { join } from "path";
import { randomBytes } from "crypto";
import { log, printSection, printStatus, printKV, icon } from "../lib/utils";
import { ProjectContext } from "../lib/context";

interface Session {
  id: string;
  title: string;
  status: "active" | "completed" | "failed";
  createdAt: string;
  updatedAt: string;
  metadata?: Record<string, unknown>;
}

function generateSessionId(): string {
  return randomBytes(4).toString("hex");
}

async function getSessionPath(id: string): Promise<string> {
  const ctx = await ProjectContext.getInstance();
  return join(ctx.sessionsDir, id, "session.json");
}

async function getSessionDir(id: string): Promise<string> {
  const ctx = await ProjectContext.getInstance();
  return join(ctx.sessionsDir, id);
}

async function readSession(id: string): Promise<Session | null> {
  const sessionPath = await getSessionPath(id);
  if (!existsSync(sessionPath)) {
    return null;
  }
  try {
    const content = await readFile(sessionPath, "utf-8");
    return JSON.parse(content);
  } catch (error) {
    log.err(`세션 읽기 실패 ${id}: ${error}`);
    return null;
  }
}

async function writeSession(session: Session): Promise<void> {
  const sessionDir = await getSessionDir(session.id);
  if (!existsSync(sessionDir)) {
    mkdirSync(sessionDir, { recursive: true });
  }
  const sessionPath = await getSessionPath(session.id);
  await writeFile(sessionPath, JSON.stringify(session, null, 2), "utf-8");
}

async function listAllSessions(): Promise<Session[]> {
  const ctx = await ProjectContext.getInstance();

  if (!existsSync(ctx.sessionsDir)) {
    return [];
  }

  const sessions: Session[] = [];
  const entries = readdirSync(ctx.sessionsDir, { withFileTypes: true });

  for (const entry of entries) {
    if (entry.isDirectory()) {
      const session = await readSession(entry.name);
      if (session) {
        sessions.push(session);
      }
    }
  }

  return sessions.sort(
    (a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
  );
}

async function deleteSession(id: string): Promise<boolean> {
  const sessionDir = await getSessionDir(id);
  if (!existsSync(sessionDir)) {
    return false;
  }

  try {
    rmSync(sessionDir, { recursive: true, force: true });
    return true;
  } catch (error) {
    log.err(`세션 삭제 실패 ${id}: ${error}`);
    return false;
  }
}

// ============================================================================
// create - 세션 생성
// ============================================================================

async function createCommand(title: string): Promise<void> {
  const id = generateSessionId();
  const now = new Date().toISOString();

  const session: Session = {
    id,
    title,
    status: "active",
    createdAt: now,
    updatedAt: now,
  };

  await writeSession(session);

  printSection("세션 생성됨");
  printKV("ID", id);
  printKV("제목", title);
  printKV("상태", session.status);
  printKV("생성일", session.createdAt);
  log.ok(`${icon.check} 세션이 성공적으로 생성되었습니다.`);

  // 세션 ID 출력 (스크립트에서 사용 가능)
  console.log();
  console.log(id);
}

// ============================================================================
// list - 세션 목록
// ============================================================================

async function listCommand(): Promise<void> {
  const sessions = await listAllSessions();

  if (sessions.length === 0) {
    log.info("세션이 없습니다.");
    return;
  }

  printSection("세션 목록");
  console.log();

  for (const session of sessions) {
    const statusIcon =
      session.status === "active"
        ? "🟢"
        : session.status === "completed"
          ? "✅"
          : "❌";

    console.log(`  ${statusIcon} ${session.id} - ${session.title}`);
    console.log(`    상태: ${session.status}`);
    console.log(`    생성: ${session.createdAt}`);
    console.log(`    수정: ${session.updatedAt}`);
    if (session.metadata && Object.keys(session.metadata).length > 0) {
      console.log(`    메타데이터: ${JSON.stringify(session.metadata)}`);
    }
    console.log();
  }
}

// ============================================================================
// show - 세션 상세
// ============================================================================

async function showCommand(id: string): Promise<void> {
  const session = await readSession(id);

  if (!session) {
    log.err(`세션을 찾을 수 없습니다: ${id}`);
    process.exit(1);
  }

  printSection("세션 상세");
  printKV("ID", session.id);
  printKV("제목", session.title);
  printKV("상태", session.status);
  printKV("생성일", session.createdAt);
  printKV("수정일", session.updatedAt);

  if (session.metadata && Object.keys(session.metadata).length > 0) {
    console.log();
    printSection("메타데이터");
    for (const [key, value] of Object.entries(session.metadata)) {
      printKV(key, JSON.stringify(value));
    }
  }
}

// ============================================================================
// delete - 세션 삭제
// ============================================================================

async function deleteCommand(id: string): Promise<void> {
  const session = await readSession(id);

  if (!session) {
    log.err(`세션을 찾을 수 없습니다: ${id}`);
    process.exit(1);
  }

  const success = await deleteSession(id);
  if (success) {
    log.ok(`${icon.check} 세션 ${id} 삭제됨`);
  } else {
    log.err(`세션 삭제 실패: ${id}`);
    process.exit(1);
  }
}

// ============================================================================
// update - 세션 업데이트
// ============================================================================

async function updateCommand(
  id: string,
  key: string,
  value: string
): Promise<void> {
  const session = await readSession(id);

  if (!session) {
    log.err(`세션을 찾을 수 없습니다: ${id}`);
    process.exit(1);
  }

  if (key === "status") {
    if (!["active", "completed", "failed"].includes(value)) {
      log.err(`유효하지 않은 상태: ${value}`);
      log.info("유효한 상태: active, completed, failed");
      process.exit(1);
    }
    session.status = value as "active" | "completed" | "failed";
  } else if (key === "title") {
    session.title = value;
  } else if (key.startsWith("metadata.")) {
    const metaKey = key.substring("metadata.".length);
    if (!session.metadata) {
      session.metadata = {};
    }
    try {
      session.metadata[metaKey] = JSON.parse(value);
    } catch {
      session.metadata[metaKey] = value;
    }
  } else {
    log.err(`유효하지 않은 키: ${key}`);
    log.info("유효한 키: status, title, metadata.*");
    process.exit(1);
  }

  session.updatedAt = new Date().toISOString();
  await writeSession(session);

  log.ok(`${icon.check} 세션 ${id} 업데이트됨`);
  printKV("변경된 필드", key);
  printKV("새 값", value);
}

// ============================================================================
// 명령어 생성
// ============================================================================

export function createSessionCommand(): Command {
  const cmd = new Command("session").description("프로젝트 세션 관리");

  cmd
    .command("create")
    .description("새 세션 생성")
    .argument("<title>", "세션 제목")
    .action(async (title: string) => {
      await createCommand(title);
    });

  cmd
    .command("list")
    .description("세션 목록")
    .action(async () => {
      await listCommand();
    });

  cmd
    .command("show")
    .description("세션 상세")
    .argument("<id>", "세션 ID")
    .action(async (id: string) => {
      await showCommand(id);
    });

  cmd
    .command("delete")
    .description("세션 삭제")
    .argument("<id>", "세션 ID")
    .action(async (id: string) => {
      await deleteCommand(id);
    });

  cmd
    .command("update")
    .description("세션 업데이트")
    .argument("<id>", "세션 ID")
    .argument("<key>", "변경할 필드 (status, title, metadata.*)")
    .argument("<value>", "새 값")
    .action(async (id: string, key: string, value: string) => {
      await updateCommand(id, key, value);
    });

  return cmd;
}
