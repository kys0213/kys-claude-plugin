import { Hono } from "hono";
import type { ApiResponse, WorkerState } from "../types";
import { workerStore } from "../store/workers";
import { summaryService } from "../services/summary.service";

const statusRouter = new Hono();

/**
 * GET /status
 * Get all workers status summary
 */
statusRouter.get("/", async (c) => {
  const workers = workerStore.getAll();
  const stats = workerStore.getStats();

  const response: ApiResponse<{
    stats: typeof stats;
    workers: WorkerState[];
  }> = {
    success: true,
    data: {
      stats,
      workers,
    },
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * GET /status/:worktree
 * Get specific worker status
 */
statusRouter.get("/:worktree", async (c) => {
  const worktree = c.req.param("worktree");
  const worker = workerStore.get(worktree);

  if (!worker) {
    const response: ApiResponse = {
      success: false,
      error: `Worker not found: ${worktree}`,
      timestamp: new Date().toISOString(),
    };
    return c.json(response, 404);
  }

  const response: ApiResponse<WorkerState> = {
    success: true,
    data: worker,
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * GET /status/summary
 * Get formatted summary for Main Claude
 */
statusRouter.get("/report/summary", async (c) => {
  const workers = workerStore.getAll();
  const summary = summaryService.generateBatchSummary(workers);

  const response: ApiResponse<{ summary: string }> = {
    success: true,
    data: { summary },
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

export { statusRouter };
