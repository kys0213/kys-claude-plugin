# Autopilot 시작 절차 (startup)

`/atelier:autopilot` 진입점이 로드하는 시작 절차. 설정 로딩 이후의 preflight → 품질 게이트 → 초기 스캔 → 루프 등록(hybrid/cron) → 상태 스냅샷 전체를 정의한다. **시작 절차 변경 시 이 파일만 수정**한다.

## 기본 설정값

`github-autopilot.local.md` 가 없거나 항목이 비어 있으면 다음 기본값을 사용한다:

```yaml
event_mode: "hybrid"
default_intervals:
  gap_watch: "30m"
  analyze_issue: "20m"
  build_issues: "15m"
  merge_prs: "10m"
  ci_watch: "20m"
  ci_fix: "15m"
  qa_boost: "1h"
  work_ledger: "10m"       # cron 모드에서만 사용 — hybrid 모드는 Monitor TASK_READY 디스패치
  release_stale: "30m"     # cron 모드에서만 사용 — hybrid 모드는 Monitor STALE_WIP 디스패치
stale_wip:
  threshold: "1h"          # Wip claims older than this trigger STALE_WIP event (hybrid) / are reaped on each tick (cron)
monitor:
  poll_sec: 60
idle_shutdown:
  max_idle: 5
test_watch: []
```

## Step 1: Preflight Check

`autopilot preflight` CLI로 환경을 검증한다:

```bash
autopilot preflight --autopilot-md github-autopilot.local.md --repo-root .
```

- Exit 0: 모든 check PASS (WARN 허용) → 계속 진행
- Exit 1: FAIL 항목 있음 → FAIL 항목을 사용자에게 보여주고 `/atelier:setup` 안내 후 중단

## Step 2: Spec Quality Gate

설정에서 `spec_paths`와 `spec_quality_threshold`를 읽는다 (기본값: `"C"`).

`spec_paths`에 스펙 파일이 있으면, `gap-auditor` 에이전트를 **Spec Quality Grading 모드**로 호출하여 스펙 품질을 평가한다 (gap-auditor 는 `tools: []` 이므로 기준과 파일 내용을 프롬프트로 전달):

전달 정보:
- 평가 기준: `spec` skill 의 `references/quality-criteria.md` 본문 (4관점 체크리스트 + 등급 기준)
- spec_files: `spec_paths`에서 `**/*.md`로 수집한 파일들의 경로 + 본문
- spec_quality_threshold: 설정값 (기본: `"C"`)

결과 처리:
- **overall_grade >= threshold**: Step 3으로 진행
- **overall_grade < threshold**: AskUserQuestion으로 사용자 확인

```
스펙 품질이 {overall_grade} 등급입니다 (기준: {threshold}).
- A (Big Picture): {grade}
- B (Detail): {grade}
- C (Verification): {grade}

자율주행 결과물의 품질을 보장하기 어렵습니다. 계속 진행하시겠습니까?
```

- **Yes**: 경고 로그를 남기고 Step 3으로 진행
- **No**: `/atelier:spec` 안내 후 종료

> `spec_paths`에 파일이 없으면 이 단계를 skip한다 (preflight에서 이미 확인).

## Step 3: 초기 스캔 (Initial Scan)

autopilot 시작 시 기존 갭과 미분석 이슈를 감지한다.

> 이벤트 드리븐 모드에서는 이벤트가 발생해야 각 절차가 트리거되므로, 시작 시점에 이미 존재하는 갭과 미분석 이슈는 이 단계에서 처리한다.

### 3a: 초기 Gap 분석

1. `spec_paths`에 스펙 파일이 있으면 `references/gap-watch.md` 절차를 수행한다
2. 발견된 갭이 이슈로 등록된다
3. 결과 로그:
   - 이슈 생성됨: "초기 갭 분석 완료 — {N}건 이슈 등록"
   - 갭 없음: "초기 갭 분석 완료 — 갭 없음"

> `spec_paths`에 파일이 없으면 이 단계를 skip한다 (Step 2와 동일 조건).

### 3b: 미분석 이슈 처리

