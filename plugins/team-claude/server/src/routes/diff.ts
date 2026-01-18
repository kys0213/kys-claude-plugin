import { Hono } from "hono";
import { join } from "path";
import type { ApiResponse, DiffSummary } from "../types";
import { gitService } from "../services/git.service";

const diffRouter = new Hono();

const PROJECT_ROOT = process.env.PROJECT_ROOT || process.cwd();

/**
 * GET /diff/:worktree
 * Get git diff summary for a specific worktree
 */
diffRouter.get("/:worktree", async (c) => {
  const worktree = c.req.param("worktree");
  const baseBranch = c.req.query("base") || "main";

  try {
    const worktreePath = join(PROJECT_ROOT, "..", "worktrees", worktree);

    const diffSummary = await gitService.getDiffSummary(worktreePath, baseBranch);

    const response: ApiResponse<DiffSummary> = {
      success: true,
      data: diffSummary,
      timestamp: new Date().toISOString(),
    };

    return c.json(response, 200);
  } catch (error) {
    console.error(`[diff] Error getting diff for ${worktree}:`, error);

    const response: ApiResponse = {
      success: false,
      error: error instanceof Error ? error.message : "Unknown error",
      timestamp: new Date().toISOString(),
    };

    return c.json(response, 500);
  }
});

/**
 * GET /diff/:worktree/uncommitted
 * Get uncommitted changes in a worktree
 */
diffRouter.get("/:worktree/uncommitted", async (c) => {
  const worktree = c.req.param("worktree");

  try {
    const worktreePath = join(PROJECT_ROOT, "..", "worktrees", worktree);

    const uncommitted = await gitService.getUncommittedChanges(worktreePath);

    const response: ApiResponse<typeof uncommitted> = {
      success: true,
      data: uncommitted,
      timestamp: new Date().toISOString(),
    };

    return c.json(response, 200);
  } catch (error) {
    console.error(`[diff] Error getting uncommitted changes for ${worktree}:`, error);

    const response: ApiResponse = {
      success: false,
      error: error instanceof Error ? error.message : "Unknown error",
      timestamp: new Date().toISOString(),
    };

    return c.json(response, 500);
  }
});

export { diffRouter };
