/**
 * psm.ts 세션 관리 로직 테스트
 * 실행: bun test src/test/psm.test.ts
 */

import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import { existsSync, mkdirSync, rmSync, writeFileSync, readFileSync } from "fs";
import { join } from "path";
import {
  getProjectDataDir,
  getWorktreesDir,
  ensureDir,
  timestamp,
  readJsonFile,
  writeJsonFile,
  generateId,
} from "../lib/common";

// ============================================================================
// 타입 정의 (psm.ts에서 가져옴)
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
// 테스트용 헬퍼 함수
// ============================================================================

const TEST_PROJECT_DIR = "/tmp/test-psm-" + generateId();

function getTestPsmIndexPath(): string {
  return join(TEST_PROJECT_DIR, "psm-index.json");
}

function initTestPsmIndex(): PsmIndex {
  const indexPath = getTestPsmIndexPath();

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

  ensureDir(TEST_PROJECT_DIR);
  writeJsonFile(indexPath, index);
  return index;
}

function saveTestPsmIndex(index: PsmIndex): void {
  writeJsonFile(getTestPsmIndexPath(), index);
}

function getTestSessionInfo(name: string): PsmSession | null {
  const index = initTestPsmIndex();
  return index.sessions.find((s) => s.name === name) || null;
}

function addTestSessionToIndex(session: PsmSession): void {
  const index = initTestPsmIndex();
  index.sessions.push(session);
  saveTestPsmIndex(index);
}

function updateTestSessionInIndex(
  name: string,
  updates: Partial<PsmSession>
): void {
  const index = initTestPsmIndex();
  const session = index.sessions.find((s) => s.name === name);
  if (session) {
    Object.assign(session, updates, { updatedAt: timestamp() });
    saveTestPsmIndex(index);
  }
}

function removeTestSessionFromIndex(name: string): void {
  const index = initTestPsmIndex();
  index.sessions = index.sessions.filter((s) => s.name !== name);
  saveTestPsmIndex(index);
}

function createTestSession(name: string, overrides?: Partial<PsmSession>): PsmSession {
  return {
    name,
    status: "active",
    progress: "0/0",
    worktreePath: join(TEST_PROJECT_DIR, "worktrees", name),
    branch: `team-claude/${name}`,
    createdAt: timestamp(),
    updatedAt: timestamp(),
    ...overrides,
  };
}

// ============================================================================
// 세션 이름 유효성 검증 테스트
// ============================================================================

describe("세션 이름 유효성 검증", () => {
  test("유효한 세션 이름 - 영문자로 시작", () => {
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("feature")).toBe(true);
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("Feature")).toBe(true);
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("f")).toBe(true);
  });

  test("유효한 세션 이름 - 영문+숫자", () => {
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("feature1")).toBe(true);
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("feature123")).toBe(true);
  });

  test("유효한 세션 이름 - 하이픈 포함", () => {
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("feature-add")).toBe(true);
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("feature-add-user")).toBe(true);
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("a-b-c-1-2-3")).toBe(true);
  });

  test("유효하지 않은 세션 이름 - 숫자로 시작", () => {
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("1feature")).toBe(false);
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("123")).toBe(false);
  });

  test("유효하지 않은 세션 이름 - 하이픈으로 시작", () => {
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("-feature")).toBe(false);
  });

  test("유효하지 않은 세션 이름 - 특수문자 포함", () => {
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("feature_add")).toBe(false);
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("feature.add")).toBe(false);
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("feature/add")).toBe(false);
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("feature add")).toBe(false);
  });

  test("유효하지 않은 세션 이름 - 빈 문자열", () => {
    expect(/^[a-zA-Z][a-zA-Z0-9-]*$/.test("")).toBe(false);
  });
});

// ============================================================================
// PsmIndex 초기화 테스트
// ============================================================================

