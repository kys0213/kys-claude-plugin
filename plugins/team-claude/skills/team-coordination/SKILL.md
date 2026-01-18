---
name: team-coordination
description: Team Claude 멀티 에이전트 협업 스킬 - Worker 조율 및 Contract 기반 개발
---

# Team Coordination Skill

멀티 에이전트 협업을 위한 조율 스킬입니다.

## 핵심 개념

### 1. Contract 기반 개발

Contract는 구성 요소 간의 인터페이스 정의입니다. 구현체 없이도 다른 Task가 작업을 시작할 수 있게 합니다.

```typescript
// Contract 예시
interface ICouponService {
  validate(code: string): Promise<ValidationResult>;
  apply(code: string, orderId: string): Promise<ApplyResult>;
}
```

### 2. 병렬 실행

의존성이 없는 Task는 동시에 실행할 수 있습니다:

```
Round 1 (병렬):
  ├── task-a (독립)
  ├── task-b (독립)
  └── task-c (독립)

Round 2 (의존):
  └── task-d (task-a, task-b 완료 후)
```

### 3. 이벤트 기반 알림

Worker의 상태 변화를 Hook으로 감지합니다:

| 이벤트 | Hook | 의미 |
|--------|------|------|
| 작업 완료 | Stop | Worker가 응답 완료 |
| 질문 발생 | PreToolUse (AskUserQuestion) | Worker가 판단 필요 |
| 장시간 대기 | Notification (idle) | 입력 대기 중 |

## 워크플로우

### Phase 1: 설치 및 환경설정

```bash
/team-claude:init
```

- 프로젝트 분석
- 에이전트 그룹 생성
- Hook 설정

### Phase 2: 요구사항 수집 및 정제

```bash
/team-claude:plan "요구사항"
```

- Outline 구조화
- Flow 도식화 (Mermaid)
- Contract 정의
- Task 분해

### Phase 3: 구현 및 피드백 루프

```bash
/team-claude:spawn task-a task-b
/team-claude:status
/team-claude:review task-a
/team-claude:merge task-a
```

## 역할 분담

### 사람 (Architect)

- 아키텍처 설계
- 모호한 부분 판단 (UserAskQuestion 응답)
- 최종 리뷰 승인

### Main Claude (Orchestrator)

- 요구사항 → 스펙 구조화
- Task 분해 및 Worker 배분
- 결과 리뷰 및 피드백 생성
- 모호한 부분 에스컬레이션

### Worker Claude (Executor)

- Contract 기반 구현
- 테스트 작성
- 완료 조건 충족까지 반복
- 완료 보고

## 컨텍스트 엔지니어링 원칙

| 원칙 | 구현 |
|------|------|
| 모호한 부분은 사람이 판단 | AskUserQuestion → 즉시 알림 |
| 명확한 부분은 AI가 실행 | 질문 없이 진행 → 완료 알림 |
| 적절한 개입 타이밍 | 이벤트 발생 시점에 정확히 알림 |
| 불필요한 방해 최소화 | Polling 없음, 필요할 때만 알림 |
