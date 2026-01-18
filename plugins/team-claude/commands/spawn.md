---
name: team-claude:spawn
description: Worker Claude 생성 및 실행 - Git worktree 생성, 터미널 세션 생성, Claude 실행
argument-hint: "<task-id> [task-id...]"
allowed-tools: ["Bash", "Read", "Write", "Glob"]
---

# Team Claude Worker 생성 커맨드

Task를 실행할 Worker Claude를 생성하고 실행합니다.

## 사용법

```bash
# 단일 Task 실행
/team-claude:spawn task-coupon-service

# 복수 Task 병렬 실행
/team-claude:spawn task-coupon-service task-coupon-repository task-admin-ui
```

## Arguments

| Argument | 필수 | 설명 |
|----------|------|------|
| task-id | O | 실행할 Task ID (복수 가능) |

---

## 실행 절차

```
/team-claude:spawn task-a task-b task-d
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                     1. Task 스펙 검증                          │
│                                                               │
│  • .team-claude/specs/tasks/{task-id}.md 존재 확인            │
│  • 의존성 충족 여부 확인                                       │
│  • 동시 실행 제한 확인 (maxConcurrent)                        │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                     2. Worktree 생성                          │
│                                                               │
│  git worktree add ../worktrees/task-a -b feature/task-a      │
│  git worktree add ../worktrees/task-b -b feature/task-b      │
│  git worktree add ../worktrees/task-d -b feature/task-d      │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                     3. Task 스펙 복사                          │
│                                                               │
│  • Task 스펙 → ../worktrees/task-a/CLAUDE.md                 │
│  • Contract 파일들 → ../worktrees/task-a/.team-claude/       │
│  • Worker용 hooks.json 복사                                   │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                     4. 터미널 세션 생성                        │
│                                                               │
│  설정에 따라:                                                 │
│  • iTerm2: AppleScript로 새 탭/분할 생성                      │
│  • tmux: new-window 또는 split-window                        │
│  • Terminal.app: AppleScript로 새 탭                         │
│  • manual: 명령어만 출력                                      │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                     5. Claude 실행                            │
│                                                               │
│  cd ../worktrees/task-a && claude --resume                   │
│                                                               │
│  CLAUDE.md를 읽고 Task 수행 시작                              │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                     6. 상태 등록                              │
│                                                               │
│  .team-claude/state/workers.json에 상태 기록                 │
│  {                                                            │
│    "task-a": {                                                │
│      "status": "running",                                     │
│      "worktree": "../worktrees/task-a",                      │
│      "branch": "feature/task-a",                             │
│      "startedAt": "2024-01-15T10:00:00Z"                     │
│    }                                                          │
│  }                                                            │
└───────────────────────────────────────────────────────────────┘
```

---

## Step 1: Task 스펙 검증

### 존재 확인

```bash
# Task 스펙 파일 존재 확인
ls .team-claude/specs/tasks/{task-id}.md
```

존재하지 않으면:

```
❌ Task를 찾을 수 없습니다: task-unknown

사용 가능한 Task:
  - task-coupon-service
  - task-coupon-repository
  - task-api-endpoint

먼저 /team-claude:plan 으로 Task를 생성해주세요.
```

### 의존성 확인

Task 스펙의 의존성 섹션을 확인:

```
⚠️ 의존성 미충족: task-api-endpoint

필요한 Task가 완료되지 않았습니다:
  - task-coupon-service (running)
  - task-coupon-repository (pending)

병렬 실행 가능한 Task: task-admin-ui
```

### 동시 실행 제한

```
⚠️ 동시 실행 제한 초과

현재 실행 중: 5개 (최대: 5)
  - task-a (running)
  - task-b (running)
  - task-c (running)
  - task-d (running)
  - task-e (running)

완료 대기 중인 Task가 끝나면 시작됩니다.
또는 /team-claude:config set worker.maxConcurrent 8
```

---

## Step 2: Worktree 생성

### Git worktree 명령어

```bash
# worktree 루트 디렉토리 생성 (없으면)
mkdir -p ../worktrees

# 각 Task별 worktree 생성
git worktree add ../worktrees/task-a -b feature/task-a
git worktree add ../worktrees/task-b -b feature/task-b
git worktree add ../worktrees/task-d -b feature/task-d
```

### 브랜치 네이밍

설정의 `worktree.branchPrefix`를 사용:

- 기본: `feature/{task-id}`
- 커스텀: `{branchPrefix}{task-id}`

### 에러 처리

```
❌ Worktree 생성 실패: task-a

원인: 브랜치 'feature/task-a'가 이미 존재합니다.

해결 방법:
  1. 기존 브랜치 삭제: git branch -D feature/task-a
  2. 다른 브랜치명 사용: /team-claude:config set worktree.branchPrefix "wip/"
```

---