세션 시작 전 생성된 이슈 중 autopilot 분석이 아직 되지 않은 이슈를 처리한다.

1. CLI로 미분석 이슈를 조회한다:
   ```bash
   autopilot issue list --stage unanalyzed --label-prefix "{label_prefix}" --limit 20
   ```
2. 반환된 이슈들에 대해 `references/build-pipeline.md` 이슈 분석 절차를 병렬 수행한다 (issue-analyzer 에이전트) (`max_parallel_agents` 단위로 분할)
3. 결과 로그:
   - 처리됨: "초기 이슈 분석 완료 — {N}건 분석 ({M}건 ready 라벨 부여)"
   - 대상 없음: "초기 이슈 분석 완료 — 미분석 이슈 없음"

> hybrid/cron 모드 모두 동일하게 적용된다.

### 3c: 세션 통계 초기화

```bash
autopilot stats init
```

> `/tmp/autopilot-{repo}/state/session-stats.json`을 초기화한다. 기존 파일이 있으면 덮어쓴다.

## Step 4: 모드 분기

`event_mode` 설정에 따라 분기한다:

- **`"hybrid"`** → Step 4A (이벤트 드리븐 + CronCreate 혼합)
- **`"cron"`** → Step 4B (기존 CronCreate 전용)

---

## Step 4A: Hybrid 모드 (이벤트 드리븐)

### Phase A: Monitor 등록

타겟 쿼리 기반 통합 Monitor 1개를 등록한다.

