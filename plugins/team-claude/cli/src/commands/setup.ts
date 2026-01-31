/**
 * tc setup - 환경 설정 및 초기화
 * Flow, PSM, HUD 등 모든 기능의 통합 초기화
 */

import { Command } from "commander";
import { existsSync, mkdirSync, writeFileSync, readFileSync } from "fs";
import { join, dirname, basename } from "path";
import { execSync } from "child_process";
import chalk from "chalk";
import YAML from "yaml";
import {
  getProjectDataDir,
  getStateDir,
  getSessionsDir,
  findGitRoot,
  getProjectHash,
  readJsonFile,
  writeJsonFile,
} from "../lib/common";

// ============================================================================
// 타입 정의
// ============================================================================

interface WorkflowState {
  currentSession: string | null;
  lastUpdated: string | null;
  status: "idle" | "running" | "paused";
}

interface PsmIndex {
  sessions: Array<{
    name: string;
    status: string;
    createdAt: string;
  }>;
  createdAt: string | null;
  lastUpdated: string | null;
}

interface SetupStatus {
  configExists: boolean;
  stateInitialized: boolean;
  psmInitialized: boolean;
  hooksInstalled: boolean;
  serverInstalled: boolean;
  dependencies: {
    yq: boolean;
    jq: boolean;
    git: boolean;
    bun: boolean;
  };
}

// ============================================================================
// 유틸리티 함수
// ============================================================================

function ensureDir(path: string): void {
  if (!existsSync(path)) {
    mkdirSync(path, { recursive: true });
  }
}

