/**
 * Team Claude Configuration Schema
 * 설정 계층: Global (~/.team-claude) → Project (.team-claude) → Session (runtime)
 */

export interface TeamClaudeConfig {
  version: "1.0";

  server: ServerConfig;
  worktree: WorktreeConfig;
  worker: WorkerConfig;
  notification: NotificationConfig;
  review: ReviewConfig;
  templates: Record<string, WorkerTemplate>;
}

export interface ServerConfig {
  /** 서버 포트 (default: 3847) */
  port: number;
  /** 서버 호스트 (default: "localhost") */
  host: string;
  /** 요청 타임아웃 ms (default: 60000) */
  timeout: number;
}

export interface WorktreeConfig {
  /** Worktree 루트 경로 (default: "../worktrees") */
  root: string;
  /** 브랜치 접두사 (default: "feature/") */
  branchPrefix: string;
  /** 완료 시 자동 정리 (default: false) */
  cleanupOnComplete: boolean;
}

export interface WorkerConfig {
  /** 동시 실행 최대 수 (default: 5) */
  maxConcurrent: number;
  /** 기본 템플릿 이름 (default: "standard") */
  defaultTemplate: string;
  /** Worker 타임아웃 초 (default: 1800 = 30분) */
  timeout: number;
  /** 실패 시 자동 재시도 (default: false) */
  autoRetry: boolean;
  /** 재시도 최대 횟수 (default: 2) */
  retryLimit: number;
}

export interface NotificationConfig {
  /** 알림 방식 */
  method: "file" | "notification" | "slack" | "webhook";
  /** Slack 설정 (method가 slack일 때) */
  slack?: {
    webhookUrl: string;
    channel?: string;
  };
  /** Webhook 설정 (method가 webhook일 때) */
  webhook?: {
    url: string;
    headers?: Record<string, string>;
  };
}

export interface ReviewConfig {
  /** 리뷰 자동화 레벨 */
  autoLevel: "manual" | "semi-auto" | "full-auto";
  /** 승인 필수 여부 (default: true) */
  requireApproval: boolean;
  /** 리뷰 규칙 목록 */
  rules: ReviewRule[];
}

export interface ReviewRule {
  /** 규칙 이름 */
  name: string;
  /** 규칙 설명 */
  description: string;
  /** 검사 타입 */
  type: "lint" | "pattern" | "ai";
  /** 타입별 설정 */
  config: LintRuleConfig | PatternRuleConfig | AIRuleConfig;
  /** 심각도 */
  severity: "error" | "warning" | "info";
  /** 활성화 여부 */
  enabled: boolean;
}

export interface LintRuleConfig {
  /** ESLint 등 린터 규칙 이름 */
  rule: string;
  /** 린터 종류 */
  linter?: "eslint" | "tsc" | "custom";
}

export interface PatternRuleConfig {
  /** 정규식 패턴 */
  pattern: string;
  /** 매칭 시 액션 (deny: 금지, require: 필수) */
  action: "deny" | "require";
  /** 검사 대상 파일 glob */
  files?: string;
}

export interface AIRuleConfig {
  /** AI에게 전달할 검사 지침 */
  prompt: string;
}

export interface WorkerTemplate {
  /** 템플릿 이름 */
  name: string;
  /** 템플릿 설명 */
  description: string;
  /** 상속할 베이스 템플릿 */
  baseTemplate?: string;
  /** CLAUDE.md 내용 */
  claudeMd: string;
  /** 추가 hooks 설정 */
  hooks?: Record<string, unknown>;
  /** 적용할 규칙 이름들 */
  rules?: string[];
}

/**
 * 설정 키 경로 (점 표기법)
 * 예: "server.port", "worker.maxConcurrent"
 */
export type ConfigPath = string;

/**
 * 설정 변경 이벤트
 */
export interface ConfigChangeEvent {
  path: ConfigPath;
  oldValue: unknown;
  newValue: unknown;
  scope: "global" | "project" | "session";
}
