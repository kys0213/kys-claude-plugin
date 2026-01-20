/**
 * WorkerExecutor Interface
 *
 * 추상화된 Worker 실행 인터페이스.
 * iTerm, Terminal.app, Kitty, Headless 등 다양한 백엔드로 교체 가능.
 */

export interface ExecutorConfig {
  /** Worktree 경로 */
  workingDirectory: string;

  /** 실행할 명령어 */
  command: string;

  /** 환경 변수 */
  env?: Record<string, string>;

  /** 태스크 ID (터미널 탭 이름 등에 사용) */
  taskId: string;

  /** 체크포인트 이름 */
  checkpointName: string;
}

export interface ExecutorResult {
  /** 실행 성공 여부 */
  success: boolean;

  /** 종료 코드 */
  exitCode: number;

  /** 표준 출력 */
  stdout: string;

  /** 표준 에러 */
  stderr: string;

  /** 실행 시간 (ms) */
  duration: number;
}

export interface ExecutorStatus {
  /** 현재 상태 */
  state: 'idle' | 'running' | 'completed' | 'failed';

  /** 시작 시간 */
  startedAt?: Date;

  /** 종료 시간 */
  completedAt?: Date;

  /** 프로세스/세션 ID */
  processId?: string;
}

/**
 * WorkerExecutor 인터페이스
 *
 * 모든 executor 백엔드가 구현해야 하는 인터페이스.
 * 새로운 터미널/실행 환경 추가 시 이 인터페이스만 구현하면 됨.
 */
export interface WorkerExecutor {
  /** Executor 이름 (예: "iterm", "headless") */
  readonly name: string;

  /** 이 executor를 현재 환경에서 사용 가능한지 확인 */
  isAvailable(): Promise<boolean>;

  /** Worker 실행 */
  execute(config: ExecutorConfig): Promise<ExecutorResult>;

  /** 실행 중인 Worker 상태 조회 */
  getStatus(taskId: string): Promise<ExecutorStatus>;

  /** 실행 중인 Worker 중단 */
  abort(taskId: string): Promise<boolean>;

  /** 정리 작업 (리소스 해제 등) */
  cleanup(taskId: string): Promise<void>;
}

/**
 * Executor 팩토리 타입
 */
export type ExecutorFactory = () => WorkerExecutor;

/**
 * Executor 레지스트리
 * 사용 가능한 executor들을 등록하고 조회
 */
export interface ExecutorRegistry {
  register(name: string, factory: ExecutorFactory): void;
  get(name: string): WorkerExecutor | undefined;
  getAvailable(): Promise<WorkerExecutor[]>;
  getDefault(): Promise<WorkerExecutor | undefined>;
}
