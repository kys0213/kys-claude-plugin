/**
 * tc agent - 에이전트 관리 커맨드
 */

import { Command } from "commander";
import { existsSync, readdirSync, readFileSync } from "fs";
import { join, basename } from "path";
import { homedir } from "os";
import { ProjectContext } from "../lib/context";

// ============================================================================
// 상수
// ============================================================================

const PROJECT_AGENTS_DIR = ".claude/agents";
const PLUGIN_AGENTS_DIR = join(homedir(), ".claude/plugins/team-claude/agents");

// ============================================================================
// 타입 정의
// ============================================================================

interface AgentInfo {
  name: string;
  source: "project" | "plugin";
  path: string;
  description?: string;
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

function extractDescription(content: string): string | undefined {
  // YAML frontmatter에서 description 추출
  const match = content.match(/^---\s*\n([\s\S]*?)\n---/);
  if (match) {
    const frontmatter = match[1];
    const descMatch = frontmatter.match(/description:\s*(.+)/);
    if (descMatch) {
      return descMatch[1].trim().replace(/^["']|["']$/g, "");
    }
  }
  // 첫 번째 줄에서 추출 시도
  const firstLine = content.split("\n").find((l) => l.trim() && !l.startsWith("#") && !l.startsWith("---"));
  return firstLine?.trim().substring(0, 100);
}

function getAgents(dir: string, source: "project" | "plugin"): AgentInfo[] {
  if (!existsSync(dir)) return [];

  const agents: AgentInfo[] = [];
  const files = readdirSync(dir).filter((f) => f.endsWith(".md"));

  for (const file of files) {
    const path = join(dir, file);
    const name = basename(file, ".md");
    let description: string | undefined;

    try {
      const content = readFileSync(path, "utf-8");
      description = extractDescription(content);
    } catch {
      // 무시
    }

    agents.push({ name, source, path, description });
  }

  return agents;
}

// ============================================================================
// list 핸들러
// ============================================================================

async function handleList(options: { json?: boolean }): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  const ctx = await ProjectContext.getInstance();
  const projectAgentsDir = join(ctx.gitRoot, PROJECT_AGENTS_DIR);

  const projectAgents = getAgents(projectAgentsDir, "project");
  const pluginAgents = getAgents(PLUGIN_AGENTS_DIR, "plugin");
  const allAgents = [...projectAgents, ...pluginAgents];

  if (json) {
    outputJson(allAgents, startTime);
  } else {
    console.log("\n━━━ Team Claude 에이전트 목록 ━━━\n");

    if (projectAgents.length > 0) {
      console.log("📁 프로젝트 에이전트 (.claude/agents/)");
      for (const agent of projectAgents) {
        console.log(`  - ${agent.name}`);
        if (agent.description) {
          console.log(`    ${agent.description}`);
        }
      }
      console.log("");
    }

    if (pluginAgents.length > 0) {
      console.log("🔌 플러그인 에이전트");
      for (const agent of pluginAgents) {
        console.log(`  - ${agent.name}`);
        if (agent.description) {
          console.log(`    ${agent.description}`);
        }
      }
      console.log("");
    }

    if (allAgents.length === 0) {
      console.log("[INFO] 에이전트가 없습니다.");
    }
  }
}

// ============================================================================
// info 핸들러
// ============================================================================

async function handleInfo(
  name: string,
  options: { json?: boolean }
): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  if (!name) {
    if (json) {
      outputError("MISSING_NAME", "에이전트 이름을 지정하세요.");
    } else {
      console.error("[ERR] 에이전트 이름을 지정하세요.");
    }
    process.exit(1);
  }

  const ctx = await ProjectContext.getInstance();
  const projectAgentsDir = join(ctx.gitRoot, PROJECT_AGENTS_DIR);

  // 에이전트 검색
  const projectAgents = getAgents(projectAgentsDir, "project");
  const pluginAgents = getAgents(PLUGIN_AGENTS_DIR, "plugin");
  const allAgents = [...projectAgents, ...pluginAgents];

  const agent = allAgents.find((a) => a.name === name);

  if (!agent) {
    if (json) {
      outputError("NOT_FOUND", `에이전트를 찾을 수 없습니다: ${name}`);
    } else {
      console.error(`[ERR] 에이전트를 찾을 수 없습니다: ${name}`);
    }
    process.exit(1);
  }

  let content = "";
  try {
    content = readFileSync(agent.path, "utf-8");
  } catch {
    // 무시
  }

  if (json) {
    outputJson({ ...agent, content }, startTime);
  } else {
    console.log("\n━━━ 에이전트 정보 ━━━\n");
    console.log(`  이름: ${agent.name}`);
    console.log(`  소스: ${agent.source}`);
    console.log(`  경로: ${agent.path}`);
    if (agent.description) {
      console.log(`  설명: ${agent.description}`);
    }
    console.log("\n━━━ 내용 ━━━\n");
    console.log(content);
  }
}

// ============================================================================
// validate 핸들러
// ============================================================================

async function handleValidate(options: { json?: boolean }): Promise<void> {
  const startTime = Date.now();
  const json = options.json ?? false;

  const ctx = await ProjectContext.getInstance();
  const projectAgentsDir = join(ctx.gitRoot, PROJECT_AGENTS_DIR);

  const projectAgents = getAgents(projectAgentsDir, "project");
  const pluginAgents = getAgents(PLUGIN_AGENTS_DIR, "plugin");

  // 이름 충돌 검사
  const projectNames = new Set(projectAgents.map((a) => a.name));
  const pluginNames = new Set(pluginAgents.map((a) => a.name));
  const conflicts = [...projectNames].filter((name) => pluginNames.has(name));

  if (json) {
    outputJson(
      {
        valid: conflicts.length === 0,
        projectCount: projectAgents.length,
        pluginCount: pluginAgents.length,
        conflicts,
      },
      startTime
    );
  } else {
    console.log("\n━━━ 에이전트 검증 ━━━\n");
    console.log(`  프로젝트 에이전트: ${projectAgents.length}개`);
    console.log(`  플러그인 에이전트: ${pluginAgents.length}개`);

    if (conflicts.length > 0) {
      console.log("");
      console.log("[WARN] 이름 충돌 발견:");
      for (const name of conflicts) {
        console.log(`  - ${name}`);
      }
      console.log("");
      console.log("프로젝트 에이전트가 플러그인 에이전트보다 우선합니다.");
    } else {
      console.log("");
      console.log("[OK] 충돌 없음");
    }
    console.log("");
  }
}

// ============================================================================
// 커맨드 생성
// ============================================================================

export function createAgentCommand(): Command {
  const agent = new Command("agent")
    .description("에이전트 관리")
    .addHelpText(
      "after",
      `
Examples:
  tc agent list
  tc agent info spec-reviewer
  tc agent validate
`
    );

  agent
    .command("list")
    .description("에이전트 목록")
    .option("--json", "JSON 형식으로 출력")
    .action(handleList);

  agent
    .command("info <name>")
    .description("에이전트 상세 정보")
    .option("--json", "JSON 형식으로 출력")
    .action(handleInfo);

  agent
    .command("validate")
    .description("에이전트 충돌 검사")
    .option("--json", "JSON 형식으로 출력")
    .action(handleValidate);

  return agent;
}
