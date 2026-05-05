---
description: "stale Wip task를 관찰한 뒤 task별로 release/fail/escalate/leave alone을 결정합니다"
argument-hint: "[--before <duration> | --candidates <JSON>]"
allowed-tools: ["Bash", "Agent"]
---

# Stale Task Review

stale Wip task 후보를 `stale-task-reviewer` 에이전트에 전달하여 task 별로 어떻게 처리할지 결정합니다. 후보 수집 경로는 두 가지입니다:

1. **`--candidates <JSON>` (이벤트 드리븐)**: `autopilot watch` daemon이 `STALE_WIP candidates=<JSON> epic=<E>` 이벤트로 이미 필터링한 task id 배열을 전달. skill은 추가 list-stale 호출을 skip합니다 (PR #701 W1).
2. **`--before <duration>` 또는 인자 없음 (cron / 매뉴얼)**: `autopilot task list-stale --before <duration> --json` 으로 직접 후보를 관찰합니다. 인자가 비어있으면 설정의 `stale_wip.threshold`를 사용합니다.

## 책임 경계 (CLAUDE.md "CLI vs Skill/Agent")

| 단계 | 담당 |
|------|------|
| stale 후보 관찰 (deterministic) | `autopilot task list-stale` (CLI) |
| task 별 결정 (judgment) | `stale-task-reviewer` 에이전트 |
| 결정 실행 (deterministic state transition) | `autopilot task release` / `task fail` / `task escalate` (CLI) |

CLI는 절대 "release할지 fail할지" 를 추측하지 않습니다. 동일 입력 → 동일 출력 보장이 깨지기 때문입니다. 결정은 컨텍스트 (task의 attempts 횟수, 최근 이벤트, 관련 PR 상태) 를 보고 에이전트가 내립니다.

## 사용법

```bash
# 1) 이벤트 드리븐 (autopilot Monitor가 STALE_WIP 이벤트 수신 시 호출)
/github-autopilot:stale-task-review --candidates '["abc123","def456"]'

# 2) 매뉴얼 / cron 모드 (cutoff 기반 직접 조회)
/github-autopilot:stale-task-review --before 1h
/github-autopilot:stale-task-review                # 인자 없음 → stale_wip.threshold 사용
```

> hybrid 모드에서는 cron 등록이 없습니다. `autopilot watch` daemon이 `STALE_WIP candidates=<JSON> epic=<E>` 이벤트를 emit하면 Monitor가 `--candidates <JSON>`을 붙여 호출합니다 (PR #701 W1 / autopilot.md Phase A 디스패치 표). cron 모드는 기존대로 `--before` 인자로 호출됩니다.

## 작업 프로세스

### Step 1: stale 후보 조회

**`--candidates <JSON>` 가 주어진 경우 (이벤트 드리븐 경로)**: list-stale 호출을 **skip**합니다. 입력 JSON 배열을 그대로 후보로 사용합니다 — daemon이 이미 cutoff 기반 필터링을 마쳤습니다.

```bash
# Step 1 동작 분기
if [ -n "$CANDIDATES" ]; then
  # 이벤트 드리븐: --candidates 입력 (task id 문자열 배열)을 그대로 사용 — list-stale skip
  CANDIDATE_JSON="$CANDIDATES"
else
  # cron / 매뉴얼: cutoff 기반 list-stale 호출 (기본값은 stale_wip.threshold)
  CANDIDATE_JSON=$(autopilot task list-stale --before "${BEFORE:-$STALE_WIP_THRESHOLD}" --json)
fi
```

**출력 (JSON):**
- `--candidates` 경로: 입력 그대로 — task id 문자열 배열.
- `--before` 경로: `Task` 객체 배열 (`find-by-pr --json` 과 동일 shape).
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
