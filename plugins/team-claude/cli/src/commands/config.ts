/**
 * config 명령어 - 설정 관리
 */

import { Command } from "commander";
import { existsSync, readFileSync, writeFileSync } from "fs";
import YAML from "yaml";
import { ProjectContext } from "../lib/context";
import {
  log,
  printSection,
  printStatus,
  printWarning,
  printKV,
  icon,
} from "../lib/utils";

export function createConfigCommand(): Command {
  const cmd = new Command("config")
    .description("설정 관리")
    .addCommand(createInfoCommand())
    .addCommand(createVerifyCommand())
    .addCommand(createShowCommand())
    .addCommand(createGetCommand())
    .addCommand(createSetCommand());

  // 기본 동작: info
  cmd.action(async () => {
    await showInfo();
  });

  return cmd;
}

// ============================================================================
// info - 프로젝트 정보 출력
// ============================================================================

function createInfoCommand(): Command {
  return new Command("info")
    .description("프로젝트 정보 출력")
    .option("--json", "JSON 형식 출력")
    .action(async (opts) => {
      if (opts.json) {
        const ctx = await ProjectContext.getInstance();
        console.log(JSON.stringify(ctx.getInfo(), null, 2));
      } else {
        await showInfo();
      }
    });
}

async function showInfo(): Promise<void> {
  const ctx = await ProjectContext.getInstance();
  const info = ctx.getInfo();

  printSection("프로젝트 정보");
  printKV("이름", info.projectName);
  printKV("Git 루트", info.gitRoot);
  printKV("해시", info.projectHash);
  console.log();
  printKV("데이터 디렉토리", info.dataDir);
  printKV("설정 파일", info.configPath);
  printKV("세션 디렉토리", info.sessionsDir);
  printKV("Worktrees 디렉토리", info.worktreesDir);
  console.log();
}

// ============================================================================
// verify - 환경 검증
// ============================================================================

function createVerifyCommand(): Command {
  return new Command("verify")
    .description("환경 설정 검증")
    .option("--json", "JSON 형식 출력")
    .action(async (opts) => {
      const result = await verifyEnvironment();

      if (opts.json) {
        console.log(JSON.stringify(result, null, 2));
        process.exit(result.errors > 0 ? 1 : 0);
      } else {
        printVerifyResult(result);
        process.exit(result.errors > 0 ? 1 : 0);
      }
    });
}

interface VerifyResult {
  project: {
    name: string;
    hash: string;
    dataDir: string;
  };
  checks: {
    name: string;
    status: "ok" | "error" | "warning";
    message?: string;
  }[];
  errors: number;
  warnings: number;
}

async function verifyEnvironment(): Promise<VerifyResult> {
  const ctx = await ProjectContext.getInstance();
  const checks: VerifyResult["checks"] = [];
  let errors = 0;
  let warnings = 0;

  // 설정 파일
  if (ctx.configExists()) {
    checks.push({ name: "설정 파일", status: "ok" });
  } else {
    checks.push({ name: "설정 파일", status: "error", message: "누락" });
    errors++;
  }

  // 전역 데이터 디렉토리
  for (const [name, dir] of [
    ["sessions", ctx.sessionsDir],
    ["state", ctx.stateDir],
    ["worktrees", ctx.worktreesDir],
  ] as const) {
    if (existsSync(dir)) {
      checks.push({ name: `전역/${name}`, status: "ok" });
    } else {
      checks.push({ name: `전역/${name}`, status: "error", message: "누락" });
      errors++;
    }
  }

  // .claude 디렉토리
  if (existsSync(ctx.agentsDir)) {
    checks.push({ name: ".claude/agents", status: "ok" });
  } else {
    checks.push({
      name: ".claude/agents",
      status: "warning",
      message: "선택",
    });
    warnings++;
  }

  if (existsSync(ctx.hooksDir)) {
    checks.push({ name: ".claude/hooks", status: "ok" });
  } else {
    checks.push({ name: ".claude/hooks", status: "error", message: "누락" });
    errors++;
  }

  // tc hook CLI 검증 (tc CLI가 PATH에 있는지 확인)
  const tcCommands = [
    "worker-complete",
    "worker-idle",
    "worker-question",
    "validation-complete",
  ];

  // tc CLI 존재 여부 확인
  try {
    Bun.spawnSync(["tc", "--version"]);
    checks.push({ name: "tc CLI", status: "ok" });

    for (const cmd of tcCommands) {
      checks.push({ name: `tc hook ${cmd}`, status: "ok" });
    }
  } catch {
    checks.push({ name: "tc CLI", status: "error", message: "미설치 (bun run build 필요)" });
    errors++;
  }

  // 레거시 .sh 스크립트 경고
  const legacyHooks = [
    "on-worker-complete.sh",
    "on-validation-complete.sh",
    "on-worker-question.sh",
    "on-worker-idle.sh",
  ];
  for (const hook of legacyHooks) {
    const hookPath = `${ctx.hooksDir}/${hook}`;
    if (existsSync(hookPath)) {
      checks.push({ name: `legacy/${hook}`, status: "warning", message: "제거 권장" });
      warnings++;
    }
  }

  return {
    project: {
      name: ctx.projectName,
      hash: ctx.projectHash,
      dataDir: ctx.dataDir,
    },
    checks,
    errors,
    warnings,
  };
}

