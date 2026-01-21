/**
 * Task Manager Service
 *
 * 태스크 큐 관리 및 실행 라이프사이클 처리.
 * Executor를 사용하여 Worker를 실행하고 결과를 추적.
 */

import { $ } from "bun";
import { readFile, writeFile, mkdir } from "fs/promises";
import { join } from "path";
import {
  type WorkerExecutor,
  type ExecutorResult,
  selectExecutor,
} from "../executors";

export interface TaskConfig {
  checkpointId: string;
  checkpointName: string;
  worktreePath: string;
  validationCommand: string;
  maxRetries: number;
  claudeCommand?: string; // 기본값: "claude --print"
}

export interface TaskIteration {
  iteration: number;
  startedAt: Date;
  completedAt?: Date;
  workerResult?: ExecutorResult;
  validationResult?: {
    success: boolean;
    output: string;
    exitCode: number;
  };
  feedback?: string;
}

export interface Task {
  id: string;
  config: TaskConfig;
  status: "queued" | "running" | "completed" | "failed" | "cancelled";
  currentIteration: number;
  iterations: TaskIteration[];
  createdAt: Date;
  startedAt?: Date;
  completedAt?: Date;
  finalResult?: "pass" | "fail";
}

export interface TaskManagerConfig {
  executor?: string; // executor 이름 (예: "iterm", "headless")
  dataDir: string; // 태스크 데이터 저장 경로
}

export class TaskManager {
  private tasks: Map<string, Task> = new Map();
  private executor!: WorkerExecutor;
  private config: TaskManagerConfig;
  private eventListeners: Map<string, Set<(task: Task) => void>> = new Map();

  constructor(config: TaskManagerConfig) {
    this.config = config;
  }

  async initialize(): Promise<void> {
    this.executor = await selectExecutor(this.config.executor);
    console.log(`TaskManager initialized with executor: ${this.executor.name}`);

    // 데이터 디렉토리 생성
    await mkdir(this.config.dataDir, { recursive: true });
  }

  /**
   * 새 태스크 생성
   */
  async createTask(config: TaskConfig): Promise<Task> {
    const id = this.generateTaskId();

    const task: Task = {
      id,
      config,
      status: "queued",
      currentIteration: 0,
      iterations: [],
      createdAt: new Date(),
    };

    this.tasks.set(id, task);
    await this.persistTask(task);

    // 비동기로 실행 시작
    this.runTask(task);

    return task;
  }

  /**
   * 태스크 조회
   */
  getTask(id: string): Task | undefined {
    return this.tasks.get(id);
  }

  /**
   * 모든 태스크 조회
   */
  getAllTasks(): Task[] {
    return Array.from(this.tasks.values());
  }

  /**
   * 태스크 취소
   */
  async cancelTask(id: string): Promise<boolean> {
    const task = this.tasks.get(id);
    if (!task || task.status === "completed" || task.status === "failed") {
      return false;
    }

    const aborted = await this.executor.abort(id);
    if (aborted) {
      task.status = "cancelled";
      task.completedAt = new Date();
      await this.persistTask(task);
      this.emit(task);
    }

    return aborted;
  }

  /**
   * 이벤트 리스너 등록
   */
  on(event: "update", listener: (task: Task) => void): void {
    if (!this.eventListeners.has(event)) {
      this.eventListeners.set(event, new Set());
    }
    this.eventListeners.get(event)!.add(listener);
  }