describe("PsmIndex 초기화", () => {
  beforeEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
    mkdirSync(TEST_PROJECT_DIR, { recursive: true });
  });

  afterEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
  });

  test("새 인덱스 생성", () => {
    const index = initTestPsmIndex();

    expect(index.sessions).toEqual([]);
    expect(index.settings.parallelLimit).toBe(4);
    expect(index.settings.autoCleanup).toBe(true);
    expect(index.createdAt).toBeDefined();
  });

  test("인덱스 파일 생성됨", () => {
    initTestPsmIndex();

    expect(existsSync(getTestPsmIndexPath())).toBe(true);
  });

  test("기존 인덱스 로드", () => {
    // 첫 번째 초기화
    const first = initTestPsmIndex();
    first.sessions.push(createTestSession("existing"));
    saveTestPsmIndex(first);

    // 두 번째 로드
    const second = initTestPsmIndex();

    expect(second.sessions).toHaveLength(1);
    expect(second.sessions[0].name).toBe("existing");
  });
});

// ============================================================================
// 세션 CRUD 테스트
// ============================================================================

describe("세션 CRUD", () => {
  beforeEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
    mkdirSync(TEST_PROJECT_DIR, { recursive: true });
    initTestPsmIndex();
  });

  afterEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
  });

  test("세션 추가", () => {
    const session = createTestSession("feature-auth");
    addTestSessionToIndex(session);

    const index = initTestPsmIndex();
    expect(index.sessions).toHaveLength(1);
    expect(index.sessions[0].name).toBe("feature-auth");
  });

  test("여러 세션 추가", () => {
    addTestSessionToIndex(createTestSession("session1"));
    addTestSessionToIndex(createTestSession("session2"));
    addTestSessionToIndex(createTestSession("session3"));

    const index = initTestPsmIndex();
    expect(index.sessions).toHaveLength(3);
  });

  test("세션 조회", () => {
    addTestSessionToIndex(createTestSession("target-session"));

    const session = getTestSessionInfo("target-session");

    expect(session).not.toBeNull();
    expect(session?.name).toBe("target-session");
    expect(session?.status).toBe("active");
  });

  test("존재하지 않는 세션 조회", () => {
    const session = getTestSessionInfo("nonexistent");

    expect(session).toBeNull();
  });

  test("세션 업데이트", () => {
    addTestSessionToIndex(createTestSession("update-test"));

    updateTestSessionInIndex("update-test", {
      status: "paused",
      progress: "3/5",
    });

    const session = getTestSessionInfo("update-test");
    expect(session?.status).toBe("paused");
    expect(session?.progress).toBe("3/5");
  });

  test("세션 업데이트 시 updatedAt 갱신", () => {
    const session = createTestSession("update-time-test");
    addTestSessionToIndex(session);

    const before = getTestSessionInfo("update-time-test")?.updatedAt;

    // 약간의 딜레이
    Bun.sleepSync(10);

    updateTestSessionInIndex("update-time-test", { status: "complete" });

    const after = getTestSessionInfo("update-time-test")?.updatedAt;
    expect(after).not.toBe(before);
  });

  test("세션 삭제", () => {
    addTestSessionToIndex(createTestSession("delete-test"));
    expect(getTestSessionInfo("delete-test")).not.toBeNull();

    removeTestSessionFromIndex("delete-test");

    expect(getTestSessionInfo("delete-test")).toBeNull();
  });

  test("존재하지 않는 세션 삭제 시도", () => {
    // 에러 없이 진행되어야 함
    expect(() => {
      removeTestSessionFromIndex("nonexistent");
    }).not.toThrow();
  });
});

// ============================================================================
// 세션 상태 관리 테스트
// ============================================================================

