/**
 * tc psm - Parallel Session Manager
 * git worktree 기반 병렬 세션 관리
 */

import { Command } from "commander";
import chalk from "chalk";
import { execSync } from "child_process";
import { join, dirname } from "path";
import {
  existsSync,
  readdirSync,
  writeFileSync,
  readFileSync,
  mkdirSync,
  rmSync,
} from "fs";
import {
  getProjectDataDir,
  getWorktreesDir,
  findGitRoot,
  ensureDir,
  timestamp,
  readJsonFile,
  writeJsonFile,
} from "../lib/common";
import { log, printSection, printKV } from "../lib/utils";

// ============================================================================
// 타입 정의
// ============================================================================

interface PsmSession {
  name: string;
  status: "active" | "paused" | "complete" | "error";
  progress: string;
  worktreePath: string;
  branch: string;
  createdAt: string;
  updatedAt: string;
}

interface PsmIndex {
  sessions: PsmSession[];
  settings: {
    parallelLimit: number;
    autoCleanup: boolean;
  };
  createdAt: string;
}

// ============================================================================
// 헬퍼 함수
// ============================================================================

function getPsmIndexPath(): string {
  return join(getProjectDataDir(), "psm-index.json");
}

function initPsmIndex(): PsmIndex {
  const indexPath = getPsmIndexPath();

  if (existsSync(indexPath)) {
    return readJsonFile<PsmIndex>(indexPath)!;
  }

  const index: PsmIndex = {
    sessions: [],
    settings: {
      parallelLimit: 4,
      autoCleanup: true,
    },
    createdAt: timestamp(),
  };

  ensureDir(getProjectDataDir());
  writeJsonFile(indexPath, index);
  return index;
}

function savePsmIndex(index: PsmIndex): void {
  writeJsonFile(getPsmIndexPath(), index);
}

function getSessionInfo(name: string): PsmSession | null {
  const index = initPsmIndex();
  return index.sessions.find((s) => s.name === name) || null;
}

function addSessionToIndex(session: PsmSession): void {
  const index = initPsmIndex();
  index.sessions.push(session);
  savePsmIndex(index);
}

function updateSessionInIndex(
  name: string,
  updates: Partial<PsmSession>
): void {
  const index = initPsmIndex();
  const session = index.sessions.find((s) => s.name === name);
  if (session) {
    Object.assign(session, updates, { updatedAt: timestamp() });
    savePsmIndex(index);
  }
}

function removeSessionFromIndex(name: string): void {
  const index = initPsmIndex();
  index.sessions = index.sessions.filter((s) => s.name !== name);
  savePsmIndex(index);
}

function execGit(args: string, cwd?: string): string {
  try {
    return execSync(`git ${args}`, {
      encoding: "utf-8",
      cwd: cwd || findGitRoot(),
      stdio: ["pipe", "pipe", "pipe"],
    }).trim();
  } catch (error: unknown) {
    const err = error as { stderr?: string; message: string };
    throw new Error(err.stderr || err.message);
  }
}

// ============================================================================
// PSM Hooks 설치 (worktree별)
// ============================================================================

// PSM hooks 설정 (settings.local.json용) - CLI 호출 사용
function getPsmHooksConfig(): Record<string, unknown[]> {
  return {
    Stop: [
      {
        matcher: "",
        description: "Worker 완료 시 자동 검증 트리거",
        hooks: [{ type: "command", command: "tc hook worker-complete", timeout: 30 }],
      },
    ],
    PreToolUse: [
      {
        matcher: "Task",
        description: "Worker 질문 시 에스컬레이션",
        hooks: [{ type: "command", command: "tc hook worker-question", timeout: 10 }],
      },
    ],
    PostToolUse: [
      {
        matcher: "Bash",
        description: "Bash 실행 후 결과 분석",
        hooks: [{ type: "command", command: "tc hook validation-complete", timeout: 60 }],
      },
    ],
    Notification: [
      {
        matcher: "idle_prompt",
        description: "Worker 대기 상태 감지",
        hooks: [{ type: "command", command: "tc hook worker-idle", timeout: 5 }],
      },
    ],
  };
}

