---
description: 구현 위임 - 확정된 Checkpoint를 자율 에이전트에게 위임하여 자동 구현/검증
argument-hint: "<checkpoint-id> | --session <session-id> [--all]"
allowed-tools: ["Task", "Bash", "Read", "Write", "Glob", "Grep", "AskUserQuestion"]
---

# Delegate Command

> **먼저 읽기**: `${CLAUDE_PLUGIN_ROOT}/INFRASTRUCTURE.md`

---

## IMMEDIATE PREREQUISITES CHECK

**모든 동작 전에 이것을 실행하세요:**

```bash
# 1. 워크플로우 상태 확인
tc state require checkpoints_approved
if [[ $? -ne 0 ]]; then
  echo "❌ Checkpoint가 아직 승인되지 않았습니다."
  echo "'/team-claude:architect'에서 Checkpoint를 승인하세요."
  exit 1
fi

# 2. 서버 실행 보장
SERVER_STATUS=$(tc server ensure)
if [[ "$SERVER_STATUS" == "started" ]]; then
  echo "🚀 서버가 자동으로 시작되었습니다. (http://localhost:7890)"
fi
```

**Prerequisites 실패 시 STOP하고 사용자에게 안내하세요.**

---

## EXECUTION PROCEDURE

### Step 1: Checkpoint 정보 로드

```bash
SESSION_ID="<세션 ID>"
CHECKPOINT_ID="<체크포인트 ID>"

# 세션 정보 확인
tc session show ${SESSION_ID}

# Checkpoint 파일 읽기
cat .team-claude/sessions/${SESSION_ID}/checkpoints/${CHECKPOINT_ID}.json
```

Checkpoint JSON 구조:
```json
{
  "id": "coupon-service",
  "name": "쿠폰 서비스 로직",
  "description": "쿠폰 검증 및 적용 로직 구현",
  "criteria": ["기준1", "기준2", "..."],
  "validation": {
    "command": "pytest tests/test_coupon_service.py",
    "expected": "passed"
  },
  "dependencies": ["coupon-model"]
}
```

### Step 2: Git Worktree 생성

```bash
# tc worktree가 자동으로 처리:
# - 디렉토리 생성
# - 브랜치 생성/체크아웃
# - worktree 설정

WORKTREE_PATH=$(tc worktree create ${CHECKPOINT_ID})
echo "Worktree 생성됨: ${WORKTREE_PATH}"
```

### Step 3: CLAUDE.md 생성

Worktree 루트에 Worker Claude 지시서를 작성합니다:

```markdown
# Task: {checkpoint-id}

## Objective
{checkpoint.description}

## Success Criteria
- [ ] {criteria[0]}
- [ ] {criteria[1]}
- [ ] ...

## Validation
\`\`\`bash
{validation.command}
\`\`\`
예상 결과: `{validation.expected}`

## Context
- 아키텍처: .team-claude/sessions/{session-id}/specs/architecture.md
- 계약: .team-claude/sessions/{session-id}/specs/contracts.md

## Instructions
1. Success Criteria를 순서대로 구현
2. Validation 명령어로 확인
3. 모든 테스트 통과 시 커밋
```

Write 도구로 `${WORKTREE_PATH}/CLAUDE.md`에 저장합니다.

### Step 4: 서버에 Task 등록

```bash
curl -X POST http://localhost:7890/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "checkpoint_id": "{checkpoint-id}",
    "checkpoint_name": "{checkpoint.name}",
    "worktree_path": "{absolute-path-to-worktree}",
    "validation_command": "{validation.command}",
    "max_retries": 3
  }'
```

응답:
```json
{ "task_id": "abc123", "status": "queued" }
```

### Step 5: 상태 업데이트

```bash
# 세션 상태 업데이트
tc session update ${SESSION_ID} status delegating

# 워크플로우 상태 업데이트
tc state transition delegating
```

### Step 6: 진행 모니터링

```bash
# 상태 확인
curl -s http://localhost:7890/tasks/{task_id}

# 실시간 스트리밍
curl -N http://localhost:7890/tasks/{task_id}/stream
```

