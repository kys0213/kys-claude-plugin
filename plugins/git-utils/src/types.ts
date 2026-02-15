// ============================================================
// git-utils CLI — Type Definitions
// ============================================================
// 기존 9개 bash/js 스크립트에 대응하는 타입 시스템.
// 모든 command와 core module은 이 타입을 기반으로 구현됩니다.
// ============================================================

// ----------------------------------------------------------
// Result — 모든 command의 통일된 반환 타입
// ----------------------------------------------------------

export type Result<T> =
  | { ok: true; data: T }
  | { ok: false; error: string };

// ----------------------------------------------------------
// Command — 모든 command가 준수하는 인터페이스
// ----------------------------------------------------------

export interface Command<TInput, TOutput> {
  readonly name: string;
  readonly description: string;
  run(input: TInput): Promise<Result<TOutput>>;
}

// ----------------------------------------------------------
// Commit (← commit.sh)
// ----------------------------------------------------------

export const COMMIT_TYPES = [
  'feat', 'fix', 'docs', 'style', 'refactor', 'test', 'chore', 'perf',
] as const;
export type CommitType = (typeof COMMIT_TYPES)[number];

export interface CommitInput {
  type: CommitType;
  description: string;
  scope?: string;
  body?: string;
  skipAdd?: boolean;
}

export interface CommitOutput {
  subject: string;
  jiraTicket?: string;
}

// ----------------------------------------------------------
// Branch (← create-branch.sh)
// ----------------------------------------------------------

export interface BranchInput {
  branchName: string;
  baseBranch?: string;
}

export interface BranchOutput {
  branchName: string;
  baseBranch: string;
}

// ----------------------------------------------------------
// PR (← create-pr.sh)
// ----------------------------------------------------------

export interface PrInput {
  title: string;
  description?: string;
}

export interface PrOutput {
  url: string;
  title: string;
  baseBranch: string;
  jiraTicket?: string;
}

// ----------------------------------------------------------
// Reviews (← unresolved-reviews.sh)
// ----------------------------------------------------------

export interface ReviewsInput {
  prNumber?: number;
}

export interface ReviewComment {
  author: string;
  body: string;
  createdAt: string;
  url: string;
}

export interface ReviewThread {
  isResolved: boolean;
  isOutdated: boolean;
  path: string;
  line: number;
  comments: ReviewComment[];
}

export interface ReviewsOutput {
  prTitle: string;
  prUrl: string;
  threads: ReviewThread[];
}

// ----------------------------------------------------------
// Guard (← default-branch-guard-hook.sh, guard-commit-hook.sh)
// ----------------------------------------------------------

export type GuardTarget = 'write' | 'commit';

export interface GuardInput {
  target: GuardTarget;
  projectDir: string;
  createBranchScript: string;
  defaultBranch?: string;
  /** commit guard 전용: stdin으로 전달된 tool_input.command */
  toolCommand?: string;
}

export interface GuardOutput {
  allowed: boolean;
  reason?: string;
  currentBranch?: string;
  defaultBranch?: string;
}

// ----------------------------------------------------------
// Hook (← register-hook.js)
// ----------------------------------------------------------

export interface HookEntry {
  type: 'command';
  command: string;
  timeout?: number;
}

export interface HookMatcher {
  matcher: string;
  hooks: HookEntry[];
}

export interface HookRegisterInput {
  hookType: string;
  matcher: string;
  command: string;
  timeout?: number;
  projectDir?: string;
}

export interface HookUnregisterInput {
  hookType: string;
  command: string;
  projectDir?: string;
}

export interface HookListInput {
  hookType?: string;
  projectDir?: string;
}

export interface HookRegisterOutput {
  action: 'created' | 'updated';
  command: string;
}

export interface HookUnregisterOutput {
  command: string;
}

// ----------------------------------------------------------
// Jira (← detect-jira-ticket.sh)
// ----------------------------------------------------------

export interface JiraTicket {
  /** 원본 매칭값 */
  raw: string;
  /** 정규화 (대문자): WAD-0212 */
  normalized: string;
}

// ----------------------------------------------------------
// Git State — core 모듈에서 공유
// ----------------------------------------------------------

export interface GitSpecialState {
  rebase: boolean;
  merge: boolean;
  detached: boolean;
}