function installPsmHooks(worktreePath: string): void {
  // settings.local.json에 hooks 설정 추가 (CLI 호출 사용)
  const settingsPath = join(worktreePath, ".claude", "settings.local.json");
  mkdirSync(dirname(settingsPath), { recursive: true });

  let existingSettings: Record<string, unknown> = {};

  // 기존 설정 읽기
  if (existsSync(settingsPath)) {
    try {
      const content = readFileSync(settingsPath, "utf-8");
      existingSettings = JSON.parse(content) as Record<string, unknown>;
    } catch {
      // JSON 파싱 실패시 빈 객체로 시작
      existingSettings = {};
    }
  }

  // hooks 설정 병합
  const existingHooks = (existingSettings.hooks || {}) as Record<
    string,
    unknown[]
  >;
  const psmHooks = getPsmHooksConfig();

  // 각 hook 타입별로 병합 (기존 hooks 보존하면서 PSM hooks 추가)
  for (const [hookType, psmHookEntries] of Object.entries(psmHooks)) {
    const existingEntries = existingHooks[hookType] || [];

    // PSM hook이 이미 추가되어 있는지 확인 (command로 체크)
    const psmCommands = psmHookEntries.map((entry) => {
      const e = entry as Record<string, unknown>;
      return e.command || (e.hooks as Array<{ command: string }>)?.[0]?.command;
    });

    const filteredPsmEntries = psmHookEntries.filter((entry) => {
      const e = entry as Record<string, unknown>;
      const cmd =
        e.command || (e.hooks as Array<{ command: string }>)?.[0]?.command;

      // 이미 동일한 command가 있으면 추가하지 않음
      return !existingEntries.some((existing) => {
        const ex = existing as Record<string, unknown>;
        const existingCmd =
          ex.command ||
          (ex.hooks as Array<{ command: string }>)?.[0]?.command;
        return existingCmd === cmd;
      });
    });

    if (filteredPsmEntries.length > 0) {
      existingHooks[hookType] = [...existingEntries, ...filteredPsmEntries];
    } else if (!existingHooks[hookType] && existingEntries.length === 0) {
      existingHooks[hookType] = psmHookEntries;
    }
  }

  existingSettings.hooks = existingHooks;

  writeFileSync(settingsPath, JSON.stringify(existingSettings, null, 2));
}

// ============================================================================
// 명령어: new
// ============================================================================

