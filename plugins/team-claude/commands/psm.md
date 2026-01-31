---
name: team-claude:psm
description: PSM (Parallel Session Manager) - git worktree 기반 병렬 세션 관리
argument-hint: "new <name> | list | switch <name> | parallel <names...> | status | cleanup"
allowed-tools: ["Bash", "Read", "Write", "Glob", "Grep", "AskUserQuestion"]
---

# PSM (Parallel Session Manager)

> **먼저 읽기**: `${CLAUDE_PLUGIN_ROOT}/INFRASTRUCTURE.md`

git worktree 기반으로 여러 세션을 병렬로 관리합니다.

---

## 핵심 개념

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  PSM: Parallel Session Manager                                               │
│                                                                              │
│  Main Repository                                                             │
│  ├── .git/                                                                  │
│  ├── src/                                                                   │
│  └── .team-claude/                                                          │
│      └── worktrees/                                                         │
│          ├── feature-a/     ← 세션 A (독립 worktree)                        │
│          │   ├── src/                                                       │
│          │   └── CLAUDE.md                                                  │
│          ├── feature-b/     ← 세션 B (독립 worktree)                        │
│          │   ├── src/                                                       │
│          │   └── CLAUDE.md                                                  │
│          └── feature-c/     ← 세션 C (독립 worktree)                        │
│              ├── src/                                                       │
│              └── CLAUDE.md                                                  │
│                                                                              │
│  각 세션:                                                                   │
│  • 독립된 git worktree                                                      │
│  • 독립된 브랜치                                                            │
│  • 독립된 Claude 에이전트                                                   │
│  • 병렬 실행 가능                                                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 사용법

```bash
# 새 세션 생성
/team-claude:psm new coupon-feature

# 기존 세션 기반 생성
/team-claude:psm new notification-v2 --from notification

# 세션 목록
/team-claude:psm list

# 세션 상태 확인
/team-claude:psm status
/team-claude:psm status coupon-feature

# 세션 전환
/team-claude:psm switch coupon-feature

# 병렬 실행
/team-claude:psm parallel coupon-feature notification-v2 user-profile

# 세션 정리
/team-claude:psm cleanup                    # 완료된 것만
/team-claude:psm cleanup coupon-feature     # 특정 세션
/team-claude:psm cleanup --all              # 모든 세션
```

---

## 명령어 상세

### `new` - 새 세션 생성

```bash
/team-claude:psm new <session-name> [--from <existing-session>]
```

**동작:**
1. git worktree 생성
2. 브랜치 생성 (`team-claude/<session-name>`)
3. 세션 메타데이터 초기화
4. CLAUDE.md 템플릿 생성

**예시:**
```
🆕 새 세션 생성: coupon-feature

  Worktree: .team-claude/worktrees/coupon-feature
  브랜치: team-claude/coupon-feature
  상태: initialized

  다음 단계:
    cd .team-claude/worktrees/coupon-feature
    또는
    /team-claude:psm switch coupon-feature
```

### `list` - 세션 목록

```bash
/team-claude:psm list [--status <status>]
```

**출력:**
```
━━━ PSM Sessions ━━━

  NAME              STATUS        BRANCH                      PROGRESS
  ─────────────────────────────────────────────────────────────────────
  coupon-feature    🔄 active     team-claude/coupon-feature  3/5 (60%)
  notification-v2   ⏸️ paused     team-claude/notification-v2  0/3 (0%)
  user-profile      ✅ complete   team-claude/user-profile     4/4 (100%)

  Total: 3 sessions (1 active, 1 paused, 1 complete)
```

### `status` - 상태 확인

```bash
/team-claude:psm status [session-name]
```

**출력 (전체):**
```
━━━ PSM Status ━━━

  Active Sessions: 1
  Paused Sessions: 1
  Complete Sessions: 1

━━━ Resource Usage ━━━

  Worktrees: 3
  Disk Usage: 450MB
  Running Workers: 2

━━━ Recent Activity ━━━

  [10:30] coupon-feature: checkpoint coupon-service passed
  [10:25] coupon-feature: checkpoint coupon-model passed
  [09:15] user-profile: all checkpoints complete
```

