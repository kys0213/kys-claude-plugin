/**
 * doctor 명령어 - 자가 진단 및 자동 수정
 */

import { Command } from "commander";
import { existsSync, readFileSync, unlinkSync, readdirSync, writeFileSync, mkdirSync } from "fs";
import { homedir } from "os";
import YAML from "yaml";
import * as readline from "readline";
import { ProjectContext, timestamp } from "../lib/context";
import {
  log,
  printSection,
  printStatus,
  printWarning,
  icon,
} from "../lib/utils";

// ============================================================================
// Interfaces
// ============================================================================

export interface DiagnosticCheck {
  category: string;
  name: string;
  status: "ok" | "error" | "warning";
  message?: string;
  fixable?: boolean;
  fixAction?: string;
}

export interface DiagnosticResult {
  timestamp: string;
  project: {
    name: string;
    hash: string;
    gitRoot: string;
    dataDir: string;
  };
  checks: DiagnosticCheck[];
  summary: {
    total: number;
    ok: number;
    errors: number;
    warnings: number;
    fixable: number;
  };
}

export interface FixResult {
  check: DiagnosticCheck;
  success: boolean;
  message: string;
}

// ============================================================================
// Diagnostic Functions
// ============================================================================

function checkCommand(cmd: string): boolean {
  try {
    const result = Bun.spawnSync([cmd, "--version"]);
    return result.exitCode === 0;
  } catch {
    return false;
  }
}

function checkCommandWithWhich(cmd: string): boolean {
  try {
    const result = Bun.spawnSync(["which", cmd]);
    return result.exitCode === 0;
  } catch {
    return false;
  }
}

/**
 * 인프라 검사: yq, jq, git, bun, curl 설치 여부
 */
export function checkInfrastructure(): DiagnosticCheck[] {
  const checks: DiagnosticCheck[] = [];
  const commands = ["yq", "jq", "git", "bun", "curl"];

  for (const cmd of commands) {
    const installed = checkCommand(cmd) || checkCommandWithWhich(cmd);
    checks.push({
      category: "infrastructure",
      name: cmd,
      status: installed ? "ok" : "error",
      message: installed ? undefined : "미설치",
      fixable: false,
    });
  }

  return checks;
}

/**
 * 서버 검사: 바이너리 존재, health 체크
 */
export function checkServer(): DiagnosticCheck[] {
  const checks: DiagnosticCheck[] = [];
  const serverPath = `${homedir()}/.claude/team-claude-server`;

  // 바이너리 존재 여부
  const binaryExists = existsSync(serverPath);
  checks.push({
    category: "server",
    name: "server-binary",
    status: binaryExists ? "ok" : "error",
    message: binaryExists ? undefined : "바이너리 없음",
    fixable: true,
    fixAction: "tc-server install",
  });

  // Health 체크
  try {
    const result = Bun.spawnSync(["curl", "-s", "-o", "/dev/null", "-w", "%{http_code}", "http://localhost:7890/health"]);
    const statusCode = result.stdout.toString().trim();
    const healthy = statusCode === "200";
    checks.push({
      category: "server",
      name: "server-health",
      status: healthy ? "ok" : "warning",
      message: healthy ? undefined : `응답 없음 (${statusCode || "연결 실패"})`,
      fixable: true,
      fixAction: "tc-server start",
    });
  } catch {
    checks.push({
      category: "server",
      name: "server-health",
      status: "warning",
      message: "health 체크 실패",
      fixable: true,
      fixAction: "tc-server start",
    });
  }

  return checks;
}

/**
 * 설정 검사: team-claude.yaml 존재, 유효한 YAML, 필수 필드
 */
