/**
 * 공통 상수 및 유틸리티 함수
 * 쉘 스크립트의 lib/common.sh를 TypeScript로 포팅
 */

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { execSync } from "child_process";
import { createHash } from "crypto";
import { homedir } from "os";
import { join, dirname } from "path";

// ============================================================================
// 경로 상수
// ============================================================================

export const TC_DATA_ROOT = join(homedir(), ".team-claude");
export const TC_SERVER_DEFAULT_PORT = 7890;
export const TC_SERVER_BINARY = join(homedir(), ".claude", "team-claude-server");
export const TC_SERVER_PID_FILE = join(homedir(), ".claude", "team-claude-server.pid");
export const TC_SERVER_LOG_FILE = join(homedir(), ".claude", "team-claude-server.log");

// ============================================================================
// 프로젝트 식별
// ============================================================================

/**
 * Git 루트 디렉토리 찾기
 */
export function findGitRoot(): string {
  try {
    const root = execSync("git rev-parse --show-toplevel", {
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    }).trim();
    return root;
  } catch {
    throw new Error("Git 저장소가 아닙니다.");
  }
}

/**
 * 프로젝트 해시 생성 (git root 경로 기반)
 */
export function getProjectHash(): string {
  try {
    const root = findGitRoot();
    return createHash("md5").update(root).digest("hex").substring(0, 12);
  } catch {
    return "default";
  }
}

/**
 * 프로젝트 데이터 디렉토리 (~/.team-claude/{hash}/)
 */
export function getProjectDataDir(): string {
  return join(TC_DATA_ROOT, getProjectHash());
}

/**
 * 설정 파일 경로 (~/.team-claude/{hash}/team-claude.yaml)
 */
export function getConfigPath(): string {
  return join(getProjectDataDir(), "team-claude.yaml");
}

/**
 * 세션 디렉토리 경로
 */
export function getSessionsDir(): string {
  return join(getProjectDataDir(), "sessions");
}

/**
 * Worktrees 디렉토리 경로
 */
export function getWorktreesDir(): string {
  return join(getProjectDataDir(), "worktrees");
}

/**
 * State 디렉토리 경로
 */
export function getStateDir(): string {
  return join(getProjectDataDir(), "state");
}

// ============================================================================
// 파일 유틸리티
// ============================================================================

/**
 * 디렉토리 안전 생성
 */
export function ensureDir(dir: string): void {
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
}

/**
 * JSON 파일 읽기
 */
export function readJsonFile<T>(path: string): T | null {
  try {
    if (!existsSync(path)) {
      return null;
    }
    const content = readFileSync(path, "utf-8");
    return JSON.parse(content) as T;
  } catch {
    return null;
  }
}

/**
 * JSON 파일 쓰기
 */
export function writeJsonFile(path: string, data: unknown): void {
  ensureDir(dirname(path));
  writeFileSync(path, JSON.stringify(data, null, 2), "utf-8");
}

// ============================================================================
// ID 및 타임스탬프 생성
// ============================================================================

/**
 * 8자리 랜덤 ID 생성
 */
