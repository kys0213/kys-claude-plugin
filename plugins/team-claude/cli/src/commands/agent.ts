/**
 * agent 명령어 - 에이전트 관리
 */

import { Command } from "commander";
import { existsSync, readdirSync } from "fs";
import { readFile, writeFile, mkdir } from "fs/promises";
import { join } from "path";
import matter from "gray-matter";
import { log, printSection, printStatus, printKV, icon } from "../lib/utils";
import { ProjectContext } from "../lib/context";
import { homedir } from "os";

// ============================================================================
// 경로 상수
// ============================================================================
const PROJECT_AGENTS_DIR = ".claude/agents";
const PLUGIN_AGENTS_DIR = join(homedir(), ".claude/plugins/team-claude/agents");

interface AgentMetadata {
  name?: string;
  description?: string;
  model?: string;
  tools?: string[];
}

interface AgentInfo {
  name: string;
  description: string;
  model: string;
  tools: string[];
  source: "project" | "plugin";
  filePath: string;
  content: string;
}

// ============================================================================
// Helper Functions
// ============================================================================

async function parseAgentFile(filePath: string): Promise<AgentInfo | null> {
  try {
    const content = await readFile(filePath, "utf-8");
    const { data, content: bodyContent } = matter(content);
    const metadata = data as AgentMetadata;

    const fileName = filePath.split("/").pop()?.replace(".md", "") || "unknown";
    const name = metadata.name || fileName;
    const description = metadata.description || "(설명 없음)";
    const model = metadata.model || "sonnet";
    const tools = metadata.tools || [];

    const source = filePath.includes(PROJECT_AGENTS_DIR) ? "project" : "plugin";

    return {
      name,
      description,
      model,
      tools,
      source,
      filePath,
      content: bodyContent,
    };
  } catch (error) {
    log.err(`파일 파싱 실패 ${filePath}: ${error}`);
    return null;
  }
}

async function getProjectAgents(): Promise<AgentInfo[]> {
  const ctx = await ProjectContext.getInstance();
  const agentsDir = join(ctx.gitRoot, PROJECT_AGENTS_DIR);

  if (!existsSync(agentsDir)) {
    return [];
  }

  const agents: AgentInfo[] = [];
  const files = readdirSync(agentsDir);

  for (const file of files) {
    if (file.endsWith(".md")) {
      const filePath = join(agentsDir, file);
      const agent = await parseAgentFile(filePath);
      if (agent) {
        agents.push(agent);
      }
    }
  }

  return agents;
}

async function getPluginAgents(): Promise<AgentInfo[]> {
  if (!existsSync(PLUGIN_AGENTS_DIR)) {
    return [];
  }

  const agents: AgentInfo[] = [];
  const files = readdirSync(PLUGIN_AGENTS_DIR);

  for (const file of files) {
    if (file.endsWith(".md")) {
      const filePath = join(PLUGIN_AGENTS_DIR, file);
      const agent = await parseAgentFile(filePath);
      if (agent) {
        agents.push(agent);
      }
    }
  }

  return agents;
}

async function findAgent(name: string): Promise<AgentInfo | null> {
  const ctx = await ProjectContext.getInstance();
  const projectFile = join(ctx.gitRoot, PROJECT_AGENTS_DIR, `${name}.md`);
  const pluginFile = join(PLUGIN_AGENTS_DIR, `${name}.md`);

  // 프로젝트 우선
  if (existsSync(projectFile)) {
    return await parseAgentFile(projectFile);
  }

  if (existsSync(pluginFile)) {
    return await parseAgentFile(pluginFile);
  }

  return null;
}

// ============================================================================
// list - 에이전트 목록 조회
// ============================================================================

async function listCommand(): Promise<void> {
  const projectAgents = await getProjectAgents();
  const pluginAgents = await getPluginAgents();

  console.log();
  printSection("Team Claude 에이전트 목록");
  console.log();

  // --- 프로젝트 로컬 에이전트 ---
  console.log("📁 프로젝트 에이전트 (.claude/agents/)");

  if (projectAgents.length === 0) {
    const ctx = await ProjectContext.getInstance();
    const agentsDir = join(ctx.gitRoot, PROJECT_AGENTS_DIR);
    if (!existsSync(agentsDir)) {
      console.log("  (디렉토리 없음 - tc agent init으로 생성)");
    } else {
      console.log("  (에이전트 없음)");
    }
  } else {
    for (const agent of projectAgents) {
      console.log(`  🟢 ${agent.name}`);
      console.log(`     ${agent.description}`);
    }
  }

  console.log();

  // --- 플러그인 기본 에이전트 ---
  console.log("📦 플러그인 에이전트 (~/.claude/plugins/team-claude/agents/)");

  if (pluginAgents.length === 0) {
    console.log("  (플러그인 에이전트 디렉토리 없음)");
  } else {
    for (const agent of pluginAgents) {
      console.log(`  🔵 ${agent.name}`);
      console.log(`     ${agent.description}`);
    }
  }

  console.log();
}

// ============================================================================
// validate - 이름 충돌 검사
// ============================================================================

