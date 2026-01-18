import { Hono } from "hono";
import type { ApiResponse, WorkerState } from "../types";
import { workerStore } from "../store/workers";
import { gitService } from "../services/git.service";

const workersRouter = new Hono();

/**
 * GET /workers
 * Get list of all active workers
 */
workersRouter.get("/", async (c) => {
  const workers = workerStore.getAll();

  const response: ApiResponse<WorkerState[]> = {
    success: true,
    data: workers,
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * POST /workers
 * Register a new worker
 */
workersRouter.post("/", async (c) => {
  try {
    const { worktree, feature, branch, pid } = await c.req.json();

    if (!worktree || !feature || !branch) {
      const response: ApiResponse = {
        success: false,
        error: "Missing required fields: worktree, feature, branch",
        timestamp: new Date().toISOString(),
      };
      return c.json(response, 400);
    }

    // Check if worker already exists
    if (workerStore.get(worktree)) {
      const response: ApiResponse = {
        success: false,
        error: `Worker already exists: ${worktree}`,
        timestamp: new Date().toISOString(),
      };
      return c.json(response, 409);
    }

    const worker = workerStore.register(worktree, feature, branch, pid);

    console.log(`[workers] Registered new worker: ${worktree}`);

    const response: ApiResponse<WorkerState> = {
      success: true,
      data: worker,
      timestamp: new Date().toISOString(),
    };

    return c.json(response, 201);
  } catch (error) {
    console.error("[workers] Error registering worker:", error);

    const response: ApiResponse = {
      success: false,
      error: error instanceof Error ? error.message : "Unknown error",
      timestamp: new Date().toISOString(),
    };

    return c.json(response, 500);
  }
});

/**
 * DELETE /workers/:worktree
 * Remove a worker
 */
workersRouter.delete("/:worktree", async (c) => {
  const worktree = c.req.param("worktree");

  const removed = workerStore.remove(worktree);

  if (!removed) {
    const response: ApiResponse = {
      success: false,
      error: `Worker not found: ${worktree}`,
      timestamp: new Date().toISOString(),
    };
    return c.json(response, 404);
  }

  console.log(`[workers] Removed worker: ${worktree}`);

  const response: ApiResponse<{ removed: string }> = {
    success: true,
    data: { removed: worktree },
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * GET /workers/worktrees
 * Get git worktrees (from git, not just registered workers)
 */
workersRouter.get("/worktrees", async (c) => {
  try {
    const worktrees = await gitService.listWorktrees();

    const response: ApiResponse<typeof worktrees> = {
      success: true,
      data: worktrees,
      timestamp: new Date().toISOString(),
    };

    return c.json(response, 200);
  } catch (error) {
    console.error("[workers] Error listing worktrees:", error);

    const response: ApiResponse = {
      success: false,
      error: error instanceof Error ? error.message : "Unknown error",
      timestamp: new Date().toISOString(),
    };

    return c.json(response, 500);
  }
});

export { workersRouter };
