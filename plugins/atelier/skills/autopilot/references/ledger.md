# Ledger 운영 (work-ledger · stale-task-review)

결정적 ledger(SQLite)를 다루는 두 흐름. 병렬 dispatch·worktree 메커니즘은 `orchestrator` skill 에 위임한다.

- **work-ledger**: Ready task 를 epic 별로 claim → issue-implementer 디스패치 → branch-promote → PR open (첫 reader). gap-watch / qa-boost / ci-watch 가 쓴 task 를 실제 코드로 옮기는 단일 경로.
- **stale-task-review**: stale Wip task 후보를 stale-task-reviewer 에이전트에 전달해 task 별 release/fail/escalate/leave-alone 을 결정.

---

## A. work-ledger — Ready task claim → 구현 → PR

> 인자: `--epic <NAME>` — hybrid 모드에서 `autopilot watch` daemon 이 `TASK_READY epic=<E> task_id=<ID>` 이벤트를 emit 하면 Monitor 가 `--epic <E>`를 붙여 호출 (Step 4 selection strategy skip, 단일 epic 만 claim). 인자 없는 cron / 매뉴얼 호출은 by-depth selection strategy 사용.

> work-ledger 는 `pipeline-control.md` 의 idle/capacity/throttling 전처리를 사용하지 않는다. base 동기화(Step 1)만 공통이고, 이후는 epic 부트스트랩 → selection → claim → dispatch 흐름이다.

### 책임 경계

이 커맨드는 **claim → dispatch → branch-promote → PR open** 까지만 담당한다.

| 단계 | 담당 |
|------|------|
| Ready task 발생 | gap-watch, qa-boost, ci-watch (writer) |
| Ready → Wip (claim) | **work-ledger** |
| 구현 / 커밋 | issue-implementer 에이전트 |
| draft → feature 승격 + PR open | branch-promoter 에이전트 |
| PR 머지 | merge-prs (pr-merger) |
| Wip → Done | merge-prs 의 ledger close-the-loop (Step 4 fast-path inline + Step 5 pr-merger, `task complete --pr`) |
| Wip → Ready 또는 Escalated (실패) | **work-ledger** — `task fail` |

PR 머지 시 Wip→Done 전환은 **merge-prs 가** 수행한다 (Step 4 all-green fast-path 또는 Step 5 pr-merger 경로 어느 쪽이든 동일 inline close-the-loop). work-ledger 는 PR 생성에 성공하면 task 를 Wip 상태로 두고, merge-prs 가 close-the-loop 을 닫을 때까지 기다린다.

### Step 1: Base 브랜치 동기화

`references/branch-sync.md` 의 절차를 수행한다.

### Step 2: 설정 로딩

`github-autopilot.local.md` frontmatter 에서 읽는다:
- `max_parallel_agents`: 동시 실행 에이전트 수 (기본 3)
- `quality_gate_command`: (optional) 커스텀 quality gate
- `work_branch` / `branch_strategy`: base 브랜치 결정 (`references/draft-branch.md`)
- `label_prefix`: GitHub 라벨 접두사 (기본 `autopilot:`)
- `work_ledger.priority`: epic 우선순위 전략 — `"by-depth"`(default) / `"by-age"` / `"round-robin"` / `["epic-a", ...]` 명시 리스트. Step 4 참조.

### Step 3: Ledger Epic 부트스트랩

3개 writer epic 이 모두 존재함을 보장(멱등):

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