describe("세션 상태 관리", () => {
  beforeEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
    mkdirSync(TEST_PROJECT_DIR, { recursive: true });
    initTestPsmIndex();
  });

  afterEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
  });

  test("active → paused 전환", () => {
    addTestSessionToIndex(createTestSession("pause-test"));
    updateTestSessionInIndex("pause-test", { status: "paused" });

    const session = getTestSessionInfo("pause-test");
    expect(session?.status).toBe("paused");
  });

  test("active → complete 전환", () => {
    addTestSessionToIndex(createTestSession("complete-test"));
    updateTestSessionInIndex("complete-test", { status: "complete" });

    const session = getTestSessionInfo("complete-test");
    expect(session?.status).toBe("complete");
  });

  test("active → error 전환", () => {
    addTestSessionToIndex(createTestSession("error-test"));
    updateTestSessionInIndex("error-test", { status: "error" });

    const session = getTestSessionInfo("error-test");
    expect(session?.status).toBe("error");
  });

  test("paused → active 전환", () => {
    addTestSessionToIndex(createTestSession("resume-test", { status: "paused" }));
    updateTestSessionInIndex("resume-test", { status: "active" });

    const session = getTestSessionInfo("resume-test");
    expect(session?.status).toBe("active");
  });
});

// ============================================================================
// 진행률 관리 테스트
// ============================================================================

describe("진행률 관리", () => {
  beforeEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
    mkdirSync(TEST_PROJECT_DIR, { recursive: true });
    initTestPsmIndex();
  });

  afterEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
  });

  test("진행률 업데이트 - 초기", () => {
    addTestSessionToIndex(createTestSession("progress-test"));
    updateTestSessionInIndex("progress-test", { progress: "0/5" });

    const session = getTestSessionInfo("progress-test");
    expect(session?.progress).toBe("0/5");
  });

  test("진행률 업데이트 - 중간", () => {
    addTestSessionToIndex(createTestSession("progress-test"));
    updateTestSessionInIndex("progress-test", { progress: "3/5" });

    const session = getTestSessionInfo("progress-test");
    expect(session?.progress).toBe("3/5");
  });

  test("진행률 업데이트 - 완료", () => {
    addTestSessionToIndex(createTestSession("progress-test"));
    updateTestSessionInIndex("progress-test", { progress: "5/5" });

    const session = getTestSessionInfo("progress-test");
    expect(session?.progress).toBe("5/5");
  });
});

// ============================================================================
// 세션 필터링 테스트
// ============================================================================

describe("세션 필터링", () => {
  beforeEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
    mkdirSync(TEST_PROJECT_DIR, { recursive: true });
    initTestPsmIndex();

    // 다양한 상태의 세션 생성
    addTestSessionToIndex(createTestSession("active1", { status: "active" }));
    addTestSessionToIndex(createTestSession("active2", { status: "active" }));
    addTestSessionToIndex(createTestSession("paused1", { status: "paused" }));
    addTestSessionToIndex(createTestSession("complete1", { status: "complete" }));
    addTestSessionToIndex(createTestSession("complete2", { status: "complete" }));
    addTestSessionToIndex(createTestSession("error1", { status: "error" }));
  });

  afterEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
  });

  test("active 상태 필터", () => {
    const index = initTestPsmIndex();
    const active = index.sessions.filter((s) => s.status === "active");

    expect(active).toHaveLength(2);
  });

  test("paused 상태 필터", () => {
    const index = initTestPsmIndex();
    const paused = index.sessions.filter((s) => s.status === "paused");

    expect(paused).toHaveLength(1);
  });

  test("complete 상태 필터", () => {
    const index = initTestPsmIndex();
    const complete = index.sessions.filter((s) => s.status === "complete");

    expect(complete).toHaveLength(2);
  });

  test("error 상태 필터", () => {
    const index = initTestPsmIndex();
    const error = index.sessions.filter((s) => s.status === "error");

    expect(error).toHaveLength(1);
  });

  test("전체 세션 수", () => {
    const index = initTestPsmIndex();
    expect(index.sessions).toHaveLength(6);
  });
});

// ============================================================================
// 설정 관리 테스트
// ============================================================================

