/**
 * tc worktree - Git Worktree 관리
 * Git worktree를 생성/관리하는 명령어
 */

import { Command } from "commander";
import { execSync } from "child_process";
import { existsSync, readdirSync } from "fs";
import { join, basename } from "path";
import { getWorktreesDir, findGitRoot, ensureDir } from "../lib/common";
import { log } from "../lib/utils";

// ============================================================================
// 타입 정의
// ============================================================================

interface WorktreeInfo {
  checkpointId: string;
  path: string;
  branch: string;
}

// ============================================================================
// 헬퍼 함수
// ============================================================================

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

function parseWorktreeList(): WorktreeInfo[] {
  const root = findGitRoot();
  const worktreesDir = getWorktreesDir();
  const worktrees: WorktreeInfo[] = [];

  try {
    const output = execGit("worktree list", root);
    const lines = output.split("\n");

    for (const line of lines) {
      if (line.includes(worktreesDir)) {
        const parts = line.split(/\s+/);
        const path = parts[0];
        const branchMatch = line.match(/\[(.*?)\]/);
        const branch = branchMatch ? branchMatch[1] : "";
        const checkpointId = basename(path);

        worktrees.push({
          checkpointId,
          path,
          branch,
        });
      }
    }
  } catch {
    // git worktree list가 실패하면 빈 배열 반환
    return [];
  }

  return worktrees;
}

// ============================================================================
// 명령어: create
// ============================================================================

async function cmdCreate(checkpointId: string): Promise<void> {
  if (!checkpointId) {
    log.err("Checkpoint ID를 지정하세요.");
    log.err("사용법: tc worktree create <checkpoint-id>");
    process.exit(1);
  }

  const root = findGitRoot();
  const worktreesDir = getWorktreesDir();
  const worktreePath = join(worktreesDir, checkpointId);
  const branchName = `team-claude/${checkpointId}`;

  // worktrees 디렉토리 생성
  ensureDir(worktreesDir);

  // 이미 존재하는지 확인
  if (existsSync(worktreePath)) {
    log.warn(`Worktree가 이미 존재합니다: ${worktreePath}`);
    console.log(worktreePath);
    return;
  }

  // 브랜치가 이미 존재하는지 확인
  let branchExists = false;
  try {
    execGit(`show-ref --verify --quiet refs/heads/${branchName}`, root);
    branchExists = true;
  } catch {
    branchExists = false;
  }

  try {
    if (branchExists) {
      log.info(`브랜치가 이미 존재함: ${branchName}`);
      execGit(`worktree add "${worktreePath}" "${branchName}"`, root);
    } else {
      execGit(`worktree add -b "${branchName}" "${worktreePath}"`, root);
    }
  } catch (error: unknown) {
    const err = error as Error;
    log.err(`Worktree 생성 실패: ${worktreePath}`);
    log.err(err.message);
    process.exit(1);
  }

  log.ok(`Worktree 생성됨: ${worktreePath}`);
  log.ok(`브랜치: ${branchName}`);
  console.log(worktreePath);
}

// ============================================================================
// 명령어: list
// ============================================================================

async function cmdList(): Promise<void> {
  const worktrees = parseWorktreeList();

  // JSON 출력 (파싱용)
  console.log(JSON.stringify(worktrees, null, 2));
}

// ============================================================================
// 명령어: delete
// ============================================================================

async function cmdDelete(checkpointId: string): Promise<void> {
  if (!checkpointId) {
    log.err("Checkpoint ID를 지정하세요.");
    log.err("사용법: tc worktree delete <checkpoint-id>");
    process.exit(1);
  }

  const root = findGitRoot();
  const worktreesDir = getWorktreesDir();
  const worktreePath = join(worktreesDir, checkpointId);
  const branchName = `team-claude/${checkpointId}`;

  if (!existsSync(worktreePath)) {
    log.err(`Worktree를 찾을 수 없습니다: ${worktreePath}`);
    process.exit(1);
  }

  // Worktree 제거
  try {
    execGit(`worktree remove "${worktreePath}" --force`, root);
  } catch {
    log.warn("git worktree remove 실패, 수동 삭제 시도...");
    try {
      execSync(`rm -rf "${worktreePath}"`);
      execGit("worktree prune", root);
    } catch (error: unknown) {
      const err = error as Error;
      log.err(`Worktree 삭제 실패: ${err.message}`);
      process.exit(1);
    }
  }

  log.ok(`Worktree 삭제됨: ${worktreePath}`);
  log.info(`브랜치 '${branchName}'는 유지됩니다.`);
  log.info(`브랜치 삭제: git branch -D ${branchName}`);
}

