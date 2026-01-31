/**
 * tc worktree - Git Worktree 관리 커맨드
 */

import { Command } from "commander";
import { existsSync, mkdirSync, rmSync } from "fs";
import { join } from "path";
import { execSync } from "child_process";
import { ProjectContext } from "../lib/context";

// ============================================================================
// 타입 정의
// ============================================================================

interface WorktreeInfo {
  id: string;
  path: string;
  branch: string;
  exists: boolean;
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

function execGit(args: string, cwd?: string): string {
  try {
    return execSync(`git ${args}`, {
      encoding: "utf-8",
      cwd,
      stdio: ["pipe", "pipe", "pipe"],
    }).trim();
  } catch (error: unknown) {
    const err = error as { stderr?: string; message: string };
    throw new Error(err.stderr || err.message);
  }
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
  id: string,
  options: { json?: boolean }
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  if (!id) {
    if (json) {
      outputError("MISSING_ID", "Worktree ID를 지정하세요.");
    } else {
      console.error("[ERR] Worktree ID를 지정하세요.");
    }
    process.exit(1);
  }

  const ctx = await ProjectContext.getInstance();
  const worktreesDir = ctx.worktreesDir;
  const worktreePath = join(worktreesDir, id);
  const branchName = `team-claude/${id}`;

  if (!existsSync(worktreesDir)) {
    mkdirSync(worktreesDir, { recursive: true });
  }

  if (existsSync(worktreePath)) {
    if (json) {
      outputJson({ path: worktreePath, branch: branchName, status: "already_exists" }, startTime);
    } else {
      console.log(`[WARN] Worktree가 이미 존재합니다: ${worktreePath}`);
    }
    return;
  }

  try {
    // 현재 브랜치 확인
    const currentBranch = execGit("rev-parse --abbrev-ref HEAD", ctx.gitRoot);

    // worktree 생성
    execGit(`worktree add -b "${branchName}" "${worktreePath}" ${currentBranch}`, ctx.gitRoot);

    if (json) {
      outputJson({ path: worktreePath, branch: branchName, status: "created" }, startTime);
    } else {
      console.log(`[OK] Worktree 생성됨`);
      console.log(`  경로: ${worktreePath}`);
      console.log(`  브랜치: ${branchName}`);
    }
  } catch (error) {
    if (json) {
      outputError("CREATE_FAILED", error instanceof Error ? error.message : String(error));
    } else {
      console.error(`[ERR] Worktree 생성 실패: ${error}`);
    }
    process.exit(1);
  }
}

// ============================================================================
// list 핸들러
// ============================================================================

async function handleList(options: { json?: boolean }): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  const ctx = await ProjectContext.getInstance();

  try {
    const output = execGit("worktree list --porcelain", ctx.gitRoot);
    const worktrees: WorktreeInfo[] = [];

    const blocks = output.split("\n\n").filter(Boolean);
    for (const block of blocks) {
      const lines = block.split("\n");
      let path = "";
      let branch = "";

      for (const line of lines) {
        if (line.startsWith("worktree ")) {
          path = line.substring(9);
        } else if (line.startsWith("branch ")) {
          branch = line.substring(7);
        }
      }

      if (path && branch.includes("team-claude/")) {
        const id = branch.replace("refs/heads/team-claude/", "");
        worktrees.push({
          id,
          path,
          branch: branch.replace("refs/heads/", ""),
          exists: existsSync(path),
        });
      }
    }

    if (json) {
      outputJson(worktrees, startTime);
    } else {
      if (worktrees.length === 0) {
        console.log("[INFO] Team Claude worktree가 없습니다.");
      } else {
        console.log("\n━━━ Worktree 목록 ━━━\n");
        for (const wt of worktrees) {
          console.log(`  ${wt.id}`);
          console.log(`    경로: ${wt.path}`);
          console.log(`    브랜치: ${wt.branch}`);
          console.log("");
        }
      }
    }
  } catch (error) {
    if (json) {
      outputError("LIST_FAILED", error instanceof Error ? error.message : String(error));
    } else {
      console.error(`[ERR] Worktree 목록 조회 실패: ${error}`);
    }
    process.exit(1);
  }
}

// ============================================================================
// path 핸들러
// ============================================================================

async function handlePath(
  id: string,
  options: { json?: boolean }
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  const ctx = await ProjectContext.getInstance();
  const worktreePath = join(ctx.worktreesDir, id);

  if (json) {
    outputJson({ id, path: worktreePath, exists: existsSync(worktreePath) }, startTime);
  } else {
    console.log(worktreePath);
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
      outputError("MISSING_ID", "Worktree ID를 지정하세요.");
    } else {
      console.error("[ERR] Worktree ID를 지정하세요.");
    }
    process.exit(1);
  }

  const ctx = await ProjectContext.getInstance();
  const worktreePath = join(ctx.worktreesDir, id);
  const branchName = `team-claude/${id}`;

  try {
    // worktree 제거
    if (existsSync(worktreePath)) {
      execGit(`worktree remove "${worktreePath}" --force`, ctx.gitRoot);
    }

    // 브랜치 삭제
    try {
      execGit(`branch -D "${branchName}"`, ctx.gitRoot);
    } catch {
      // 브랜치가 없으면 무시
    }

    if (json) {
      outputJson({ id, deleted: true }, startTime);
    } else {
      console.log(`[OK] Worktree 삭제됨: ${id}`);
    }
  } catch (error) {
    if (json) {
      outputError("DELETE_FAILED", error instanceof Error ? error.message : String(error));
    } else {
      console.error(`[ERR] Worktree 삭제 실패: ${error}`);
    }
    process.exit(1);
  }
}

// ============================================================================
// cleanup 핸들러
// ============================================================================

async function handleCleanup(options: { json?: boolean }): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  const ctx = await ProjectContext.getInstance();

  try {
    // prune
    execGit("worktree prune", ctx.gitRoot);

    // team-claude worktree 디렉토리 정리
    if (existsSync(ctx.worktreesDir)) {
      rmSync(ctx.worktreesDir, { recursive: true, force: true });
    }

    if (json) {
      outputJson({ cleaned: true }, startTime);
    } else {
      console.log("[OK] Worktree 정리 완료");
    }
  } catch (error) {
    if (json) {
      outputError("CLEANUP_FAILED", error instanceof Error ? error.message : String(error));
    } else {
      console.error(`[ERR] 정리 실패: ${error}`);
    }
    process.exit(1);
  }
}

// ============================================================================
// 커맨드 생성
// ============================================================================

export function createWorktreeCommand(): Command {
  const worktree = new Command("worktree")
    .description("Git Worktree 관리")
    .addHelpText(
      "after",
      `
Examples:
  tc worktree create auth-service
  tc worktree list
  tc worktree path auth-service
  tc worktree delete auth-service
  tc worktree cleanup
`
    );

  worktree
    .command("create <id>")
    .description("Worktree 생성")
    .option("--json", "JSON 형식으로 출력")
    .action(handleCreate);

  worktree
    .command("list")
    .description("Worktree 목록")
    .option("--json", "JSON 형식으로 출력")
    .action(handleList);

  worktree
    .command("path <id>")
    .description("Worktree 경로 출력")
    .option("--json", "JSON 형식으로 출력")
    .action(handlePath);

  worktree
    .command("delete <id>")
    .description("Worktree 삭제")
    .option("--json", "JSON 형식으로 출력")
    .action(handleDelete);

  worktree
    .command("cleanup")
    .description("모든 Team Claude worktree 정리")
    .option("--json", "JSON 형식으로 출력")
    .action(handleCleanup);

  return worktree;
}