### Step 7: 결과 처리

**성공 시:**
```
✅ {checkpoint-id} 구현 완료

  시도 횟수: N회
  브랜치: team-claude/{checkpoint-id}

  다음 단계:
  - 다음 checkpoint 위임
  - 또는 /team-claude:merge
```

**에스컬레이션 시:**
```
⚠️ {checkpoint-id} 에스컬레이션

  시도 횟수: 3/3 (최대 도달)

  권장 조치:
  1. 설계 재검토: /team-claude:architect --resume {session-id}
  2. 수동 구현: 직접 worktree에서 작업
```

---

## 사용법

```bash
# 특정 Checkpoint 위임
/team-claude:delegate coupon-service

# 세션의 모든 Checkpoint 병렬 위임
/team-claude:delegate --session abc12345 --all

# 특정 세션의 특정 Checkpoint 위임
/team-claude:delegate --session abc12345 coupon-api

# 실패한 Checkpoint 재시도
/team-claude:delegate --retry coupon-service
```

---

## 스크립트 도구

```bash
# Worktree 관리
tc worktree create {checkpoint-id}
tc worktree list
tc worktree delete {checkpoint-id}
tc worktree cleanup

# 세션 관리
tc session show {session-id}
tc session update {session-id} status delegating

# 상태 관리
tc state check
tc state transition delegating

# 서버 관리
tc server ensure
tc server status
```

---

## 출력 예시

### 위임 시작

```
🚀 구현 위임 시작

  세션: abc12345 (쿠폰 할인 기능)

━━━ 실행 계획 ━━━

  Round 1 (병렬):
    • coupon-model - 쿠폰 도메인 모델

  Round 2:
    • coupon-service - 쿠폰 서비스 로직

  Round 3:
    • coupon-api - 쿠폰 API 엔드포인트

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

⏳ Round 1 시작...
```

### 완료

```
✅ 구현 위임 완료: abc12345

━━━ 결과 ━━━

  ✅ coupon-model       1회 시도, 통과
  ✅ coupon-service     3회 시도, 통과
  ✅ coupon-api         1회 시도, 통과

━━━ 다음 단계 ━━━

  /team-claude:merge --session abc12345
```

---

## Reference

### 핵심 원칙

```
┌─────────────────────────────────────────────────────────────────┐
│  AUTONOMOUS DELEGATION                                          │
│                                                                 │
│  인간: 위임 시작만 결정, 에스컬레이션 시 개입                   │
│  에이전트: 구현 방법 자율 결정, 자동 검증/재시도                │
└─────────────────────────────────────────────────────────────────┘
```

### 의존성 기반 실행 순서

```
Round 1 (병렬):
  ├── coupon-model (의존성 없음)
  └── ...

Round 2 (Round 1 완료 후):
  ├── coupon-service (depends: coupon-model)
  └── ...

Round 3 (Round 2 완료 후):
  └── coupon-api (depends: coupon-service)
```

### 자동 피드백 루프

실패 시 자동으로 피드백을 생성하고 Worker에게 전달:

```markdown
## 🔄 자동 피드백 (Iteration 2/5)

### 실패한 기준
❌ "중복 적용 시 에러 발생"

### 테스트 출력
AssertionError: expected 200 to equal 409

### 분석
중복 적용 검사 로직이 구현되지 않았습니다.

### 제안 수정
CouponService.apply()에서 이미 적용된 쿠폰 체크 필요
```

### 파일 구조

```
.team-claude/
└── sessions/{session-id}/
    └── delegations/
        ├── status.json           # 전체 위임 상태
        └── {checkpoint-id}/
            ├── status.json       # 개별 상태
            ├── iterations/
            │   ├── 1/
            │   │   ├── prompt.md
            │   │   ├── result.json
            │   │   └── feedback.md
            │   └── ...
            └── final-result.json
```

### 설정

```yaml
# .claude/team-claude.yaml
delegation:
  autoValidateOnComplete: true
  autoRetryOnFail: true
  maxRetries: 3
  retryDelay: 5000
  parallelWorkers: 3
```