## Step 3: Task 스펙 복사

### CLAUDE.md 생성

Task 스펙을 Worker가 읽을 수 있는 형식으로 변환:

```markdown
# Worker Task: task-coupon-service

이 작업은 Team Claude 시스템에 의해 생성되었습니다.
아래 스펙에 따라 구현을 진행해주세요.

---

## Task 스펙

[.team-claude/specs/tasks/task-coupon-service.md 내용]

---

## Contract

[.team-claude/specs/contracts/coupon-service.ts 내용]

---

## 완료 시

작업이 완료되면:
1. 모든 테스트 통과 확인
2. lint/typecheck 통과 확인
3. 커밋 생성
4. "/team-claude:done" 이라고 입력

완료 hook이 Main Claude에 알림을 보냅니다.
```

### 관련 파일 복사

```bash
# Contract 파일들 복사
mkdir -p ../worktrees/task-a/.team-claude/contracts
cp .team-claude/specs/contracts/* ../worktrees/task-a/.team-claude/contracts/

# Hook 설정 복사
cp .team-claude/hooks/hooks.json ../worktrees/task-a/.claude/hooks.json
cp .team-claude/hooks/*.sh ../worktrees/task-a/.team-claude/hooks/
```

---

## Step 4: 터미널 세션 생성

### iTerm2 (AppleScript)

```applescript
tell application "iTerm2"
    tell current window
        -- 새 탭 생성
        create tab with default profile
        tell current session
            write text "cd ../worktrees/task-a && claude"
        end tell
    end tell
end tell
```

### tmux

```bash
# 새 윈도우 생성
tmux new-window -n "task-a" -c "../worktrees/task-a"
tmux send-keys "claude" Enter

# 또는 pane 분할 (split 레이아웃)
tmux split-window -h -c "../worktrees/task-a"
tmux send-keys "claude" Enter
```

### Terminal.app (AppleScript)

```applescript
tell application "Terminal"
    do script "cd ../worktrees/task-a && claude"
end tell
```

### Manual 모드

```
📝 수동 모드: 다음 명령어를 각 터미널에서 실행해주세요.

  [터미널 1] cd ../worktrees/task-a && claude
  [터미널 2] cd ../worktrees/task-b && claude
  [터미널 3] cd ../worktrees/task-d && claude
```

---

## Step 5: Claude 실행

### 실행 명령어

```bash
cd ../worktrees/task-a && claude
```

Worker Claude는:
1. CLAUDE.md를 읽고 Task 컨텍스트 파악
2. Contract를 기반으로 구현 시작
3. 완료 조건 충족까지 반복
4. 완료 시 Stop hook 실행 → Main에 알림

---

## Step 6: 상태 등록

### workers.json 구조

```json
{
  "task-a": {
    "status": "running",
    "worktree": "../worktrees/task-a",
    "branch": "feature/task-a",
    "startedAt": "2024-01-15T10:00:00Z",
    "pid": 12345
  },
  "task-b": {
    "status": "running",
    "worktree": "../worktrees/task-b",
    "branch": "feature/task-b",
    "startedAt": "2024-01-15T10:00:05Z",
    "pid": 12346
  }
}
```

### 상태 값

| Status | 설명 |
|--------|------|
| pending | 대기 중 (의존성 미충족) |
| running | 실행 중 |
| waiting | 질문 대기 중 |
| completed | 완료됨 |
| failed | 실패 |

---

## 최종 출력

```
🚀 Worker 3개 시작

  [탭 2] task-coupon-service
         worktree: ../worktrees/task-coupon-service
         branch: feature/task-coupon-service

  [탭 3] task-coupon-repository
         worktree: ../worktrees/task-coupon-repository
         branch: feature/task-coupon-repository

  [탭 4] task-admin-ui
         worktree: ../worktrees/task-admin-ui
         branch: feature/task-admin-ui

완료되면 시스템 알림을 보내드립니다.

상태 확인: /team-claude:status
리뷰 요청: /team-claude:review <task-id>
```

---

## 에러 처리

### Git worktree 실패

```
❌ Worktree 생성 실패

원인: uncommitted changes가 있습니다.

해결 방법:
  git stash  # 임시 저장
  또는
  git commit -am "WIP"  # 커밋
```

### 터미널 실행 실패

```
❌ 터미널 세션 생성 실패

원인: iTerm2가 설치되어 있지 않습니다.

해결 방법:
  1. iTerm2 설치
  2. 다른 터미널 설정: /team-claude:setup terminal
```

### 최대 동시 실행 초과

```
⚠️ 동시 실행 제한

현재 5개 Worker가 실행 중입니다 (최대: 5)

대기열에 추가됨: task-api-endpoint
다른 Worker 완료 시 자동 시작됩니다.

또는 제한 변경: /team-claude:config set worker.maxConcurrent 8
```