// ============================================================================
// 명령어: cleanup
// ============================================================================

async function cmdCleanup(options: { dryRun?: boolean }): Promise<void> {
  const root = findGitRoot();
  const worktrees = parseWorktreeList();

  console.log();
  console.log("━━━ Team Claude Worktree 정리 ━━━");
  console.log();

  if (worktrees.length === 0) {
    log.info("정리할 worktree가 없습니다.");
    return;
  }

  let cleaned = 0;

  for (const worktree of worktrees) {
    if (options.dryRun) {
      log.info(`[DRY RUN] 삭제 예정: ${worktree.checkpointId}`);
      cleaned++;
      continue;
    }

    log.info(`삭제 중: ${worktree.checkpointId}`);

    try {
      execGit(`worktree remove "${worktree.path}" --force`, root);
    } catch {
      log.warn("git worktree remove 실패, 수동 삭제...");
      try {
        execSync(`rm -rf "${worktree.path}"`);
      } catch {
        log.err(`삭제 실패: ${worktree.checkpointId}`);
        continue;
      }
    }

    cleaned++;
  }

  // prune 실행 (dry-run이 아닐 때만)
  if (!options.dryRun) {
    execGit("worktree prune", root);
  }

  console.log();
  if (options.dryRun) {
    log.info(`삭제 예정: ${cleaned}개의 worktree`);
  } else {
    log.ok(`${cleaned}개의 worktree 정리됨`);
  }

  // 디렉토리가 비어있으면 삭제 (dry-run이 아닐 때만)
  if (!options.dryRun) {
    const worktreesDir = getWorktreesDir();
    if (existsSync(worktreesDir)) {
      try {
        const files = readdirSync(worktreesDir);
        if (files.length === 0) {
          execSync(`rmdir "${worktreesDir}"`);
        }
      } catch {
        // 디렉토리 삭제 실패는 무시
      }
    }
  }

  console.log();
}

// ============================================================================
// 명령어: path
// ============================================================================

async function cmdPath(checkpointId: string): Promise<void> {
  if (!checkpointId) {
    log.err("Checkpoint ID를 지정하세요.");
    log.err("사용법: tc worktree path <checkpoint-id>");
    process.exit(1);
  }

  const worktreesDir = getWorktreesDir();
  const worktreePath = join(worktreesDir, checkpointId);

  if (!existsSync(worktreePath)) {
    log.err(`Worktree를 찾을 수 없습니다: ${worktreePath}`);
    process.exit(1);
  }

  console.log(worktreePath);
}

// ============================================================================
// 명령어 등록
// ============================================================================

export function createWorktreeCommand(): Command {
  const worktree = new Command("worktree").description(
    "Git Worktree 관리 - 브랜치별 독립 작업 공간"
  );

  worktree
    .command("create <checkpoint-id>")
    .description("Worktree + 브랜치 생성")
    .action(cmdCreate);

  worktree
    .command("list")
    .description("Worktree 목록 (JSON 출력)")
    .action(cmdList);

  worktree
    .command("delete <checkpoint-id>")
    .description("Worktree 삭제")
    .action(cmdDelete);

  worktree
    .command("cleanup")
    .description("모든 team-claude worktree 정리")
    .option("--dry-run", "실제 삭제 없이 확인만")
    .action(cmdCleanup);

  worktree
    .command("path <checkpoint-id>")
    .description("Worktree 경로 출력")
    .action(cmdPath);

  return worktree;
}
