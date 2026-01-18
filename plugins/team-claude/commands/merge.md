---
name: team-claude:merge
description: 완료된 Task PR 머지 - 최종 검증 후 base branch로 머지
argument-hint: "<task-id> [--squash] [--no-delete-branch]"
allowed-tools: ["Bash", "Read", "Write"]
---

# Team Claude 머지 커맨드

완료되고 리뷰된 Task를 base branch로 머지합니다.

## 사용법

```bash
# 기본 머지
/team-claude:merge task-coupon-service

# squash 머지
/team-claude:merge task-coupon-service --squash

# 브랜치 유지
/team-claude:merge task-coupon-service --no-delete-branch
```

## Arguments

| Argument | 필수 | 설명 |
|----------|------|------|
| task-id | O | 머지할 Task ID |
| --squash | X | squash merge (기본: false) |
| --no-delete-branch | X | 머지 후 브랜치 유지 |

---

## 머지 프로세스

```
/team-claude:merge task-coupon-service
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                     1. 머지 조건 확인                          │
│                                                               │
│  • Task 상태: completed                                       │
│  • 리뷰 상태: approved (차단 항목 없음)                        │
│  • 필수 체크: lint, typecheck, test 통과                      │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                     2. 최종 검증                               │
│                                                               │
│  • 충돌 확인: git merge-base                                  │
│  • CI 체크 실행 (설정된 경우)                                  │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                     3. 머지 실행                               │
│                                                               │
│  • git checkout main                                          │
│  • git merge feature/task-coupon-service [--squash]          │
│  • git push                                                   │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                     4. 정리                                    │
│                                                               │
│  • 브랜치 삭제 (--no-delete-branch 없으면)                    │
│  • Worktree 제거 (cleanupOnMerge 설정에 따라)                 │
│  • 상태 업데이트                                               │
└───────────────────────────────────────────────────────────────┘
```

---

## Step 1: 머지 조건 확인

### 상태 확인

```
🔍 머지 조건 확인: task-coupon-service

  ✅ Task 상태: completed
  ✅ 리뷰 상태: approved
  ✅ 차단 항목: 없음
  ✅ 필수 체크:
     - lint: ✅ 통과
     - typecheck: ✅ 통과
     - test: ✅ 통과 (커버리지 87%)

모든 조건 충족. 머지를 진행합니다.
```

### 조건 미충족 시

```
❌ 머지 조건 미충족: task-coupon-service

  ✅ Task 상태: completed
  ❌ 리뷰 상태: 미리뷰
  ⬜ 필수 체크: 미확인

먼저 리뷰를 완료해주세요:
  /team-claude:review task-coupon-service
```

또는:

```
❌ 머지 조건 미충족: task-coupon-service

  ✅ Task 상태: completed
  ⚠️ 리뷰 상태: 차단 항목 있음

차단 항목:
  - [Security] 하드코딩된 시크릿 발견

피드백 전달 후 재리뷰가 필요합니다:
  /team-claude:feedback task-coupon-service "시크릿을 환경변수로 이동"
```

---

## Step 2: 최종 검증

### 충돌 확인

```bash
git fetch origin main
git merge-base --is-ancestor origin/main feature/task-coupon-service
```

충돌 시:

```
⚠️ 충돌 발생 가능성

feature/task-coupon-service와 main 사이에 충돌이 있을 수 있습니다.

충돌 파일:
  - src/types/index.ts

해결 방법:
  1. Worker worktree에서 rebase:
     cd ../worktrees/task-coupon-service
     git rebase origin/main
     (충돌 해결)
     git rebase --continue

  2. 수동 머지 진행

계속하시겠습니까? [y/N]
```

### CI 체크 (선택)

```
🔄 CI 체크 실행 중...

  lint: ✅ 통과
  typecheck: ✅ 통과
  test: ✅ 통과 (87% 커버리지)
  build: ✅ 통과

모든 체크 통과. 머지를 진행합니다.
```

---

## Step 3: 머지 실행

### 일반 머지

```bash
git checkout main
git pull origin main
git merge feature/task-coupon-service --no-ff -m "Merge feature/task-coupon-service: CouponService 구현"
git push origin main
```

### Squash 머지

```bash
git checkout main
git pull origin main
git merge feature/task-coupon-service --squash
git commit -m "feat(coupon): implement CouponService (#task-coupon-service)

- Add CouponService with validate/apply methods
- Add unit tests (87% coverage)
- Add rate limiting for security"
git push origin main
```

---

## Step 4: 정리

### 브랜치 삭제

```bash
# 로컬 브랜치 삭제
git branch -d feature/task-coupon-service

# 원격 브랜치 삭제
git push origin --delete feature/task-coupon-service
```

### Worktree 제거

```bash
git worktree remove ../worktrees/task-coupon-service
```

### 상태 업데이트

```json
{
  "task-coupon-service": {
    "status": "merged",
    "mergedAt": "2024-01-15T12:00:00Z",
    "mergedTo": "main",
    "squash": false
  }
}
```

---

## 최종 출력

### 성공

```
✅ Task-coupon-service 머지 완료

  branch: feature/task-coupon-service → main
  commits: 3
  files: +2, ~1

  브랜치 삭제됨: feature/task-coupon-service
  worktree 정리됨: ../worktrees/task-coupon-service

남은 작업:
  - task-coupon-repository: 🔄 진행 중
  - task-api-endpoint: ⏳ 대기 중

의존성 업데이트:
  task-api-endpoint의 의존성이 충족되었습니다.
  자동 시작하시겠습니까? [Y/n]
```

### 의존성 자동 시작

머지 후 대기 중이던 Task가 시작 가능해지면:

```
🔔 의존성 충족 알림

task-api-endpoint의 의존성이 모두 충족되었습니다:
  ✅ task-coupon-service (merged)
  ✅ task-coupon-repository (merged)

자동으로 시작합니다...

🚀 Worker 시작: task-api-endpoint
  worktree: ../worktrees/task-api-endpoint
  branch: feature/task-api-endpoint
```

---

## 에러 처리

### 머지 충돌

```
❌ 머지 실패: 충돌 발생

충돌 파일:
  - src/types/index.ts (양쪽에서 수정)

해결 방법:
  1. 수동 해결:
     cd ../worktrees/task-coupon-service
     git rebase origin/main
     # 충돌 해결
     git rebase --continue

  2. Worker에게 해결 요청:
     /team-claude:feedback task-coupon-service "main과 충돌 해결 필요"
```

### 권한 부족

```
❌ 푸시 실패: 권한 부족

main 브랜치에 직접 푸시할 권한이 없습니다.

대안:
  1. PR 생성:
     gh pr create --base main --head feature/task-coupon-service

  2. 관리자에게 요청
```

---

## 머지 취소

머지 직후 문제 발견 시:

```bash
# 로컬 머지 취소 (푸시 전)
git reset --hard HEAD~1

# 푸시 후 revert
git revert HEAD
git push
```