function printVerifyResult(result: VerifyResult): void {
  printSection("Team Claude 환경 검증");

  log.info(`프로젝트: ${result.project.name}`);
  log.info(`해시: ${result.project.hash}`);
  log.info(`데이터: ${result.project.dataDir}`);
  console.log();

  for (const check of result.checks) {
    if (check.status === "ok") {
      printStatus(check.name, true);
    } else if (check.status === "warning") {
      printWarning(check.name, check.message);
    } else {
      printStatus(check.name, false, check.message);
    }
  }

  console.log();
  console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

  if (result.errors === 0 && result.warnings === 0) {
    console.log(`${icon.check} 모든 검증 통과`);
  } else if (result.errors === 0) {
    console.log(`${icon.warn} 경고 ${result.warnings}개 (선택적 항목)`);
  } else {
    console.log(`${icon.cross} 오류 ${result.errors}개, 경고 ${result.warnings}개`);
  }
  console.log();
}

// ============================================================================
// show - 전체 설정 출력
// ============================================================================

function createShowCommand(): Command {
  return new Command("show")
    .description("전체 설정 출력")
    .action(async () => {
      const ctx = await ProjectContext.getInstance();

      if (!ctx.configExists()) {
        log.err(`설정 파일이 없습니다: ${ctx.configPath}`);
        log.err("먼저 'tc config init'을 실행하세요.");
        process.exit(1);
      }

      const content = readFileSync(ctx.configPath, "utf-8");
      console.log(content);
    });
}

// ============================================================================
// get - YAML 경로 값 읽기
// ============================================================================

function createGetCommand(): Command {
  return new Command("get")
    .description("설정 값 읽기")
    .argument("<path>", "YAML 경로 (예: project.name)")
    .action(async (path: string) => {
      const ctx = await ProjectContext.getInstance();

      if (!ctx.configExists()) {
        log.err(`설정 파일이 없습니다: ${ctx.configPath}`);
        process.exit(1);
      }

      const content = readFileSync(ctx.configPath, "utf-8");
      const config = YAML.parse(content);

      const value = getNestedValue(config, path);
      if (value === undefined) {
        log.err(`경로를 찾을 수 없습니다: ${path}`);
        process.exit(1);
      }

      if (typeof value === "object") {
        console.log(YAML.stringify(value));
      } else {
        console.log(value);
      }
    });
}

// ============================================================================
// set - YAML 경로 값 쓰기
// ============================================================================

function createSetCommand(): Command {
  return new Command("set")
    .description("설정 값 쓰기")
    .argument("<path>", "YAML 경로 (예: project.name)")
    .argument("<value>", "설정할 값")
    .action(async (path: string, value: string) => {
      const ctx = await ProjectContext.getInstance();

      if (!ctx.configExists()) {
        log.err(`설정 파일이 없습니다: ${ctx.configPath}`);
        process.exit(1);
      }

      const content = readFileSync(ctx.configPath, "utf-8");
      const config = YAML.parse(content);

      setNestedValue(config, path, value);

      writeFileSync(ctx.configPath, YAML.stringify(config));
      log.ok(`설정 변경됨: ${path} = ${value}`);
    });
}

// ============================================================================
// 헬퍼 함수
// ============================================================================

function getNestedValue(obj: Record<string, unknown>, path: string): unknown {
  const keys = path.split(".");
  let current: unknown = obj;

  for (const key of keys) {
    if (current === null || current === undefined) {
      return undefined;
    }
    if (typeof current !== "object") {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }

  return current;
}

function setNestedValue(
  obj: Record<string, unknown>,
  path: string,
  value: unknown
): void {
  const keys = path.split(".");
  let current = obj;

  for (let i = 0; i < keys.length - 1; i++) {
    const key = keys[i];
    if (!(key in current) || typeof current[key] !== "object") {
      current[key] = {};
    }
    current = current[key] as Record<string, unknown>;
  }

  current[keys[keys.length - 1]] = value;
}
