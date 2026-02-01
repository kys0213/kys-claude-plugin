/**
 * flow.ts 명령어 로직 테스트
 * 실행: bun test src/test/flow.test.ts
 */

import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import { existsSync, rmSync } from "fs";
import { join } from "path";
import {
  getSessionsDir,
  getStateDir,
  ensureDir,
  generateId,
  timestamp,
  readJsonFile,
  writeJsonFile,
  parseMagicKeyword,
  type ImplStrategy,
} from "../lib/common";

// ============================================================================
// 타입 정의 (flow.ts에서 가져옴)
// ============================================================================

interface FlowState {
  sessionId: string;
  mode: string;
  implStrategy: ImplStrategy;
  requirement: string;
  status: string;
  currentPhase: string;
  phases: {
    spec: PhaseState;
    impl: PhaseState;
    merge: PhaseState;
  };
  escalations: Escalation[];
  createdAt: string;
  updatedAt: string;
}

interface PhaseState {
  status: string;
  strategy?: string;
  iterations: number;
  startedAt: string | null;
  completedAt: string | null;
}

interface Escalation {
  phase: string;
  reason: string;
  timestamp: string;
}

interface WorkflowState {
  currentSession: string | null;
  phase: string;
}

// ============================================================================
// 헬퍼 함수 (flow.ts에서 추출)
// ============================================================================

function getFlowStatePath(sessionId: string): string {
  return join(getSessionsDir(), sessionId, "flow-state.json");
}

function getWorkflowStatePath(): string {
  return join(getStateDir(), "workflow.json");
}

function initFlowState(
  sessionId: string,
  mode: string,
  requirement: string,
  implStrategy: ImplStrategy
): FlowState {
  const now = timestamp();

  const state: FlowState = {
    sessionId,
    mode,
    implStrategy,
    requirement,
    status: "started",
    currentPhase: "spec",
    phases: {
      spec: {
        status: "pending",
        iterations: 0,
        startedAt: null,
        completedAt: null,
      },
      impl: {
        status: "pending",
        strategy: implStrategy,
        iterations: 0,
        startedAt: null,
        completedAt: null,
      },
      merge: {
        status: "pending",
        iterations: 0,
        startedAt: null,
        completedAt: null,
      },
    },
    escalations: [],
    createdAt: now,
    updatedAt: now,
  };

  const flowPath = getFlowStatePath(sessionId);
  ensureDir(join(getSessionsDir(), sessionId));
  writeJsonFile(flowPath, state);

  return state;
}

function updateWorkflowState(sessionId: string): void {
  const statePath = getWorkflowStatePath();
  ensureDir(getStateDir());

  const state: WorkflowState = {
    currentSession: sessionId,
    phase: "flow_started",
  };

  writeJsonFile(statePath, state);
}

function updatePhaseState(
  sessionId: string,
  phase: keyof FlowState["phases"],
  updates: Partial<PhaseState>
): void {
  const flowPath = getFlowStatePath(sessionId);
  const state = readJsonFile<FlowState>(flowPath);
  if (!state) return;

  state.phases[phase] = { ...state.phases[phase], ...updates };
  state.updatedAt = timestamp();
  writeJsonFile(flowPath, state);
}

function addEscalation(
  sessionId: string,
  phase: string,
  reason: string
): void {
  const flowPath = getFlowStatePath(sessionId);
  const state = readJsonFile<FlowState>(flowPath);
  if (!state) return;

  state.escalations.push({
    phase,
    reason,
    timestamp: timestamp(),
  });
  state.updatedAt = timestamp();
  writeJsonFile(flowPath, state);
}

function transitionPhase(
  sessionId: string,
  newPhase: "spec" | "impl" | "merge"
): void {
  const flowPath = getFlowStatePath(sessionId);
  const state = readJsonFile<FlowState>(flowPath);
  if (!state) return;

  state.currentPhase = newPhase;
  state.updatedAt = timestamp();
  writeJsonFile(flowPath, state);
}

