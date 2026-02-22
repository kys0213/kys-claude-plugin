---
name: barrier-sync
description: Use this skill when running 2+ background Tasks in parallel and need to wait for all to complete without consuming LLM turns. Provides FIFO-based barrier synchronization via SubagentStop hook.
allowed-tools: Bash, Read, Task
---

# Barrier Sync Skill

## Description

병렬 background Task를 실행한 뒤, **LLM 턴을 소모하지 않고** 전원 완료를 감지하는 barrier 패턴입니다.

**이 스킬이 필요한 상황:**

- background Task 2개 이상을 동시에 실행하고, 전부 끝난 뒤 결과를 통합해야 할 때
- background Bash 명령 여러 개의 완료를 동기화해야 할 때
- polling(주기적 Read) 없이 완료를 감지하고 싶을 때

**동작 원리:**

```
Main Agent                    Background
─────────────                 ──────────
1. Bash(background)           wait-for-tasks.sh (FIFO read, blocking)
   → run_in_background=true
2. Task(background) ×N        SubagentStop hook → signal-done.cjs
   → run_in_background=true      → FIFO write (unblock consumer)
3. Read(output_file)          ← stdout에 결과 출력됨
   → 턴 소모 없이 결과 획득
```

---

## 사전 조건

SubagentStop hook이 등록되어 있어야 합니다. `/barrier-setup`으로 자동 설정하거나, 수동으로 settings.json에 추가합니다:

```json
{
  "hooks": {
    "SubagentStop": [
      {
        "matcher": "",
        "hooks": [
          {
            "command": "node /path/to/plugins/barrier-sync/hooks/signal-done.cjs",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

---

## 사용 패턴

### Step 1: Barrier 시작 (background Bash)

```bash
# BARRIER_ID는 세션 내에서 고유해야 함
BARRIER_ID="review-barrier" bash /path/to/plugins/barrier-sync/scripts/wait-for-tasks.sh 3 300
```

파라미터:
- 첫 번째 인자: 대기할 Task 수 (필수)
- 두 번째 인자: 타임아웃 초 (기본: 300)
- `BARRIER_ID` 환경변수: barrier 식별자 (기본: `barrier-$$`)

**반드시 `run_in_background=true`로 실행합니다.** output_file 경로를 기억해둡니다.

### Step 2: Background Task 실행

```
Task(run_in_background=true) — "코드 리뷰 수행"
Task(run_in_background=true) — "테스트 실행"
Task(run_in_background=true) — "린트 검사"
```

각 Task가 완료되면 SubagentStop hook이 자동으로 signal-done.cjs를 호출하고, FIFO에 완료 신호를 기록합니다.

### Step 3: 결과 확인 (Read)

Step 1의 output_file을 Read합니다. 모든 Task가 완료되었으면 결과가 출력됩니다:

```
--- BARRIER COMPLETE (3/3) ---
agents: abc123 def456 ghi789

=== abc123 ===
코드 리뷰 결과: 3개 이슈 발견...

=== def456 ===
테스트 실행 결과: 42/42 통과...

=== ghi789 ===
린트 검사 결과: 0개 경고...
```

아직 완료되지 않았으면 output_file이 비어있습니다. 잠시 후 다시 Read합니다.

---

## 파라미터 레퍼런스

### wait-for-tasks.sh

| 파라미터 | 타입 | 필수 | 기본값 | 설명 |
|----------|------|------|--------|------|
| `$1` (expected_count) | int | O | - | 대기할 Task 수 |
| `$2` (timeout_sec) | int | X | 300 | 타임아웃 (초) |
| `BARRIER_ID` | env | X | `barrier-$$` | barrier 격리 식별자 |

### signal-done.cjs (자동 — 수동 호출 불필요)

SubagentStop hook으로 자동 실행됩니다. stdin으로 hook JSON을 받습니다:

```json
{
  "agent_id": "abc123",
  "last_assistant_message": "작업 결과..."
}
```

---

## 출력 형식

### 정상 완료 (exit 0)

```
--- BARRIER COMPLETE (N/N) ---
agents: <agent_id_1> <agent_id_2> ...

=== <agent_id_1> ===
<last_assistant_message 앞 500자>

=== <agent_id_2> ===
<last_assistant_message 앞 500자>
```

### 타임아웃 (exit 1)

```
--- BARRIER TIMEOUT (300s) M/N completed ---
agents: <완료된 agent_id들>

=== <agent_id> ===
<부분 결과>
```

---

## 주의사항

### 1. expected_count는 정확해야 합니다

실행할 background Task 수와 정확히 일치해야 합니다. 적으면 일찍 해제되고, 많으면 영원히 대기합니다 (타임아웃까지).

### 2. BARRIER_ID 충돌 방지

동시에 여러 barrier를 사용하면 각각 다른 BARRIER_ID를 지정합니다:

```bash
BARRIER_ID="review-batch" bash wait-for-tasks.sh 3
BARRIER_ID="test-batch" bash wait-for-tasks.sh 2
```

### 3. 완료 순서는 보장되지 않음

agents 목록의 순서는 Task 완료 순서입니다 (실행 순서 아님).

### 4. 결과는 500자로 절삭됨

각 agent의 last_assistant_message는 최대 500자까지만 캡처됩니다. 상세 결과가 필요하면 각 Task의 개별 output_file을 Read합니다.

### 5. 플랫폼 제약

- macOS: 지원
- Linux: 지원
- Windows: 미지원 (POSIX named pipe 필요)

---

## 전체 예시: 병렬 코드 리뷰

```
# 1. Barrier 시작 (3개 Task 대기, 5분 타임아웃)
Bash(run_in_background=true):
  BARRIER_ID="code-review" bash /path/to/wait-for-tasks.sh 3 300
  → output_file: /tmp/barrier-output.txt

# 2. 3개 리뷰어 병렬 실행
Task(run_in_background=true): "보안 관점에서 코드 리뷰"
Task(run_in_background=true): "성능 관점에서 코드 리뷰"
Task(run_in_background=true): "아키텍처 관점에서 코드 리뷰"

# 3. 결과 확인 — 전부 끝나면 output_file에 결과가 있음
Read(/tmp/barrier-output.txt)
→ --- BARRIER COMPLETE (3/3) ---
  agents: sec123 perf456 arch789
  === sec123 ===
  보안 리뷰: SQL injection 위험 2건...
  === perf456 ===
  성능 리뷰: N+1 쿼리 1건...
  === arch789 ===
  아키텍처 리뷰: 레이어 위반 없음...
```

---

## 관련 커맨드

| 커맨드 | 설명 |
|--------|------|
| `/barrier-setup` | SubagentStop hook 등록 (최초 1회) |