export async function checkConfiguration(ctx: ProjectContext): Promise<DiagnosticCheck[]> {
  const checks: DiagnosticCheck[] = [];

  // 설정 파일 존재
  if (!ctx.configExists()) {
    checks.push({
      category: "configuration",
      name: "config-file",
      status: "error",
      message: "team-claude.yaml 없음",
      fixable: true,
      fixAction: "tc init",
    });
    return checks;
  }

  checks.push({
    category: "configuration",
    name: "config-file",
    status: "ok",
  });

  // YAML 유효성
  try {
    const content = readFileSync(ctx.configPath, "utf-8");
    const config = YAML.parse(content);

    checks.push({
      category: "configuration",
      name: "config-valid-yaml",
      status: "ok",
    });

    // 필수 필드 검사
    const requiredFields = ["project"];
    for (const field of requiredFields) {
      if (config && config[field]) {
        checks.push({
          category: "configuration",
          name: `config-field-${field}`,
          status: "ok",
        });
      } else {
        checks.push({
          category: "configuration",
          name: `config-field-${field}`,
          status: "warning",
          message: `필드 누락: ${field}`,
        });
      }
    }
  } catch (err) {
    checks.push({
      category: "configuration",
      name: "config-valid-yaml",
      status: "error",
      message: `YAML 파싱 오류: ${err instanceof Error ? err.message : String(err)}`,
      fixable: false,
    });
  }

  return checks;
}

/**
 * 훅 검사: tc CLI 사용 가능, settings.local.json에 hooks 설정, 레거시 .sh 파일 경고
 */
export function checkHooks(ctx: ProjectContext): DiagnosticCheck[] {
  const checks: DiagnosticCheck[] = [];

  // tc CLI 존재 여부
  const tcAvailable = checkCommandWithWhich("tc");
  checks.push({
    category: "hooks",
    name: "tc-cli",
    status: tcAvailable ? "ok" : "error",
    message: tcAvailable ? undefined : "tc CLI 미설치 (bun run build 필요)",
    fixable: false,
  });

  // settings.local.json 검사
  const settingsPath = `${ctx.claudeDir}/settings.local.json`;
  if (existsSync(settingsPath)) {
    try {
      const content = readFileSync(settingsPath, "utf-8");
      const settings = JSON.parse(content);

      if (settings.hooks) {
        checks.push({
          category: "hooks",
          name: "settings-hooks",
          status: "ok",
        });
      } else {
        checks.push({
          category: "hooks",
          name: "settings-hooks",
          status: "warning",
          message: "hooks 설정 없음",
        });
      }
    } catch {
      checks.push({
        category: "hooks",
        name: "settings-hooks",
        status: "error",
        message: "settings.local.json 파싱 오류",
      });
    }
  } else {
    checks.push({
      category: "hooks",
      name: "settings-hooks",
      status: "warning",
      message: "settings.local.json 없음",
    });
  }

  // 레거시 .sh 파일 경고
  const legacyHooks = [
    "on-worker-complete.sh",
    "on-validation-complete.sh",
    "on-worker-question.sh",
    "on-worker-idle.sh",
  ];

  for (const hook of legacyHooks) {
    const hookPath = `${ctx.hooksDir}/${hook}`;
    if (existsSync(hookPath)) {
      checks.push({
        category: "hooks",
        name: `legacy-${hook}`,
        status: "warning",
        message: "레거시 스크립트 - 제거 권장",
        fixable: true,
        fixAction: `rm ${hookPath}`,
      });
    }
  }

  return checks;
}

/**
 * 상태 검사: workflow.json, psm-index.json 존재 및 유효성
 */
export function checkState(ctx: ProjectContext): DiagnosticCheck[] {
  const checks: DiagnosticCheck[] = [];

  const stateFiles = [
    { name: "workflow.json", required: false },
    { name: "psm-index.json", required: false },
  ];

  for (const { name, required } of stateFiles) {
    const filePath = `${ctx.stateDir}/${name}`;

    if (!existsSync(filePath)) {
      checks.push({
        category: "state",
        name: `state-${name}`,
        status: required ? "error" : "ok",
        message: required ? "파일 없음" : "파일 없음 (정상)",
        fixable: required,
        fixAction: required ? "initialize state" : undefined,
      });
      continue;
    }

    // JSON 유효성 검사
    try {
      const content = readFileSync(filePath, "utf-8");
      JSON.parse(content);
      checks.push({
        category: "state",
        name: `state-${name}`,
        status: "ok",
      });
    } catch {
      checks.push({
        category: "state",
        name: `state-${name}`,
        status: "error",
        message: "JSON 파싱 오류",
        fixable: true,
        fixAction: "reset state file",
      });
    }
  }

  return checks;
}

/**
 * 워크트리 검사: orphan 워크트리 체크
 */