async function validateCommand(): Promise<void> {
  const projectAgents = await getProjectAgents();
  const pluginAgents = await getPluginAgents();

  console.log();
  printSection("에이전트 이름 충돌 검사");
  console.log();

  if (projectAgents.length === 0) {
    log.info("프로젝트 에이전트가 없습니다. (.claude/agents/)");
    console.log();
    return;
  }

  // 플러그인 에이전트 이름 맵
  const pluginAgentMap = new Map<string, AgentInfo>();
  for (const agent of pluginAgents) {
    pluginAgentMap.set(agent.name, agent);
  }

  let warnings = 0;

  // 프로젝트 에이전트 검사
  for (const agent of projectAgents) {
    const pluginAgent = pluginAgentMap.get(agent.name);

    if (pluginAgent) {
      console.log(`  ⚠️  ${agent.name}`);
      console.log(`     프로젝트: ${agent.filePath}`);
      console.log(`     플러그인: ${pluginAgent.filePath}`);
      console.log(`     → 프로젝트 에이전트가 플러그인을 오버라이드합니다`);
      console.log();
      warnings++;
    } else {
      console.log(`  ✓ ${agent.name}`);
    }
  }

  console.log();
  console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

  if (warnings === 0) {
    console.log(`${icon.check} 충돌 없음`);
  } else {
    console.log(`⚠️  오버라이드 ${warnings}개 (의도된 경우 무시 가능)`);
  }
  console.log();
}

// ============================================================================
// info - 에이전트 상세 정보
// ============================================================================

async function infoCommand(name: string): Promise<void> {
  if (!name) {
    log.err("에이전트 이름을 지정하세요.");
    log.info("사용법: tc agent info <name>");
    process.exit(1);
  }

  const agent = await findAgent(name);

  if (!agent) {
    const ctx = await ProjectContext.getInstance();
    const projectFile = join(ctx.gitRoot, PROJECT_AGENTS_DIR, `${name}.md`);
    const pluginFile = join(PLUGIN_AGENTS_DIR, `${name}.md`);

    log.err(`에이전트를 찾을 수 없습니다: ${name}`);
    log.err("확인할 위치:");
    log.err(`  - ${projectFile}`);
    log.err(`  - ${pluginFile}`);
    process.exit(1);
  }

  console.log();
  printSection(`에이전트 상세: ${name}`);
  console.log();

  printKV("소스", agent.source === "project" ? "프로젝트" : "플러그인");
  printKV("파일", agent.filePath);
  console.log();
  printKV("설명", agent.description);
  printKV("모델", agent.model);
  printKV("도구", JSON.stringify(agent.tools));
  console.log();

  // 마크다운 본문 미리보기 (첫 15줄)
  printSection("본문 미리보기");
  console.log();

  const lines = agent.content.trim().split("\n").slice(0, 15);
  for (const line of lines) {
    console.log(line);
  }

  console.log();
  console.log(`(전체 보기: cat ${agent.filePath})`);
  console.log();
}

// ============================================================================
// init - 에이전트 디렉토리 초기화
// ============================================================================

async function initCommand(): Promise<void> {
  const ctx = await ProjectContext.getInstance();
  const agentsDir = join(ctx.gitRoot, PROJECT_AGENTS_DIR);

  if (existsSync(agentsDir)) {
    log.info(`에이전트 디렉토리가 이미 존재합니다: ${agentsDir}`);
    return;
  }

  await mkdir(agentsDir, { recursive: true });
  log.ok(`${icon.check} 에이전트 디렉토리 생성됨: ${agentsDir}`);

  // 예제 템플릿 생성
  const templateFile = join(agentsDir, ".example-agent.md");
  const templateContent = `---
name: example-agent
description: 예제 에이전트 - 이 파일을 복사하여 커스텀 에이전트를 만드세요
model: sonnet
tools: ["Read", "Glob", "Grep"]
---

# Example Agent

이 파일은 에이전트 템플릿 예제입니다.

## 역할

- 역할 1 설명
- 역할 2 설명

## 리뷰 체크리스트

- [ ] 체크 항목 1
- [ ] 체크 항목 2

## 프로젝트 컨텍스트

(선택) 이 프로젝트에 특화된 지침을 여기에 작성하세요.
`;

  await writeFile(templateFile, templateContent, "utf-8");

  log.info(`예제 템플릿 생성됨: ${templateFile}`);
  console.log();
  console.log("다음 단계:");
  console.log("  1. .example-agent.md를 복사하여 새 에이전트 생성");
  console.log("  2. tc agent list 로 에이전트 확인");
  console.log("  3. tc agent validate 로 충돌 검사");
  console.log();
}

// ============================================================================
// 명령어 생성
// ============================================================================

export function createAgentCommand(): Command {
  const cmd = new Command("agent").description("에이전트 관리");

  cmd
    .command("list")
    .description("모든 에이전트 목록 조회 (프로젝트 + 플러그인)")
    .action(async () => {
      await listCommand();
    });

  cmd
    .command("validate")
    .description("에이전트 이름 충돌 검사")
    .action(async () => {
      await validateCommand();
    });

  cmd
    .command("info")
    .description("에이전트 상세 정보")
    .argument("<name>", "에이전트 이름")
    .action(async (name: string) => {
      await infoCommand(name);
    });

  cmd
    .command("init")
    .description(".claude/agents 디렉토리 생성")
    .action(async () => {
      await initCommand();
    });

  return cmd;
}
