---
name: team-claude:status
description: Worker 상태 조회 - 전체 또는 특정 Task의 진행 상황 확인
argument-hint: "[task-id]"
allowed-tools: ["Read", "Bash", "Glob", "AskUserQuestion"]
---

# Team Claude 상태 조회 커맨드

모든 Worker의 상태를 조회하거나 특정 Task의 상세 정보를 확인합니다.

## 사용법

```bash
# 전체 상태 조회
/team-claude:status

# 특정 Task 상세 조회
/team-claude:status task-coupon-service
```

## Arguments

| Argument | 필수 | 설명 |
|----------|------|------|
| task-id | X | 특정 Task만 조회 |

---

## 전체 상태 조회

### 출력 형식

```
📊 Worker 상태

┌────────────────────────┬────────────┬──────────┬─────────────────────┐
│ Task                   │ Status     │ Progress │ Note                │
├────────────────────────┼────────────┼──────────┼─────────────────────┤
│ task-coupon-service    │ ✅ 완료     │ 100%     │ 리뷰 대기 중        │
│ task-coupon-repository │ 🔄 진행 중  │ 60%      │ 테스트 작성 중      │
│ task-api-endpoint      │ ⏳ 대기     │ -        │ 의존성 대기         │
│ task-admin-ui          │ ✅ 완료     │ 100%     │ 리뷰 대기 중        │
└────────────────────────┴────────────┴──────────┴─────────────────────┘

요약:
  ✅ 완료: 2
  🔄 진행 중: 1
  ⏳ 대기: 1
  ❓ 질문 대기: 0
  ❌ 실패: 0

다음 명령:
  완료된 작업 리뷰: /team-claude:review task-coupon-service
  상세 정보: /team-claude:status task-coupon-repository
```

### 상태 아이콘

| 아이콘 | 상태 | 설명 |
|--------|------|------|
| ⏳ | pending | 대기 중 (의존성 미충족) |
| 🔄 | running | 실행 중 |
| ❓ | waiting | 질문/권한 대기 중 |
| ✅ | completed | 완료됨 |
| ❌ | failed | 실패 |

---

## 특정 Task 상세 조회

### 출력 형식

```
📋 Task 상세: task-coupon-service

기본 정보:
  상태: ✅ 완료
  브랜치: feature/task-coupon-service
  Worktree: ../worktrees/task-coupon-service
  시작: 2024-01-15 10:00:00
  완료: 2024-01-15 10:45:00
  소요: 45분

변경 사항:
  +src/services/coupon.service.ts (신규, 156줄)
  +src/services/coupon.service.test.ts (신규, 234줄)
  ~src/types/index.ts (수정, +15줄)

커밋:
  abc1234 feat(coupon): implement CouponService
  def5678 test(coupon): add unit tests for CouponService

완료 조건:
  ✅ ICouponService 모든 메서드 구현
  ✅ 단위 테스트 커버리지 80% 이상 (87%)
  ✅ lint/typecheck 통과

다음 명령:
  리뷰 시작: /team-claude:review task-coupon-service
  머지: /team-claude:merge task-coupon-service
```

### 진행 중인 Task 상세

```
📋 Task 상세: task-coupon-repository

기본 정보:
  상태: 🔄 진행 중
  브랜치: feature/task-coupon-repository
  Worktree: ../worktrees/task-coupon-repository
  시작: 2024-01-15 10:05:00
  경과: 25분

현재 작업:
  테스트 케이스 작성 중 (3/5 완료)

변경 사항 (WIP):
  +src/repositories/coupon.repository.ts (신규, 89줄)
  +src/repositories/coupon.repository.test.ts (신규, 112줄)

완료 조건:
  ✅ Repository 인터페이스 구현
  ✅ CRUD 메서드 구현
  🔄 단위 테스트 작성 중 (60%)
  ⬜ lint/typecheck 통과

터미널로 이동: 탭 3 (또는 tmux select-window -t task-coupon-repository)
```

### 질문 대기 중인 Task 상세

```
📋 Task 상세: task-payment-service

기본 정보:
  상태: ❓ 질문 대기
  브랜치: feature/task-payment-service
  Worktree: ../worktrees/task-payment-service
  시작: 2024-01-15 10:10:00
  대기 시작: 10:35:00 (5분 전)

⚠️ Worker가 질문을 기다리고 있습니다:

  "결제 실패 시 쿠폰 사용 상태를 어떻게 처리할까요?
   1. 자동으로 미사용 상태로 복구
   2. 수동 처리 필요 상태로 변경
   3. 관리자 알림 후 대기"

답변하려면 해당 터미널로 이동해주세요.
터미널로 이동: 탭 4 (또는 tmux select-window -t task-payment-service)
```

---

## 상태 데이터 소스

### workers.json 읽기

```json
{
  "task-coupon-service": {
    "status": "completed",
    "worktree": "../worktrees/task-coupon-service",
    "branch": "feature/task-coupon-service",
    "startedAt": "2024-01-15T10:00:00Z",
    "completedAt": "2024-01-15T10:45:00Z",
    "events": [
      { "type": "started", "timestamp": "..." },
      { "type": "completed", "timestamp": "..." }
    ]
  }
}
```

### Git 정보 수집

```bash
# 변경 파일 목록
cd ../worktrees/task-coupon-service
git diff --stat main

# 커밋 목록
git log main..HEAD --oneline

# 현재 브랜치
git branch --show-current
```

### 진행률 추정

완료 조건을 기반으로 진행률 계산:

```
완료 조건 5개 중 3개 충족 → 60%
```

---

## 실시간 업데이트

### Hook 이벤트 반영

Worker의 Hook 이벤트가 발생하면 workers.json이 업데이트됩니다:

| 이벤트 | 상태 변경 |
|--------|----------|
| Stop (정상) | running → completed |
| Stop (에러) | running → failed |
| AskUserQuestion | running → waiting |
| idle (60초) | running → idle |

### 의존성 자동 시작

Task 완료 시 의존하던 Task가 자동으로 시작됩니다:

```
🔔 task-coupon-service 완료

의존성 충족:
  task-api-endpoint → 자동 시작됨

/team-claude:status 로 상태를 확인하세요.
```

---

## 필터링 옵션

### 상태별 필터 (추후 구현)

```bash
# 진행 중인 것만
/team-claude:status --running

# 완료된 것만
/team-claude:status --completed

# 문제 있는 것만
/team-claude:status --issues
```

---

## 에러 처리

### 초기화 안 됨

```
❌ Team Claude가 초기화되지 않았습니다.

먼저 /team-claude:init 을 실행해주세요.
```

### Worker 없음

```
📊 Worker 상태

현재 실행 중인 Worker가 없습니다.

Task 실행: /team-claude:spawn <task-id>
Task 목록: ls .team-claude/specs/tasks/
```

### 알 수 없는 Task

```
❌ Task를 찾을 수 없습니다: unknown-task

사용 가능한 Task:
  - task-coupon-service (완료)
  - task-coupon-repository (진행 중)
  - task-api-endpoint (대기)
```
