import type { TeamClaudeConfig, WorkerTemplate } from "./config.types";

/**
 * 내장 템플릿: minimal
 * 최소한의 지시, 자유도 높음
 */
export const TEMPLATE_MINIMAL: WorkerTemplate = {
  name: "minimal",
  description: "최소 지시, 자유도 높음",
  claudeMd: `# Worker Task

아래 Task를 구현하세요.

## Task
{{TASK_DESCRIPTION}}

## 완료 조건
- 기능 동작 확인
`,
};

/**
 * 내장 템플릿: standard
 * TDD + 커밋 컨벤션, 균형잡힌 설정
 */
export const TEMPLATE_STANDARD: WorkerTemplate = {
  name: "standard",
  description: "TDD + 커밋 컨벤션 (기본값)",
  claudeMd: `# Worker Task

## Task
{{TASK_DESCRIPTION}}

## 작업 규칙
1. 구현 전 테스트 먼저 작성 (TDD)
2. 커밋은 conventional commits 형식
3. 완료 전 셀프 리뷰

## 완료 조건
- [ ] 모든 테스트 통과
- [ ] 타입 에러 없음
- [ ] 기능 동작 확인

## 막히면
- 구체적인 blocker 설명과 함께 완료 보고
- \`.claude/blockers.md\` 파일에 상세 내용 기록
`,
  rules: ["test-required", "conventional-commits"],
};

/**
 * 내장 템플릿: strict
 * 린트/테스트 통과 필수, 엄격한 규칙
 */
export const TEMPLATE_STRICT: WorkerTemplate = {
  name: "strict",
  description: "린트/테스트 통과 필수",
  claudeMd: `# Worker Task

## Task
{{TASK_DESCRIPTION}}

## 필수 규칙
1. TDD 필수 - 구현 전 테스트 먼저 작성
2. 테스트 커버리지 80% 이상
3. ESLint/Prettier 통과 필수
4. TypeScript strict mode
5. 모든 exported 함수에 JSDoc
6. Conventional Commits 형식

## 완료 전 체크리스트
- [ ] \`npm run lint\` 통과
- [ ] \`npm run test\` 통과
- [ ] \`npm run type-check\` 통과
- [ ] 커버리지 80% 이상

## 실패 시
- 체크리스트 미통과 항목 보고
- 해결 시도한 내용 포함
- \`.claude/blockers.md\` 파일에 상세 내용 기록

## 금지 사항
- console.log 사용 금지 (logger 사용)
- any 타입 사용 금지
- 주석 처리된 코드 커밋 금지
`,
  rules: [
    "test-required",
    "test-coverage-80",
    "conventional-commits",
    "no-console",
    "no-any",
    "lint-required",
  ],
};

/**
 * 기본 설정값
 */
export const DEFAULT_CONFIG: TeamClaudeConfig = {
  version: "1.0",

  server: {
    port: 3847,
    host: "localhost",
    timeout: 60000,
  },

  worktree: {
    root: "../worktrees",
    branchPrefix: "feature/",
    cleanupOnComplete: false,
  },

  worker: {
    maxConcurrent: 5,
    defaultTemplate: "standard",
    timeout: 1800, // 30분
    autoRetry: false,
    retryLimit: 2,
  },

  notification: {
    method: "file",
  },

  review: {
    autoLevel: "semi-auto",
    requireApproval: true,
    rules: [],
  },

  templates: {
    minimal: TEMPLATE_MINIMAL,
    standard: TEMPLATE_STANDARD,
    strict: TEMPLATE_STRICT,
  },
};

/**
 * 설정 키별 유효성 검사 규칙
 */
export const CONFIG_VALIDATION: Record<
  string,
  {
    type: "number" | "string" | "boolean" | "enum";
    min?: number;
    max?: number;
    enum?: string[];
    description: string;
  }
> = {
  "server.port": {
    type: "number",
    min: 1024,
    max: 65535,
    description: "서버 포트 (1024-65535)",
  },
  "server.host": {
    type: "string",
    description: "서버 호스트",
  },
  "server.timeout": {
    type: "number",
    min: 1000,
    max: 300000,
    description: "요청 타임아웃 ms (1000-300000)",
  },
  "worktree.root": {
    type: "string",
    description: "Worktree 루트 경로",
  },
  "worktree.branchPrefix": {
    type: "string",
    description: "브랜치 접두사",
  },
  "worktree.cleanupOnComplete": {
    type: "boolean",
    description: "완료 시 자동 정리",
  },
  "worker.maxConcurrent": {
    type: "number",
    min: 1,
    max: 20,
    description: "동시 Worker 수 (1-20)",
  },
  "worker.defaultTemplate": {
    type: "string",
    description: "기본 Worker 템플릿",
  },
  "worker.timeout": {
    type: "number",
    min: 60,
    max: 7200,
    description: "Worker 타임아웃 초 (60-7200)",
  },
  "worker.autoRetry": {
    type: "boolean",
    description: "실패 시 자동 재시도",
  },
  "worker.retryLimit": {
    type: "number",
    min: 0,
    max: 5,
    description: "재시도 최대 횟수 (0-5)",
  },
  "notification.method": {
    type: "enum",
    enum: ["file", "notification", "slack", "webhook"],
    description: "알림 방식",
  },
  "review.autoLevel": {
    type: "enum",
    enum: ["manual", "semi-auto", "full-auto"],
    description: "리뷰 자동화 레벨",
  },
  "review.requireApproval": {
    type: "boolean",
    description: "승인 필수 여부",
  },
};
