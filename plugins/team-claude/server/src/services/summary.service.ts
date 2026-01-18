import type { CompletionReport, WorkerState } from "../types";
import { gitService } from "./git.service";

/**
 * Service for generating summaries of worker changes
 */
export class SummaryService {
  /**
   * Generate a comprehensive summary of a worker's changes
   */
  async generateWorkerSummary(
    worktreePath: string,
    report: CompletionReport
  ): Promise<string> {
    const diffSummary = await gitService.getDiffSummary(worktreePath);
    const uncommitted = await gitService.getUncommittedChanges(worktreePath);

    const lines: string[] = [
      `## Worker Summary: ${report.worktree}`,
      "",
      `**Status**: ${this.formatStatus(report.status)}`,
      `**Timestamp**: ${report.timestamp}`,
      "",
    ];

    // Add work summary
    if (report.summary) {
      lines.push("### Work Summary", "", report.summary, "");
    }

    // Add git diff summary
    lines.push(
      "### Changes",
      "",
      `- **Branch**: ${diffSummary.branch}`,
      `- **Files Changed**: ${diffSummary.filesChanged}`,
      `- **Lines**: +${diffSummary.insertions} / -${diffSummary.deletions}`,
      ""
    );

    // List changed files
    if (diffSummary.files.length > 0) {
      lines.push("#### Modified Files", "");
      for (const file of diffSummary.files.slice(0, 20)) {
        const icon = this.getStatusIcon(file.status);
        lines.push(`- ${icon} \`${file.path}\` (+${file.insertions} -${file.deletions})`);
      }
      if (diffSummary.files.length > 20) {
        lines.push(`- ... and ${diffSummary.files.length - 20} more files`);
      }
      lines.push("");
    }

    // Add uncommitted changes warning
    const totalUncommitted = uncommitted.staged.length + uncommitted.unstaged.length + uncommitted.untracked.length;
    if (totalUncommitted > 0) {
      lines.push(
        "### Uncommitted Changes",
        "",
        `- Staged: ${uncommitted.staged.length}`,
        `- Unstaged: ${uncommitted.unstaged.length}`,
        `- Untracked: ${uncommitted.untracked.length}`,
        ""
      );
    }

    // Add test results
    if (report.testsRun !== undefined) {
      lines.push(
        "### Tests",
        "",
        `- Tests Run: ${report.testsRun ? "Yes" : "No"}`,
        report.testsRun ? `- Tests Passed: ${report.testsPassed ? "Yes" : "No"}` : "",
        ""
      );
    }

    // Add blockers
    if (report.blockers && report.blockers.length > 0) {
      lines.push("### Blockers", "");
      for (const blocker of report.blockers) {
        lines.push(`- ${blocker}`);
      }
      lines.push("");
    }

    return lines.filter(l => l !== undefined).join("\n");
  }

  /**
   * Generate a batch summary for multiple workers
   */
  generateBatchSummary(workers: WorkerState[]): string {
    const lines: string[] = [
      "# Team Claude - Worker Status Report",
      "",
      `**Generated**: ${new Date().toISOString()}`,
      `**Total Workers**: ${workers.length}`,
      "",
    ];

    // Group by status
    const byStatus = {
      running: workers.filter(w => w.status === "running"),
      completed: workers.filter(w => w.status === "completed"),
      blocked: workers.filter(w => w.status === "blocked"),
      pending_review: workers.filter(w => w.status === "pending_review"),
    };

    lines.push(
      "## Status Summary",
      "",
      `- Running: ${byStatus.running.length}`,
      `- Pending Review: ${byStatus.pending_review.length}`,
      `- Blocked: ${byStatus.blocked.length}`,
      `- Completed: ${byStatus.completed.length}`,
      ""
    );

    // Detail each worker
    for (const worker of workers) {
      lines.push(
        `### ${worker.feature}`,
        "",
        `- **Worktree**: ${worker.worktree}`,
        `- **Branch**: ${worker.branch}`,
        `- **Status**: ${this.formatStatus(worker.status)}`,
        `- **Started**: ${worker.startedAt}`,
      );

      if (worker.lastReport) {
        lines.push(`- **Last Report**: ${worker.lastReport.timestamp}`);
        lines.push(`- **Summary**: ${worker.lastReport.summary.slice(0, 100)}...`);
      }

      lines.push("");
    }

    return lines.join("\n");
  }

  private formatStatus(status: string): string {
    const statusMap: Record<string, string> = {
      success: "Completed Successfully",
      partial: "Partially Complete",
      blocked: "Blocked",
      running: "Running",
      completed: "Completed",
      pending_review: "Pending Review",
    };
    return statusMap[status] || status;
  }

  private getStatusIcon(status: string): string {
    const iconMap: Record<string, string> = {
      added: "+",
      modified: "~",
      deleted: "-",
      renamed: ">",
    };
    return iconMap[status] || "?";
  }
}

// Singleton instance
export const summaryService = new SummaryService();