> `epic create --idempotent`(PR #663)는 동일 spec_path 로 이미 존재하면 exit 0. 부트스트랩 실패 시 해당 epic 처리를 skip 하고 cycle 은 중단하지 않는다.

### Step 4: Selection Strategy (어느 epic 부터 claim)

> **`--epic <NAME>` 인자가 주어지면 이 단계를 skip**한다 — Monitor 가 이미 `TASK_READY` 이벤트로 epic 을 지정했으므로 selection strategy 불필요. `RANKED=("$EPIC")` 단일 원소 배열로 두고 Step 5 로 진행.

> **Why this is a Skill decision, not a CLI flag.** `CLAUDE.md` §"책임 경계"는 우선순위 결정을 Skill 책임으로 둔다. CLI(`epic status --json`, `task claim --epic <NAME>`)는 결정적이고 인자만큼만 동작해야 하며 같은 입력에 매번 같은 결과를 돌려줘야 한다. "어느 backlog 가 지금 우선인가"는 런타임 상태 의존적인 *판단*이므로 이 reference(=Skill 지식) 안에서 결정하고, 결정 결과만 기존 CLI 에 인자로 넘긴다 (인자 없는 경우에 한해).

**설정 (override)**: `github-autopilot.local.md` frontmatter `[work_ledger] priority`에서 지정.

| 값 | 의미 | 비고 |
|----|------|------|
| `"by-depth"` | **(default)** 각 epic 의 `counts.ready`가 큰 순으로 claim. tie 는 `DEFAULT_ORDER`로 결정 | lazy fairness — 깊은 큐가 굶지 않음 |
| `"by-age"` | 각 epic 의 가장 오래된 ready task `created_at`가 빠른(=오래된) 순으로 claim | anti-starvation — 오래 묵은 task 우선 |
| `"round-robin"` | 고정 순서 `gap-backlog → qa-backlog → ci-backlog` | PR #689 도입 동작 — 명시 opt-in 으로만 유지 |
| `["a", "b", ...]` | 명시적 운영자 override 순서 | 리스트에 없는 epic 은 cycle 에서 skip |

> 전략은 **claim 순서**만 바꾼다. 한 cycle 에서 epic 당 최대 1 task 라는 fairness 규칙(§"주의사항")은 유지되며, 모든 전략은 동일한 `task claim --epic <NAME>` CLI 호출로 끝난다 — CLI 는 새 인자를 받지 않는다.

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

> 어떤 전략이든 결정의 산출물은 **하나의 정렬된 epic name 배열 `RANKED`** 다. CLI 는 이 결정을 모르고, 그저 결정된 순서대로 `task claim --epic <NAME>`을 받을 뿐이다 — 도구와 지능의 분리.

### Step 5: Task Claim (RANKED 순서대로 epic 당 1개)

`--epic <NAME>` 호출이면 `RANKED`는 단일 epic, 인자 없는 호출이면 Step 4 의 selection strategy 결과. 두 경로 모두 동일 루프를 사용한다 — CLI 는 호출 패턴을 모른다.

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

**Failure isolation**: `claim` exit 1 (empty queue)은 정상이고, exit ≥ 2 는 WARN 후 해당 epic 만 skip. `--epic` 호출에서도 동일 — 이벤트와 claim 사이의 race 로 큐가 비어있으면 다음 `TASK_READY` 이벤트에서 재시도. 상세 분기는 §"에러 처리" 표 참조.

`CLAIMED_JSON`이 빈 배열이면 "Ready task 없음" 출력 후 Step 8(결과 보고)로 이동.

### Step 6: 디스패치 (Agent Team — orchestrator 위임)

claimed task 들을 `max_parallel_agents` 단위로 분할해 issue-implementer 에 위임한다. **서브그룹 분할 / rate-limit 백오프 / 병렬 실행 메커니즘은 `orchestrator` skill 에 위임**하며, build-issues 의 구현 단계와 동일한 단일 출처를 사용한다 (분할 로직을 두 곳에 복제하지 않음).

**전달 정보 (task → issue-implementer 입력 매핑):**

| issue-implementer 입력 | 매핑 값 |
|------------------------|---------|
| `issue_number` | (생략) — ledger task 에는 GitHub issue 없음 |
| `issue_title` | task 의 `title` |
| `issue_body` | task 의 `body` (없으면 `task의 title을 단일 요구사항으로 간주합니다 (source: <task.source>).`) |
| `issue_comments` | `[]` |
| `recommended_persona` | (생략) — ledger task 에는 코멘트 기반 failure-analysis 없음 |
| `draft_branch` | `draft/task-{task_id}` (task_id 는 claim JSON 의 `id` 12-hex) |
| `base_branch` | Step 1 에서 결정한 base 브랜치 |
| `quality_gate_command` | 설정값 (비어있으면 자동 감지) |

> **branch naming**: GitHub issue 가 없으므로 `draft/issue-{N}` 대신 `draft/task-{id}`를 사용한다. issue-implementer 는 `draft_branch` 값을 그대로 사용하므로 에이전트 변경은 불필요.

### Step 7: 결과 수집 및 PR 생성

각 에이전트의 결과를 수집한다.

**성공 (issue-implementer status=success)** — 해당 task 에 대해 branch-promoter 에이전트**를** 호출해 draft → feature 승격 + PR open:
- `draft_branch`: `draft/task-{task_id}`
- `issue_number`: (생략) — ledger task 에는 GitHub issue 없음
- `issue_title`: task 의 title
- `base_branch`: 결정된 base
- `pr_type`: `auto`

> branch-promoter 는 `issue_number`가 비어 있으면 PR body 의 `Closes #N` 라인과 이슈 코멘트 단계를 자동 생략한다. ledger task 경로에서는 `issue_number`를 전달하지 않으므로 GitHub 이슈와 무관한 PR 이 안전하게 생성된다.

PR 생성 성공 시: task 는 **Wip 상태로 유지** — `task complete`는 호출하지 않음 (pr-merger 의 책임). PR 번호와 task id 를 보고에 누적.

**실패 (issue-implementer status=failed 또는 PR 미생성)** — failure_category 에 따라 분기:

| failure_category | 동작 | 이유 |
|------------------|------|------|
| `rate_limit` (429 등 transient) | `autopilot task release {id}` | task 의 잘못이 아님. attempts 증가 없이 재시도 큐로 |
| 그 외 (test_failure, lint_failure, complexity_exceeded, dependency_error 등) | `autopilot task fail {id}` | attempts 증가, max 도달 시 자동 escalate |

```bash
# 예: lint_failure
autopilot task fail "$TASK_ID"
# 출력 (JSON): {"outcome": "retried", "attempts": 1} 또는 {"outcome": "escalated", "attempts": 3}
```

> **결정**: `release`(무한 재시도)가 아닌 `fail`을 default 로 선택. `fail`은 attempts 를 증가시키고 outcome=`retried`이면 Wip → Ready, outcome=`escalated`(max_attempts 도달)이면 Wip → Escalated 로 자동 전환하므로 poison task 무한 루프를 방지한다. transient 실패만 `release`로 격리.

draft 브랜치는 issue-implementer 가 worktree 에서 `wip: partial work` 커밋을 남겨두므로 다음 cycle 에서 재시도 시 이어서 작업한다.

### Step 8: 결과 보고 + 세션 통계

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

**8b. 세션 누적 통계** — 매 cycle 종료 시 세션 통계 업데이트:

- `PROCESSED` = 이번 cycle 에서 claim 된 ledger task 수 (`CLAIMED_JSON`의 길이)
- `SUCCESS` = issue-implementer + branch-promoter 흐름이 성공해 PR 을 생성한 task 수 (Wip 유지)
- `FAILED` = `task fail` 또는 `task release`로 ready/escalated 로 되돌린 task 수
- `FALSE_POSITIVE` = `0` (work-ledger 는 false-positive 분류 단계 없음)

```bash
autopilot stats update --command work-ledger \
  --processed ${PROCESSED} --success ${SUCCESS} --failed ${FAILED} --false-positive ${FALSE_POSITIVE}
autopilot stats show --command work-ledger
```

> `processed=0`이면 `idle_cycles`, `processed>0`이면 `agent_calls` 자동 누적. 통계는 `/tmp/autopilot-{repo}/state/session-stats.json`, 세션 시작 시 `autopilot stats init`으로 초기화.

### 에러 처리

| 케이스 | 동작 |
|--------|------|
| `epic create --idempotent` 실패 | WARN 로그 후 해당 epic skip, 다른 epic 계속 처리 |
| `task claim` exit 1 (empty queue) | 정상 — skip, 다음 epic 진행 |
| `task claim` exit ≥ 2 (DB 오류) | WARN 로그 후 해당 epic skip |
| `task fail` 호출 실패 | WARN 로그 후 cycle 계속 — task 는 Wip 로 남음 (다음 cycle 에서 stale Wip 감지 follow-up 필요) |
| issue-implementer 타임아웃 / 크래시 | task `fail` 호출 (실패 카테고리: `agent_crash`) |

### Output Examples

**성공 케이스:**

```
[STEP 3] epic 부트스트랩 완료: gap-backlog, qa-backlog, ci-backlog
[STEP 4] strategy=by-depth → ranked: gap-backlog(ready=3) ci-backlog(ready=1)  # qa-backlog=0 skipped
[STEP 5] claimed: gap-backlog/a1b2c3d4e5f6
[STEP 5] claimed: ci-backlog/7e8f9a0b1c2d
[STEP 6] dispatching 2 tasks (max_parallel_agents=3, single subgroup)
[STEP 7] a1b2c3d4e5f6 → success → PR #142 (Wip)
[STEP 7] 7e8f9a0b1c2d → failed (test_failure) → task fail → retried (attempts=1)
```

**빈 큐 케이스:**

```
[STEP 3] epic 부트스트랩 완료
[STEP 4] strategy=by-depth → ranked: (none — all 3 epics ready=0)
[STEP 8] Ready task 없음 — cycle 종료
```

### work-ledger 주의사항

- 한 cycle 에서 epic 당 **최대 1개** task 만 claim (per-epic fairness). 어느 epic 부터 claim 할지는 Step 4 selection strategy 가 결정(default `by-depth`). max_parallel_agents 가 3이어도 epic 이 3개를 넘으면 일부는 다음 cycle 에서 처리.
- task complete 은 **호출하지 않음** — pr-merger 의 close-the-loop 단계가 PR 머지 시 호출.
- 실패 시 default 는 `task fail` (attempts 증가). transient 실패만 `task release` 사용.
- draft 브랜치는 `draft/task-{12-hex-id}` 형식이며 로컬 only (remote push 금지).
- ledger reader 는 GitHub 라벨/이슈 상태와 독립적으로 동작 — `:wip`, `:ready` 라벨을 사용하지 않음.

---

## B. stale-task-review — stale Wip 회수 결정

stale Wip task 후보를 `stale-task-reviewer` 에이전트에 전달해 task 별 처리 방법을 결정한다. 후보 수집 경로 두 가지:

1. **`--candidates <JSON>` (이벤트 드리븐)**: `autopilot watch` daemon 이 `STALE_WIP candidates=<JSON> epic=<E>` 이벤트로 이미 필터링한 task id 배열을 전달. 추가 list-stale 호출을 skip (PR #701 W1).
2. **`--before <duration>` 또는 인자 없음 (cron / 매뉴얼)**: `autopilot task list-stale --before <duration> --json` 으로 직접 후보를 관찰. 인자가 비어있으면 설정의 `stale_wip.threshold` 사용.

> hybrid 모드에서는 cron 등록이 없다. daemon 이 `STALE_WIP candidates=<JSON> epic=<E>` 이벤트를 emit 하면 Monitor 가 `--candidates <JSON>`을 붙여 호출 (autopilot.md Phase A 디스패치 표). cron 모드는 기존대로 `--before` 인자로 호출.

### 책임 경계 (CLAUDE.md "CLI vs Skill/Agent")

| 단계 | 담당 |
|------|------|
| stale 후보 관찰 (deterministic) | `autopilot task list-stale` (CLI) |
| task 별 결정 (judgment) | `stale-task-reviewer` 에이전트 |
| 결정 실행 (deterministic state transition) | `autopilot task release` / `task fail` / `task escalate` (CLI) |

CLI 는 절대 "release 할지 fail 할지"를 추측하지 않는다. 동일 입력 → 동일 출력 보장이 깨지기 때문. 결정은 컨텍스트(task 의 attempts 횟수, 최근 이벤트, 관련 PR 상태)를 보고 에이전트가 내린다.

### Step 1: stale 후보 조회

**`--candidates <JSON>` 가 주어진 경우 (이벤트 드리븐)**: list-stale 호출을 **skip**한다. 입력 JSON 배열을 그대로 후보로 사용 — daemon 이 이미 cutoff 기반 필터링을 마쳤다.

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

`stale-task-reviewer` 에이전트에 JSON 배열을 전달한다. 에이전트는 각 task 에 대해 다음 중 하나를 결정:

| 결정 | 트리거 조건 (가이드) | 실행 명령 |
|------|---------------------|----------|
| release | 일시적 stall — worker crash 가능성, attempts 여유 있음 | `autopilot task release <ID>` |
| fail | 진행 불가 시도가 명백 — escalation policy 에 위임 | `autopilot task fail <ID>` |
| escalate | HITL 필요 — 컨텍스트 상 자동 복구 부적절 | `autopilot task escalate <ID> --issue <N>` (이슈 선등록 후) |
| leave alone | 아직 progress 가능 — 다음 cycle 에서 재평가 | (no-op, 다음 tick 에 재관찰) |

> 단건 회수는 `release-stale --task-id` 가 아닌 `release` 를 사용한다 — 두 명령은 100% 동일하지만 "release-stale" 이름은 단건 회수에 부적합 (PR #696 audit). `release-stale --task-id` 는 deprecated alias 로 유지되며 기존 호출자 호환만 보장한다.

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

### stale-task-review 에러 처리

- `autopilot task list-stale` 가 exit 2 (DB 접근 실패 등) → autopilot cycle 중단하지 않고 다음 tick 으로 넘김 (failure isolation).
- 개별 task 결정 실행 실패 → 해당 task 만 skip, 나머지 진행.

### stale-task-review Output Examples

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