async function cmdNew(
  sessionName: string,
  options: { from?: string }
): Promise<void> {
  // 유효한 세션 이름인지 확인
  if (!/^[a-zA-Z][a-zA-Z0-9-]*$/.test(sessionName)) {
    log.err(`유효하지 않은 세션 이름: ${sessionName}`);
    log.err("영문자로 시작하고, 영문자/숫자/하이픈만 사용 가능합니다.");
    process.exit(1);
  }

  const worktreesDir = getWorktreesDir();
  const worktreePath = join(worktreesDir, sessionName);
  const branchName = `team-claude/${sessionName}`;

  // 이미 존재하는지 확인
  if (existsSync(worktreePath)) {
    log.warn(`세션이 이미 존재합니다: ${sessionName}`);
    console.log(worktreePath);
    return;
  }

  ensureDir(worktreesDir);

  const root = findGitRoot();

  // 기준 브랜치 결정
  let baseBranch: string;
  if (options.from) {
    baseBranch = `team-claude/${options.from}`;
    try {
      execGit(`show-ref --verify --quiet refs/heads/${baseBranch}`);
    } catch {
      log.err(`소스 세션 브랜치가 없습니다: ${baseBranch}`);
      process.exit(1);
    }
  } else {
    baseBranch = execGit("rev-parse --abbrev-ref HEAD");
  }

  // 브랜치가 이미 존재하는지 확인
  let branchExists = false;
  try {
    execGit(`show-ref --verify --quiet refs/heads/${branchName}`);
    branchExists = true;
  } catch {
    branchExists = false;
  }

  try {
    if (branchExists) {
      log.info(`브랜치가 이미 존재함: ${branchName}`);
      execGit(`worktree add "${worktreePath}" "${branchName}"`);
    } else {
      execGit(`worktree add -b "${branchName}" "${worktreePath}" "${baseBranch}"`);
    }
  } catch (error: unknown) {
    const err = error as Error;
    log.err(`Worktree 생성 실패: ${worktreePath}`);
    log.err(err.message);
    process.exit(1);
  }

  // PSM Hooks 설치 (worktree에)
  installPsmHooks(worktreePath);

  // 세션 메타데이터 생성
  const sessionMetaDir = join(worktreePath, ".team-claude-session");
  ensureDir(sessionMetaDir);

  const meta = {
    name: sessionName,
    status: "active",
    worktreePath,
    branch: branchName,
    baseBranch,
    fromSession: options.from || null,
    createdAt: timestamp(),
    updatedAt: timestamp(),
    progress: {
      total: 0,
      completed: 0,
      inProgress: 0,
      pending: 0,
    },
    checkpoints: [],
  };

  writeJsonFile(join(sessionMetaDir, "meta.json"), meta);

  // CLAUDE.md 생성
  const claudeMd = `# Session: ${sessionName}

## Overview
이 세션은 PSM(Parallel Session Manager)에 의해 생성되었습니다.

## Branch
\`${branchName}\`

## Instructions
1. 이 worktree에서 독립적으로 작업합니다.
2. 작업 완료 후 PR을 생성합니다.
3. 다른 세션과의 충돌에 주의하세요.

## Context
- 생성일: ${new Date().toISOString()}
- 기준 브랜치: ${baseBranch}
${options.from ? `- 소스 세션: ${options.from}` : ""}
`;

  writeJsonFile(join(worktreePath, "CLAUDE.md"), claudeMd);

  // PSM 인덱스에 추가
  addSessionToIndex({
    name: sessionName,
    status: "active",
    progress: "0/0",
    worktreePath,
    branch: branchName,
    createdAt: timestamp(),
    updatedAt: timestamp(),
  });

  console.log();
  log.ok(`새 세션 생성: ${sessionName}`);
  console.log();
  printKV("Worktree", worktreePath);
  printKV("브랜치", branchName);
  printKV("상태", "initialized");
  console.log();
  console.log("  다음 단계:");
  console.log(`    cd ${worktreePath}`);
  console.log("    또는");
  console.log(`    /team-claude:psm switch ${sessionName}`);
  console.log();

  console.log(worktreePath);
}

// ============================================================================
// 명령어: list
// ============================================================================

async function cmdList(options: { status?: string }): Promise<void> {
  const index = initPsmIndex();

  let sessions = index.sessions;
  if (options.status) {
    sessions = sessions.filter((s) => s.status === options.status);
  }

  printSection("PSM Sessions");

  if (sessions.length === 0) {
    log.info("세션이 없습니다.");
    console.log();
    return;
  }

  // 헤더
  console.log(
    chalk.gray(
      `  ${"NAME".padEnd(20)} ${"STATUS".padEnd(12)} ${"BRANCH".padEnd(35)} ${"PROGRESS"}`
    )
  );
  console.log("  " + "─".repeat(75));

  // 세션 목록
  for (const session of sessions) {
    let icon = "❓";
    switch (session.status) {
      case "active":
        icon = "🔄";
        break;
      case "paused":
        icon = "⏸️";
        break;
      case "complete":
        icon = "✅";
        break;
      case "error":
        icon = "❌";
        break;
    }

    console.log(
      `  ${session.name.padEnd(20)} ${icon} ${session.status.padEnd(10)} ${session.branch.padEnd(35)} ${session.progress}`
    );
  }

  console.log();

  // 통계
  const stats = {
    active: sessions.filter((s) => s.status === "active").length,
    paused: sessions.filter((s) => s.status === "paused").length,
    complete: sessions.filter((s) => s.status === "complete").length,
  };

  console.log(
    `  Total: ${sessions.length} sessions (${stats.active} active, ${stats.paused} paused, ${stats.complete} complete)`
  );
  console.log();
}

// ============================================================================
// 명령어: status
// ============================================================================

