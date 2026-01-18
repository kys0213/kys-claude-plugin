/**
 * Worker completion report sent from Worker Claude via Stop hook
 */
export interface CompletionReport {
  worktree: string;          // e.g., "feature-auth"
  sessionId: string;         // Claude Code session ID
  status: "success" | "partial" | "blocked";
  summary: string;           // Work summary
  filesChanged: string[];    // List of changed files
  testsRun?: boolean;
  testsPassed?: boolean;
  blockers?: string[];       // Blocking issues (questions, etc.)
  timestamp: string;
}

/**
 * Feedback from Main Claude to Worker
 */
export interface FeedbackRequest {
  worktree: string;
  feedback: string;
  action: "continue" | "revise" | "complete";
  priority?: "low" | "normal" | "high";
}

/**
 * Worker state stored in memory
 */
export interface WorkerState {
  worktree: string;
  feature: string;
  branch: string;
  status: "running" | "completed" | "blocked" | "pending_review";
  startedAt: string;
  lastReport?: CompletionReport;
  feedbackHistory: FeedbackRequest[];
  pid?: number;
}

/**
 * Git diff summary for a worktree
 */
export interface DiffSummary {
  worktree: string;
  branch: string;
  filesChanged: number;
  insertions: number;
  deletions: number;
  files: Array<{
    path: string;
    status: "added" | "modified" | "deleted" | "renamed";
    insertions: number;
    deletions: number;
  }>;
  summary: string;  // Human-readable summary
}

/**
 * API Response wrapper
 */
export interface ApiResponse<T = unknown> {
  success: boolean;
  data?: T;
  error?: string;
  timestamp: string;
}