export function checkWorktrees(ctx: ProjectContext): DiagnosticCheck[] {
  const checks: DiagnosticCheck[] = [];

  if (!existsSync(ctx.worktreesDir)) {
    checks.push({
      category: "worktrees",
      name: "worktrees-dir",
      status: "ok",
      message: "디렉토리 없음 (정상)",
    });
    return checks;
  }

  try {
    const entries = readdirSync(ctx.worktreesDir, { withFileTypes: true });
    const worktrees = entries.filter(e => e.isDirectory());

    if (worktrees.length === 0) {
      checks.push({
        category: "worktrees",
        name: "worktrees-count",
        status: "ok",
        message: "워크트리 없음",
      });
      return checks;
    }

    // git worktree list로 실제 워크트리 목록 가져오기
    const result = Bun.spawnSync(["git", "worktree", "list", "--porcelain"], {
      cwd: ctx.gitRoot,
    });
    const worktreeList = result.stdout.toString();
    const registeredPaths = new Set<string>();

    for (const line of worktreeList.split("\n")) {
      if (line.startsWith("worktree ")) {
        registeredPaths.add(line.replace("worktree ", ""));
      }
    }

    // 등록되지 않은 워크트리 찾기
    let orphanCount = 0;
    for (const wt of worktrees) {
      const wtPath = `${ctx.worktreesDir}/${wt.name}`;
      if (!registeredPaths.has(wtPath)) {
        orphanCount++;
        checks.push({
          category: "worktrees",
          name: `orphan-${wt.name}`,
          status: "warning",
          message: "orphan 워크트리",
          fixable: true,
          fixAction: `rm -rf ${wtPath}`,
        });
      }
    }

    if (orphanCount === 0) {
      checks.push({
        category: "worktrees",
        name: "worktrees-health",
        status: "ok",
        message: `${worktrees.length}개 워크트리 정상`,
      });
    }
  } catch (err) {
    checks.push({
      category: "worktrees",
      name: "worktrees-check",
      status: "error",
      message: `검사 실패: ${err instanceof Error ? err.message : String(err)}`,
    });
  }

  return checks;
}

// ============================================================================
// Fix Functions
// ============================================================================

/**
 * 누락된 디렉토리 생성
 */
export function fixMissingDirectories(ctx: ProjectContext): FixResult[] {
  const results: FixResult[] = [];

  try {
    ctx.ensureDataDirs();
    ctx.ensureClaudeDirs();
    results.push({
      check: {
        category: "directories",
        name: "create-dirs",
        status: "ok",
      },
      success: true,
      message: "디렉토리 생성 완료",
    });
  } catch (err) {
    results.push({
      check: {
        category: "directories",
        name: "create-dirs",
        status: "error",
      },
      success: false,
      message: `디렉토리 생성 실패: ${err instanceof Error ? err.message : String(err)}`,
    });
  }

  return results;
}

/**
 * 손상된 상태 파일 초기화
 */
export function fixCorruptedState(ctx: ProjectContext): FixResult[] {
  const results: FixResult[] = [];

  // stateDir 확인
  if (!existsSync(ctx.stateDir)) {
    mkdirSync(ctx.stateDir, { recursive: true });
  }

  const stateDefaults: Record<string, object> = {
    "workflow.json": { version: 1, workflows: [] },
    "psm-index.json": { version: 1, sessions: {} },
  };

  for (const [filename, defaultContent] of Object.entries(stateDefaults)) {
    const filePath = `${ctx.stateDir}/${filename}`;

    // 파일이 없거나 파싱 오류인 경우만 처리
    let needsFix = false;
    if (existsSync(filePath)) {
      try {
        const content = readFileSync(filePath, "utf-8");
        JSON.parse(content);
      } catch {
        needsFix = true;
      }
    }

    if (needsFix) {
      try {
        writeFileSync(filePath, JSON.stringify(defaultContent, null, 2));
        results.push({
          check: {
            category: "state",
            name: filename,
            status: "ok",
          },
          success: true,
          message: `${filename} 초기화 완료`,
        });
      } catch (err) {
        results.push({
          check: {
            category: "state",
            name: filename,
            status: "error",
          },
          success: false,
          message: `${filename} 초기화 실패: ${err instanceof Error ? err.message : String(err)}`,
        });
      }
    }
  }

  return results;
}

/**
 * 레거시 훅 제거 (확인 후)
 */