async function cmdStatus(sessionName?: string): Promise<void> {
  const index = initPsmIndex();

  if (sessionName) {
    // 특정 세션 상태
    const session = getSessionInfo(sessionName);

    if (!session) {
      log.err(`세션을 찾을 수 없습니다: ${sessionName}`);
      process.exit(1);
    }

    let icon = "❓";
    switch (session.status) {
      case "active":
        icon = "🔄";
        break;
      case "paused":
        icon = "⏸️";
        break;
      case "complete":
        icon = "✅";
        break;
      case "error":
        icon = "❌";
        break;
    }

    printSection(`Session: ${sessionName}`);

    printKV("상태", `${icon} ${session.status}`);
    printKV("브랜치", session.branch);
    printKV("Worktree", session.worktreePath);
    printKV("진행률", session.progress);
    console.log();
  } else {
    // 전체 상태
    printSection("PSM Status");

    const stats = {
      active: index.sessions.filter((s) => s.status === "active").length,
      paused: index.sessions.filter((s) => s.status === "paused").length,
      complete: index.sessions.filter((s) => s.status === "complete").length,
    };

    printKV("Active Sessions", String(stats.active));
    printKV("Paused Sessions", String(stats.paused));
    printKV("Complete Sessions", String(stats.complete));
    console.log();

    printSection("Resource Usage");

    const worktreesDir = getWorktreesDir();
    let worktreeCount = 0;

    if (existsSync(worktreesDir)) {
      worktreeCount = readdirSync(worktreesDir).length;
    }

    printKV("Worktrees", String(worktreeCount));
    console.log();
  }
}

// ============================================================================
// 명령어: switch
// ============================================================================

async function cmdSwitch(sessionName: string): Promise<void> {
  const session = getSessionInfo(sessionName);

  if (!session) {
    log.err(`세션을 찾을 수 없습니다: ${sessionName}`);
    process.exit(1);
  }

  if (!existsSync(session.worktreePath)) {
    log.err(`Worktree 디렉토리가 없습니다: ${session.worktreePath}`);
    log.err("세션을 정리하고 다시 생성하세요.");
    process.exit(1);
  }

  console.log();
  log.ok(`세션 전환: ${sessionName}`);
  console.log();
  printKV("Worktree", session.worktreePath);
  printKV("상태", session.status);
  printKV("진행률", session.progress);
  console.log();
  console.log("  실행:");
  console.log(`    cd ${session.worktreePath}`);
  console.log();

  // 환경 변수로 경로 출력
  console.log(`WORKTREE_PATH=${session.worktreePath}`);
}

// ============================================================================
// 명령어: parallel
// ============================================================================

async function cmdParallel(sessions: string[]): Promise<void> {
  if (sessions.length < 2) {
    log.err("최소 2개의 세션을 지정하세요.");
    log.err("사용법: tc psm parallel <session1> <session2> [session3...]");
    process.exit(1);
  }

  console.log();
  console.log(chalk.bold("🚀 병렬 실행 준비"));
  console.log();
  printKV("Sessions", String(sessions.length));
  console.log();

  printSection("세션 검증");

  const validSessions: PsmSession[] = [];

  for (const name of sessions) {
    const session = getSessionInfo(name);

    if (!session) {
      log.warn(`세션을 찾을 수 없음: ${name} (건너뜀)`);
      continue;
    }

    if (session.status === "complete") {
      log.info(`이미 완료됨: ${name} (건너뜀)`);
      continue;
    }

    if (!existsSync(session.worktreePath)) {
      log.warn(`Worktree 없음: ${name} (건너뜀)`);
      continue;
    }

    validSessions.push(session);
    log.ok(`준비됨: ${name}`);
  }

  console.log();

  if (validSessions.length === 0) {
    log.err("실행할 세션이 없습니다.");
    process.exit(1);
  }

  printSection("실행 계획");

  console.log(
    chalk.gray(`  ${"Session".padEnd(20)} ${"Status".padEnd(15)} Workers`)
  );
  console.log("  " + "─".repeat(50));

  for (const session of validSessions) {
    console.log(`  ${session.name.padEnd(20)} ${"ready".padEnd(15)} 1`);
  }

  console.log();
  console.log(`  총 Workers: ${validSessions.length}`);
  console.log();

  // 상태 업데이트
  for (const session of validSessions) {
    updateSessionInIndex(session.name, { status: "active" });
  }

  log.info(
    "병렬 실행을 시작하려면 각 세션의 worktree에서 Claude를 실행하세요."
  );
  console.log();

  for (const session of validSessions) {
    console.log(`  ${session.name}: cd ${session.worktreePath} && claude`);
  }

  console.log();
}

