---
description: "autopilot 루프를 설정된 모드로 시작합니다 (event-driven hybrid 또는 cron 기반)"
argument-hint: ""
allowed-tools: ["Read", "Bash", "CronCreate", "Monitor", "TaskStop"]
---

# Autopilot

설정의 `event_mode`에 따라 autopilot 루프를 시작합니다.

- **hybrid** (기본): Monitor 기반 이벤트 드리븐 + CronCreate 혼합
- **cron**: 기존 CronCreate 기반 폴링

## 사용법

```bash
/github-autopilot:autopilot
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 작업 프로세스

### Step 1: 설정 로딩

`github-autopilot.local.md`에서 설정을 읽습니다.

기본값:
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
monitor:
  poll_sec: 60
idle_shutdown:
  max_idle: 5
test_watch: []
```

### Step 1.5: Preflight Check

`autopilot preflight` CLI로 환경을 검증합니다:

```bash
autopilot preflight --config github-autopilot.local.md --repo-root .
```

- Exit 0: 모든 check PASS (WARN 허용) → 계속 진행
- Exit 1: FAIL 항목 있음 → FAIL 항목을 사용자에게 보여주고 `/github-autopilot:setup` 안내 후 중단

### Step 1.6: Spec Quality Gate

설정에서 `spec_paths`와 `spec_quality_threshold`를 읽습니다 (기본값: `"C"`).

`spec_paths`에 스펙 파일이 있으면, `spec-validator` 에이전트를 호출하여 스펙 품질을 평가합니다:

전달 정보:
- spec_files: `spec_paths`에서 `**/*.md`로 수집한 파일 목록
- spec_quality_threshold: 설정값 (기본: `"C"`)

결과 처리:
- **overall_grade >= threshold**: Step 2로 진행
- **overall_grade < threshold**: AskUserQuestion으로 사용자 확인

```
스펙 품질이 {overall_grade} 등급입니다 (기준: {threshold}).
- A (Big Picture): {grade}
- B (Detail): {grade}
- C (Verification): {grade}

자율주행 결과물의 품질을 보장하기 어렵습니다. 계속 진행하시겠습니까?
```

- **Yes**: 경고 로그를 남기고 Step 2로 진행
- **No**: `/spec-kit:design` 또는 `/spec-kit:spec-review` 안내 후 종료

> `spec_paths`에 파일이 없으면 이 단계를 skip합니다 (preflight에서 이미 확인).

### Step 1.7: 초기 스캔 (Initial Scan)

autopilot 시작 시 기존 갭과 미분석 이슈를 감지합니다.

> 이벤트 드리븐 모드에서는 이벤트가 발생해야 각 커맨드가 트리거되므로, 시작 시점에 이미 존재하는 갭과 미분석 이슈는 이 단계에서 처리합니다.

#### 1.7a: 초기 Gap 분석

1. `spec_paths`에 스펙 파일이 있으면 `/github-autopilot:gap-watch`를 실행합니다
2. 발견된 갭이 이슈로 등록됩니다
3. 결과 로그:
   - 이슈 생성됨: "초기 갭 분석 완료 — {N}건 이슈 등록"
   - 갭 없음: "초기 갭 분석 완료 — 갭 없음"

> `spec_paths`에 파일이 없으면 이 단계를 skip합니다 (Step 1.6과 동일 조건).

#### 1.7b: 미분석 이슈 처리

세션 시작 전 생성된 이슈 중 autopilot 분석이 아직 되지 않은 이슈를 처리합니다.

1. CLI로 미분석 이슈를 조회합니다:
   ```bash
   autopilot issue list --stage unanalyzed --label-prefix "{label_prefix}" --limit 20
   ```
2. 반환된 이슈들에 대해 `/github-autopilot:analyze-issue {number}`를 병렬 실행합니다 (`max_parallel_agents` 단위로 분할)
4. 결과 로그:
   - 처리됨: "초기 이슈 분석 완료 — {N}건 분석 ({M}건 ready 라벨 부여)"
   - 대상 없음: "초기 이슈 분석 완료 — 미분석 이슈 없음"

> hybrid/cron 모드 모두 동일하게 적용됩니다.

#### 1.7c: 세션 통계 초기화

```bash
autopilot stats init
```

> `/tmp/autopilot-{repo}/state/session-stats.json`을 초기화합니다. 기존 파일이 있으면 덮어씁니다.

### Step 2: 모드 분기

`event_mode` 설정에 따라 분기합니다:

- **`"hybrid"`** → Step 2A (이벤트 드리븐 + CronCreate 혼합)
- **`"cron"`** → Step 2B (기존 CronCreate 전용)

---

### Step 2A: Hybrid 모드 (이벤트 드리븐)

#### Phase A: Monitor 등록

타겟 쿼리 기반 통합 Monitor 1개를 등록합니다.

**Singleton 보호** (#622): 등록 전에 기존 Monitor가 살아있는지 확인합니다:

1. `/tmp/autopilot-{repo}/state/watch.json` 파일이 존재하는지 확인
2. 파일이 존재하면 `task_id` 필드와 mtime을 확인
3. mtime이 `5 * poll_sec`초 이내(Monitor가 최근에 상태를 기록함)이면:
   - "Monitor already running (task_id={task_id})" 로그 출력
   - Monitor 등록을 **skip**하고 Phase B로 진행
4. 파일이 없거나 mtime이 오래되었으면 아래 등록을 진행

```
Monitor(
  command: "autopilot watch --poll-sec={poll_sec} --branch={base_branch} --branch-filter={branch_filter} --label-prefix={label_prefix}",
  description: "push/CI/이슈 이벤트 감시 → gap-watch, qa-boost, ci-watch, ci-fix, merge-prs, analyze-issue 트리거",
  persistent: true,
  timeout_ms: 300000
)
```

등록 후 반환된 task ID를 `/tmp/autopilot-{repo}/state/watch.json`의 `task_id` 필드에 기록합니다.

> `poll_sec`은 `monitor.poll_sec` 설정값 (기본: 5). push 감지는 매 tick, CI는 30초, 이슈는 60초 간격.
> `base_branch`는 branch-sync 스킬의 base branch 결정 로직을 따릅니다.
> `branch_filter`는 `ci_watch.branch_filter` 설정값 (기본: `"autopilot"`).
> 상태는 `/tmp/autopilot-{repo}/state/watch.json`에 주기적으로 저장되어 세션 재시작 시 중복 emit을 방지합니다.

Monitor가 출력하는 이벤트를 수신하면, 다음 규칙에 따라 디스패치합니다:

| 이벤트 | 조건 | 액션 |
|--------|------|------|
| `MAIN_UPDATED before=<sha> after=<sha> count=<N>` | — | `/github-autopilot:gap-watch` 후 `/github-autopilot:qa-boost {before}` |
| `CI_FAILURE run_id=<id> workflow=<name> branch=<branch>` | default branch | `/github-autopilot:ci-watch --run-id={run_id} --branch={branch}` |
| `CI_FAILURE run_id=<id> workflow=<name> branch=<branch>` | autopilot branch | `/github-autopilot:ci-fix --branch={branch}` |
| `CI_SUCCESS run_id=<id> workflow=<name> branch=<branch>` | autopilot branch | `/github-autopilot:merge-prs --branch={branch}` |
| `NEW_ISSUE number=<N> title=<title>` | — | `/github-autopilot:analyze-issue {number}` |

> 동시에 여러 이벤트가 도착하면 독립적인 이벤트는 병렬 디스패치합니다. 같은 브랜치에 CI_FAILURE와 CI_SUCCESS가 동시에 도착하면 최신 이벤트만 처리합니다.

#### Phase B: CronCreate 등록 (폴링 유지 컴포넌트)

폴링이 적합한 컴포넌트를 CronCreate로 등록합니다:

1. `CronCreate(cron: "{build_issues_cron}", prompt: "/github-autopilot:build-issues")`
2. test_watch 배열이 비어있지 않으면 각 스위트별:
   `CronCreate(cron: "{suite_interval_cron}", prompt: "/github-autopilot:test-watch {suite.name}")`

#### Phase C: Monitor Health Check

Monitor 생존 감시를 위한 supervisor cron을 등록합니다 (#620):

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

Step 3으로 진행합니다.

---

### Step 2B: Cron 모드 (기존 호환)

설정의 인터벌을 cron 표현식으로 변환합니다:

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

기본 7개 루프를 `CronCreate`로 등록합니다:

1. `CronCreate(cron: "{gap_watch_cron}", prompt: "/github-autopilot:gap-watch")`
2. `CronCreate(cron: "{analyze_issue_cron}", prompt: "/github-autopilot:analyze-issue")`
3. `CronCreate(cron: "{build_issues_cron}", prompt: "/github-autopilot:build-issues")`
4. `CronCreate(cron: "{merge_prs_cron}", prompt: "/github-autopilot:merge-prs")`
5. `CronCreate(cron: "{ci_watch_cron}", prompt: "/github-autopilot:ci-watch")`
6. `CronCreate(cron: "{ci_fix_cron}", prompt: "/github-autopilot:ci-fix")`
7. `CronCreate(cron: "{qa_boost_cron}", prompt: "/github-autopilot:qa-boost")`

test_watch 배열이 비어있지 않으면 각 스위트별 루프 추가:
```
CronCreate(cron: "{suite_interval_cron}", prompt: "/github-autopilot:test-watch {suite.name}")
```

Step 3으로 진행합니다.

---

### Step 2.5: Ledger 상태 스냅샷

루프 등록이 끝난 직후, 각 backlog epic의 ledger 상태를 한 번 스냅샷하여 사용자에게 보여줍니다. "어떤 backlog에 task가 몇 개 쌓여 있고, 라이프사이클의 어디에 있는가" 를 한 눈에 보여주는 정보성 단계입니다.

> 정보성 출력입니다. 실패해도 autopilot 사이클을 중단하지 않습니다 (failure isolation).

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

**`autopilot epic status --json` 응답 형식** (cli/src/cmd/epic.rs::EpicStatusReport): 길이 1 배열, 각 원소는 `{ epic, status, total, counts: { pending, ready, wip, blocked, done, escalated } }`. 단일 epic 조회 시에도 배열이므로 `.[0]`로 언랩합니다. 존재하지 않는 epic은 exit 1 + stderr `"epic '<name>' not found"` — 위 스니펫은 stderr를 버리고 "(epic not yet bootstrapped)"로 표시합니다.

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

Step 3으로 진행합니다.

---

### Step 3: 결과 출력

#### Hybrid 모드

```
## Autopilot 시작 (hybrid 모드)

### Monitor (Events API, ETag 기반)

| Watcher | 감시 이벤트 | 트리거 대상 | Poll |
|---------|-----------|------------|------|
| events | PushEvent (default branch) | gap-watch, qa-boost | {poll_sec}s |
|        | WorkflowRunEvent (failure) | ci-watch, ci-fix | |
|        | WorkflowRunEvent (success) | merge-prs | |
|        | IssuesEvent (opened) | analyze-issue | |

### CronCreate (폴링)

| Loop | Command | Interval | Cron |
|------|---------|----------|------|
| Build Issues | /github-autopilot:build-issues | {interval} | {cron} |
| Monitor Health | health check | 30m | */30 * * * * |
| Test: {name} | /github-autopilot:test-watch {name} | {interval} | {cron} |

Monitor 1개 + CronCreate {M}개 등록되었습니다.
```

#### Cron 모드

```
## Autopilot 시작 (cron 모드)

| Loop | Command | Interval | Cron |
|------|---------|----------|------|
| Gap Watch | /github-autopilot:gap-watch | 30m | */30 * * * * |
| Analyze Issue | /github-autopilot:analyze-issue | 20m | */20 * * * * |
| Build Issues | /github-autopilot:build-issues | 15m | */15 * * * * |
| Merge PRs | /github-autopilot:merge-prs | 10m | */10 * * * * |
| CI Watch | /github-autopilot:ci-watch | 20m | */20 * * * * |
| CI Fix | /github-autopilot:ci-fix | 15m | */15 * * * * |
| QA Boost | /github-autopilot:qa-boost | 1h | 7 * * * * |
| Test: e2e | /github-autopilot:test-watch e2e | 2h | 7 */2 * * * |

{N}개 루프가 등록되었습니다.
CronList로 확인 가능합니다.
```

## 주의사항

- **hybrid 모드**: Monitor는 이벤트가 발생할 때만 커맨드를 트리거합니다. 변경이 없으면 실행하지 않습니다.
- **cron 모드**: CronCreate는 REPL이 idle일 때만 실행됩니다.
- 세션 종료 시 모든 Monitor와 CronCreate가 자동 정리됩니다.
- 7일 후 자동 만료 — 장기 운영 시 재등록 필요
- 수동 해제: `CronDelete(id)` 또는 `TaskStop(id)` (Monitor용)
- `event_mode: "cron"`으로 설정하면 기존과 100% 동일하게 동작합니다.
- **idle auto-shutdown**: 각 cron 루프는 연속 `idle_shutdown.max_idle`회 (기본: 5) idle 시 자동 해제됩니다. 새 이벤트 발생 시 autopilot이 해당 루프를 재등록합니다.
