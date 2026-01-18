import { mkdir, writeFile, readFile } from "fs/promises";
import { join } from "path";
import type { WorkerState, CompletionReport } from "../types";
import { summaryService } from "./summary.service";

const PROJECT_ROOT = process.env.PROJECT_ROOT || process.cwd();
const NOTIFICATION_DIR = join(PROJECT_ROOT, ".team-claude", "notifications");

/**
 * Service for notifying Main Claude about worker events
 */
export class NotifyService {
  private initialized = false;

  /**
   * Initialize notification directory
   */
  async init(): Promise<void> {
    if (this.initialized) return;
    await mkdir(NOTIFICATION_DIR, { recursive: true });
    this.initialized = true;
  }

  /**
   * Notify about worker completion
   */
  async notifyCompletion(
    worker: WorkerState,
    report: CompletionReport,
    worktreePath: string
  ): Promise<string> {
    await this.init();

    const summary = await summaryService.generateWorkerSummary(worktreePath, report);
    const filename = `completion-${report.worktree}-${Date.now()}.md`;
    const filepath = join(NOTIFICATION_DIR, filename);

    await writeFile(filepath, summary, "utf-8");

    // Also update the latest notification file for easy access
    const latestPath = join(NOTIFICATION_DIR, "latest.md");
    await writeFile(latestPath, summary, "utf-8");

    console.log(`[notify] Worker completion notification written to ${filepath}`);

    return filepath;
  }

  /**
   * Notify about blocked worker
   */
  async notifyBlocked(worker: WorkerState, blockers: string[]): Promise<string> {
    await this.init();

    const content = [
      `# Worker Blocked: ${worker.feature}`,
      "",
      `**Worktree**: ${worker.worktree}`,
      `**Branch**: ${worker.branch}`,
      `**Time**: ${new Date().toISOString()}`,
      "",
      "## Blockers",
      "",
      ...blockers.map(b => `- ${b}`),
      "",
      "## Recommended Actions",
      "",
      "1. Review the blockers above",
      "2. Provide clarification or updated specs",
      "3. Use `/team-claude:feedback` to send instructions to the worker",
      "",
    ].join("\n");

    const filename = `blocked-${worker.worktree}-${Date.now()}.md`;
    const filepath = join(NOTIFICATION_DIR, filename);

    await writeFile(filepath, content, "utf-8");

    console.log(`[notify] Worker blocked notification written to ${filepath}`);

    return filepath;
  }

  /**
   * Get all pending notifications
   */
  async getPendingNotifications(): Promise<Array<{
    filename: string;
    content: string;
    timestamp: number;
  }>> {
    await this.init();

    const { readdir } = await import("fs/promises");
    const files = await readdir(NOTIFICATION_DIR);

    const notifications: Array<{
      filename: string;
      content: string;
      timestamp: number;
    }> = [];

    for (const file of files) {
      if (file === "latest.md") continue;

      const filepath = join(NOTIFICATION_DIR, file);
      const content = await readFile(filepath, "utf-8");

      // Extract timestamp from filename
      const match = file.match(/-(\d+)\.md$/);
      const timestamp = match ? parseInt(match[1], 10) : 0;

      notifications.push({
        filename: file,
        content,
        timestamp,
      });
    }

    return notifications.sort((a, b) => b.timestamp - a.timestamp);
  }

  /**
   * Clear processed notifications
   */
  async clearNotification(filename: string): Promise<void> {
    const { unlink } = await import("fs/promises");
    const filepath = join(NOTIFICATION_DIR, filename);
    await unlink(filepath);
    console.log(`[notify] Cleared notification: ${filename}`);
  }
}

// Singleton instance
export const notifyService = new NotifyService();