**출력 (특정 세션):**
```
━━━ Session: coupon-feature ━━━

  상태: 🔄 active
  브랜치: team-claude/coupon-feature
  Worktree: .team-claude/worktrees/coupon-feature

━━━ Checkpoints ━━━

  ✅ coupon-model      완료 (2회 시도)
  ✅ coupon-service    완료 (1회 시도)
  🔄 coupon-api        진행 중 (3/5회)
  ⏸️ coupon-integration 대기 중

━━━ Recent Logs ━━━

  [10:35] coupon-api: validation failed (attempt 3)
  [10:30] coupon-service: passed
  [10:25] coupon-model: passed
```

### `switch` - 세션 전환

```bash
/team-claude:psm switch <session-name>
```

**동작:**
1. 해당 worktree 경로로 컨텍스트 전환
2. 세션 상태 로드
3. 이전 진행 상황 표시

**출력:**
```
🔄 세션 전환: coupon-feature

  Worktree: .team-claude/worktrees/coupon-feature
  상태: 3/5 checkpoints 완료

  현재 진행 중:
    coupon-api (3/5 시도)

  컨텍스트:
    .team-claude/sessions/abc12345/specs/architecture.md
    .team-claude/sessions/abc12345/specs/contracts.md
```

### `parallel` - 병렬 실행

```bash
/team-claude:psm parallel <session1> <session2> [session3...]
```

**동작:**
1. 각 세션의 독립성 확인
2. 병렬 Worker 생성
3. 실시간 진행 상황 모니터링

**출력:**
```
🚀 병렬 실행 시작

  Sessions: 3
  Mode: parallel

━━━ Execution Plan ━━━

  Session              Checkpoints   Workers
  ─────────────────────────────────────────────
  coupon-feature       2 remaining   1
  notification-v2      3 remaining   1
  user-profile         0 remaining   (skip)

━━━ Progress ━━━

  [coupon-feature]      ████████░░ 80%  coupon-api
  [notification-v2]     ███░░░░░░░ 30%  notif-service

  Elapsed: 5m 23s
  Estimated: 8m remaining
```

### `cleanup` - 정리

```bash
/team-claude:psm cleanup [session-name] [--all] [--force]
```

**동작:**
1. Worktree 삭제
2. 브랜치 삭제 (선택적)
3. 메타데이터 정리

**출력:**
```
🧹 세션 정리

  정리 대상:
    ✅ user-profile (완료됨)

  건너뛴 세션:
    ⏸️ coupon-feature (진행 중)
    ⏸️ notification-v2 (진행 중)

  정리 완료: 1 세션
  해제 용량: 150MB
```

---

## 스크립트

```bash
SCRIPTS="${CLAUDE_PLUGIN_ROOT}/scripts"

# 새 세션
${SCRIPTS}/tc-psm.sh new "feature-name"
${SCRIPTS}/tc-psm.sh new "feature-v2" --from "feature"

# 목록
${SCRIPTS}/tc-psm.sh list
${SCRIPTS}/tc-psm.sh list --status active

# 상태
${SCRIPTS}/tc-psm.sh status
${SCRIPTS}/tc-psm.sh status "feature-name"

# 전환
${SCRIPTS}/tc-psm.sh switch "feature-name"

# 병렬 실행
${SCRIPTS}/tc-psm.sh parallel session1 session2 session3

# 정리
${SCRIPTS}/tc-psm.sh cleanup
${SCRIPTS}/tc-psm.sh cleanup "feature-name"
${SCRIPTS}/tc-psm.sh cleanup --all
```

---

## 데이터 구조

### 세션 메타데이터

```json
// .team-claude/sessions/{session-name}/psm.json
{
  "name": "coupon-feature",
  "status": "active",
  "worktreePath": ".team-claude/worktrees/coupon-feature",
  "branch": "team-claude/coupon-feature",
  "createdAt": "2024-01-15T10:00:00Z",
  "updatedAt": "2024-01-15T12:30:00Z",
  "progress": {
    "total": 5,
    "completed": 3,
    "inProgress": 1,
    "pending": 1
  },
  "checkpoints": [
    { "id": "coupon-model", "status": "complete", "attempts": 2 },
    { "id": "coupon-service", "status": "complete", "attempts": 1 },
    { "id": "coupon-api", "status": "in_progress", "attempts": 3 },
    { "id": "coupon-validation", "status": "pending", "attempts": 0 },
    { "id": "coupon-integration", "status": "pending", "attempts": 0 }
  ],
  "linkedSession": "abc12345"
}
```