// ============================================================================
// 명령어: cleanup
// ============================================================================

async function cmdCleanup(
  sessionName?: string,
  options?: { all?: boolean; force?: boolean }
): Promise<void> {
  const index = initPsmIndex();

  console.log();
  console.log(chalk.bold("🧹 세션 정리"));
  console.log();

  let cleaned = 0;
  let skipped = 0;

  const root = findGitRoot();

  if (sessionName) {
    // 특정 세션 정리
    const session = getSessionInfo(sessionName);

    if (!session) {
      log.err(`세션을 찾을 수 없습니다: ${sessionName}`);
      process.exit(1);
    }

    if (session.status !== "complete" && !options?.force) {
      log.warn(
        `세션이 완료되지 않았습니다: ${sessionName} (status: ${session.status})`
      );
      log.warn("--force 옵션으로 강제 정리할 수 있습니다.");
      process.exit(1);
    }

    // Worktree 삭제
    if (existsSync(session.worktreePath)) {
      try {
        execGit(`worktree remove "${session.worktreePath}" --force`, root);
      } catch {
        log.warn("git worktree remove 실패, 수동 삭제...");
        rmSync(session.worktreePath, { recursive: true, force: true });
        execGit("worktree prune", root);
      }
    }

    removeSessionFromIndex(sessionName);
    log.ok(`정리 완료: ${sessionName}`);
    cleaned = 1;
  } else if (options?.all) {
    // 모든 세션 정리
    for (const session of [...index.sessions]) {
      if (session.status !== "complete" && !options?.force) {
        log.warn(`건너뜀 (미완료): ${session.name}`);
        skipped++;
        continue;
      }

      if (existsSync(session.worktreePath)) {
        try {
          execGit(`worktree remove "${session.worktreePath}" --force`, root);
        } catch {
          rmSync(session.worktreePath, { recursive: true, force: true });
        }
      }

      removeSessionFromIndex(session.name);
      log.info(`정리됨: ${session.name}`);
      cleaned++;
    }

    execGit("worktree prune", root);
  } else {
    // 완료된 세션만 정리
    const completedSessions = index.sessions.filter(
      (s) => s.status === "complete"
    );

    if (completedSessions.length === 0) {
      log.info("정리할 완료된 세션이 없습니다.");
      return;
    }

    for (const session of completedSessions) {
      if (existsSync(session.worktreePath)) {
        try {
          execGit(`worktree remove "${session.worktreePath}" --force`, root);
        } catch {
          rmSync(session.worktreePath, { recursive: true, force: true });
        }
      }

      removeSessionFromIndex(session.name);
      log.info(`정리됨: ${session.name}`);
      cleaned++;
    }

    execGit("worktree prune", root);
  }

  console.log();
  console.log(`  정리 완료: ${cleaned} 세션`);
  if (skipped > 0) {
    console.log(`  건너뜀: ${skipped} 세션`);
  }
  console.log();
}

// ============================================================================
// 명령어 등록
// ============================================================================

export function createPsmCommand(): Command {
  const psm = new Command("psm").description(
    "PSM (Parallel Session Manager) - git worktree 기반 병렬 세션 관리"
  );

  psm
    .command("new <session-name>")
    .description("새 세션 생성")
    .option("--from <session>", "기존 세션 기반으로 생성")
    .action(cmdNew);

  psm
    .command("list")
    .description("세션 목록")
    .option("--status <status>", "상태로 필터 (active|paused|complete)")
    .action(cmdList);

  psm
    .command("status [session-name]")
    .description("세션 상태 확인")
    .action(cmdStatus);

  psm
    .command("switch <session-name>")
    .description("세션 전환")
    .action(cmdSwitch);

  psm
    .command("parallel <sessions...>")
    .description("병렬 실행")
    .action(cmdParallel);

  psm
    .command("cleanup [session-name]")
    .description("세션 정리")
    .option("--all", "모든 세션 정리")
    .option("--force", "강제 정리 (미완료 세션 포함)")
    .action(cmdCleanup);

  return psm;
}