  /**
   * 태스크 실행 루프
   */
  private async runTask(task: Task): Promise<void> {
    task.status = "running";
    task.startedAt = new Date();
    this.emit(task);

    while (task.currentIteration < task.config.maxRetries) {
      task.currentIteration++;

      const iteration: TaskIteration = {
        iteration: task.currentIteration,
        startedAt: new Date(),
      };
      task.iterations.push(iteration);
      this.emit(task);

      // 1. Worker 실행
      const claudeCommand = task.config.claudeCommand || "claude --print";
      const prompt = await this.buildPrompt(task);

      const workerResult = await this.executor.execute({
        taskId: task.id,
        checkpointName: task.config.checkpointName,
        workingDirectory: task.config.worktreePath,
        command: `${claudeCommand} "${prompt.replace(/"/g, '\\"')}"`,
        env: {
          AFL_TASK_ID: task.id,
          AFL_ITERATION: String(task.currentIteration),
        },
      });

      iteration.workerResult = workerResult;
      iteration.completedAt = new Date();
      this.emit(task);

      // Worker 실패 시 (Claude 자체 오류)
      if (!workerResult.success && workerResult.exitCode !== 0) {
        // Claude 실행 자체가 실패한 경우
        if (workerResult.stderr.includes("error") || workerResult.exitCode === 1) {
          // 재시도
          iteration.feedback = `Worker execution failed: ${workerResult.stderr}`;
          await this.persistTask(task);
          continue;
        }
      }

      // 2. Validation 실행
      const validationResult = await this.runValidation(task);
      iteration.validationResult = validationResult;
      this.emit(task);

      // 3. 결과 확인
      if (validationResult.success) {
        // 성공!
        task.status = "completed";
        task.finalResult = "pass";
        task.completedAt = new Date();
        await this.persistTask(task);
        await this.executor.cleanup(task.id);
        this.emit(task);
        return;
      }

      // 4. 실패 - 피드백 생성
      if (task.currentIteration < task.config.maxRetries) {
        const feedback = await this.generateFeedback(task, validationResult);
        iteration.feedback = feedback;
        await this.updateClaudeMd(task, feedback);
        await this.persistTask(task);
      }
    }

    // 최대 재시도 초과
    task.status = "failed";
    task.finalResult = "fail";
    task.completedAt = new Date();
    await this.persistTask(task);
    await this.executor.cleanup(task.id);
    this.emit(task);
  }

  /**
   * Validation 명령 실행
   */
  private async runValidation(
    task: Task
  ): Promise<{ success: boolean; output: string; exitCode: number }> {
    try {
      const result = await $`bash -c ${task.config.validationCommand}`
        .cwd(task.config.worktreePath)
        .quiet()
        .nothrow();

      return {
        success: result.exitCode === 0,
        output: result.stdout.toString() + result.stderr.toString(),
        exitCode: result.exitCode,
      };
    } catch (error) {
      return {
        success: false,
        output: error instanceof Error ? error.message : String(error),
        exitCode: 1,
      };
    }
  }

  /**
   * 피드백 생성 (Test Oracle 역할)
   */
  private async generateFeedback(
    task: Task,
    validationResult: { success: boolean; output: string; exitCode: number }
  ): Promise<string> {
    // 간단한 피드백 생성 (향후 LLM 기반으로 확장 가능)
    const lines = validationResult.output.split("\n").slice(-50); // 마지막 50줄

    return `
## Iteration ${task.currentIteration} - FAILED

**Validation Command**: \`${task.config.validationCommand}\`
**Exit Code**: ${validationResult.exitCode}

**Output (last 50 lines)**:
\`\`\`
${lines.join("\n")}
\`\`\`

**Suggestion**: 위 에러를 분석하고 수정하세요.
`.trim();
  }

  /**
   * CLAUDE.md에 피드백 추가
   */
  private async updateClaudeMd(task: Task, feedback: string): Promise<void> {
    const claudeMdPath = join(task.config.worktreePath, "CLAUDE.md");

    try {
      let content = await readFile(claudeMdPath, "utf-8");

      // Feedback 섹션 추가
      if (!content.includes("## Feedback History")) {
        content += "\n\n---\n## Feedback History\n";
      }

      content += `\n${feedback}\n`;

      await writeFile(claudeMdPath, content);
    } catch (error) {
      console.error("Failed to update CLAUDE.md:", error);
    }
  }

  /**
   * Worker에 전달할 프롬프트 생성
   */
  private async buildPrompt(task: Task): Promise<string> {
    if (task.currentIteration === 1) {
      return "CLAUDE.md 파일을 읽고 지시사항을 수행하세요. 완료 후 변경사항을 커밋하세요.";
    }

    return `CLAUDE.md 파일의 Feedback History를 확인하고 이전 시도의 문제를 수정하세요.
이번이 ${task.currentIteration}번째 시도입니다.
완료 후 변경사항을 커밋하세요.`;
  }

  /**
   * 태스크 데이터 영속화
   */
  private async persistTask(task: Task): Promise<void> {
    const filePath = join(this.config.dataDir, `${task.id}.json`);
    await writeFile(filePath, JSON.stringify(task, null, 2));
  }

  /**
   * 이벤트 발생
   */
  private emit(task: Task): void {
    const listeners = this.eventListeners.get("update");
    if (listeners) {
      for (const listener of listeners) {
        try {
          listener(task);
        } catch (error) {
          console.error("Event listener error:", error);
        }
      }
    }
  }

  /**
   * 태스크 ID 생성
   */
  private generateTaskId(): string {
    const timestamp = Date.now().toString(36);
    const random = Math.random().toString(36).substring(2, 8);
    return `${timestamp}-${random}`;
  }
}
