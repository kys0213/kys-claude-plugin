/**
 * ProjectContext - 프로젝트 컨텍스트 관리 (캐싱 포함)
 */

import { createHash } from "crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { join, basename } from "path";
import { homedir } from "os";
import { $ } from "bun";

// 전역 캐시 (프로세스 수명 동안 유지)
const cache = new Map<string, string>();

export class ProjectContext {
  private static instance: ProjectContext | null = null;
  private static initPromise: Promise<ProjectContext> | null = null;

  readonly gitRoot: string;
  readonly projectHash: string;
  readonly dataDir: string;
  readonly projectName: string;

  private constructor(gitRoot: string) {
    this.gitRoot = gitRoot;
    this.projectHash = this.computeHash(gitRoot);
    this.dataDir = join(
      homedir(),
      ".team-claude",
      this.projectHash
    );
    this.projectName = basename(gitRoot);
  }

  /**
   * 싱글톤 인스턴스 가져오기 (동시성 안전)
   */
  static async getInstance(): Promise<ProjectContext> {
    // 이미 인스턴스가 있으면 반환
    if (this.instance) {
      return this.instance;
    }

    // 초기화 중이면 기다림 (동시성 처리)
    if (this.initPromise) {
      return this.initPromise;
    }

    // 초기화 시작
    this.initPromise = this.createInstance();

    try {
      this.instance = await this.initPromise;
      return this.instance;
    } finally {
      this.initPromise = null;
    }
  }

  /**
   * 인스턴스 생성 (내부용)
   */
  private static async createInstance(): Promise<ProjectContext> {
    const gitRoot = await this.findGitRoot();
    return new ProjectContext(gitRoot);
  }

  /**
   * 캐시 무효화 (테스트용)
   */
  static resetInstance(): void {
    this.instance = null;
    this.initPromise = null;
    cache.clear();
  }

  /**
   * Git 루트 디렉토리 찾기
   */
  private static async findGitRoot(): Promise<string> {
    const cached = cache.get("gitRoot");
    if (cached) return cached;

    try {
      const result = await $`git rev-parse --show-toplevel`.text();
      const root = result.trim();
      cache.set("gitRoot", root);
      return root;
    } catch {
      throw new Error("Git 저장소가 아닙니다.");
    }
  }

  /**
   * 프로젝트 해시 계산 (md5 앞 12자리)
   */
  private computeHash(path: string): string {
    const cached = cache.get(`hash:${path}`);
    if (cached) return cached;

    const hash = createHash("md5").update(path).digest("hex").slice(0, 12);
    cache.set(`hash:${path}`, hash);
    return hash;
  }

  // ============================================================================
  // 경로 헬퍼
  // ============================================================================

  get configPath(): string {
    return join(this.dataDir, "team-claude.yaml");
  }

  get sessionsDir(): string {
    return join(this.dataDir, "sessions");
  }

  get stateDir(): string {
    return join(this.dataDir, "state");
  }

  get worktreesDir(): string {
    return join(this.dataDir, "worktrees");
  }

  get claudeDir(): string {
    return join(this.gitRoot, ".claude");
  }

  get agentsDir(): string {
    return join(this.claudeDir, "agents");
  }

  get hooksDir(): string {
    return join(this.claudeDir, "hooks");
  }

  // ============================================================================
  // 상태 확인
  // ============================================================================

  isInitialized(): boolean {
    return existsSync(this.configPath);
  }

  configExists(): boolean {
    return existsSync(this.configPath);
  }

  sessionExists(sessionId: string): boolean {
    return existsSync(join(this.sessionsDir, sessionId));
  }

  // ============================================================================
  // 디렉토리 생성
  // ============================================================================

  ensureDataDirs(): void {
    const dirs = [this.dataDir, this.sessionsDir, this.stateDir, this.worktreesDir];
    for (const dir of dirs) {
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }
    }
  }

  ensureClaudeDirs(): void {
    const dirs = [this.claudeDir, this.agentsDir, this.hooksDir];
    for (const dir of dirs) {
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }
    }
  }

  // ============================================================================
  // 정보 출력
  // ============================================================================

  getInfo(): Record<string, string> {
    return {
      projectName: this.projectName,
      gitRoot: this.gitRoot,
      projectHash: this.projectHash,
      dataDir: this.dataDir,
      configPath: this.configPath,
      sessionsDir: this.sessionsDir,
      worktreesDir: this.worktreesDir,
      claudeDir: this.claudeDir,
    };
  }
}

// ============================================================================
// 유틸리티 함수
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
