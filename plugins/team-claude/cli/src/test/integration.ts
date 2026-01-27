/**
 * Integration Tests - 통합 테스트
 */

import { existsSync, rmSync, mkdirSync, writeFileSync, readFileSync } from "fs";
import { join } from "path";
import { ProjectContext, generateId } from "../lib/context";
import { TestResult, measureAsync } from "../lib/utils";

type TestFn = () => Promise<void>;

interface IntegrationTest {
  name: string;
  fn: TestFn;
}

const tests: IntegrationTest[] = [];

function test(name: string, fn: TestFn): void {
  tests.push({ name, fn });
}

// ============================================================================
// 디렉토리 생성 테스트
// ============================================================================

test("ensureDataDirs() - 전역 데이터 디렉토리 생성", async () => {
  const ctx = await ProjectContext.getInstance();
  ctx.ensureDataDirs();

  const dirs = [ctx.dataDir, ctx.sessionsDir, ctx.stateDir, ctx.worktreesDir];
  for (const dir of dirs) {
    if (!existsSync(dir)) {
      throw new Error(`디렉토리 생성 실패: ${dir}`);
    }
  }
});

test("ensureClaudeDirs() - .claude 디렉토리 생성", async () => {
  const ctx = await ProjectContext.getInstance();
  ctx.ensureClaudeDirs();

  const dirs = [ctx.claudeDir, ctx.agentsDir, ctx.hooksDir];
  for (const dir of dirs) {
    if (!existsSync(dir)) {
      throw new Error(`디렉토리 생성 실패: ${dir}`);
    }
  }
});

// ============================================================================
// 세션 CRUD 테스트
// ============================================================================

test("세션 생성/조회/삭제 사이클", async () => {
  const ctx = await ProjectContext.getInstance();
  ctx.ensureDataDirs();

  const sessionId = generateId();
  const sessionDir = join(ctx.sessionsDir, sessionId);

  // 생성
  mkdirSync(sessionDir, { recursive: true });
  const metaPath = join(sessionDir, "meta.json");
  const meta = {
    sessionId,
    title: "테스트 세션",
    status: "designing",
    createdAt: new Date().toISOString(),
  };
  writeFileSync(metaPath, JSON.stringify(meta, null, 2));

  // 조회
  if (!ctx.sessionExists(sessionId)) {
    throw new Error("세션 존재 확인 실패");
  }

  const readMeta = JSON.parse(readFileSync(metaPath, "utf-8"));
  if (readMeta.title !== "테스트 세션") {
    throw new Error("메타 데이터 불일치");
  }

  // 삭제
  rmSync(sessionDir, { recursive: true, force: true });

  if (ctx.sessionExists(sessionId)) {
    throw new Error("세션 삭제 실패");
  }
});

// ============================================================================
// 설정 파일 테스트
// ============================================================================

test("설정 파일 생성/읽기", async () => {
  const ctx = await ProjectContext.getInstance();
  ctx.ensureDataDirs();

  const testConfig = `
version: "1.0"
project:
  name: test-project
  language: typescript
`.trim();

  // 백업 (기존 설정이 있을 경우)
  const backupPath = ctx.configPath + ".backup";
  let hadExisting = false;
  if (existsSync(ctx.configPath)) {
    hadExisting = true;
    const existing = readFileSync(ctx.configPath, "utf-8");
    writeFileSync(backupPath, existing);
  }

  try {
    // 테스트 설정 작성
    writeFileSync(ctx.configPath, testConfig);

    // 읽기 확인
    const read = readFileSync(ctx.configPath, "utf-8");
    if (!read.includes("test-project")) {
      throw new Error("설정 파일 내용 불일치");
    }

    // 상태 확인
    if (!ctx.configExists()) {
      throw new Error("configExists() 실패");
    }
  } finally {
    // 복원
    if (hadExisting) {
      const backup = readFileSync(backupPath, "utf-8");
      writeFileSync(ctx.configPath, backup);
      rmSync(backupPath);
    }
  }
});

// ============================================================================
// Git 연동 테스트
// ============================================================================

test("Git 루트에서 .claude 디렉토리 접근", async () => {
  const ctx = await ProjectContext.getInstance();

  // gitRoot가 실제 Git 저장소인지 확인
  const gitDir = join(ctx.gitRoot, ".git");
  if (!existsSync(gitDir)) {
    throw new Error(`Git 디렉토리가 없음: ${gitDir}`);
  }
});

// ============================================================================
// 파일 시스템 테스트
// ============================================================================

test("worktrees 디렉토리 CRUD", async () => {
  const ctx = await ProjectContext.getInstance();
  ctx.ensureDataDirs();

  const testDir = join(ctx.worktreesDir, "test-checkpoint");

  // 생성
  mkdirSync(testDir, { recursive: true });
  if (!existsSync(testDir)) {
    throw new Error("worktree 디렉토리 생성 실패");
  }

  // 삭제
  rmSync(testDir, { recursive: true, force: true });
  if (existsSync(testDir)) {
    throw new Error("worktree 디렉토리 삭제 실패");
  }
});

// ============================================================================
// 동시성 테스트
// ============================================================================

test("동시 getInstance() 호출", async () => {
  ProjectContext.resetInstance();

  const promises = Array(10)
    .fill(null)
    .map(() => ProjectContext.getInstance());

  const instances = await Promise.all(promises);
  const first = instances[0];

  for (const instance of instances) {
    if (instance !== first) {
      throw new Error("동시 호출 시 다른 인스턴스 반환");
    }
  }
});

// ============================================================================
// 테스트 실행
// ============================================================================

export async function runIntegrationTests(): Promise<TestResult[]> {
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
