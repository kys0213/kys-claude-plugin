import { Hono } from "hono";
import type { CompletionReport, ApiResponse } from "../types";
import { workerStore } from "../store/workers";
import { notifyService } from "../services/notify.service";

const completeRouter = new Hono();

/**
 * POST /complete
 * Receive completion report from Worker Claude via Stop hook
 */
completeRouter.post("/", async (c) => {
  try {
    const report: CompletionReport = await c.req.json();

    console.log(`[complete] Received completion report from: ${report.worktree}`);
    console.log(`[complete] Status: ${report.status}`);
    console.log(`[complete] Files changed: ${report.filesChanged.length}`);

    // Validate required fields
    if (!report.worktree || !report.sessionId || !report.status) {
      const response: ApiResponse = {
        success: false,
        error: "Missing required fields: worktree, sessionId, status",
        timestamp: new Date().toISOString(),
      };
      return c.json(response, 400);
    }

    // Update worker store
    let worker = workerStore.get(report.worktree);

    if (!worker) {
      // Worker not registered yet, register it now
      console.log(`[complete] Auto-registering worker: ${report.worktree}`);
      worker = workerStore.register(
        report.worktree,
        report.worktree.replace(/^feature-/, ""),
        `feature/${report.worktree.replace(/^feature-/, "")}`
      );
    }

    worker = workerStore.updateWithReport(report.worktree, report);

    if (!worker) {
      const response: ApiResponse = {
        success: false,
        error: `Worker not found: ${report.worktree}`,
        timestamp: new Date().toISOString(),
      };
      return c.json(response, 404);
    }

    // Determine worktree path
    const PROJECT_ROOT = process.env.PROJECT_ROOT || process.cwd();
    const worktreePath = `${PROJECT_ROOT}/../worktrees/${report.worktree}`;

    // Generate notification for Main Claude
    let notificationPath: string;

    if (report.status === "blocked" && report.blockers) {
      notificationPath = await notifyService.notifyBlocked(worker, report.blockers);
    } else {
      notificationPath = await notifyService.notifyCompletion(worker, report, worktreePath);
    }

    const response: ApiResponse<{
      worker: typeof worker;
      notificationPath: string;
    }> = {
      success: true,
      data: {
        worker,
        notificationPath,
      },
      timestamp: new Date().toISOString(),
    };

    console.log(`[complete] Successfully processed report for: ${report.worktree}`);

    return c.json(response, 200);
  } catch (error) {
    console.error("[complete] Error processing completion report:", error);

    const response: ApiResponse = {
      success: false,
      error: error instanceof Error ? error.message : "Unknown error",
      timestamp: new Date().toISOString(),
    };

    return c.json(response, 500);
  }
});

export { completeRouter };
