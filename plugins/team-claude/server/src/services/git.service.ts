import { simpleGit, type SimpleGit, type DiffResult } from "simple-git";
import type { DiffSummary } from "../types";

const PROJECT_ROOT = process.env.PROJECT_ROOT || process.cwd();

/**
 * Git service for worktree management and diff operations
 */
export class GitService {
  private git: SimpleGit;

  constructor(basePath: string = PROJECT_ROOT) {
    this.git = simpleGit(basePath);
  }

  /**
   * Create a new worktree for a feature
   */
  async createWorktree(feature: string, baseBranch: string = "main"): Promise<{
    worktreePath: string;
    branch: string;
  }> {
    const worktreeName = feature.startsWith("feature-") ? feature : `feature-${feature}`;
    const worktreePath = `../worktrees/${worktreeName}`;
    const branchName = `feature/${feature}`;

    // Fetch latest from remote
    await this.git.fetch("origin", baseBranch);

    // Create worktree with new branch
    await this.git.raw([
      "worktree",
      "add",
      worktreePath,
      "-b",
      branchName,
      `origin/${baseBranch}`,
    ]);

    return {
      worktreePath: worktreePath,
      branch: branchName,
    };
  }

  /**
   * List all worktrees
   */
  async listWorktrees(): Promise<Array<{
    path: string;
    branch: string;
    commit: string;
  }>> {
    const result = await this.git.raw(["worktree", "list", "--porcelain"]);
    const worktrees: Array<{ path: string; branch: string; commit: string }> = [];

    const entries = result.split("\n\n").filter(Boolean);
    for (const entry of entries) {
      const lines = entry.split("\n");
      const pathLine = lines.find(l => l.startsWith("worktree "));
      const branchLine = lines.find(l => l.startsWith("branch "));
      const commitLine = lines.find(l => l.startsWith("HEAD "));

      if (pathLine) {
        worktrees.push({
          path: pathLine.replace("worktree ", ""),
          branch: branchLine?.replace("branch refs/heads/", "") || "detached",
          commit: commitLine?.replace("HEAD ", "").slice(0, 7) || "",
        });
      }
    }

    return worktrees;
  }

  /**
   * Remove a worktree
   */
  async removeWorktree(worktreePath: string, force: boolean = false): Promise<void> {
    const args = ["worktree", "remove"];
    if (force) args.push("--force");
    args.push(worktreePath);
    await this.git.raw(args);
  }

  /**
   * Get diff summary for a worktree
   */
  async getDiffSummary(worktreePath: string, baseBranch: string = "main"): Promise<DiffSummary> {
    const worktreeGit = simpleGit(worktreePath);

    // Get current branch
    const branch = await worktreeGit.revparse(["--abbrev-ref", "HEAD"]);

    // Get diff against base branch
    const diffResult: DiffResult = await worktreeGit.diffSummary([`origin/${baseBranch}...HEAD`]);

    const files = diffResult.files.map(f => ({
      path: f.file,
      status: this.getFileStatus(f),
      insertions: f.insertions || 0,
      deletions: f.deletions || 0,
    }));

    const worktreeName = worktreePath.split("/").pop() || worktreePath;

    return {
      worktree: worktreeName,
      branch: branch.trim(),
      filesChanged: diffResult.files.length,
      insertions: diffResult.insertions,
      deletions: diffResult.deletions,
      files,
      summary: this.generateSummary(diffResult, files),
    };
  }

  /**
   * Get uncommitted changes in a worktree
   */
  async getUncommittedChanges(worktreePath: string): Promise<{
    staged: string[];
    unstaged: string[];
    untracked: string[];
  }> {
    const worktreeGit = simpleGit(worktreePath);
    const status = await worktreeGit.status();

    return {
      staged: status.staged,
      unstaged: status.modified.filter(f => !status.staged.includes(f)),
      untracked: status.not_added,
    };
  }

  private getFileStatus(file: {
    file: string;
    changes?: number;
    insertions?: number;
    deletions?: number;
    binary?: boolean;
  }): "added" | "modified" | "deleted" | "renamed" {
    const insertions = file.insertions || 0;
    const deletions = file.deletions || 0;

    if (deletions === 0 && insertions > 0) return "added";
    if (insertions === 0 && deletions > 0) return "deleted";
    return "modified";
  }

  private generateSummary(
    diffResult: DiffResult,
    files: Array<{ path: string; status: string; insertions: number; deletions: number }>
  ): string {
    const added = files.filter(f => f.status === "added").length;
    const modified = files.filter(f => f.status === "modified").length;
    const deleted = files.filter(f => f.status === "deleted").length;

    const parts: string[] = [];
    if (added > 0) parts.push(`${added} added`);
    if (modified > 0) parts.push(`${modified} modified`);
    if (deleted > 0) parts.push(`${deleted} deleted`);

    const filesSummary = parts.length > 0 ? parts.join(", ") : "no changes";

    return `${diffResult.files.length} files (${filesSummary}), +${diffResult.insertions} -${diffResult.deletions} lines`;
  }
}

// Singleton instance
export const gitService = new GitService();
