import type { WorkerState, CompletionReport, FeedbackRequest } from "../types";

/**
 * In-memory store for worker states
 * TODO: Consider persistent storage (file-based) for crash recovery
 */
class WorkerStore {
  private workers: Map<string, WorkerState> = new Map();

  /**
   * Register a new worker
   */
  register(worktree: string, feature: string, branch: string, pid?: number): WorkerState {
    const state: WorkerState = {
      worktree,
      feature,
      branch,
      status: "running",
      startedAt: new Date().toISOString(),
      feedbackHistory: [],
      pid,
    };
    this.workers.set(worktree, state);
    return state;
  }

  /**
   * Get worker state by worktree name
   */
  get(worktree: string): WorkerState | undefined {
    return this.workers.get(worktree);
  }

  /**
   * Get all workers
   */
  getAll(): WorkerState[] {
    return Array.from(this.workers.values());
  }

  /**
   * Get workers by status
   */
  getByStatus(status: WorkerState["status"]): WorkerState[] {
    return this.getAll().filter(w => w.status === status);
  }

  /**
   * Update worker with completion report
   */
  updateWithReport(worktree: string, report: CompletionReport): WorkerState | undefined {
    const worker = this.workers.get(worktree);
    if (!worker) return undefined;

    worker.lastReport = report;
    worker.status = report.status === "blocked"
      ? "blocked"
      : report.status === "success"
        ? "pending_review"
        : "running";

    this.workers.set(worktree, worker);
    return worker;
  }

  /**
   * Add feedback to worker
   */
  addFeedback(worktree: string, feedback: FeedbackRequest): WorkerState | undefined {
    const worker = this.workers.get(worktree);
    if (!worker) return undefined;

    worker.feedbackHistory.push(feedback);

    // Update status based on feedback action
    if (feedback.action === "complete") {
      worker.status = "completed";
    } else if (feedback.action === "revise" || feedback.action === "continue") {
      worker.status = "running";
    }

    this.workers.set(worktree, worker);
    return worker;
  }

  /**
   * Remove a worker
   */
  remove(worktree: string): boolean {
    return this.workers.delete(worktree);
  }

  /**
   * Clear all workers
   */
  clear(): void {
    this.workers.clear();
  }

  /**
   * Get summary statistics
   */
  getStats(): {
    total: number;
    running: number;
    completed: number;
    blocked: number;
    pendingReview: number;
  } {
    const all = this.getAll();
    return {
      total: all.length,
      running: all.filter(w => w.status === "running").length,
      completed: all.filter(w => w.status === "completed").length,
      blocked: all.filter(w => w.status === "blocked").length,
      pendingReview: all.filter(w => w.status === "pending_review").length,
    };
  }
}

// Singleton instance
export const workerStore = new WorkerStore();