// ============================================================================
// FlowState 초기화 테스트
// ============================================================================

describe("FlowState 초기화", () => {
  const testSessionId = "test-flow-" + generateId();

  afterEach(() => {
    // 테스트 세션 정리
    const sessionDir = join(getSessionsDir(), testSessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("기본 FlowState 생성", () => {
    const state = initFlowState(
      testSessionId,
      "assisted",
      "테스트 요구사항",
      "psm"
    );

    expect(state.sessionId).toBe(testSessionId);
    expect(state.mode).toBe("assisted");
    expect(state.implStrategy).toBe("psm");
    expect(state.requirement).toBe("테스트 요구사항");
    expect(state.status).toBe("started");
    expect(state.currentPhase).toBe("spec");
  });

  test("모든 phase가 pending 상태로 초기화", () => {
    const state = initFlowState(
      testSessionId,
      "autopilot",
      "테스트",
      "swarm"
    );

    expect(state.phases.spec.status).toBe("pending");
    expect(state.phases.impl.status).toBe("pending");
    expect(state.phases.merge.status).toBe("pending");
  });

  test("impl phase에 strategy 저장", () => {
    const state = initFlowState(testSessionId, "manual", "테스트", "sequential");

    expect(state.phases.impl.strategy).toBe("sequential");
  });

  test("escalations 빈 배열로 초기화", () => {
    const state = initFlowState(testSessionId, "assisted", "테스트", "psm");

    expect(state.escalations).toEqual([]);
  });

  test("타임스탬프 생성", () => {
    const state = initFlowState(testSessionId, "assisted", "테스트", "psm");

    expect(state.createdAt).toBeDefined();
    expect(state.updatedAt).toBeDefined();
    expect(new Date(state.createdAt).getTime()).not.toBeNaN();
  });

  test("파일 시스템에 저장됨", () => {
    initFlowState(testSessionId, "assisted", "테스트", "psm");

    const flowPath = getFlowStatePath(testSessionId);
    expect(existsSync(flowPath)).toBe(true);
  });
});

// ============================================================================
// Phase 상태 업데이트 테스트
// ============================================================================

describe("Phase 상태 업데이트", () => {
  const testSessionId = "test-phase-" + generateId();

  beforeEach(() => {
    initFlowState(testSessionId, "assisted", "테스트", "psm");
  });

  afterEach(() => {
    const sessionDir = join(getSessionsDir(), testSessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("spec phase 상태 변경", () => {
    updatePhaseState(testSessionId, "spec", {
      status: "in_progress",
      startedAt: timestamp(),
    });

    const state = readJsonFile<FlowState>(getFlowStatePath(testSessionId));
    expect(state?.phases.spec.status).toBe("in_progress");
    expect(state?.phases.spec.startedAt).not.toBeNull();
  });

  test("impl phase iterations 증가", () => {
    updatePhaseState(testSessionId, "impl", {
      iterations: 3,
    });

    const state = readJsonFile<FlowState>(getFlowStatePath(testSessionId));
    expect(state?.phases.impl.iterations).toBe(3);
  });

  test("merge phase 완료 처리", () => {
    const completedAt = timestamp();
    updatePhaseState(testSessionId, "merge", {
      status: "complete",
      completedAt,
    });

    const state = readJsonFile<FlowState>(getFlowStatePath(testSessionId));
    expect(state?.phases.merge.status).toBe("complete");
    expect(state?.phases.merge.completedAt).toBe(completedAt);
  });

  test("updatedAt 자동 갱신", () => {
    const beforeUpdate = readJsonFile<FlowState>(
      getFlowStatePath(testSessionId)
    );
    const beforeTime = beforeUpdate?.updatedAt;

    // 약간의 딜레이
    Bun.sleepSync(10);

    updatePhaseState(testSessionId, "spec", { status: "in_progress" });

    const afterUpdate = readJsonFile<FlowState>(
      getFlowStatePath(testSessionId)
    );
    expect(afterUpdate?.updatedAt).not.toBe(beforeTime);
  });
});

// ============================================================================
// Phase 전환 테스트
// ============================================================================

describe("Phase 전환", () => {
  const testSessionId = "test-transition-" + generateId();

  beforeEach(() => {
    initFlowState(testSessionId, "assisted", "테스트", "psm");
  });

  afterEach(() => {
    const sessionDir = join(getSessionsDir(), testSessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("spec → impl 전환", () => {
    transitionPhase(testSessionId, "impl");

    const state = readJsonFile<FlowState>(getFlowStatePath(testSessionId));
    expect(state?.currentPhase).toBe("impl");
  });

  test("impl → merge 전환", () => {
    transitionPhase(testSessionId, "impl");
    transitionPhase(testSessionId, "merge");

    const state = readJsonFile<FlowState>(getFlowStatePath(testSessionId));
    expect(state?.currentPhase).toBe("merge");
  });

  test("비순차 전환도 허용 (merge → spec)", () => {
    transitionPhase(testSessionId, "merge");
    transitionPhase(testSessionId, "spec");

    const state = readJsonFile<FlowState>(getFlowStatePath(testSessionId));
    expect(state?.currentPhase).toBe("spec");
  });
});

// ============================================================================
// Escalation 테스트
// ============================================================================

describe("Escalation 관리", () => {
  const testSessionId = "test-escalation-" + generateId();

  beforeEach(() => {
    initFlowState(testSessionId, "autopilot", "테스트", "psm");
  });

  afterEach(() => {
    const sessionDir = join(getSessionsDir(), testSessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("escalation 추가", () => {
    addEscalation(testSessionId, "spec", "사용자 확인 필요");

    const state = readJsonFile<FlowState>(getFlowStatePath(testSessionId));
    expect(state?.escalations).toHaveLength(1);
    expect(state?.escalations[0].phase).toBe("spec");
    expect(state?.escalations[0].reason).toBe("사용자 확인 필요");
  });

  test("여러 escalation 누적", () => {
    addEscalation(testSessionId, "spec", "첫 번째");
    addEscalation(testSessionId, "impl", "두 번째");
    addEscalation(testSessionId, "merge", "세 번째");

    const state = readJsonFile<FlowState>(getFlowStatePath(testSessionId));
    expect(state?.escalations).toHaveLength(3);
  });

  test("escalation에 타임스탬프 포함", () => {
    addEscalation(testSessionId, "spec", "테스트");

    const state = readJsonFile<FlowState>(getFlowStatePath(testSessionId));
    expect(state?.escalations[0].timestamp).toBeDefined();
    expect(new Date(state?.escalations[0].timestamp || "").getTime()).not.toBeNaN();
  });
});

// ============================================================================
// WorkflowState 테스트
// ============================================================================

describe("WorkflowState 관리", () => {
  const testSessionId = "test-workflow-" + generateId();

  afterEach(() => {
    const sessionDir = join(getSessionsDir(), testSessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("현재 세션 설정", () => {
    initFlowState(testSessionId, "assisted", "테스트", "psm");
    updateWorkflowState(testSessionId);

    const workflowState = readJsonFile<WorkflowState>(getWorkflowStatePath());
    expect(workflowState?.currentSession).toBe(testSessionId);
    expect(workflowState?.phase).toBe("flow_started");
  });

  test("세션 전환 시 업데이트", () => {
    const sessionId1 = "test-workflow-1-" + generateId();
    const sessionId2 = "test-workflow-2-" + generateId();

    updateWorkflowState(sessionId1);
    updateWorkflowState(sessionId2);

    const workflowState = readJsonFile<WorkflowState>(getWorkflowStatePath());
    expect(workflowState?.currentSession).toBe(sessionId2);
  });
});

// ============================================================================
// 모드별 동작 테스트
// ============================================================================

describe("모드별 FlowState", () => {
  const testSessionId = "test-mode-" + generateId();

  afterEach(() => {
    const sessionDir = join(getSessionsDir(), testSessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("autopilot 모드", () => {
    const state = initFlowState(testSessionId, "autopilot", "테스트", "psm");
    expect(state.mode).toBe("autopilot");
  });

  test("assisted 모드", () => {
    const state = initFlowState(testSessionId, "assisted", "테스트", "psm");
    expect(state.mode).toBe("assisted");
  });

  test("manual 모드", () => {
    const state = initFlowState(testSessionId, "manual", "테스트", "psm");
    expect(state.mode).toBe("manual");
  });

  test("spec 모드 (단일 phase)", () => {
    const state = initFlowState(testSessionId, "spec", "테스트", "psm");
    expect(state.mode).toBe("spec");
  });

  test("impl 모드 (단일 phase)", () => {
    const state = initFlowState(testSessionId, "impl", "테스트", "psm");
    expect(state.mode).toBe("impl");
  });
});

// ============================================================================
// 구현 전략별 테스트
// ============================================================================

describe("구현 전략별 FlowState", () => {
  const testSessionId = "test-strategy-" + generateId();

  afterEach(() => {
    const sessionDir = join(getSessionsDir(), testSessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("psm 전략", () => {
    const state = initFlowState(testSessionId, "assisted", "테스트", "psm");
    expect(state.implStrategy).toBe("psm");
    expect(state.phases.impl.strategy).toBe("psm");
  });

  test("swarm 전략", () => {
    const state = initFlowState(testSessionId, "assisted", "테스트", "swarm");
    expect(state.implStrategy).toBe("swarm");
    expect(state.phases.impl.strategy).toBe("swarm");
  });

  test("sequential 전략", () => {
    const state = initFlowState(
      testSessionId,
      "assisted",
      "테스트",
      "sequential"
    );
    expect(state.implStrategy).toBe("sequential");
    expect(state.phases.impl.strategy).toBe("sequential");
  });
});

// ============================================================================
// Magic Keyword와 Flow 통합 테스트
// ============================================================================

describe("Magic Keyword와 Flow 통합", () => {
  const testSessionId = "test-keyword-flow-" + generateId();

  afterEach(() => {
    const sessionDir = join(getSessionsDir(), testSessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("autopilot: 키워드로 모드 결정", () => {
    const message = "autopilot: 새 기능 추가";
    const parsed = parseMagicKeyword(message);

    const state = initFlowState(
      testSessionId,
      parsed.mode || "assisted",
      parsed.cleanMessage,
      parsed.implStrategy || "psm"
    );

    expect(state.mode).toBe("autopilot");
    expect(state.requirement).toBe("새 기능 추가");
  });

  test("autopilot+swarm: 조합 키워드", () => {
    const message = "autopilot+swarm: 병렬 구현";
    const parsed = parseMagicKeyword(message);

    const state = initFlowState(
      testSessionId,
      parsed.mode || "assisted",
      parsed.cleanMessage,
      parsed.implStrategy || "psm"
    );

    expect(state.mode).toBe("autopilot");
    expect(state.implStrategy).toBe("swarm");
  });

  test("키워드 없는 경우 기본값 사용", () => {
    const message = "일반 요구사항";
    const parsed = parseMagicKeyword(message);

    const state = initFlowState(
      testSessionId,
      parsed.mode || "assisted",
      parsed.cleanMessage,
      parsed.implStrategy || "psm"
    );

    expect(state.mode).toBe("assisted");
    expect(state.implStrategy).toBe("psm");
    expect(state.requirement).toBe("일반 요구사항");
  });
});

// ============================================================================
// 복잡한 워크플로우 시나리오 테스트
// ============================================================================

describe("복잡한 워크플로우 시나리오", () => {
  const testSessionId = "test-complex-" + generateId();

  afterEach(() => {
    const sessionDir = join(getSessionsDir(), testSessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("전체 워크플로우 시뮬레이션", () => {
    // 1. 초기화
    initFlowState(testSessionId, "assisted", "기능 구현", "psm");

    // 2. Spec 시작
    updatePhaseState(testSessionId, "spec", {
      status: "in_progress",
      startedAt: timestamp(),
    });

    // 3. Spec 반복 (리뷰 피드백)
    updatePhaseState(testSessionId, "spec", { iterations: 1 });
    updatePhaseState(testSessionId, "spec", { iterations: 2 });

    // 4. Spec 완료
    updatePhaseState(testSessionId, "spec", {
      status: "complete",
      completedAt: timestamp(),
    });

    // 5. Impl로 전환
    transitionPhase(testSessionId, "impl");

    // 6. Escalation 발생
    addEscalation(testSessionId, "impl", "복잡한 로직 확인 필요");

    // 7. Impl 완료
    updatePhaseState(testSessionId, "impl", {
      status: "complete",
      completedAt: timestamp(),
    });

    // 8. Merge로 전환
    transitionPhase(testSessionId, "merge");

    // 9. Merge 완료
    updatePhaseState(testSessionId, "merge", {
      status: "complete",
      completedAt: timestamp(),
    });

    // 최종 상태 검증
    const finalState = readJsonFile<FlowState>(getFlowStatePath(testSessionId));

    expect(finalState?.currentPhase).toBe("merge");
    expect(finalState?.phases.spec.status).toBe("complete");
    expect(finalState?.phases.spec.iterations).toBe(2);
    expect(finalState?.phases.impl.status).toBe("complete");
    expect(finalState?.phases.merge.status).toBe("complete");
    expect(finalState?.escalations).toHaveLength(1);
  });

  test("실패 시나리오 - impl에서 에러", () => {
    initFlowState(testSessionId, "autopilot", "버그 수정", "psm");

    // Spec 완료
    updatePhaseState(testSessionId, "spec", { status: "complete" });
    transitionPhase(testSessionId, "impl");

    // Impl 시작 후 에러
    updatePhaseState(testSessionId, "impl", {
      status: "in_progress",
      startedAt: timestamp(),
    });

    updatePhaseState(testSessionId, "impl", {
      status: "error",
    });

    addEscalation(testSessionId, "impl", "빌드 실패 - 의존성 문제");

    const state = readJsonFile<FlowState>(getFlowStatePath(testSessionId));
    expect(state?.phases.impl.status).toBe("error");
    expect(state?.escalations).toHaveLength(1);
  });
});

// ============================================================================
// Edge Cases
// ============================================================================

describe("Edge Cases", () => {
  test("존재하지 않는 세션 업데이트 시도", () => {
    // 에러 없이 무시되어야 함
    expect(() => {
      updatePhaseState("nonexistent-session", "spec", { status: "complete" });
    }).not.toThrow();
  });

  test("빈 requirement", () => {
    const sessionId = "test-empty-req-" + generateId();
    const state = initFlowState(sessionId, "assisted", "", "psm");

    expect(state.requirement).toBe("");

    // 정리
    const sessionDir = join(getSessionsDir(), sessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("긴 requirement", () => {
    const sessionId = "test-long-req-" + generateId();
    const longReq = "A".repeat(10000);
    const state = initFlowState(sessionId, "assisted", longReq, "psm");

    expect(state.requirement.length).toBe(10000);

    // 정리
    const sessionDir = join(getSessionsDir(), sessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });

  test("특수문자 포함 requirement", () => {
    const sessionId = "test-special-req-" + generateId();
    const specialReq = 'function() { return "test\nwith\nnewlines"; }';
    const state = initFlowState(sessionId, "assisted", specialReq, "psm");

    expect(state.requirement).toBe(specialReq);

    // 정리
    const sessionDir = join(getSessionsDir(), sessionId);
    if (existsSync(sessionDir)) {
      rmSync(sessionDir, { recursive: true });
    }
  });
});
