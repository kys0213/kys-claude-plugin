---
name: team-claude:cleanup
description: Worktree 및 리소스 정리 - 완료된 Task의 worktree와 브랜치 제거
argument-hint: "[task-id | --all | --completed]"
allowed-tools: ["Bash", "Read", "Write"]
---

# Team Claude 정리 커맨드

Worktree와 관련 리소스를 정리합니다.

## 사용법

```bash
# 특정 Task 정리
/team-claude:cleanup task-coupon-service

# 완료된 것만 정리
/team-claude:cleanup --completed

# 모든 worktree 정리
/team-claude:cleanup --all
```

## Arguments

| Argument | 필수 | 설명 |
|----------|------|------|
| task-id | X | 특정 Task만 정리 |
| --completed | X | 완료/머지된 것만 정리 |
| --all | X | 모든 worktree 정리 |

---

## 정리 대상

### 정리되는 리소스

| 리소스 | 설명 | 위치 |
|--------|------|------|
| Worktree | Git worktree 디렉토리 | ../worktrees/{task-id}/ |
| 브랜치 | Feature 브랜치 | feature/{task-id} |
| 상태 파일 | Worker 상태 기록 | .team-claude/state/ |
| 리뷰 파일 | 리뷰 결과 | .team-claude/reviews/{task-id}/ |

---

## 특정 Task 정리

### /team-claude:cleanup task-coupon-service

```
🧹 정리 대상: task-coupon-service

현재 상태: ✅ merged

정리할 리소스:
  📁 Worktree: ../worktrees/task-coupon-service
  🌿 브랜치: feature/task-coupon-service
  📋 상태 기록: .team-claude/state/task-coupon-service.json
  📝 리뷰 기록: .team-claude/reviews/task-coupon-service/

계속하시겠습니까? [Y/n]
```

### 실행 결과

```
✅ task-coupon-service 정리 완료

  ✅ Worktree 제거됨
  ✅ 로컬 브랜치 삭제됨
  ✅ 원격 브랜치 삭제됨
  ✅ 상태 기록 아카이브됨

정리된 공간: 45MB
```

---

## 완료된 것만 정리

### /team-claude:cleanup --completed

```
🧹 완료된 Task 정리

정리 대상:
  ✅ task-coupon-service (merged)
  ✅ task-admin-ui (merged)
  ✅ task-coupon-repository (merged)

제외 (진행 중):
  🔄 task-api-endpoint (running)
  ⏳ task-integration-test (pending)

3개 Task를 정리하시겠습니까? [Y/n]
```

### 실행 결과

```
✅ 완료된 Task 정리 완료

정리됨:
  ✅ task-coupon-service
  ✅ task-admin-ui
  ✅ task-coupon-repository

정리된 공간: 128MB
남은 worktree: 2개
```

---

## 모든 것 정리

### /team-claude:cleanup --all

```
⚠️ 모든 worktree 정리

정리 대상:
  ✅ task-coupon-service (merged)
  ✅ task-admin-ui (merged)
  ✅ task-coupon-repository (merged)
  🔄 task-api-endpoint (running)
  ⏳ task-integration-test (pending)

⚠️ 경고: 진행 중인 작업도 포함됩니다!
  - task-api-endpoint는 현재 실행 중입니다.
  - 진행 중인 작업이 손실될 수 있습니다.

정말 모든 Task를 정리하시겠습니까? [y/N]
```

### 확인 후 실행

```
⚠️ 진행 중인 Worker 종료 중...

  task-api-endpoint: Worker 종료됨

✅ 모든 Task 정리 완료

정리됨:
  ✅ task-coupon-service
  ✅ task-admin-ui
  ✅ task-coupon-repository
  ✅ task-api-endpoint (진행 중이었음)
  ✅ task-integration-test

정리된 공간: 215MB
남은 worktree: 0개
```

---

## 정리 명령어 상세

### Worktree 제거

```bash
git worktree remove ../worktrees/task-coupon-service --force
```

### 브랜치 삭제

```bash
# 로컬 브랜치
git branch -D feature/task-coupon-service

# 원격 브랜치
git push origin --delete feature/task-coupon-service
```

### 상태 아카이브

상태 기록은 삭제하지 않고 아카이브합니다:

```bash
mv .team-claude/state/task-coupon-service.json \
   .team-claude/archive/task-coupon-service-$(date +%Y%m%d).json
```

---

## 선택적 정리

### 브랜치만 유지

```
🧹 정리 대상: task-coupon-service

정리할 항목을 선택하세요:
  [x] Worktree
  [ ] 로컬 브랜치
  [ ] 원격 브랜치
  [x] 상태 기록
  [x] 리뷰 기록
```

### Dry-run 모드 (미구현)

```bash
/team-claude:cleanup --completed --dry-run
```

```
🧹 정리 대상 (dry-run, 실제 삭제 안함)

삭제 예정:
  📁 ../worktrees/task-coupon-service (45MB)
  📁 ../worktrees/task-admin-ui (38MB)
  🌿 feature/task-coupon-service
  🌿 feature/task-admin-ui

총 정리 예정: 83MB

실제 정리: /team-claude:cleanup --completed
```

---

## 복구

### 브랜치 복구

```bash
# reflog에서 복구
git reflog
git checkout -b feature/task-coupon-service abc1234
```

### Worktree 재생성

```bash
git worktree add ../worktrees/task-coupon-service feature/task-coupon-service
```

---

## 에러 처리

### Worktree 사용 중

```
❌ Worktree 제거 실패: task-api-endpoint

원인: 현재 사용 중인 worktree입니다.

해결 방법:
  1. Worker 종료 후 재시도
  2. 강제 제거: git worktree remove --force ../worktrees/task-api-endpoint
```

### 브랜치 삭제 실패

```
⚠️ 원격 브랜치 삭제 실패: feature/task-coupon-service

원인: 권한 부족 또는 보호된 브랜치

로컬 정리는 완료되었습니다.
원격 브랜치는 수동으로 삭제해주세요.
```

### 정리할 것 없음

```
✅ 정리할 worktree가 없습니다.

현재 상태:
  - 실행 중: 0
  - 대기 중: 0
  - 완료됨: 0
```
