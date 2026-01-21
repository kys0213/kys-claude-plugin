/**
 * iTerm2 Executor
 *
 * macOS iTerm2를 사용하여 새 탭에서 Worker Claude를 실행.
 * 사용자가 실시간으로 작업 진행 상황을 볼 수 있음.
 */

import { $ } from "bun";
import type {
  WorkerExecutor,
  ExecutorConfig,
  ExecutorResult,
  ExecutorStatus,
} from "./types";

interface RunningTask {
  taskId: string;
  sessionId: string;
  startedAt: Date;
  status: ExecutorStatus["state"];
  markerFile: string;
}

export class ITermExecutor implements WorkerExecutor {
  readonly name = "iterm";

  private runningTasks: Map<string, RunningTask> = new Map();

  async isAvailable(): Promise<boolean> {
    if (process.platform !== "darwin") {
      return false;
    }

    try {
      // iTerm2가 설치되어 있는지 확인
      const result =
        await $`osascript -e 'tell application "System Events" to (name of processes) contains "iTerm2"'`
          .quiet()
          .nothrow();

      // 설치 여부만 확인 (실행 중이 아니어도 됨)
      const appExists =
        await $`test -d "/Applications/iTerm.app"`.quiet().nothrow();
      return appExists.exitCode === 0;
    } catch {
      return false;
    }
  }

  async execute(config: ExecutorConfig): Promise<ExecutorResult> {
    const startTime = Date.now();
    const markerFile = `/tmp/afl-task-${config.taskId}.done`;
    const outputFile = `/tmp/afl-task-${config.taskId}.output`;
    const exitCodeFile = `/tmp/afl-task-${config.taskId}.exitcode`;

    // 이전 실행 파일 정리
    await $`rm -f ${markerFile} ${outputFile} ${exitCodeFile}`.quiet().nothrow();

    // iTerm2에서 새 탭을 열고 명령어 실행
    const script = this.buildAppleScript(config, markerFile, outputFile, exitCodeFile);

    try {
      // 태스크 등록
      this.runningTasks.set(config.taskId, {
        taskId: config.taskId,
        sessionId: "", // AppleScript 실행 후 알 수 없음
        startedAt: new Date(),
        status: "running",
        markerFile,
      });

      // AppleScript 실행하여 iTerm 탭 열기
      await $`osascript -e ${script}`.quiet();

      // 완료 대기 (marker 파일 polling)
      const result = await this.waitForCompletion(
        config.taskId,
        markerFile,
        outputFile,
        exitCodeFile
      );

      return {
        ...result,
        duration: Date.now() - startTime,
      };
    } catch (error) {
      this.runningTasks.delete(config.taskId);
      return {
        success: false,
        exitCode: 1,
        stdout: "",
        stderr: error instanceof Error ? error.message : String(error),
        duration: Date.now() - startTime,
      };
    }
  }

  async getStatus(taskId: string): Promise<ExecutorStatus> {
    const task = this.runningTasks.get(taskId);

    if (!task) {
      return { state: "idle" };
    }

    // marker 파일로 완료 여부 확인
    const markerExists = await $`test -f ${task.markerFile}`.quiet().nothrow();

    if (markerExists.exitCode === 0) {
      return {
        state: "completed",
        startedAt: task.startedAt,
        completedAt: new Date(),
      };
    }

    return {
      state: task.status,
      startedAt: task.startedAt,
    };
  }

  async abort(taskId: string): Promise<boolean> {
    const task = this.runningTasks.get(taskId);
    if (!task) return false;

    try {
      // iTerm 세션에 Ctrl+C 보내기
      const script = `
        tell application "iTerm2"
          tell current window
            repeat with aTab in tabs
              tell aTab
                repeat with aSession in sessions
                  if name of aSession contains "${taskId}" then
                    tell aSession
                      write text (ASCII character 3)
                    end tell
                  end if
                end repeat
              end tell
            end repeat
          end tell
        end tell
      `;

      await $`osascript -e ${script}`.quiet().nothrow();
      task.status = "failed";
      return true;
    } catch {
      return false;
    }
  }

  async cleanup(taskId: string): Promise<void> {
    this.runningTasks.delete(taskId);

    // 임시 파일 정리
    await $`rm -f /tmp/afl-task-${taskId}.*`.quiet().nothrow();
  }

  private buildAppleScript(
    config: ExecutorConfig,
    markerFile: string,
    outputFile: string,
    exitCodeFile: string
  ): string {
    // 환경 변수 설정 문자열 생성
    const envSetup = config.env
      ? Object.entries(config.env)
          .map(([k, v]) => `export ${k}="${v}"`)
          .join("; ")
      : "";

    // 실행할 전체 명령어 (완료 시 marker 파일 생성)
    const fullCommand = `
cd "${config.workingDirectory}" && \\
${envSetup ? envSetup + " && \\" : ""}
(${config.command}) 2>&1 | tee "${outputFile}"; \\
echo $? > "${exitCodeFile}"; \\
touch "${markerFile}"
`.trim();

    // 탭 이름 설정
    const tabName = `AFL: ${config.checkpointName}`;

    return `
tell application "iTerm2"
  activate

  tell current window
    -- 새 탭 생성
    create tab with default profile

    tell current session
      -- 탭 이름 설정
      set name to "${tabName}"

      -- 명령어 실행
      write text "${fullCommand.replace(/"/g, '\\"').replace(/\n/g, "\\n")}"
    end tell
  end tell
end tell
    `.trim();
  }

  private async waitForCompletion(
    taskId: string,
    markerFile: string,
    outputFile: string,
    exitCodeFile: string,
    timeoutMs: number = 30 * 60 * 1000 // 30분 기본 타임아웃
  ): Promise<Omit<ExecutorResult, "duration">> {
    const startTime = Date.now();
    const pollInterval = 1000; // 1초마다 확인

    while (Date.now() - startTime < timeoutMs) {
      // marker 파일 확인
      const markerExists = await $`test -f ${markerFile}`.quiet().nothrow();

      if (markerExists.exitCode === 0) {
        // 완료됨 - 결과 읽기
        const output = await $`cat ${outputFile} 2>/dev/null || echo ""`.quiet().text();
        const exitCodeStr = await $`cat ${exitCodeFile} 2>/dev/null || echo "1"`.quiet().text();
        const exitCode = parseInt(exitCodeStr.trim(), 10) || 1;

        const task = this.runningTasks.get(taskId);
        if (task) {
          task.status = exitCode === 0 ? "completed" : "failed";
        }

        return {
          success: exitCode === 0,
          exitCode,
          stdout: output,
          stderr: "",
        };
      }

      // 대기
      await Bun.sleep(pollInterval);
    }

    // 타임아웃
    const task = this.runningTasks.get(taskId);
    if (task) {
      task.status = "failed";
    }

    return {
      success: false,
      exitCode: 124, // timeout exit code
      stdout: "",
      stderr: "Execution timed out",
    };
  }
}

export const createITermExecutor = (): WorkerExecutor => new ITermExecutor();
