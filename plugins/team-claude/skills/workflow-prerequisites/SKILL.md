---
name: workflow-prerequisites
description: Team Claude 커맨드 실행 전 전제조건을 확인하는 공유 로직
arch-ignore: true
---

# Workflow Prerequisites Skill

이 스킬은 Team Claude 커맨드 실행 전 전제조건을 확인하는 공유 로직을 제공합니다.

## 사용 시나리오

- 커맨드 실행 전 필수 조건 확인
- 설정 파일 존재 여부 체크
- 서버 상태 확인
- 세션/Checkpoint 승인 상태 확인

---

## 전제조건 체크

### tc CLI 사용

tc CLI를 통해 전제조건을 확인합니다:

```bash
# 설정 파일 존재 확인
tc config show &>/dev/null

# 상태 파일 존재 확인
tc state check &>/dev/null

# 서버 healthy 확인
tc server ensure

# 세션 존재 확인
tc session show "abc12345"

# Checkpoint 승인 확인
tc state get phase  # checkpoints_approved 확인
```

---

## 커맨드별 전제조건

### /team-claude:setup

```bash
# 전제조건 없음 (첫 실행 가능)
```

### /team-claude:architect

```bash
# 1. 설정 파일 존재
tc config show &>/dev/null || {
  echo "'/team-claude:setup'을 먼저 실행하세요."
  exit 1
}

# 2. 상태 파일 존재
tc state check &>/dev/null || {
  echo "'/team-claude:setup'을 먼저 실행하세요."
  exit 1
}
```

### /team-claude:delegate

```bash
# 1. 워크플로우 상태 확인
tc state require checkpoints_approved

# 2. 서버 실행 보장
tc server ensure
```

### /team-claude:checkpoint

```bash
# 1. 설정 파일 존재
tc config show &>/dev/null

# 2. 세션 존재 (세션 지정 시)
tc session show ${SESSION_ID}
```

### /team-claude:merge

```bash
# 1. 설정 파일 존재
tc config show &>/dev/null

# 2. 세션 존재
tc session show ${SESSION_ID}

# 3. 위임 완료 상태 (권장)
tc state get phase  # delegating 또는 이후
```

---

## 상태 전이 다이어그램

```
idle ──────────────────────────────────────────────────────────────▶
  │                                                                 │
  │ /team-claude:setup                                              │
  ▼                                                                 │
setup ─────────────────────────────────────────────────────────────▶
  │                                                                 │
  │ /team-claude:architect                                          │
  ▼                                                                 │
designing ─────────────────────────────────────────────────────────▶
  │                                                                 │
  │ Checkpoint 승인                                                 │
  ▼                                                                 │
checkpoints_approved ──────────────────────────────────────────────▶
  │                                                                 │
  │ /team-claude:delegate                                           │
  ▼                                                                 │
delegating ────────────────────────────────────────────────────────▶
  │                                                                 │
  │ 모든 Worker 완료                                                │
  ▼                                                                 │
merging ───────────────────────────────────────────────────────────▶
  │                                                                 │
  │ /team-claude:merge 완료                                         │
  ▼                                                                 │
completed ◀────────────────────────────────────────────────────────┘
```

---

## 에러 메시지 템플릿

### 설정 파일 없음

```
❌ 설정 파일이 없습니다.

'/team-claude:setup'을 먼저 실행하세요.
```

### 상태 파일 없음

```
❌ 상태 파일이 없습니다.

'/team-claude:setup'을 먼저 실행하세요.
```

### 서버 미실행

```
❌ 서버가 실행 중이지 않습니다.

자동 시작 시도 중...
(실패 시) '/team-claude:setup server'를 실행하세요.
```

### Checkpoint 미승인

```
❌ Checkpoint가 승인되지 않았습니다.

'/team-claude:architect --resume {session-id}'에서 승인하세요.
```

### 세션 없음

```
❌ 세션을 찾을 수 없습니다: {session-id}

현재 세션 목록:
  - abc12345: 쿠폰 할인 기능 (설계 중)
  - def67890: 알림 시스템 (완료)

'/team-claude:architect --list'로 전체 목록을 확인하세요.
```

---

## 복합 체크 함수 사용

```bash
# tc doctor를 사용하여 전제조건 확인
tc doctor

# 또는 개별 체크
tc config show &>/dev/null && \
tc state check &>/dev/null && \
tc server ensure
```

---

## 상태 표시

```bash
# 전체 상태 출력
tc doctor

# 출력 예:
# ━━━ Prerequisites Status ━━━
#
#   ✅ Config: .claude/team-claude.yaml
#   ✅ State: .team-claude/state/workflow.json
#   ✅ Server Binary: ~/.claude/team-claude-server
#   ✅ Server: Running (healthy)
#   ✅ Session: abc12345
#   ✅ Checkpoints: Approved
```