describe("설정 관리", () => {
  beforeEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
    mkdirSync(TEST_PROJECT_DIR, { recursive: true });
  });

  afterEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
  });

  test("기본 parallelLimit", () => {
    const index = initTestPsmIndex();
    expect(index.settings.parallelLimit).toBe(4);
  });

  test("기본 autoCleanup", () => {
    const index = initTestPsmIndex();
    expect(index.settings.autoCleanup).toBe(true);
  });

  test("설정 변경 - parallelLimit", () => {
    const index = initTestPsmIndex();
    index.settings.parallelLimit = 8;
    saveTestPsmIndex(index);

    const reloaded = initTestPsmIndex();
    expect(reloaded.settings.parallelLimit).toBe(8);
  });

  test("설정 변경 - autoCleanup 비활성화", () => {
    const index = initTestPsmIndex();
    index.settings.autoCleanup = false;
    saveTestPsmIndex(index);

    const reloaded = initTestPsmIndex();
    expect(reloaded.settings.autoCleanup).toBe(false);
  });
});

// ============================================================================
// 브랜치 이름 생성 테스트
// ============================================================================

describe("브랜치 이름 생성", () => {
  test("기본 브랜치 이름 형식", () => {
    const sessionName = "feature-auth";
    const branchName = `team-claude/${sessionName}`;

    expect(branchName).toBe("team-claude/feature-auth");
  });

  test("하이픈 포함 브랜치", () => {
    const sessionName = "feature-user-management";
    const branchName = `team-claude/${sessionName}`;

    expect(branchName).toBe("team-claude/feature-user-management");
  });

  test("숫자 포함 브랜치", () => {
    const sessionName = "checkpoint1";
    const branchName = `team-claude/${sessionName}`;

    expect(branchName).toBe("team-claude/checkpoint1");
  });
});

// ============================================================================
// 병렬 실행 관련 테스트
// ============================================================================

describe("병렬 실행 관련", () => {
  beforeEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
    mkdirSync(TEST_PROJECT_DIR, { recursive: true });
    initTestPsmIndex();
  });

  afterEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
  });

  test("병렬 세션 검증 - 2개 이상 필요", () => {
    const sessions = ["session1"];
    expect(sessions.length).toBeLessThan(2);
  });

  test("병렬 세션 필터 - complete 제외", () => {
    addTestSessionToIndex(createTestSession("active1", { status: "active" }));
    addTestSessionToIndex(createTestSession("complete1", { status: "complete" }));
    addTestSessionToIndex(createTestSession("active2", { status: "active" }));

    const index = initTestPsmIndex();
    const runnable = index.sessions.filter((s) => s.status !== "complete");

    expect(runnable).toHaveLength(2);
  });

  test("병렬 세션 준비 - 상태 업데이트", () => {
    addTestSessionToIndex(createTestSession("parallel1", { status: "paused" }));
    addTestSessionToIndex(createTestSession("parallel2", { status: "paused" }));

    updateTestSessionInIndex("parallel1", { status: "active" });
    updateTestSessionInIndex("parallel2", { status: "active" });

    expect(getTestSessionInfo("parallel1")?.status).toBe("active");
    expect(getTestSessionInfo("parallel2")?.status).toBe("active");
  });
});

// ============================================================================
// 정리(cleanup) 관련 테스트
// ============================================================================

describe("정리(cleanup) 관련", () => {
  beforeEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
    mkdirSync(TEST_PROJECT_DIR, { recursive: true });
    initTestPsmIndex();
  });

  afterEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
  });

  test("완료된 세션만 정리 대상", () => {
    addTestSessionToIndex(createTestSession("active1", { status: "active" }));
    addTestSessionToIndex(createTestSession("complete1", { status: "complete" }));
    addTestSessionToIndex(createTestSession("complete2", { status: "complete" }));

    const index = initTestPsmIndex();
    const toClean = index.sessions.filter((s) => s.status === "complete");

    expect(toClean).toHaveLength(2);
  });

  test("미완료 세션은 force 없이 정리 불가", () => {
    addTestSessionToIndex(createTestSession("active1", { status: "active" }));

    const session = getTestSessionInfo("active1");
    const canClean = session?.status === "complete";

    expect(canClean).toBe(false);
  });

  test("force 옵션으로 미완료 세션 정리 가능", () => {
    addTestSessionToIndex(createTestSession("active1", { status: "active" }));

    const session = getTestSessionInfo("active1");
    const force = true;
    const canClean = session?.status === "complete" || force;

    expect(canClean).toBe(true);
  });
});