function checkCommand(cmd: string): boolean {
  try {
    execSync(`command -v ${cmd}`, { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function getPluginRoot(): string {
  // CLI가 실행되는 위치 기준으로 플러그인 루트 찾기
  const cliDir = dirname(dirname(__dirname));
  return dirname(cliDir); // plugins/team-claude
}

// 기본 설정 생성
function createDefaultConfig(gitRoot: string): Record<string, unknown> {
  const projectName = basename(gitRoot);
  const projectHash = getProjectHash();

  return {
    version: "1.0",
    _meta: {
      project_root: gitRoot,
      project_hash: projectHash,
    },
    project: {
      name: projectName,
      language: "",
      framework: "",
      domain: "",
      test_command: "",
      build_command: "",
      lint_command: "",
    },
    feedback_loop: {
      mode: "auto",
      max_iterations: 5,
      auto_retry_delay: 5000,
    },
    validation: {
      method: "test",
      timeout: 120000,
    },
    notification: {
      method: "system",
      slack: {
        webhook_url: "",
        channel: "",
      },
    },
    server: {
      port: 7890,
      executor: "iterm",
    },
    agents: {
      enabled: ["spec_validator", "test_oracle", "impl_reviewer"],
      custom: [],
      overrides: {},
    },
    // Flow 설정 (v0.5.0+)
    flow: {
      defaultMode: "assisted",
      autoReview: {
        enabled: true,
        maxIterations: 5,
      },
      escalation: {
        onMaxIterations: true,
        onConflict: true,
      },
    },
    // PSM 설정 (v0.5.0+)
    psm: {
      parallelLimit: 4,
      autoCleanup: true,
      conflictCheck: {
        enabled: true,
        action: "warn",
      },
    },
    // Swarm 설정 (v0.5.0+)
    swarm: {
      enabled: true,
      maxParallel: 4,
      conflictCheck: {
        enabled: true,
        action: "warn",
      },
    },
    // Magic Keywords 설정 (v0.5.0+)
    keywords: {
      enabled: true,
      aliases: {
        auto: "autopilot",
        ap: "autopilot",
        sp: "spec",
        im: "impl",
      },
    },
  };
}

// YAML 설정 파일에 누락된 설정 추가
function ensureConfigSettings(configPath: string): void {
  if (!existsSync(configPath)) return;

  try {
    const content = readFileSync(configPath, "utf-8");
    const config = YAML.parse(content) as Record<string, unknown>;
    let updated = false;

    // flow 설정 추가
    if (!config.flow) {
      config.flow = {
        defaultMode: "assisted",
        autoReview: { enabled: true, maxIterations: 5 },
        escalation: { onMaxIterations: true, onConflict: true },
      };
      updated = true;
    }

    // psm 설정 추가
    if (!config.psm) {
      config.psm = {
        parallelLimit: 4,
        autoCleanup: true,
        conflictCheck: { enabled: true, action: "warn" },
      };
      updated = true;
    }

    // swarm 설정 추가
    if (!config.swarm) {
      config.swarm = {
        enabled: true,
        maxParallel: 4,
        conflictCheck: { enabled: true, action: "warn" },
      };
      updated = true;
    }

    // keywords 설정 추가
    if (!config.keywords) {
      config.keywords = {
        enabled: true,
        aliases: { auto: "autopilot", ap: "autopilot", sp: "spec", im: "impl" },
      };
      updated = true;
    }

    if (updated) {
      writeFileSync(configPath, YAML.stringify(config, { indent: 2 }));
      console.log(chalk.green("  ✓ Flow/PSM/Swarm/Keywords settings added"));
    }
  } catch (e) {
    console.log(chalk.yellow("  ⚠ Could not update config settings"));
  }
}

// ============================================================================
// 상태 확인
// ============================================================================

function checkSetupStatus(): SetupStatus {
  const dataDir = getProjectDataDir();
  const stateDir = getStateDir();
  const gitRoot = findGitRoot();

  return {
    configExists: existsSync(join(dataDir, "team-claude.yaml")),
    stateInitialized: existsSync(join(stateDir, "workflow.json")),
    psmInitialized: existsSync(join(dataDir, "psm-index.json")),
    hooksInstalled: existsSync(join(gitRoot, ".claude", "hooks")),
    serverInstalled: existsSync(
      join(process.env.HOME || "", ".claude", "team-claude-server")
    ),
    dependencies: {
      yq: checkCommand("yq"),
      jq: checkCommand("jq"),
      git: checkCommand("git"),
      bun: checkCommand("bun"),
    },
  };
}

function printStatus(status: SetupStatus): void {
  console.log("\n━━━ Team Claude Setup Status ━━━\n");

  // 의존성
  console.log("🔧 Dependencies");
  const deps = status.dependencies;
  console.log(`  ${deps.yq ? chalk.green("✓") : chalk.red("✗")} yq`);
  console.log(`  ${deps.jq ? chalk.green("✓") : chalk.red("✗")} jq`);
  console.log(`  ${deps.git ? chalk.green("✓") : chalk.red("✗")} git`);
  console.log(
    `  ${deps.bun ? chalk.green("✓") : chalk.yellow("⚠")} bun ${!deps.bun ? "(optional)" : ""}`
  );
  console.log();

  // 설정
  console.log("📁 Configuration");
  console.log(
    `  ${status.configExists ? chalk.green("✓") : chalk.red("✗")} team-claude.yaml`
  );
  console.log(
    `  ${status.hooksInstalled ? chalk.green("✓") : chalk.red("✗")} hooks/`
  );
  console.log();

  // Flow/PSM
  console.log("🚀 Flow/PSM");
  console.log(
    `  ${status.stateInitialized ? chalk.green("✓") : chalk.yellow("⚠")} workflow.json`
  );
  console.log(
    `  ${status.psmInitialized ? chalk.green("✓") : chalk.yellow("⚠")} psm-index.json`
  );
  console.log();

  // 서버
  console.log("🖥️  Server");
  console.log(
    `  ${status.serverInstalled ? chalk.green("✓") : chalk.yellow("⚠")} team-claude-server`
  );
  console.log();

  // 요약
  const allGood =
    status.configExists &&
    status.stateInitialized &&
    status.psmInitialized &&
    deps.yq &&
    deps.jq &&
    deps.git;

  if (allGood) {
    console.log(chalk.green("✓ Setup complete"));
  } else {
    console.log(
      chalk.yellow("⚠ Some components need initialization. Run: tc setup init")
    );
  }
}

// ============================================================================
// 초기화
// ============================================================================

async function initSetup(options: { force?: boolean }): Promise<void> {
  const dataDir = getProjectDataDir();
  const stateDir = getStateDir();
  const sessionsDir = getSessionsDir();
  const gitRoot = findGitRoot();

  console.log("\n━━━ Team Claude Setup ━━━\n");
  console.log(`Project: ${gitRoot}`);
  console.log(`Data: ${dataDir}`);
  console.log();

  // 1. 디렉토리 생성
  console.log("📂 Creating directories...");
  ensureDir(dataDir);
  ensureDir(stateDir);
  ensureDir(sessionsDir);
  ensureDir(join(dataDir, "worktrees"));
  console.log(chalk.green("  ✓ Directories created"));

  // 2. Flow 상태 초기화
  const workflowPath = join(stateDir, "workflow.json");
  if (!existsSync(workflowPath) || options.force) {
    const workflowState: WorkflowState = {
      currentSession: null,
      lastUpdated: null,
      status: "idle",
    };
    writeJsonFile(workflowPath, workflowState);
    console.log(chalk.green("  ✓ Flow state initialized"));
  } else {
    console.log(chalk.gray("  - Flow state already exists"));
  }

  // 3. PSM 인덱스 초기화
  const psmPath = join(dataDir, "psm-index.json");
  if (!existsSync(psmPath) || options.force) {
    const psmIndex: PsmIndex = {
      sessions: [],
      createdAt: new Date().toISOString(),
      lastUpdated: null,
    };
    writeJsonFile(psmPath, psmIndex);
    console.log(chalk.green("  ✓ PSM index initialized"));
  } else {
    console.log(chalk.gray("  - PSM index already exists"));
  }

  // 4. Hooks 복사
  const hooksDir = join(gitRoot, ".claude", "hooks");
  const pluginHooksDir = join(getPluginRoot(), "hooks", "scripts");

  if (!existsSync(hooksDir)) {
    ensureDir(hooksDir);
    if (existsSync(pluginHooksDir)) {
      try {
        execSync(`cp -r "${pluginHooksDir}/"* "${hooksDir}/" 2>/dev/null`, {
          stdio: "ignore",
        });
        execSync(`chmod +x "${hooksDir}/"*.sh 2>/dev/null`, { stdio: "ignore" });
        console.log(chalk.green("  ✓ Hooks installed"));
      } catch {
        console.log(chalk.yellow("  ⚠ Could not copy hooks"));
      }
    }
  } else {
    console.log(chalk.gray("  - Hooks already exist"));
  }

  // 5. settings.local.json 설정
  const settingsPath = join(gitRoot, ".claude", "settings.local.json");
  if (!existsSync(settingsPath)) {
    const hooksConfig = {
      hooks: {
        Stop: [
          {
            type: "command",
            command: ".claude/hooks/on-worker-complete.sh",
          },
        ],
        PreToolUse: [
          {
            matcher: "Task",
            hooks: [
              {
                type: "command",
                command: ".claude/hooks/on-worker-question.sh",
              },
            ],
          },
        ],
        Notification: [
          {
            matcher: ".*",
            hooks: [
              {
                type: "command",
                command: ".claude/hooks/on-worker-idle.sh",
              },
            ],
          },
        ],
      },
    };
    ensureDir(dirname(settingsPath));
    writeFileSync(settingsPath, JSON.stringify(hooksConfig, null, 2));
    console.log(chalk.green("  ✓ settings.local.json created"));
  } else {
    console.log(chalk.gray("  - settings.local.json already exists"));
  }

  // 6. team-claude.yaml 생성 (TypeScript로 직접 생성)
  const configPath = join(dataDir, "team-claude.yaml");
  if (!existsSync(configPath) || options.force) {
    const config = createDefaultConfig(gitRoot);
    writeFileSync(configPath, YAML.stringify(config, { indent: 2 }));
    console.log(chalk.green("  ✓ team-claude.yaml created"));
  } else {
    console.log(chalk.gray("  - team-claude.yaml already exists"));
    // 기존 설정에 누락된 flow/psm/swarm/keywords 추가
    ensureConfigSettings(configPath);
  }

  // 7. .claude/agents 디렉토리 생성
  const agentsDir = join(gitRoot, ".claude", "agents");
  if (!existsSync(agentsDir)) {
    ensureDir(agentsDir);
    console.log(chalk.green("  ✓ .claude/agents created"));
  }

  console.log();
  console.log(chalk.green("━━━ Setup Complete ━━━"));
  console.log();
  console.log("Next steps:");
  console.log("  1. Run: tc setup status    - Check setup status");
  console.log("  2. Run: tc flow start      - Start a workflow");
  console.log("  3. Run: tc hud setup       - Configure HUD");
}

// ============================================================================
// HUD 안내
// ============================================================================

function printHudSetup(): void {
  console.log(`
━━━ Team Claude HUD Setup ━━━

HUD displays workflow status in Claude Code's statusline.

Setup:

1. Add to ~/.claude/settings.json:

   {
     "statusLine": {
       "type": "command",
       "command": "tc hud output",
       "padding": 0
     }
   }

2. Or integrate with existing statusline:

   #!/bin/bash
   existing=$(your_existing_statusline)
   tc_hud=$(tc hud output 2>/dev/null)
   echo "\${existing} │ \${tc_hud}"

3. Test:

   tc hud output

   Expected output (when workflow active):
   🚀 auto │ 📋 spec ████████░░ 80% │ 🌳 2/3 │ ⏱️ 5m23s
`);
}

// ============================================================================
// 명령어
// ============================================================================

async function cmdStatus(): Promise<void> {
  const status = checkSetupStatus();
  printStatus(status);
}

async function cmdInit(options: { force?: boolean }): Promise<void> {
  await initSetup(options);
}

async function cmdHud(): Promise<void> {
  printHudSetup();
}

async function cmdVerify(): Promise<void> {
  const scriptPath = join(getPluginRoot(), "scripts", "tc-config.sh");
  if (existsSync(scriptPath)) {
    try {
      execSync(`bash "${scriptPath}" verify`, {
        cwd: findGitRoot(),
        stdio: "inherit",
      });
    } catch {
      process.exit(1);
    }
  } else {
    console.error("tc-config.sh not found");
    process.exit(1);
  }
}

// ============================================================================
// 명령어 등록
// ============================================================================

export function createSetupCommand(): Command {
  const setup = new Command("setup").description(
    "환경 설정 및 초기화 (Flow, PSM, HUD 포함)"
  );

  setup
    .command("status")
    .description("Setup 상태 확인")
    .action(cmdStatus);

  setup
    .command("init")
    .description("Team Claude 초기화 (Flow, PSM, HUD 포함)")
    .option("-f, --force", "기존 설정 덮어쓰기")
    .action(cmdInit);

  setup
    .command("hud")
    .description("HUD 설정 안내")
    .action(cmdHud);

  setup
    .command("verify")
    .description("환경 검증")
    .action(cmdVerify);

  // 기본 동작: status
  setup.action(cmdStatus);

  return setup;
}
