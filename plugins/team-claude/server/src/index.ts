/**
 * Team Claude Server
 *
 * Team Claude 로컬 HTTP 서버.
 * Main Claude와 Worker Claude 간의 통신을 중개하고 피드백 루프를 관리.
 *
 * 실행: bun run src/index.ts
 * 또는: ./team-claude-server (빌드 후)
 */

import { Hono } from "hono";
import { cors } from "hono/cors";
import { logger } from "hono/logger";
import { TaskManager, type TaskConfig, type Task } from "./services/task-manager";
import { executorRegistry } from "./executors";

// 설정
const PORT = parseInt(process.env.TEAM_CLAUDE_PORT || "7890", 10);
const DATA_DIR = process.env.TEAM_CLAUDE_DATA_DIR || ".team-claude/server-data";
const EXECUTOR = process.env.TEAM_CLAUDE_EXECUTOR; // 미지정 시 자동 선택

// 서버 초기화
const app = new Hono();
const taskManager = new TaskManager({
  executor: EXECUTOR,
  dataDir: DATA_DIR,
});

// 미들웨어
app.use("*", cors());
app.use("*", logger());

// ============================================================================
// Health & Info
// ============================================================================

app.get("/health", (c) => {
  return c.json({
    status: "ok",
    timestamp: new Date().toISOString(),
  });
});

app.get("/info", async (c) => {
  const available = await executorRegistry.getAvailable();
  return c.json({
    version: "0.1.0",
    executors: {
      available: available.map((e) => e.name),
      current: EXECUTOR || "auto",
    },
    config: {
      port: PORT,
      dataDir: DATA_DIR,
    },
  });
});

// ============================================================================
// Tasks API
// ============================================================================

/**
 * POST /tasks - 새 태스크 생성
 */
app.post("/tasks", async (c) => {
  try {
    const body = await c.req.json<{
      checkpoint_id: string;
      checkpoint_name: string;
      worktree_path: string;
      validation_command: string;
      max_retries?: number;
      claude_command?: string;
    }>();

    // 필수 필드 검증
    if (!body.checkpoint_id || !body.worktree_path || !body.validation_command) {
      return c.json(
        { error: "Missing required fields: checkpoint_id, worktree_path, validation_command" },
        400
      );
    }

    const taskConfig: TaskConfig = {
      checkpointId: body.checkpoint_id,
      checkpointName: body.checkpoint_name || body.checkpoint_id,
      worktreePath: body.worktree_path,
      validationCommand: body.validation_command,
      maxRetries: body.max_retries || 3,
      claudeCommand: body.claude_command,
    };

    const task = await taskManager.createTask(taskConfig);

    return c.json(
      {
        task_id: task.id,
        status: task.status,
        message: "Task created and queued",
      },
      202
    );
  } catch (error) {
    console.error("Failed to create task:", error);
    return c.json(
      { error: error instanceof Error ? error.message : "Unknown error" },
      500
    );
  }
});

/**
 * GET /tasks - 모든 태스크 조회
 */
app.get("/tasks", (c) => {
  const tasks = taskManager.getAllTasks();
  return c.json({
    tasks: tasks.map(summarizeTask),
    total: tasks.length,
  });
});

/**
 * GET /tasks/:id - 특정 태스크 상세 조회
 */
app.get("/tasks/:id", (c) => {
  const task = taskManager.getTask(c.req.param("id"));

  if (!task) {
    return c.json({ error: "Task not found" }, 404);
  }

  return c.json(formatTaskDetail(task));
});

/**
 * GET /tasks/:id/stream - 실시간 태스크 상태 스트리밍 (SSE)
 */
