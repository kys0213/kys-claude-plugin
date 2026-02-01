/**
 * Unit Tests - 함수 단위 테스트
 */

import { existsSync } from "fs";
import { homedir } from "os";
import { ProjectContext, generateId, timestamp } from "../lib/context";
import { TestResult, measureAsync } from "../lib/utils";

type TestFn = () => Promise<void>;

interface UnitTest {
  name: string;
  fn: TestFn;
}

const tests: UnitTest[] = [];

function test(name: string, fn: TestFn): void {
  tests.push({ name, fn });
}

// ============================================================================
// ProjectContext 테스트
// ============================================================================

test("ProjectContext.getInstance() - 싱글톤 반환", async () => {
  const ctx1 = await ProjectContext.getInstance();
  const ctx2 = await ProjectContext.getInstance();
  if (ctx1 !== ctx2) {
    throw new Error("싱글톤이 아님");
  }
});

test("ProjectContext.gitRoot - Git 루트 경로", async () => {
  const ctx = await ProjectContext.getInstance();
  if (!ctx.gitRoot || ctx.gitRoot.length === 0) {
    throw new Error("gitRoot가 비어있음");
  }
  if (!existsSync(ctx.gitRoot)) {
    throw new Error(`gitRoot가 존재하지 않음: ${ctx.gitRoot}`);
  }
});

test("ProjectContext.projectHash - 12자리 해시", async () => {
  const ctx = await ProjectContext.getInstance();
  if (ctx.projectHash.length !== 12) {
    throw new Error(`해시 길이가 12가 아님: ${ctx.projectHash.length}`);
  }
  if (!/^[a-f0-9]+$/.test(ctx.projectHash)) {
    throw new Error(`유효하지 않은 해시 형식: ${ctx.projectHash}`);
  }
});

test("ProjectContext.dataDir - 데이터 디렉토리 경로", async () => {
  const ctx = await ProjectContext.getInstance();
  const expected = `${homedir()}/.team-claude/${ctx.projectHash}`;
  if (ctx.dataDir !== expected) {
    throw new Error(`dataDir 경로 불일치: ${ctx.dataDir} !== ${expected}`);
  }
});

test("ProjectContext.configPath - 설정 파일 경로", async () => {
  const ctx = await ProjectContext.getInstance();
  if (!ctx.configPath.endsWith("team-claude.yaml")) {
    throw new Error(`configPath가 잘못됨: ${ctx.configPath}`);
  }
});

test("ProjectContext.sessionsDir - 세션 디렉토리 경로", async () => {
  const ctx = await ProjectContext.getInstance();
  if (!ctx.sessionsDir.endsWith("sessions")) {
    throw new Error(`sessionsDir가 잘못됨: ${ctx.sessionsDir}`);
  }
});

test("ProjectContext.worktreesDir - Worktree 디렉토리 경로", async () => {
  const ctx = await ProjectContext.getInstance();
  if (!ctx.worktreesDir.endsWith("worktrees")) {
    throw new Error(`worktreesDir가 잘못됨: ${ctx.worktreesDir}`);
  }
});

test("ProjectContext.claudeDir - .claude 디렉토리 경로", async () => {
  const ctx = await ProjectContext.getInstance();
  if (!ctx.claudeDir.endsWith(".claude")) {
    throw new Error(`claudeDir가 잘못됨: ${ctx.claudeDir}`);
  }
});

test("ProjectContext.getInfo() - 정보 객체 반환", async () => {
  const ctx = await ProjectContext.getInstance();
  const info = ctx.getInfo();
  const requiredKeys = [
    "projectName",
    "gitRoot",
    "projectHash",
    "dataDir",
    "configPath",
  ];
  for (const key of requiredKeys) {
    if (!(key in info)) {
      throw new Error(`getInfo()에 ${key} 누락`);
    }
  }
});

// ============================================================================
// 유틸리티 함수 테스트
// ============================================================================

test("generateId() - 8자리 랜덤 ID", async () => {
  const id = generateId();
  if (id.length !== 8) {
    throw new Error(`ID 길이가 8이 아님: ${id.length}`);
  }
  if (!/^[a-z0-9]+$/.test(id)) {
    throw new Error(`유효하지 않은 ID 형식: ${id}`);
  }
});

test("generateId() - 고유성", async () => {
  const ids = new Set<string>();
  for (let i = 0; i < 100; i++) {
    ids.add(generateId());
  }
  if (ids.size < 95) {
    // 100개 중 95개 이상 고유해야 함
    throw new Error(`ID 고유성 부족: ${ids.size}/100`);
  }
});

test("timestamp() - ISO 8601 형식", async () => {
  const ts = timestamp();
  const parsed = new Date(ts);
  if (isNaN(parsed.getTime())) {
    throw new Error(`유효하지 않은 타임스탬프: ${ts}`);
  }
});

// ============================================================================
// 캐싱 성능 테스트
// ============================================================================

test("캐싱 성능 - getInstance() 반복 호출", async () => {
  // 캐시 워밍업
  await ProjectContext.getInstance();

  const iterations = 100;
  const start = performance.now();
  for (let i = 0; i < iterations; i++) {
    await ProjectContext.getInstance();
  }
  const duration = performance.now() - start;
  const avgMs = duration / iterations;

  if (avgMs > 1) {
    throw new Error(`캐싱이 작동하지 않음: 평균 ${avgMs.toFixed(2)}ms/호출`);
  }
});

// ============================================================================
// 테스트 실행
// ============================================================================

export async function runUnitTests(): Promise<TestResult[]> {
  const results: TestResult[] = [];

  for (const { name, fn } of tests) {
    const { duration } = await measureAsync(async () => {
      try {
        await fn();
        results.push({ name, passed: true, duration: 0 });
      } catch (e) {
        results.push({
          name,
          passed: false,
          duration: 0,
          error: e instanceof Error ? e.message : String(e),
        });
      }
    });
    results[results.length - 1].duration = duration;
  }

  return results;
}