export async function fixLegacyHooks(
  ctx: ProjectContext,
  interactive: boolean
): Promise<FixResult[]> {
  const results: FixResult[] = [];
  const legacyHooks = [
    "on-worker-complete.sh",
    "on-validation-complete.sh",
    "on-worker-question.sh",
    "on-worker-idle.sh",
  ];

  for (const hook of legacyHooks) {
    const hookPath = `${ctx.hooksDir}/${hook}`;
    if (!existsSync(hookPath)) continue;

    if (interactive) {
      const confirmed = await askConfirmation(`레거시 훅 삭제: ${hook}?`);
      if (!confirmed) {
        results.push({
          check: {
            category: "hooks",
            name: hook,
            status: "warning",
          },
          success: false,
          message: "사용자 취소",
        });
        continue;
      }
    }

    try {
      unlinkSync(hookPath);
      results.push({
        check: {
          category: "hooks",
          name: hook,
          status: "ok",
        },
        success: true,
        message: `${hook} 삭제 완료`,
      });
    } catch (err) {
      results.push({
        check: {
          category: "hooks",
          name: hook,
          status: "error",
        },
        success: false,
        message: `${hook} 삭제 실패: ${err instanceof Error ? err.message : String(err)}`,
      });
    }
  }

  return results;
}

/**
 * 서버 설치/시작
 */
export function fixServer(): FixResult[] {
  const results: FixResult[] = [];
  const serverPath = `${homedir()}/.claude/team-claude-server`;

  if (!existsSync(serverPath)) {
    // 서버 설치
    try {
      const result = Bun.spawnSync(["tc-server", "install"]);
      if (result.exitCode === 0) {
        results.push({
          check: {
            category: "server",
            name: "server-install",
            status: "ok",
          },
          success: true,
          message: "서버 설치 완료",
        });
      } else {
        results.push({
          check: {
            category: "server",
            name: "server-install",
            status: "error",
          },
          success: false,
          message: "서버 설치 실패",
        });
        return results;
      }
    } catch {
      results.push({
        check: {
          category: "server",
          name: "server-install",
          status: "error",
        },
        success: false,
        message: "tc-server 명령어 없음",
      });
      return results;
    }
  }

  // 서버 시작
  try {
    const result = Bun.spawnSync(["tc-server", "start"]);
    results.push({
      check: {
        category: "server",
        name: "server-start",
        status: result.exitCode === 0 ? "ok" : "error",
      },
      success: result.exitCode === 0,
      message: result.exitCode === 0 ? "서버 시작 완료" : "서버 시작 실패",
    });
  } catch {
    results.push({
      check: {
        category: "server",
        name: "server-start",
        status: "error",
      },
      success: false,
      message: "tc-server 명령어 없음",
    });
  }

  return results;
}

/**
 * 모든 수정 실행
 */
export async function runFixes(
  checks: DiagnosticCheck[],
  ctx: ProjectContext,
  interactive: boolean
): Promise<FixResult[]> {
  const fixableChecks = checks.filter(c => c.fixable && c.status !== "ok");
  const allResults: FixResult[] = [];

  if (fixableChecks.length === 0) {
    log.info("수정할 항목이 없습니다.");
    return allResults;
  }

  log.info(`${fixableChecks.length}개 항목 수정 시도...`);
  console.log();

  // 디렉토리 수정
  allResults.push(...fixMissingDirectories(ctx));

  // 상태 파일 수정
  allResults.push(...fixCorruptedState(ctx));

  // 레거시 훅 수정
  const legacyHookChecks = fixableChecks.filter(c => c.category === "hooks");
  if (legacyHookChecks.length > 0) {
    allResults.push(...await fixLegacyHooks(ctx, interactive));
  }

  // 서버 수정
  const serverChecks = fixableChecks.filter(c => c.category === "server");
  if (serverChecks.length > 0) {
    allResults.push(...fixServer());
  }

  return allResults;
}

// ============================================================================
// Helper Functions
// ============================================================================

async function askConfirmation(question: string): Promise<boolean> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  return new Promise((resolve) => {
    rl.question(`${question} (y/N) `, (answer) => {
      rl.close();
      resolve(answer.toLowerCase() === "y" || answer.toLowerCase() === "yes");
    });
  });
}

function computeSummary(checks: DiagnosticCheck[]): DiagnosticResult["summary"] {
  const total = checks.length;
  const ok = checks.filter(c => c.status === "ok").length;
  const errors = checks.filter(c => c.status === "error").length;
  const warnings = checks.filter(c => c.status === "warning").length;
  const fixable = checks.filter(c => c.fixable && c.status !== "ok").length;

  return { total, ok, errors, warnings, fixable };
}