app.get("/tasks/:id/stream", async (c) => {
  const taskId = c.req.param("id");
  const task = taskManager.getTask(taskId);

  if (!task) {
    return c.json({ error: "Task not found" }, 404);
  }

  // Server-Sent Events 설정
  c.header("Content-Type", "text/event-stream");
  c.header("Cache-Control", "no-cache");
  c.header("Connection", "keep-alive");

  return c.streamText(async (stream) => {
    // 현재 상태 즉시 전송
    await stream.write(`data: ${JSON.stringify(formatTaskDetail(task))}\n\n`);

    // 완료된 태스크는 바로 종료
    if (task.status === "completed" || task.status === "failed" || task.status === "cancelled") {
      return;
    }

    // 업데이트 리스너 등록
    const sendUpdate = async (updatedTask: Task) => {
      if (updatedTask.id === taskId) {
        await stream.write(`data: ${JSON.stringify(formatTaskDetail(updatedTask))}\n\n`);
      }
    };

    taskManager.on("update", sendUpdate);

    // 완료 대기 (최대 30분)
    const timeout = 30 * 60 * 1000;
    const start = Date.now();

    while (Date.now() - start < timeout) {
      const currentTask = taskManager.getTask(taskId);
      if (!currentTask ||
          currentTask.status === "completed" ||
          currentTask.status === "failed" ||
          currentTask.status === "cancelled") {
        break;
      }
      await Bun.sleep(1000);
    }
  });
});

/**
 * DELETE /tasks/:id - 태스크 취소
 */
app.delete("/tasks/:id", async (c) => {
  const taskId = c.req.param("id");
  const cancelled = await taskManager.cancelTask(taskId);

  if (!cancelled) {
    return c.json({ error: "Task not found or cannot be cancelled" }, 400);
  }

  return c.json({ message: "Task cancelled" });
});

// ============================================================================
// Helpers
// ============================================================================

function summarizeTask(task: Task) {
  return {
    id: task.id,
    checkpoint_id: task.config.checkpointId,
    checkpoint_name: task.config.checkpointName,
    status: task.status,
    current_iteration: task.currentIteration,
    max_retries: task.config.maxRetries,
    created_at: task.createdAt,
    final_result: task.finalResult,
  };
}

function formatTaskDetail(task: Task) {
  return {
    ...summarizeTask(task),
    worktree_path: task.config.worktreePath,
    validation_command: task.config.validationCommand,
    started_at: task.startedAt,
    completed_at: task.completedAt,
    iterations: task.iterations.map((iter) => ({
      iteration: iter.iteration,
      started_at: iter.startedAt,
      completed_at: iter.completedAt,
      worker_success: iter.workerResult?.success,
      worker_exit_code: iter.workerResult?.exitCode,
      worker_duration_ms: iter.workerResult?.duration,
      validation_success: iter.validationResult?.success,
      validation_exit_code: iter.validationResult?.exitCode,
      feedback: iter.feedback ? iter.feedback.substring(0, 500) + "..." : undefined,
    })),
  };
}

// ============================================================================
// Server Start
// ============================================================================

async function main() {
  try {
    // TaskManager 초기화
    await taskManager.initialize();

    // 서버 시작
    console.log(`
╔═══════════════════════════════════════════════════════════╗
║                    Team Claude Server v0.1.0                      ║
╠═══════════════════════════════════════════════════════════╣
║  Port:      ${String(PORT).padEnd(44)}║
║  Data Dir:  ${DATA_DIR.padEnd(44)}║
║  Executor:  ${(EXECUTOR || "auto").padEnd(44)}║
╚═══════════════════════════════════════════════════════════╝

Endpoints:
  GET  /health        - Health check
  GET  /info          - Server info
  POST /tasks         - Create new task
  GET  /tasks         - List all tasks
  GET  /tasks/:id     - Get task details
  GET  /tasks/:id/stream - Stream task updates (SSE)
  DELETE /tasks/:id   - Cancel task

Ready to accept connections...
    `);

    Bun.serve({
      port: PORT,
      fetch: app.fetch,
    });
  } catch (error) {
    console.error("Failed to start server:", error);
    process.exit(1);
  }
}

main();