### PSM 인덱스

```json
// .team-claude/psm-index.json
{
  "sessions": [
    {
      "name": "coupon-feature",
      "status": "active",
      "progress": "3/5"
    },
    {
      "name": "notification-v2",
      "status": "paused",
      "progress": "0/3"
    }
  ],
  "settings": {
    "parallelLimit": 4,
    "autoCleanup": true
  }
}
```

---

## 병렬 실행 전략

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Parallel Execution Strategy                                                 │
│                                                                              │
│  1. 독립성 검증                                                             │
│     • 파일 충돌 검사                                                        │
│     • 의존성 검사                                                           │
│                                                                              │
│  2. 리소스 할당                                                             │
│     • Worker 수 결정 (parallelLimit)                                        │
│     • 우선순위 기반 스케줄링                                                │
│                                                                              │
│  3. 실행                                                                    │
│     Session A ─────────────▶ Worker 1 ──▶ Result A                         │
│     Session B ─────────────▶ Worker 2 ──▶ Result B                         │
│     Session C ─────────────▶ Worker 3 ──▶ Result C                         │
│                                                                              │
│  4. 동기화                                                                  │
│     • 각 세션 완료 시 알림                                                  │
│     • 에러 발생 시 해당 세션만 중단                                        │
│     • 전체 완료 시 통합 보고                                                │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 충돌 방지

### 파일 충돌 검사

```bash
# 병렬 실행 전 검사
check_conflicts() {
  local sessions=("$@")

  for i in "${!sessions[@]}"; do
    for j in "${!sessions[@]}"; do
      if [[ $i -lt $j ]]; then
        # 두 세션의 변경 파일 비교
        files_a=$(get_session_files "${sessions[$i]}")
        files_b=$(get_session_files "${sessions[$j]}")

        overlap=$(comm -12 <(echo "$files_a") <(echo "$files_b"))

        if [[ -n "$overlap" ]]; then
          warn "충돌 가능: ${sessions[$i]} ↔ ${sessions[$j]}"
          echo "$overlap"
        fi
      fi
    done
  done
}
```

### 해결 전략

```
충돌 감지 시:
1. 경고 표시
2. 사용자에게 선택 요청:
   • 순차 실행으로 전환
   • 충돌 파일 제외하고 병렬 실행
   • 그대로 병렬 실행 (위험)
```

---

## 사용 시나리오

### 시나리오 1: 독립 기능 병렬 개발

```bash
# 1. 세 개의 독립 기능 세션 생성
/team-claude:psm new auth-system
/team-claude:psm new payment-gateway
/team-claude:psm new notification-service

# 2. 각 세션에서 스펙 설계 (순차)
/team-claude:psm switch auth-system
/team-claude:architect "OAuth 2.0 인증 시스템"

/team-claude:psm switch payment-gateway
/team-claude:architect "결제 게이트웨이 통합"

/team-claude:psm switch notification-service
/team-claude:architect "실시간 알림 시스템"

# 3. 병렬 구현
/team-claude:psm parallel auth-system payment-gateway notification-service

# 4. 상태 모니터링
/team-claude:psm status
```

### 시나리오 2: 기능 브랜치 분할

```bash
# 1. 메인 기능 세션
/team-claude:psm new coupon-feature

# 2. 스펙 설계 후 하위 기능으로 분할
/team-claude:psm new coupon-model --from coupon-feature
/team-claude:psm new coupon-service --from coupon-feature
/team-claude:psm new coupon-api --from coupon-feature

# 3. 병렬 구현
/team-claude:psm parallel coupon-model coupon-service coupon-api

# 4. 순차 머지 (의존성 순서)
/team-claude:merge coupon-model
/team-claude:merge coupon-service
/team-claude:merge coupon-api
```

---

## 설정

```yaml
# .claude/team-claude.yaml
psm:
  # 최대 병렬 세션 수
  parallelLimit: 4

  # 완료 후 자동 정리
  autoCleanup: true

  # 정리 대상 상태
  cleanupStatuses:
    - complete
    - abandoned

  # 충돌 검사
  conflictCheck:
    enabled: true
    action: warn  # warn | block | ignore

  # 리소스 제한
  resources:
    maxDiskUsage: 2GB
    maxWorktrees: 10
```
