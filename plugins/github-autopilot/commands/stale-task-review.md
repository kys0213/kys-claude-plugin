---
description: "stale Wip task를 관찰한 뒤 task별로 release/fail/escalate/leave alone을 결정합니다"
argument-hint: "--before <duration>"
allowed-tools: ["Bash", "Agent"]
---

# Stale Task Review

`autopilot task list-stale --before <duration> --json` 으로 stale Wip task 후보를 read-only 로 관찰한 뒤, `stale-task-reviewer` 에이전트가 task 별로 어떻게 처리할지 결정합니다.

## 책임 경계 (CLAUDE.md "CLI vs Skill/Agent")

| 단계 | 담당 |
|------|------|
| stale 후보 관찰 (deterministic) | `autopilot task list-stale` (CLI) |
| task 별 결정 (judgment) | `stale-task-reviewer` 에이전트 |
| 결정 실행 (deterministic state transition) | `autopilot task release` / `task fail` / `task escalate` (CLI) |

CLI는 절대 "release할지 fail할지" 를 추측하지 않습니다. 동일 입력 → 동일 출력 보장이 깨지기 때문입니다. 결정은 컨텍스트 (task의 attempts 횟수, 최근 이벤트, 관련 PR 상태) 를 보고 에이전트가 내립니다.

## 사용법

```bash
/github-autopilot:stale-task-review --before 1h
```

> 반복 실행은 `/github-autopilot:autopilot`이 `CronCreate`로 관리합니다.

## 작업 프로세스

### Step 1: stale 후보 조회

```bash
autopilot task list-stale --before $BEFORE --json
```

**출력 (JSON):** `Task` 객체 배열 (`find-by-pr --json` 과 동일 shape).
- 빈 배열 `[]` 인 경우: "stale Wip 없음" 로그 후 즉시 종료 (idempotent).

### Step 2: 에이전트 디스패치

`stale-task-reviewer` 에이전트에 JSON 배열을 전달합니다. 에이전트는 각 task 에 대해 다음 중 하나를 결정:

| 결정 | 트리거 조건 (가이드) | 실행 명령 |
|------|---------------------|----------|
| release | 일시적 stall — worker crash 가능성, attempts 여유 있음 | `autopilot task release <ID>` |
| fail | 진행 불가 시도가 명백 — escalation policy 에 위임 | `autopilot task fail <ID>` |
| escalate | HITL 필요 — 컨텍스트 상 자동 복구 부적절 | `autopilot task escalate <ID> --issue <N>` (이슈 선등록 후) |
| leave alone | 아직 progress 가능 — 다음 cycle 에서 재평가 | (no-op, 다음 tick 에 재관찰) |

> 단건 회수는 `release-stale --task-id` 가 아닌 `release` 를 사용합니다 — 두 명령은 100% 동일하지만 "release-stale" 이름은 단건 회수에 부적합 (PR #696 audit). `release-stale --task-id` 는 deprecated alias 로 유지되며 기존 호출자 호환만 보장합니다.

상세 결정 기준은 `agents/stale-task-reviewer.md` 참조.

### Step 3: 결과 로그

```
## Stale Task Review (--before {BEFORE})
- 관찰: {N}건
- release: {R}건
- fail: {F}건
- escalate: {E}건
- leave alone: {L}건
```

## 에러 처리

- `autopilot task list-stale` 가 exit 2 (DB 접근 실패 등) → autopilot cycle 중단하지 않고 다음 tick 으로 넘김 (failure isolation).
- 개별 task 결정 실행 실패 → 해당 task 만 skip, 나머지 진행.

## Output Examples

**stale 없음 (가장 흔한 케이스):**
```
## Stale Task Review (--before 1h)
- 관찰: 0건 (no stale Wip tasks)
```

**stale 있음 + 혼합 결정:**
```
## Stale Task Review (--before 1h)
- 관찰: 3건
- release: 2건 (g-abc123, q-def456)
- escalate: 1건 (c-ghi789 — attempts=3, 반복 실패)
```
