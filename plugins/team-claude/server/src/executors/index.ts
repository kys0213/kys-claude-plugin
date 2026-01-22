/**
 * Executors Module
 *
 * Worker 실행 백엔드 추상화 레이어.
 *
 * 새로운 executor 추가 방법:
 * 1. executors/ 디렉토리에 새 파일 생성 (예: kitty.ts)
 * 2. WorkerExecutor 인터페이스 구현
 * 3. registry.ts에서 등록
 */

export type {
  WorkerExecutor,
  ExecutorConfig,
  ExecutorResult,
  ExecutorStatus,
  ExecutorFactory,
  ExecutorRegistry,
} from "./types";

export { ITermExecutor, createITermExecutor } from "./iterm";
export { HeadlessExecutor, createHeadlessExecutor } from "./headless";
export { executorRegistry, selectExecutor } from "./registry";
