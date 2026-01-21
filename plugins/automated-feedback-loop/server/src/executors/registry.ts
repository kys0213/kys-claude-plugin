/**
 * Executor Registry
 *
 * 사용 가능한 executor들을 등록하고 관리.
 * 새로운 터미널/실행 환경 추가 시 여기에 등록만 하면 됨.
 */

import type { WorkerExecutor, ExecutorFactory, ExecutorRegistry } from "./types";
import { createITermExecutor } from "./iterm";
import { createHeadlessExecutor } from "./headless";

class ExecutorRegistryImpl implements ExecutorRegistry {
  private executors: Map<string, ExecutorFactory> = new Map();
  private instances: Map<string, WorkerExecutor> = new Map();

  constructor() {
    // 기본 executor들 등록
    this.register("iterm", createITermExecutor);
    this.register("headless", createHeadlessExecutor);
  }

  register(name: string, factory: ExecutorFactory): void {
    this.executors.set(name, factory);
  }

  get(name: string): WorkerExecutor | undefined {
    // 캐시된 인스턴스가 있으면 반환
    if (this.instances.has(name)) {
      return this.instances.get(name);
    }

    // 팩토리에서 생성
    const factory = this.executors.get(name);
    if (!factory) {
      return undefined;
    }

    const instance = factory();
    this.instances.set(name, instance);
    return instance;
  }

  async getAvailable(): Promise<WorkerExecutor[]> {
    const available: WorkerExecutor[] = [];

    for (const name of this.executors.keys()) {
      const executor = this.get(name);
      if (executor && (await executor.isAvailable())) {
        available.push(executor);
      }
    }

    return available;
  }

  async getDefault(): Promise<WorkerExecutor | undefined> {
    // 우선순위: iterm > terminal-app > kitty > headless
    const priority = ["iterm", "terminal-app", "kitty", "gnome-terminal", "headless"];

    for (const name of priority) {
      const executor = this.get(name);
      if (executor && (await executor.isAvailable())) {
        return executor;
      }
    }

    return undefined;
  }

  /**
   * 등록된 모든 executor 이름 반환
   */
  getRegisteredNames(): string[] {
    return Array.from(this.executors.keys());
  }
}

// 싱글톤 인스턴스
export const executorRegistry = new ExecutorRegistryImpl();

/**
 * 설정에 따라 executor 선택
 */
export async function selectExecutor(
  preferred?: string
): Promise<WorkerExecutor> {
  // 선호 executor가 지정된 경우
  if (preferred) {
    const executor = executorRegistry.get(preferred);
    if (executor && (await executor.isAvailable())) {
      return executor;
    }
    console.warn(
      `Preferred executor '${preferred}' is not available, falling back to default`
    );
  }

  // 기본 executor 선택
  const defaultExecutor = await executorRegistry.getDefault();
  if (defaultExecutor) {
    return defaultExecutor;
  }

  throw new Error("No available executor found");
}
