/**
 * Headless Executor
 *
 * 백그라운드에서 Worker Claude를 실행.
 * UI 없이 조용히 실행되며, 로그는 파일로 저장됨.
 */

import { spawn, type Subprocess } from "bun";
import { mkdir, writeFile, readFile, unlink, appendFile } from "fs/promises";
import { join, dirname } from "path";
import type {
  WorkerExecutor,
  ExecutorConfig,
  ExecutorResult,
  ExecutorStatus,
} from "./types";

interface RunningTask {
  taskId: string;
  process: Subprocess;
  startedAt: Date;
  status: ExecutorStatus["state"];
  logFile: string;
  outputChunks: string[];
}

export class HeadlessExecutor implements WorkerExecutor {
  readonly name = "headless";

  private runningTasks: Map<string, RunningTask> = new Map();
  private logDir: string;

  constructor(logDir: string = "/tmp/afl-logs") {
    this.logDir = logDir;
  }

  async isAvailable(): Promise<boolean> {
    // Headless는 항상 사용 가능
    return true;
  }

  async execute(config: ExecutorConfig): Promise<ExecutorResult> {
    const startTime = Date.now();
    const logFile = join(this.logDir, `${config.taskId}.log`);

    // 로그 디렉토리 생성
    await mkdir(dirname(logFile), { recursive: true });

    // 초기 로그 파일 생성
    await writeFile(
      logFile,
      `=== AFL Task: ${config.taskId} ===\n` +
        `Checkpoint: ${config.checkpointName}\n` +
        `Started: ${new Date().toISOString()}\n` +
        `Working Directory: ${config.workingDirectory}\n` +
        `Command: ${config.command}\n` +
        `${"=".repeat(50)}\n\n`
    );

    return new Promise<ExecutorResult>((resolve) => {
      const outputChunks: string[] = [];

      // 프로세스 실행
      const proc = spawn({
        cmd: ["bash", "-c", config.command],
        cwd: config.workingDirectory,
        env: {
          ...process.env,
          ...config.env,
        },
        stdout: "pipe",
        stderr: "pipe",
      });

      // 태스크 등록
      const task: RunningTask = {
        taskId: config.taskId,
        process: proc,
        startedAt: new Date(),
        status: "running",
        logFile,
        outputChunks,
      };
      this.runningTasks.set(config.taskId, task);

      // stdout 스트리밍
      this.streamOutput(proc.stdout, logFile, outputChunks, "[stdout]");

      // stderr 스트리밍
      this.streamOutput(proc.stderr, logFile, outputChunks, "[stderr]");

      // 완료 대기
      proc.exited.then(async (exitCode) => {
        const duration = Date.now() - startTime;
        const output = outputChunks.join("");

        // 완료 로그 추가
        await appendFile(
          logFile,
          `\n${"=".repeat(50)}\n` +
            `Completed: ${new Date().toISOString()}\n` +
            `Exit Code: ${exitCode}\n` +
            `Duration: ${duration}ms\n`
        );

        task.status = exitCode === 0 ? "completed" : "failed";

        resolve({
          success: exitCode === 0,
          exitCode: exitCode ?? 1,
          stdout: output,
          stderr: "",
          duration,
        });
      });
    });
  }

  async getStatus(taskId: string): Promise<ExecutorStatus> {
    const task = this.runningTasks.get(taskId);

    if (!task) {
      return { state: "idle" };
    }

    return {
      state: task.status,
      startedAt: task.startedAt,
      processId: String(task.process.pid),
    };
  }

  async abort(taskId: string): Promise<boolean> {
    const task = this.runningTasks.get(taskId);
    if (!task) return false;

    try {
      task.process.kill("SIGTERM");

      // 잠시 대기 후 강제 종료
      setTimeout(() => {
        try {
          task.process.kill("SIGKILL");
        } catch {
          // 이미 종료됨
        }
      }, 5000);

      task.status = "failed";
      return true;
    } catch {
      return false;
    }
  }

  async cleanup(taskId: string): Promise<void> {
    const task = this.runningTasks.get(taskId);
    this.runningTasks.delete(taskId);

    // 로그 파일은 유지 (디버깅용)
    // 필요시 삭제: await unlink(task.logFile).catch(() => {});
  }

  /**
   * 로그 파일 내용 조회
   */
  async getLog(taskId: string): Promise<string> {
    const logFile = join(this.logDir, `${taskId}.log`);
    try {
      return await readFile(logFile, "utf-8");
    } catch {
      return "";
    }
  }

  /**
   * 실시간 출력 스트리밍 (로그 파일 + 메모리)
   */
  private async streamOutput(
    stream: ReadableStream<Uint8Array> | null,
    logFile: string,
    chunks: string[],
    prefix: string
  ): Promise<void> {
    if (!stream) return;

    const reader = stream.getReader();
    const decoder = new TextDecoder();

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const text = decoder.decode(value, { stream: true });
        chunks.push(text);

        // 로그 파일에도 기록
        await appendFile(logFile, text).catch(() => {});
      }
    } catch {
      // 스트림 종료
    } finally {
      reader.releaseLock();
    }
  }
}

export const createHeadlessExecutor = (
  logDir?: string
): WorkerExecutor => new HeadlessExecutor(logDir);
