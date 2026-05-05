---
description: "ledger의 Ready task를 claim하여 issue-implementer로 구현하고 PR을 생성합니다 (첫 reader)"
argument-hint: "[--epic <NAME>]"
allowed-tools: ["Bash", "Read", "Agent"]
---

# Work Ledger

결정적 ledger(SQLite)에 누적된 Ready task를 epic별로 claim하여 `issue-implementer` 에이전트에 디스패치하는 첫 reader 파이프라인입니다. gap-watch / qa-boost / ci-watch가 쓴 task를 실제 코드로 옮기는 단일 경로입니다.

## 사용법

```bash
# 1) 이벤트 드리븐 모드 (autopilot Monitor가 TASK_READY 이벤트 수신 시 호출)
/github-autopilot:work-ledger --epic <NAME>

# 2) 매뉴얼 / cron 모드 (인자 없음 — 모든 epic을 selection strategy로 순회)
/github-autopilot:work-ledger
```

> hybrid 모드에서는 `autopilot watch` daemon이 `TASK_READY epic=<E> task_id=<ID>` 이벤트를 emit하면 Monitor가 `--epic <E>`를 붙여 호출합니다 (PR #701 W1 / autopilot.md Phase A 디스패치 표). 해당 호출은 Step 4 (selection strategy)를 skip하고 단일 epic만 claim합니다.
> cron 모드 또는 매뉴얼 호출(인자 없음)은 기존 by-depth selection strategy를 그대로 사용합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 책임 경계

이 커맨드는 **claim → dispatch → branch-promote → PR open** 까지만 담당합니다.

| 단계 | 담당 |
|------|------|
| Ready task 발생 | gap-watch, qa-boost, ci-watch (writer) |
| Ready → Wip (claim) | **work-ledger (이 커맨드)** |
| 구현 / 커밋 | issue-implementer 에이전트 |
| draft → feature 승격 + PR open | branch-promoter 에이전트 |
| PR 머지 | merge-prs (pr-merger) |
| Wip → Done | merge-prs의 ledger close-the-loop (Step 4 fast-path inline + Step 5 pr-merger, `task complete --pr`) |
| Wip → Ready 또는 Escalated (실패) | **work-ledger (이 커맨드)** — `task fail` |

PR 머지 시 Wip→Done 전환은 **merge-prs 가** 수행합니다 — Step 4 (all-green fast-path) 또는 Step 5 (pr-merger 경로) 어느 쪽이든 동일한 inline close-the-loop 로직이 호출됩니다. 이 커맨드는 PR 생성에 성공하면 task를 Wip 상태로 두고, merge-prs 가 close-the-loop을 닫을 때까지 기다립니다.

## 작업 프로세스

### Step 1: Base 브랜치 동기화

**branch-sync** 스킬의 절차를 수행합니다.

### Step 2: 설정 로딩

설정 파일(`github-autopilot.local.md`) frontmatter에서 다음 값을 읽습니다:

- `max_parallel_agents`: 동시 실행 에이전트 수 (기본값: 3)
- `quality_gate_command`: (optional) 커스텀 quality gate
- `work_branch` / `branch_strategy`: base 브랜치 결정 (draft-branch 스킬)
- `label_prefix`: GitHub 라벨 접두사 (기본값: `autopilot:`)
- `work_ledger.priority`: epic 우선순위 전략 — `"by-depth"`(default) / `"by-age"` / `"round-robin"` / `["epic-a", "epic-b", ...]` 명시 리스트. 자세한 동작은 Step 4 참조.

### Step 3: Ledger Epic 부트스트랩

3개 writer epic이 모두 존재함을 보장합니다 (멱등):

```bash
EPICS=("gap-backlog:spec/gap-backlog.md" "qa-backlog:spec/qa-backlog.md" "ci-backlog:spec/ci-backlog.md")
for entry in "${EPICS[@]}"; do
  name="${entry%%:*}"
  spec="${entry##*:}"
  if ! autopilot epic create --name "$name" --spec "$spec" --idempotent 2>&1; then
    echo "WARN: epic '$name' 부트스트랩 실패 — 이번 cycle에서 해당 epic은 skip됩니다"
  fi
done
```

> `epic create --idempotent`(PR #663)는 동일 spec_path로 이미 존재하면 exit 0입니다. 부트스트랩 실패 시 해당 epic 처리를 skip하고, cycle을 중단하지 않습니다.

### Step 4: Selection Strategy (어느 epic부터 claim할 것인가)

> **`--epic <NAME>` 인자가 주어지면 이 단계를 skip합니다** — Monitor가 이미 `TASK_READY` 이벤트로 epic을 지정했으므로 selection strategy는 불필요합니다. `RANKED=("$EPIC")` 단일 원소 배열로 두고 Step 5로 진행합니다.

> **Why this is a Skill decision, not a CLI flag.** `CLAUDE.md` §"책임 경계 (CLI vs Skill/Agent)"는 우선순위 결정을 명시적으로 Skill의 책임으로 둡니다. CLI(`epic status --json`, `task claim --epic <NAME>`)는 결정적이고 인자만큼만 동작해야 하며, 같은 입력에 매번 같은 결과를 돌려줘야 합니다. "어느 backlog가 지금 우선인가"는 런타임 상태 의존적인 *판단*이므로 이 markdown(=Skill) 안에서 결정하고, 결정 결과만 기존 CLI에 인자로 넘깁니다. 이 커맨드는 그 원칙의 정전(canonical) 사례입니다 (인자 없는 경우에 한해).

**설정 (override)**: `github-autopilot.local.md` frontmatter `[work_ledger] priority`에서 전략을 지정합니다.

| 값 | 의미 | 비고 |
|----|------|------|
| `"by-depth"` | **(default)** 각 epic의 `counts.ready`가 큰 순으로 claim. tie는 `DEFAULT_ORDER`로 결정 | lazy fairness — 깊은 큐가 굶지 않음 |
| `"by-age"` | 각 epic의 가장 오래된 ready task `created_at`가 빠른(=오래된) 순으로 claim | anti-starvation — 오래 묵은 task 우선 |
| `"round-robin"` | 고정 순서 `gap-backlog → qa-backlog → ci-backlog` | PR #689 도입 동작 — 명시 opt-in으로만 유지 |
| `["a", "b", ...]` | 명시적 운영자 override 순서 | 리스트에 없는 epic은 cycle에서 skip |

> 전략은 **claim 순서**만 바꿉니다. 한 cycle에서 epic당 최대 1 task라는 fairness 규칙(§"주의사항")은 유지되며, 모든 전략은 동일한 `task claim --epic <NAME>` CLI 호출로 끝납니다 — CLI는 새 인자를 받지 않습니다.

**ranking 의사결정 흐름**:

```bash
DEFAULT_ORDER=(gap-backlog qa-backlog ci-backlog)
PRIORITY="${WORK_LEDGER_PRIORITY:-by-depth}"   # frontmatter에서 로드, 미설정 시 default

case "$PRIORITY" in
  round-robin)
    RANKED=("${DEFAULT_ORDER[@]}") ;;
  by-depth)
    # epic status --json → counts.ready 기준 desc, tie는 DEFAULT_ORDER 인덱스 asc
    # 형식: <ready>\t<default-idx>\t<epic>, ready=0인 epic은 cycle에서 skip
    RANKED=()
    while IFS= read -r line; do RANKED+=("$line"); done < <(
      i=0
      for epic in "${DEFAULT_ORDER[@]}"; do
        ready=$(autopilot epic status "$epic" --json 2>/dev/null | jq -r '.[0].counts.ready // 0')
        [ "$ready" -gt 0 ] && printf '%d\t%d\t%s\n' "$ready" "$i" "$epic"
        i=$((i+1))
      done | sort -k1,1nr -k2,2n | cut -f3
    ) ;;
  by-age)
    # task list --status ready --json → 가장 오래된 created_at(=oldest ready) asc, tie는 DEFAULT_ORDER asc
    RANKED=()
    while IFS= read -r line; do RANKED+=("$line"); done < <(
      i=0
      for epic in "${DEFAULT_ORDER[@]}"; do
        oldest=$(autopilot task list --epic "$epic" --status ready --json 2>/dev/null \
          | jq -r 'min_by(.created_at).created_at // empty')
        [ -n "$oldest" ] && printf '%s\t%d\t%s\n' "$oldest" "$i" "$epic"
        i=$((i+1))
      done | sort -k1,1 -k2,2n | cut -f3
    ) ;;
  *)
    # 명시적 리스트 (frontmatter에서 array로 로드 → ARR로 받았다고 가정)
    RANKED=("${PRIORITY_LIST[@]}") ;;
esac
```

> 어떤 전략이든 결정의 산출물은 **하나의 정렬된 epic name 배열 `RANKED`** 입니다. CLI는 이 결정을 모르고, 그저 결정된 순서대로 `task claim --epic <NAME>`을 받을 뿐입니다 — 도구와 지능의 분리.

### Step 5: Task Claim (RANKED 순서대로 epic당 1개)

`--epic <NAME>` 호출이면 `RANKED`는 단일 epic, 인자 없는 호출이면 Step 4의 selection strategy 결과입니다. 두 경로 모두 동일한 루프를 사용합니다 — CLI는 호출 패턴을 모릅니다.

```bash
# --epic 인자가 있으면: RANKED=("$EPIC_ARG"), Step 4 skip
# 인자가 없으면: Step 4의 selection strategy 결과 사용
CLAIMED_JSON="[]"
for EPIC in "${RANKED[@]}"; do
  out=$(autopilot task claim --epic "$EPIC" --json 2>/dev/null)
  rc=$?
  if [ "$rc" = "0" ] && [ -n "$out" ]; then
    # claim 성공 → CLAIMED_JSON에 누적 (epic 메타데이터 포함)
    CLAIMED_JSON=$(printf '%s' "$CLAIMED_JSON" | jq --argjson t "$out" --arg e "$EPIC" '. + [$t + {epic: $e}]')
  fi
  # exit 1: empty queue (정상 — by-depth/by-age는 사전 필터링되지만 race로 빌 수 있음, --epic 호출은 이벤트와 claim 사이의 race)
  # exit 2+: 환경 오류 → WARN 후 skip
  if [ "$rc" -ge 2 ]; then
    echo "WARN: task claim --epic $EPIC 실패 (exit $rc) — skip"
  fi
done
```

**Failure isolation**: `claim` exit 1 (empty queue)은 정상이고, exit ≥ 2는 WARN 후 해당 epic만 skip합니다. `--epic` 호출에서도 동일 — 이벤트와 claim 사이의 race로 큐가 비어있으면 다음 `TASK_READY` 이벤트에서 재시도됩니다. 상세 분기는 §"에러 처리" 표 참조.

`CLAIMED_JSON`이 빈 배열이면 "Ready task 없음" 출력 후 Step 8 (결과 보고)로 이동합니다.

### Step 6: 디스패치 (Agent Team)

claimed task들을 `max_parallel_agents` 단위로 분할하여 issue-implementer에 위임합니다. 서브그룹 분할 / rate-limit 백오프 / 병렬 실행 규칙은 **build-issues Step 8과 동일**합니다 (단일 출처 유지 — 분할 로직을 두 곳에 복제하지 않습니다).

**전달 정보 (task → issue-implementer 입력 매핑):**

| issue-implementer 입력 | 매핑 값 |
|------------------------|---------|
| `issue_number` | (생략) — ledger task에는 GitHub issue가 없음 |
| `issue_title` | task의 `title` |
| `issue_body` | task의 `body` (없으면 `task의 title을 단일 요구사항으로 간주합니다 (source: <task.source>).`) |
| `issue_comments` | `[]` |
| `recommended_persona` | (생략) — ledger task에는 코멘트 기반 failure-analysis가 없음 |
| `draft_branch` | `draft/task-{task_id}` (task_id는 claim JSON의 `id` 12-hex) |
| `base_branch` | Step 1에서 결정한 base 브랜치 |
| `quality_gate_command` | 설정값 (비어있으면 자동 감지) |

> **branch naming**: GitHub issue가 없으므로 `draft/issue-{N}` 대신 `draft/task-{id}`를 사용합니다. issue-implementer는 `draft_branch` 값을 그대로 사용하므로 에이전트 변경은 불필요합니다.

### Step 7: 결과 수집 및 PR 생성

각 에이전트의 결과를 수집합니다.

#### 성공 (issue-implementer status=success)

해당 task에 대해 branch-promoter 에이전트를 호출하여 draft → feature 승격 + PR open:
- `draft_branch`: `draft/task-{task_id}`
- `issue_number`: (생략) — ledger task에는 GitHub issue가 없음
- `issue_title`: task의 title
- `base_branch`: 결정된 base
- `pr_type`: `auto`

> branch-promoter는 `issue_number`가 비어 있으면 PR body의 `Closes #N` 라인과 이슈 코멘트 단계를 자동으로 생략합니다. ledger task 경로에서는 `issue_number`를 전달하지 않으므로 GitHub 이슈와 무관한 PR이 안전하게 생성됩니다.

PR 생성 성공 시:
- task는 **Wip 상태로 유지** — `task complete`는 호출하지 않습니다 (pr-merger의 책임)
- PR 번호와 task id를 보고에 누적

#### 실패 (issue-implementer status=failed 또는 PR 미생성)

failure_category에 따라 분기합니다:

| failure_category | 동작 | 이유 |
|------------------|------|------|
| `rate_limit` (429 등 transient) | `autopilot task release {id}` | task의 잘못이 아님. attempts 증가 없이 재시도 큐로 |
| 그 외 (test_failure, lint_failure, complexity_exceeded, dependency_error 등) | `autopilot task fail {id}` | attempts 증가, max 도달 시 자동 escalate |

```bash
# 예: lint_failure
autopilot task fail "$TASK_ID"
# 출력 (JSON): {"outcome": "retried", "attempts": 1} 또는 {"outcome": "escalated", "attempts": 3}
```

> **결정**: `release`(무한 재시도)가 아닌 `fail`을 default로 선택했습니다. `fail`은 attempts를 증가시키고 outcome=`retried`이면 Wip → Ready, outcome=`escalated`(max_attempts 도달)이면 Wip → Escalated 로 자동 전환하므로 poison task 무한 루프를 방지합니다. transient 실패만 `release`로 격리합니다.

draft 브랜치는 issue-implementer가 worktree에서 `wip: partial work` 커밋을 남겨두므로 다음 cycle에서 재시도 시 이어서 작업합니다.

### Step 8: 결과 보고

```
## Work Ledger 결과

### Claim
- gap-backlog: a1b2c3d4e5f6 ("Add /healthz endpoint")
- qa-backlog: (empty)
- ci-backlog: 7e8f9a0b1c2d ("Fix flaky test in api_test.rs")

### 구현
- 성공: a1b2c3d4e5f6 → PR #142 (Wip 유지, pr-merger 대기)
- 실패: 7e8f9a0b1c2d (test_failure) → fail → outcome=retried, attempts=1

### 다음 cycle
- 7e8f9a0b1c2d: ready (attempts=1) — 다음 cycle에서 재시도
```

## 에러 처리

| 케이스 | 동작 |
|--------|------|
| `epic create --idempotent` 실패 | WARN 로그 후 해당 epic skip, 다른 epic 계속 처리 |
| `task claim` exit 1 (empty queue) | 정상 — skip, 다음 epic 진행 |
| `task claim` exit ≥ 2 (DB 오류) | WARN 로그 후 해당 epic skip |
| `task fail` 호출 실패 | WARN 로그 후 cycle 계속 — task는 Wip로 남음 (다음 cycle에서 stale Wip 감지 follow-up 필요) |
| issue-implementer 타임아웃 / 크래시 | task `fail` 호출 (실패 카테고리: `agent_crash`) |

## Output Examples

### 성공 케이스

```
[STEP 3] epic 부트스트랩 완료: gap-backlog, qa-backlog, ci-backlog
[STEP 4] strategy=by-depth → ranked: gap-backlog(ready=3) ci-backlog(ready=1)  # qa-backlog=0 skipped
[STEP 5] claimed: gap-backlog/a1b2c3d4e5f6
[STEP 5] claimed: ci-backlog/7e8f9a0b1c2d
[STEP 6] dispatching 2 tasks (max_parallel_agents=3, single subgroup)
[STEP 7] a1b2c3d4e5f6 → success → PR #142 (Wip)
[STEP 7] 7e8f9a0b1c2d → failed (test_failure) → task fail → retried (attempts=1)
```

### 빈 큐 케이스

```
[STEP 3] epic 부트스트랩 완료
[STEP 4] strategy=by-depth → ranked: (none — all 3 epics ready=0)
[STEP 8] Ready task 없음 — cycle 종료
```

## 주의사항

- 한 cycle에서 epic당 **최대 1개** task만 claim합니다 (per-epic fairness). 어느 epic부터 claim할지는 Step 4의 selection strategy가 결정합니다 (default `by-depth`). max_parallel_agents가 3이어도 epic이 3개를 넘으면 일부는 다음 cycle에서 처리됩니다.
- task complete은 **호출하지 않습니다** — pr-merger의 close-the-loop 단계가 PR 머지 시 호출합니다.
- 실패 시 default는 `task fail` (attempts 증가) 입니다. transient 실패만 `task release`를 사용합니다.
- draft 브랜치는 `draft/task-{12-hex-id}` 형식이며 로컬 only입니다 (remote push 금지).
- ledger reader는 GitHub 라벨/이슈 상태와 독립적으로 동작합니다 — `:wip`, `:ready` 라벨을 사용하지 않습니다.