function printDiagnosticResult(result: DiagnosticResult): void {
  printSection("Team Claude 자가 진단");

  log.info(`프로젝트: ${result.project.name}`);
  log.info(`해시: ${result.project.hash}`);
  log.info(`데이터: ${result.project.dataDir}`);
  console.log();

  // 카테고리별 출력
  const categories = [...new Set(result.checks.map(c => c.category))];

  for (const category of categories) {
    console.log(`  ${category.toUpperCase()}`);
    const categoryChecks = result.checks.filter(c => c.category === category);

    for (const check of categoryChecks) {
      if (check.status === "ok") {
        printStatus(check.name, true);
      } else if (check.status === "warning") {
        printWarning(check.name, check.message);
      } else {
        printStatus(check.name, false, check.message);
      }
    }
    console.log();
  }

  console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
  const { ok, errors, warnings, fixable, total } = result.summary;

  if (errors === 0 && warnings === 0) {
    console.log(`${icon.check} 모든 검증 통과 (${ok}/${total})`);
  } else {
    const parts: string[] = [];
    if (errors > 0) parts.push(`오류 ${errors}개`);
    if (warnings > 0) parts.push(`경고 ${warnings}개`);
    if (fixable > 0) parts.push(`수정 가능 ${fixable}개`);

    const statusIcon = errors > 0 ? icon.cross : icon.warn;
    console.log(`${statusIcon} ${parts.join(", ")}`);

    if (fixable > 0) {
      console.log(`  ${icon.dot} 'tc doctor --fix' 로 자동 수정 가능`);
    }
  }
  console.log();
}

function printFixResults(results: FixResult[]): void {
  if (results.length === 0) return;

  printSection("수정 결과");

  for (const result of results) {
    if (result.success) {
      printStatus(result.check.name, true, result.message);
    } else {
      printStatus(result.check.name, false, result.message);
    }
  }
  console.log();
}

// ============================================================================
// Command
// ============================================================================

export function createDoctorCommand(): Command {
  return new Command("doctor")
    .description("Team Claude 자가 진단 및 자동 수정")
    .option("--fix", "문제 자동 수정 시도")
    .option("--json", "JSON 형식 출력")
    .option("--category <cat>", "특정 카테고리만 검사 (infrastructure|server|configuration|hooks|state|worktrees)")
    .action(async (opts) => {
      const ctx = await ProjectContext.getInstance();

      // 모든 검사 실행
      const allChecks: DiagnosticCheck[] = [];

      const categoryFilter = opts.category as string | undefined;

      if (!categoryFilter || categoryFilter === "infrastructure") {
        allChecks.push(...checkInfrastructure());
      }
      if (!categoryFilter || categoryFilter === "server") {
        allChecks.push(...checkServer());
      }
      if (!categoryFilter || categoryFilter === "configuration") {
        allChecks.push(...await checkConfiguration(ctx));
      }
      if (!categoryFilter || categoryFilter === "hooks") {
        allChecks.push(...checkHooks(ctx));
      }
      if (!categoryFilter || categoryFilter === "state") {
        allChecks.push(...checkState(ctx));
      }
      if (!categoryFilter || categoryFilter === "worktrees") {
        allChecks.push(...checkWorktrees(ctx));
      }

      const result: DiagnosticResult = {
        timestamp: timestamp(),
        project: {
          name: ctx.projectName,
          hash: ctx.projectHash,
          gitRoot: ctx.gitRoot,
          dataDir: ctx.dataDir,
        },
        checks: allChecks,
        summary: computeSummary(allChecks),
      };

      if (opts.json) {
        console.log(JSON.stringify(result, null, 2));
        process.exit(result.summary.errors > 0 ? 1 : 0);
      }

      printDiagnosticResult(result);

      // --fix 옵션 처리
      if (opts.fix) {
        const fixResults = await runFixes(allChecks, ctx, true);
        printFixResults(fixResults);

        const failedFixes = fixResults.filter(r => !r.success);
        if (failedFixes.length > 0) {
          process.exit(1);
        }
      }

      process.exit(result.summary.errors > 0 ? 1 : 0);
    });
}