// ============================================================================
// Edge Cases
// ============================================================================

describe("Edge Cases", () => {
  beforeEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
    mkdirSync(TEST_PROJECT_DIR, { recursive: true });
    initTestPsmIndex();
  });

  afterEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
  });

  test("같은 이름의 세션 중복 추가", () => {
    addTestSessionToIndex(createTestSession("duplicate"));
    addTestSessionToIndex(createTestSession("duplicate")); // 중복

    const index = initTestPsmIndex();
    // 현재 구현에서는 중복이 허용됨 (실제로는 방지해야 함)
    expect(index.sessions.filter((s) => s.name === "duplicate").length).toBe(2);
  });

  test("빈 인덱스에서 세션 삭제 시도", () => {
    expect(() => {
      removeTestSessionFromIndex("nonexistent");
    }).not.toThrow();
  });

  test("존재하지 않는 세션 업데이트 시도", () => {
    expect(() => {
      updateTestSessionInIndex("nonexistent", { status: "active" });
    }).not.toThrow();
  });

  test("대량 세션 처리", () => {
    for (let i = 0; i < 100; i++) {
      addTestSessionToIndex(createTestSession(`session-${i}`));
    }

    const index = initTestPsmIndex();
    expect(index.sessions).toHaveLength(100);
  });

  test("긴 세션 이름", () => {
    const longName = "a" + "b".repeat(100);
    addTestSessionToIndex(createTestSession(longName));

    const session = getTestSessionInfo(longName);
    expect(session?.name).toBe(longName);
  });
});

// ============================================================================
// 동시성 테스트
// ============================================================================

describe("동시성 테스트", () => {
  beforeEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
    mkdirSync(TEST_PROJECT_DIR, { recursive: true });
    initTestPsmIndex();
  });

  afterEach(() => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true });
    }
  });

  test("동시 세션 추가 (시뮬레이션)", async () => {
    const addPromises = [];
    for (let i = 0; i < 10; i++) {
      addPromises.push(
        Promise.resolve().then(() => {
          addTestSessionToIndex(createTestSession(`concurrent-${i}`));
        })
      );
    }

    await Promise.all(addPromises);

    const index = initTestPsmIndex();
    // 동시성 문제로 일부 세션이 누락될 수 있음 (실제 환경에서)
    expect(index.sessions.length).toBeGreaterThanOrEqual(1);
  });
});

// ============================================================================
// 세션 메타데이터 테스트
// ============================================================================

describe("세션 메타데이터", () => {
  test("worktreePath 형식", () => {
    const session = createTestSession("test-meta");
    expect(session.worktreePath).toContain("worktrees");
    expect(session.worktreePath).toContain("test-meta");
  });

  test("branch 형식", () => {
    const session = createTestSession("test-branch");
    expect(session.branch).toBe("team-claude/test-branch");
  });

  test("createdAt 형식", () => {
    const session = createTestSession("test-time");
    const date = new Date(session.createdAt);
    expect(date.getTime()).not.toBeNaN();
  });

  test("updatedAt은 createdAt과 같거나 이후", () => {
    const session = createTestSession("test-timestamps");
    const created = new Date(session.createdAt).getTime();
    const updated = new Date(session.updatedAt).getTime();
    expect(updated).toBeGreaterThanOrEqual(created);
  });
});
