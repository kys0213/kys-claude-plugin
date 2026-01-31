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