**Singleton 보호** (#622): 등록 전에 기존 Monitor가 살아있는지 확인한다:

1. `/tmp/autopilot-{repo}/state/watch.json` 파일이 존재하는지 확인
2. 파일이 존재하면 `task_id` 필드와 mtime을 확인
3. mtime이 `5 * poll_sec`초 이내(Monitor가 최근에 상태를 기록함)이면:
   - "Monitor already running (task_id={task_id})" 로그 출력
   - Monitor 등록을 **skip**하고 Phase B로 진행
4. 파일이 없거나 mtime이 오래되었으면 아래 등록을 진행

```
Monitor(
  command: "autopilot watch --poll-sec={poll_sec} --branch={base_branch} --branch-filter={branch_filter} --label-prefix={label_prefix} --stale-threshold={stale_wip.threshold}",
  description: "push/CI/이슈 + ledger 이벤트 감시 → gap-watch, qa-boost, ci-watch, ci-fix, merge-prs, analyze-issue, work-ledger, stale-task-review 트리거",
  persistent: true,
  timeout_ms: 300000
)
```

> `--stale-threshold`(W1, PR #701)는 daemon이 SQLite 폴링으로 stale Wip을 감지하여 `STALE_WIP` 이벤트를 emit하기 위한 cutoff다. `stale_wip.threshold` 설정값을 그대로 전달한다.

등록 후 반환된 task ID를 `/tmp/autopilot-{repo}/state/watch.json`의 `task_id` 필드에 기록한다.

> `poll_sec`은 `monitor.poll_sec` 설정값 (기본: 5). push 감지는 매 tick, CI는 30초, 이슈는 60초 간격.
> `base_branch`는 `references/branch-sync.md` 의 base branch 결정 로직을 따른다.
> `branch_filter`는 `ci_watch.branch_filter` 설정값 (기본: `"autopilot"`).
> 상태는 `/tmp/autopilot-{repo}/state/watch.json`에 주기적으로 저장되어 세션 재시작 시 중복 emit을 방지한다.

Monitor가 출력하는 이벤트를 수신하면, 다음 규칙에 따라 디스패치한다:

| 이벤트 | 조건 | 액션 |
|--------|------|------|
| `MAIN_UPDATED before=<sha> after=<sha> count=<N>` | — | `references/gap-watch.md` 후 `references/qa-boost.md` ({before}) |
| `CI_FAILURE run_id=<id> workflow=<name> branch=<branch>` | default branch | `references/ci.md` §ci-watch ({run_id}, {branch}) |
| `CI_FAILURE run_id=<id> workflow=<name> branch=<branch>` | autopilot branch | `references/ci.md` §ci-fix ({branch}) |
| `CI_SUCCESS run_id=<id> workflow=<name> branch=<branch>` | autopilot branch | `references/merge.md` ({branch}) |
| `NEW_ISSUE number=<N> title=<title>` | — | `references/build-pipeline.md` 이슈 분석 ({number}) |
| `TASK_READY epic=<E> task_id=<ID>` | — | `references/ledger.md` §A work-ledger (--epic {E}, 해당 epic만 claim+dispatch) |
| `STALE_WIP candidates=<JSON> epic=<E>` | — | `references/ledger.md` §B stale-task-review (--candidates {JSON}, list-stale skip 후 stale-task-reviewer 디스패치) |
| `EPIC_DONE epic=<E> total=<N>` | — | **로그만 기록** (epic-rollup 은 후속 epic 에서 추가 예정) |

> 동시에 여러 이벤트가 도착하면 독립적인 이벤트는 병렬 디스패치한다. 같은 브랜치에 CI_FAILURE와 CI_SUCCESS가 동시에 도착하면 최신 이벤트만 처리한다.
>
> **Ledger 이벤트 (W1, PR #701)**: `autopilot watch`가 SQLite events 테이블을 폴링하여 emit한다. daemon은 idempotency state(`watch.json`)를 유지하므로 동일 이벤트가 두 번 디스패치되지 않는다 — Monitor 측에서 별도 dedup이 필요 없다. `STALE_WIP`의 `<JSON>`은 task id 문자열 배열(예: `["abc123","def456"]`)로 그대로 `--candidates`에 전달한다.

### Phase B: CronCreate 등록 (이벤트 신호가 없는 폴링 컴포넌트만)

이벤트로 신호화할 수 없는 컴포넌트만 CronCreate로 등록한다.

> **W1 (PR #701) 이후 변경**: `work-ledger` / `stale-task-review` 의 cron 등록은 **제거되었다**. `autopilot watch` daemon이 ledger의 SQLite events를 폴링하여 `TASK_READY` / `STALE_WIP` 이벤트를 emit하므로, Phase A Monitor 디스패치 표가 cron을 대체한다. 동일한 트리거를 두 경로에서 중복 등록하지 않는다 (CLAUDE.md "책임 경계" — daemon은 emit, Monitor는 dispatch).

1. `CronCreate(cron: "{build_issues_cron}", prompt: "autopilot skill references/build-pipeline.md 절차 수행")` — GitHub issues를 직접 읽으므로 SQLite events에 신호가 없다. 폴링 유지.
2. test_watch 배열이 비어있지 않으면 각 스위트별:
   `CronCreate(cron: "{suite_interval_cron}", prompt: "autopilot skill references/qa-boost.md 의 test-watch 절차 수행 {suite.name}")` — 시간 기반 스케줄 테스트.

### Phase C: Monitor Health Check

Monitor 생존 감시를 위한 supervisor cron을 등록한다 (#620):

```
CronCreate(
  cron: "*/30 * * * *",
  prompt: "Monitor health check: /tmp/autopilot-{repo}/state/watch.json의 mtime을 확인하여 Monitor 생존을 판단합니다.

1. `stat`으로 watch.json의 mtime을 확인
2. mtime이 5 * poll_sec 초 이내 → Monitor 정상, 아무것도 하지 않음
3. 파일이 없거나 mtime이 오래됨 → Monitor가 죽은 것으로 판단:
   a. watch.json에 task_id가 있으면 TaskStop(task_id)으로 정리 시도
   b. Phase A와 동일한 설정으로 Monitor를 재등록
   c. 새 task_id를 watch.json에 기록"
)
```

Step 5로 진행한다.

---

## Step 4B: Cron 모드 (기존 호환)

설정의 인터벌을 cron 표현식으로 변환한다:

| 인터벌 | cron 표현식 |
|--------|------------|
| `"10m"` | `"*/10 * * * *"` |
| `"15m"` | `"*/15 * * * *"` |
| `"20m"` | `"*/20 * * * *"` |
| `"30m"` | `"*/30 * * * *"` |
| `"1h"` | `"7 * * * *"` |
| `"2h"` | `"7 */2 * * *"` |
| `"6h"` | `"7 */6 * * *"` |

> 분 단위(`Nm`)는 `*/N * * * *`, 시간 단위(`Nh`)는 `7 */N * * *` (`:00` 회피).

기본 루프를 `CronCreate`로 등록한다:

1. `CronCreate(cron: "{gap_watch_cron}", prompt: "autopilot skill references/gap-watch.md 수행")`
2. `CronCreate(cron: "{analyze_issue_cron}", prompt: "autopilot skill references/build-pipeline.md 이슈 분석 수행")`
3. `CronCreate(cron: "{build_issues_cron}", prompt: "autopilot skill references/build-pipeline.md 절차 수행")`
4. `CronCreate(cron: "{merge_prs_cron}", prompt: "autopilot skill references/merge.md 수행")`
5. `CronCreate(cron: "{ci_watch_cron}", prompt: "autopilot skill references/ci.md §ci-watch 수행")`
6. `CronCreate(cron: "{ci_fix_cron}", prompt: "autopilot skill references/ci.md §ci-fix 수행")`
7. `CronCreate(cron: "{qa_boost_cron}", prompt: "autopilot skill references/qa-boost.md 수행")`
8. `CronCreate(cron: "{work_ledger_cron}", prompt: "autopilot skill references/ledger.md §A work-ledger 수행")` — gap-backlog / qa-backlog / ci-backlog ledger reader (Step 5에서 스냅샷으로 보여주는 3개 backlog의 Ready task를 claim해 issue-implementer로 디스패치)
9. `CronCreate(cron: "{release_stale_cron}", prompt: "autopilot skill references/ledger.md §B stale-task-review --before {stale_wip.threshold} 수행")` — stale Wip 관찰 + 에이전트 리뷰 (worker crash / ctrl-C / worktree 파괴 복구). CLI (`autopilot task list-stale`) 는 cutoff 보다 오래된 Wip 후보를 read-only 로 반환하고, `stale-task-reviewer` 에이전트가 task 별로 release / fail / escalate / leave alone 을 결정한다. exit 0 (idempotent — 후보가 0건이어도 정상). 결정은 컨텍스트 의존이므로 CLI 가 아닌 에이전트가 수행한다 (CLAUDE.md "책임 경계").

test_watch 배열이 비어있지 않으면 각 스위트별 루프 추가:
```
CronCreate(cron: "{suite_interval_cron}", prompt: "autopilot skill references/qa-boost.md 의 test-watch 절차 수행 {suite.name}")
```

Step 5로 진행한다.

---

## Step 5: Ledger 상태 스냅샷

루프 등록이 끝난 직후, 각 backlog epic의 ledger 상태를 한 번 스냅샷하여 사용자에게 보여준다. "어떤 backlog에 task가 몇 개 쌓여 있고, 라이프사이클의 어디에 있는가" 를 한 눈에 보여주는 정보성 단계다.

> 정보성 출력이다. 실패해도 autopilot 사이클을 중단하지 않는다 (failure isolation).

```bash
echo "## 📋 Ledger 상태"
for EPIC in gap-backlog ci-backlog qa-backlog; do
  if ! JSON=$(autopilot epic status "$EPIC" --json 2>/dev/null); then
    # 아직 부트스트랩되지 않은 epic은 INFO로 안내하고 skip (WARN 아님 — 정보성)
    echo "- $EPIC: (epic not yet bootstrapped)"
    continue
  fi
  echo "$JSON" | jq -r --arg e "$EPIC" \
    '.[0] | "- \($e): \(.counts.ready) ready · \(.counts.wip) wip · \(.counts.done) done · \(.counts.escalated) escalated"'
done

echo
echo "### 최근 이벤트 (최대 5건)"
autopilot events list --limit 5 2>/dev/null || echo "(이벤트 조회 실패 — skipped)"
```

**`autopilot epic status --json` 응답 형식** (cli/src/cmd/epic.rs::EpicStatusReport): 길이 1 배열, 각 원소는 `{ epic, status, total, counts: { pending, ready, wip, blocked, done, escalated } }`. 단일 epic 조회 시에도 배열이므로 `.[0]`로 언랩한다. 존재하지 않는 epic은 exit 1 + stderr `"epic '<name>' not found"` — 위 스니펫은 stderr를 버리고 "(epic not yet bootstrapped)"로 표시한다.

**예상 출력 예시**:

```
## 📋 Ledger 상태
- gap-backlog: 12 ready · 1 wip · 5 done · 0 escalated
- ci-backlog: 3 ready · 0 wip · 8 done · 0 escalated
- qa-backlog: (epic not yet bootstrapped)

### 최근 이벤트 (최대 5건)
AT                         KIND            EPIC          TASK   PAYLOAD-SUMMARY
2026-05-05T01:19:45+00:00  task_inserted   gap-backlog   g1     {"source":"gap-watch",...}
2026-05-05T01:19:45+00:00  task_claimed    gap-backlog   g1     {"attempts":1}
```

---

## Step 6: 결과 출력

### Hybrid 모드

```
## Autopilot 시작 (hybrid 모드)

### Monitor (Events API + Ledger Events)

| Watcher | 감시 이벤트 | 트리거 대상 | Poll |
|---------|-----------|------------|------|
| events | PushEvent (default branch) | gap-watch, qa-boost | {poll_sec}s |
|        | WorkflowRunEvent (failure) | ci-watch, ci-fix | |
|        | WorkflowRunEvent (success) | merge-prs | |
|        | IssuesEvent (opened) | analyze-issue | |
| ledger | TASK_READY | work-ledger --epic {E} | {poll_sec}s |
|        | STALE_WIP | stale-task-review --candidates {JSON} | |
|        | EPIC_DONE | (TODO: epic-rollup, feat-epic-flow epic) | |

### CronCreate (폴링)

| Loop | Command | Interval | Cron |
|------|---------|----------|------|
| Build Issues | autopilot ref build-pipeline.md | {interval} | {cron} |
| Monitor Health | health check | 30m | */30 * * * * |
| Test: {name} | autopilot ref qa-boost.md (test {name}) | {interval} | {cron} |

Monitor 1개 + CronCreate {M}개 등록되었습니다.
```

### Cron 모드

```
## Autopilot 시작 (cron 모드)

| Loop | Command | Interval | Cron |
|------|---------|----------|------|
| Gap Watch | autopilot ref gap-watch.md | 30m | */30 * * * * |
| Analyze Issue | autopilot ref build-pipeline.md (분석) | 20m | */20 * * * * |
| Build Issues | autopilot ref build-pipeline.md | 15m | */15 * * * * |
| Merge PRs | autopilot ref merge.md | 10m | */10 * * * * |
| CI Watch | autopilot ref ci.md §ci-watch | 20m | */20 * * * * |
| CI Fix | autopilot ref ci.md §ci-fix | 15m | */15 * * * * |
| QA Boost | autopilot ref qa-boost.md | 1h | 7 * * * * |
| Work Ledger | autopilot ref ledger.md §A | 10m | */10 * * * * |
| Stale Task Review | autopilot ref ledger.md §B (--before 1h) | 30m | */30 * * * * |
| Test: e2e | autopilot ref qa-boost.md (test e2e) | 2h | 7 */2 * * * |

{N}개 루프가 등록되었습니다.
CronList로 확인 가능합니다.
```

## 운영 주의사항

- **hybrid 모드**: Monitor는 이벤트가 발생할 때만 절차를 트리거한다. 변경이 없으면 실행하지 않는다.
- **cron 모드**: CronCreate는 REPL이 idle일 때만 실행된다.
- 세션 종료 시 모든 Monitor와 CronCreate가 자동 정리된다.
- 7일 후 자동 만료 — 장기 운영 시 재등록 필요
- 수동 해제: `CronDelete(id)` 또는 `TaskStop(id)` (Monitor용)
- `event_mode: "cron"`으로 설정하면 기존과 100% 동일하게 동작한다.
- **idle auto-shutdown**: 각 cron 루프는 연속 `idle_shutdown.max_idle`회 (기본: 5) idle 시 자동 해제된다. 새 이벤트 발생 시 autopilot이 해당 루프를 재등록한다.