export function generateId(): string {
  const chars = "abcdefghijklmnopqrstuvwxyz0123456789";
  let result = "";
  for (let i = 0; i < 8; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
}

/**
 * ISO 8601 타임스탬프 생성
 */
export function timestamp(): string {
  return new Date().toISOString();
}

// ============================================================================
// 설정/세션 확인
// ============================================================================

/**
 * 설정 파일 존재 확인
 */
export function configExists(): boolean {
  return existsSync(getConfigPath());
}

/**
 * 세션 디렉토리 존재 확인
 */
export function sessionExists(sessionId: string): boolean {
  return existsSync(join(getSessionsDir(), sessionId));
}

// ============================================================================
// Magic Keywords
// ============================================================================

export const MAGIC_KEYWORDS: Record<string, string> = {
  autopilot: "autopilot",
  auto: "autopilot",
  ap: "autopilot",
  spec: "spec",
  sp: "spec",
  impl: "impl",
  im: "impl",
  review: "review",
  rv: "review",
  parallel: "parallel",
  pl: "parallel",
  ralph: "ralph",
  rl: "ralph",
  swarm: "swarm",
  sw: "swarm",
};

export const IMPL_STRATEGIES = ["psm", "swarm", "sequential"] as const;
export type ImplStrategy = (typeof IMPL_STRATEGIES)[number];

// ============================================================================
// Spec Refine 상태 타입
// ============================================================================

/** 리뷰 관점 정의 */
export interface Perspective {
  role: string; // "보안 전문가", "PM", "DBA" 등
  reason: string; // 이 관점을 선택한 이유
  focus: string[]; // 집중 영역 목록
  engine: "claude" | "codex" | "gemini";
  weight: number; // 0.0 ~ 1.0, 합계 = 1.0
}

/** 개별 리뷰 결과 */
export interface ReviewResult {
  perspective: string; // role 이름
  engine: "claude" | "codex" | "gemini";
  score: number; // 0-100
  issues: {
    critical: string[];
    important: string[];
    niceToHave: string[];
  };
  suggestions: string[]; // 구체적 개선 제안
  reviewFile: string; // 결과 파일 경로
}

/** 이슈의 합의 수준 */
export interface ConsensusIssue {
  summary: string; // 이슈 요약
  level: "consensus" | "majority" | "minority";
  agreedBy: string[]; // 동의한 관점 목록
  details: Record<string, string>; // 관점별 원문
  resolved: boolean; // 정제 후 해결 여부
  resolvedAt?: string; // 해결 시점 (iteration)
}

/** 단일 반복(iteration)의 상태 */
export interface RefineIteration {
  iteration: number;
  startedAt: string;
  completedAt: string | null;

  // PHASE A: 관점 결정
  perspectives: Perspective[];
  plannerReasoning: string; // Planner가 왜 이 관점을 선택했는지

  // PHASE B: 리뷰 결과
  reviews: ReviewResult[];

  // PHASE C: 합의 분석
  consensusIssues: ConsensusIssue[];
  weightedScore: number; // 가중 평균 점수

  // PHASE D: 판정
  verdict: "pass" | "warn" | "fail";

  // PHASE E: 정제 (fail인 경우)
  refinementActions: string[]; // 수행한 정제 작업 목록
}

/** spec-refine 전체 상태 */
export interface SpecRefineState {
  sessionId: string;
  status: "idle" | "running" | "passed" | "warned" | "failed" | "escalated";

  // 설정
  config: {
    maxIterations: number;
    passThreshold: number;
    warnThreshold: number;
    maxPerspectives: number;
  };

  // 반복 이력
  currentIteration: number;
  iterations: RefineIteration[];

  // 반복 간 전달 상태 (이것이 핵심)
  carry: {
    unresolvedIssues: ConsensusIssue[]; // 미해결 이슈 → 다음 Planner 입력
    resolvedIssues: ConsensusIssue[]; // 해결된 이슈 → 관점 제외 근거
    scoreHistory: number[]; // 점수 추이 → 개선 추세 판단
    perspectiveHistory: string[][]; // 관점 이력 → 중복 방지
  };

  // 타임스탬프
  startedAt: string;
  updatedAt: string;
  completedAt: string | null;
}

/**
 * Magic Keyword 파싱
 */
export function parseMagicKeyword(message: string): {
  keyword: string | null;
  mode: string | null;
  implStrategy: ImplStrategy | null;
  cleanMessage: string;
} {
  const match = message.match(/^([a-zA-Z+]+):\s*(.*)/);

  if (!match) {
    return {
      keyword: null,
      mode: null,
      implStrategy: null,
      cleanMessage: message,
    };
  }

  const rawKeyword = match[1].toLowerCase();
  const cleanMessage = match[2];

  // 조합 키워드 처리 (예: autopilot+swarm)
  if (rawKeyword.includes("+")) {
    const parts = rawKeyword.split("+");
    const mode = MAGIC_KEYWORDS[parts[0]] || null;
    const strategy = IMPL_STRATEGIES.includes(parts[1] as ImplStrategy)
      ? (parts[1] as ImplStrategy)
      : null;

    return {
      keyword: rawKeyword,
      mode,
      implStrategy: strategy,
      cleanMessage,
    };
  }

  // 단일 키워드
  const mapped = MAGIC_KEYWORDS[rawKeyword];

  if (IMPL_STRATEGIES.includes(rawKeyword as ImplStrategy)) {
    return {
      keyword: rawKeyword,
      mode: null,
      implStrategy: rawKeyword as ImplStrategy,
      cleanMessage,
    };
  }

  return {
    keyword: rawKeyword,
    mode: mapped || null,
    implStrategy: null,
    cleanMessage,
  };
}

// ============================================================================
// 시간 포맷
// ============================================================================

/**
 * 초를 사람이 읽기 쉬운 형식으로 변환
 */
export function formatDuration(seconds: number): string {
  if (seconds < 60) {
    return `${seconds}s`;
  } else if (seconds < 3600) {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}m${s}s`;
  } else {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    return `${h}h${m}m`;
  }
}

/**
 * 진행률 바 생성
 */
export function progressBar(
  percent: number,
  width = 10,
  filled = "█",
  empty = "░"
): string {
  const filledCount = Math.round((percent / 100) * width);
  const emptyCount = width - filledCount;
  return filled.repeat(filledCount) + empty.repeat(emptyCount);
}
