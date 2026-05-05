# github-autopilot

CronCreate 기반 자율 개발 루프 — gap 탐지, 테스트 갭 발행, CI 감시, 이슈 구현, PR 머지를 자동화합니다.

## 아키텍처

```
┌─────────────────────────────────────────────────────────────────┐
│                       이슈 소스                                   │
├──────────────┬──────────────┬──────────────┬────────────────────-┤
│  gap-watch   │   qa-boost   │   ci-watch   │    사람 (HITL)      │
│  스펙 갭 탐지 │  테스트 갭 탐지│  CI 실패 감지 │   수동 이슈 등록     │
└──────┬───────┴──────┬───────┴──────┬───────┴────────┬──────────-┘
       │              │              │                │
       ▼              ▼              ▼                ▼
    :ready          :ready     :ready + :ci-failure  라벨 없음
       │              │              │                │
       │              │              │       /analyze-issue (사람 실행)
       │              │              │                │
       │              │              │          ┌─────┴─────┐
       │              │              │          ▼           ▼
       │              │              │        ready        skip
       │              │              │        :ready    (코멘트만)
       │              │              │          │           │
       └──────────────┴──────────────┴──────────┘           │
                                                            │
                       ┌────────────────────────────────────┘
                       │
 ┌─────────────────────┼──────────────────────────────────────────┐
 │  build-issues       │                                          │
 │                     ▼                                          │
 │  Step 3: skip 이슈 대기 중?                                     │
 │           │                                                    │
 │           ├─ notification 설정 있음 → 자연어 지시대로 알림 전송     │
 │           │  (MCP/Skill 활용, 예: "Slack DM으로 알려줘")          │
 │           │                                                    │
 │  Step 4: :ready 이슈 조회                                       │
 │           │                                                    │
 │  Step 5: 의존성 분석                                             │
 │           │                                                    │
 │  Step 6: :wip 라벨 추가                                         │
 │           │                                                    │
 │  Step 7: issue-implementer (worktree 병렬 구현)                  │
 │           │                                                    │
 │  Step 8: 결과 수집                                               │
 │           │                                                    │
 │  Step 9: branch-promoter → PR + :auto 라벨                     │
 │           │                                                    │
 │  Step 10: :wip, :ready 제거                                     │
 └───────────┼────────────────────────────────────────────────────┘
             │
             ▼
 ┌──────────────────┐
 │    merge-prs     │  :auto PR 조회 → squash merge
 └──────────────────┘
```

## 이슈 소스

| 소스 | 설명 | 부여 라벨 |
|------|------|----------|
| `gap-watch` | 스펙 문서와 코드 사이의 갭을 탐지하여 이슈 발행 | `:ready` |
| `qa-boost` | 최근 변경사항의 테스트 커버리지 갭을 탐지하여 이슈 발행 | `:ready` |
| `ci-watch` | CI 실패를 분석하여 이슈 발행 | `:ready` + `:ci-failure` |
| `analyze-issue` (HITL) | 사람이 등록한 이슈를 분석하여 ready/skip 판정 | `:ready` (ready 판정만) |

모든 이슈는 `:ready` 라벨을 통해 단일 파이프라인(`build-issues`)에 합류합니다.

## HITL (Human-in-the-Loop)

사람이 직접 등록한 이슈는 자동으로 라벨이 붙지 않습니다.

**자동 탐색 모드** (autopilot 루프): `analyze-issue`가 주기적으로 라벨 없는 open 이슈를 탐색하여 자동 분석합니다.

**수동 모드**: 특정 이슈를 직접 분석할 수도 있습니다.

1. 사람이 이슈 등록
2. `/analyze-issue #42` 실행 (사람이 트리거) 또는 autopilot 루프가 자동 탐색
3. ready → `:ready` 라벨 → build-issues 파이프라인 진입
4. skip → 코멘트만 게시, 다음 build-issues tick에서 `notification` 설정에 따라 알림

`notification` 설정은 자연어로 지정합니다:

```yaml
notification: "Slack DM으로 @irene에게 알려줘"
```

## 라벨

| 라벨 | 용도 |
|------|------|
| `:ready` | 구현 대상 — 유일한 파이프라인 진입점 |
| `:wip` | 구현 진행 중 — 중복 작업 방지 |
| `:ci-failure` | CI 실패 표시 — `:ready`와 함께 부여 |
| `:auto` | autopilot PR — `merge-prs` 대상 |

라벨 접두사(`label_prefix`)는 설정에서 변경 가능합니다 (기본값: `autopilot:`).

모든 라벨은 `/github-autopilot:setup`에서 일괄 생성됩니다.

## 중복 방지

이슈 body에 fingerprint를 HTML 주석으로 삽입하고, `autopilot` CLI로 중복을 확인합니다.

| 소스 | fingerprint 형식 | 예시 |
|------|-----------------|------|
| `gap-watch` | `gap:{spec_path}:{keyword}` | `gap:spec/auth.md:token-refresh` |
| `qa-boost` | `qa:{source_path}:{test_type}` | `qa:src/auth/refresh.rs:unit` |
| `ci-watch` | `ci:{workflow}:{branch}:{failure_type}` | `ci:validate.yml:main:test-failure` |

```bash
# 이슈 생성 (중복 확인 + fingerprint 삽입 내장)
autopilot issue create --title "..." --label "autopilot:ready" --fingerprint "gap:spec/auth.md:token-refresh" --body "..."

# 중복 확인만
autopilot issue check-dup --fingerprint "gap:spec/auth.md:token-refresh"

# CI failure 이슈 자동 정리 (PR 머지 시)
autopilot issue close-resolved --label-prefix "autopilot:"

# 파이프라인 idle 체크
autopilot pipeline idle --label-prefix "autopilot:"
```

## 커맨드

| 커맨드 | 설명 |
|--------|------|
| `/github-autopilot:setup` | 초기 설정 (rules, 설정 파일, 라벨 생성) |
| `/github-autopilot:autopilot` | 7개 루프를 설정된 인터벌로 모두 시작 |
| `/github-autopilot:gap-watch [interval]` | 스펙 갭 탐지 → ledger task 등록 (gap-backlog epic, GitHub issue 미생성) |
| `/github-autopilot:qa-boost [commit] [interval]` | 테스트 갭 탐지 → ledger task 등록 (qa-backlog epic, GitHub issue 미생성) |
| `/github-autopilot:ci-watch [interval]` | CI 실패 감지 → 이슈 발행 |
| `/github-autopilot:build-issues [interval]` | `:ready` 이슈 구현 → PR |
| `/github-autopilot:merge-prs [interval]` | `:auto` PR 머지 |
| `/github-autopilot:analyze-issue [numbers]` | 이슈 분석 (인자 없으면 라벨 없는 이슈 자동 탐색) |
| `/github-autopilot:work-ledger` | ledger Ready task를 epic당 1개씩 claim → issue-implementer 디스패치 → PR open (첫 reader) |

## 에이전트

| 에이전트 | model | 호출 위치 | 역할 |
|----------|-------|----------|------|
| `gap-detector` | - | gap-watch | 스펙 파싱 → 구조 매핑 → call chain 갭 분석 |
| `gap-ledger-writer` | haiku | gap-watch | 갭 리포트 → autopilot ledger task 등록 (gap-backlog epic, fingerprint 기반 멱등) |
| `issue-analyzer` | sonnet | analyze-issue | 이슈 분석 → ready/skip 판정 (HITL) |
| `issue-dependency-analyzer` | - | build-issues | 이슈 간 의존성 → 배치 분류 |
| `issue-implementer` | opus | build-issues | worktree에서 이슈 구현 |
| `branch-promoter` | haiku | build-issues | draft → feature 브랜치 승격 + PR (:auto 라벨) |
| `pr-merger` | - | merge-prs | PR 문제 해결 (conflict, CI 실패) |
| `ci-failure-analyzer` | - | ci-watch | CI 로그 분석 → 실패 원인 리포트 |

## Ledger Integration

GitHub 이슈 파이프라인과 별도로, 결정적 SQLite ledger(`autopilot` CLI의 `epic`/`task`/`events` 서브커맨드)를 운영합니다. **ledger-followups 롤업 (PR #684–#688) + ledger-polish 롤업 (PR #693–#696) + watch-unified 롤업 (PR #699–#702) 머지 이후 lifecycle 은 완전 자동입니다**: writer 가 task 를 기록하고, work-ledger reader 가 claim 하고, issue-implementer → branch-promoter 가 구현 + PR 을 열고, pr-merger 가 머지 시 close-the-loop 을 닫고, stale-task-review 가 worker crash / ctrl-C 발생 시 **CLI 관찰 + 에이전트 결정** 흐름으로 회수합니다. **hybrid 모드 (default)** 에서는 `autopilot watch` daemon 이 SQLite events 를 폴링하여 `TASK_READY` / `STALE_WIP` / `EPIC_DONE` 이벤트를 emit 하고 Monitor 가 즉시 디스패치하므로 ledger 관련 cron 이 제거되었습니다. **cron 모드 (legacy)** 는 기존 cron 동작을 유지합니다. 운영자는 자리를 비울 수 있습니다.

### External vs Internal boundary (watch-unified 롤업)

watch-unified 롤업 (W2 PR #700, W3 PR #699) 은 자동 발견 to-do 의 보관소를 **사용 목적별로 분리**합니다 — 사람과 협업하는 채널과 autopilot 내부 큐를 섞지 않아 GitHub UI 노이즈를 없앱니다.

| 보관소 | 항목 | 가시성 |
|--------|------|-------|
| **GitHub (External)** | 사람이 등록한 issue, PR, CI 실패 (`ci-watch` dual-write — 팀 가시성 필요) | 팀 협업 / UI / notification |
| **SQLite Ledger (Internal)** | gap-watch 발견 spec gap (`gap-backlog`), qa-boost 발견 test gap (`qa-backlog`), 향후 feature epic (feat-epic-flow epic 예정) | autopilot 내부 to-do — work-ledger reader 가 직접 PR 발행 |

자세한 운영 의미는 [`RUNBOOK.md`](./RUNBOOK.md) Section F.6 참조.

| Backlog Epic | Writer | 역할 |
|--------------|--------|------|
| `gap-backlog` | `/github-autopilot:gap-watch` | **ledger-only writer (W2 PR #700)** — 스펙 갭 발견 시 ledger task 만 기록 (gap-ledger-writer agent). GitHub issue 미생성 |
| `qa-backlog` | `/github-autopilot:qa-boost` | **ledger-only writer (W3 PR #699)** — 테스트 갭 발견 시 ledger task 만 기록. GitHub issue 미생성 |
| `ci-backlog` | `/github-autopilot:ci-watch` | dual-write 유지 — CI 실패 발견 시 GitHub issue 와 동시에 ledger task 기록 (팀 가시성 필요, watch-unified 에서도 변경 없음) |
| (모든 epic) | `pr-merger` 에이전트 + `merge-prs` Step 4 fast-path | PR 머지 후 `task complete --pr <N>` 호출 (Wip→Done). PR #666 + PR #686 (F1) |
| (모든 epic) | `/github-autopilot:work-ledger` (**hybrid: Monitor `TASK_READY` 디스패치 — W4 PR #702 / cron: 10m cadence — PR #684 F2**) | reader — Ready task를 epic당 1개씩 selection strategy 로 claim → `issue-implementer` 디스패치 → `branch-promoter` (Closes #N suppress when missing, PR #685 F3) → PR open. **default `by-depth`** (PR #694 P1) |
| (모든 epic) | `/github-autopilot:stale-task-review` (**hybrid: Monitor `STALE_WIP` 디스패치 — W4 PR #702 / cron: 30m cadence — PR #695 P2**) | stale Wip 회수 — daemon이 list-stale 을 미리 수행 (hybrid) 또는 skill 이 수행 (cron) → `stale-task-reviewer` 에이전트가 task 별 release / fail / escalate / leave alone 결정 |

### Monitor-driven ledger dispatch (watch-unified 롤업)

`autopilot watch` daemon (W1 PR #701) 은 매 tick 마다 SQLite events 테이블을 폴링하여 다음 3 종 이벤트를 stdout 으로 emit 합니다:

| 이벤트 | 형식 | Monitor 디스패치 (W4 PR #702) |
|--------|------|----------------------------|
| `TASK_READY` | `TASK_READY epic=<E> task_id=<ID>` | `/github-autopilot:work-ledger --epic <E>` (해당 epic 만 single-epic claim) |
| `STALE_WIP` | `STALE_WIP candidates=<JSON> epic=<E>` | `/github-autopilot:stale-task-review --candidates <JSON>` (skill 은 list-stale 호출 skip) |
| `EPIC_DONE` | `EPIC_DONE epic=<E> total=<N>` | **로그만** — TODO: feat-epic-flow epic 에서 `/github-autopilot:epic-rollup` 디스패치 예정 |

daemon idempotency 는 `watch.json` 의 `last_event_at` / `seen_keys` / `epics_done` / `stale_seen` state 가 보장합니다 — 재시작 후에도 동일 이벤트가 두 번 emit 되지 않습니다. `--stale-threshold <D>` 는 STALE_WIP 의 cutoff (default `1h`, `s` / `m` / `h` / `d` / `w` 지원). 자세한 dispatch 표 / cron 등록 / 검증 절차는 [`RUNBOOK.md`](./RUNBOOK.md) Section A.4 / Section G 참조.

### Epic priority strategy (PR #694 P1)

`/github-autopilot:work-ledger` 의 epic claim 순서는 default 가 `by-depth` (각 epic 의 ready 큐 깊이가 큰 순) 입니다. `github-autopilot.local.md` frontmatter 또는 `WORK_LEDGER_PRIORITY` 환경변수로 override 합니다:

```yaml
work_ledger:
  priority: "by-depth"      # default — lazy fairness, 깊은 큐가 굶지 않음
  # 또는: "by-age" — oldest created_at 우선
  # 또는: "round-robin" — PR #694 이전과 동일 (back-compat)
  # 또는: ["gap-backlog", "qa-backlog", "ci-backlog"] — 명시 리스트 순회
```

CLI 자체는 변경되지 않았습니다 — 판단은 Skill 레이어 (CLAUDE.md "책임 경계: CLI vs Skill/Agent") 가 `epic status --json` 으로 ranking 을 결정한 뒤 기존 `task claim --epic <NAME>` 을 호출합니다.

### Stale 회수: list-stale + agent decision (PR #695 P2)

PR #688 (F5) 의 cron 은 `autopilot task release-stale --before <D>` 를 직접 호출하여 cutoff 보다 오래된 모든 Wip 을 자동 bulk-release 했습니다. PR #695 (P2) 이후 cron 은 `/github-autopilot:stale-task-review` 를 호출하며, 이 skill 은 새 read-only CLI (`autopilot task list-stale --json`) 로 후보를 관찰하고 `stale-task-reviewer` 에이전트가 task 별로 결정합니다 (release / fail / escalate / leave alone). 결정적 변환은 CLI, 컨텍스트 의존 판단은 에이전트 — CLAUDE.md "책임 경계" 적용.

bulk `autopilot task release-stale --before <D>` 는 운영자 비상 우회용으로 CLI 에 유지됩니다 (날짜/주 단위 지원: `1d`, `1w`, `2d12h` — PR #693 P4).

### Naming clarity (PR #696 P3)

PR #696 의 user-facing audit 후 두 명령이 더 직관적인 이름으로 정리되었습니다:

- `task force-status <ID> --to <STATUS>` → **`task set-status <ID> --to <STATUS>`** (canonical). `force-status` 는 deprecated alias 로 한 릴리스 더 유지됩니다.
- 단건 회수는 **`task release <ID>`** 가 canonical 입니다. `task release-stale --task-id <ID>` 는 100% 동일 효과의 deprecated alias 로 유지됩니다 (`-stale` suffix 가 단건 회수에 부적합).

자동화 스크립트를 신규 작성한다면 canonical 이름을 사용하세요. 기존 호출자는 한 릴리스 더 동작합니다.

### 자동화된 lifecycle

**hybrid 모드** (default — W1 PR #701 + W4 PR #702):

```
gap/qa/ci-watch (writer) ──task add──> Ready
                                         │  autopilot watch daemon (poll_sec, SQLite events)
                                         │  emit: TASK_READY epic=<E> task_id=<ID>
                                         │  Monitor 디스패치: work-ledger --epic <E>
                                         ▼
                                        Wip ──fail (retried)──> Ready (attempts++)
                                         │   fail (escalated, attempts >= max)──> Escalated
                                         │   complete --pr <N>──> Done — daemon emit: EPIC_DONE
                                         │   (worker crash) ──> daemon emit: STALE_WIP candidates=<JSON>
                                         │                       Monitor 디스패치: stale-task-review --candidates <JSON>
                                         │                       └─ stale-task-reviewer agent: per-task 결정
                                         └─ release ──> Ready (transient infra failures only)
```

**cron 모드** (legacy):

```
gap/qa/ci-watch (writer cron) ──task add──> Ready
                                              │  work-ledger cron (10m, by-depth strategy)
                                              ▼
                                             Wip → ... → Done (merge-prs Step 4/5)
                                              │   stale-task-review cron (30m)
                                              └─ release / fail / complete (위와 동일)
```

`/github-autopilot:autopilot` 시작 시 Step 2.5 (PR #681)에서 epic 상태 스냅샷과 최근 이벤트 5건을 출력합니다 (best-effort). `autopilot stats update --command work-ledger` 도 canonical 목록에 포함되어 (PR #687 F4) 모든 loop의 통계가 일관되게 수집됩니다.

ci-watch 의 ledger 쓰기는 GitHub issue 흐름의 **보조 observer**입니다 (best-effort). gap-watch / qa-boost 는 watch-unified 롤업 이후 ledger-only writer 로 전환되어 ledger 가 단일 출력입니다 — 실패 시 다음 cycle 에서 동일 fingerprint 로 재시도됩니다.

### Follow-up 처리 현황 (모두 해소)

- **F1 (PR #686)** — `merge-prs` Step 4 fast-path 도 `find-by-pr` → `task complete --pr` inline 호출 (best-effort).
- **F2 (PR #684)** — `/github-autopilot:work-ledger` 가 autopilot Step 2 에서 10m cron 으로 자동 등록.
- **F3 (PR #685)** — `branch-promoter` 가 `issue_number` 누락 시 PR body 에서 `Closes #N` 줄 suppress (깨진 링크 방지).
- **F4 (PR #687)** — `autopilot stats update --command work-ledger` 가 canonical 목록에 포함 (`--command` 는 free string 유지).
- **F5 (PR #688)** — `autopilot task release-stale --before <duration>` + 30m cron 으로 stale Wip 자동 회수 (idempotent). cron 동작 진화 사항은 위 "Stale 회수" 서브섹션 참조.

### Ledger-polish 롤업 (P1–P4)

- **P1 (PR #694)** — work-ledger epic priority strategy: `by-depth` default + Skill-side ranking. CLI 변경 없음.
- **P2 (PR #695)** — stale 회수를 CLI observation (`list-stale`, read-only) + agent decision (`stale-task-reviewer`) 으로 분리. cron 동작 변경.
- **P3 (PR #696)** — task 서브커맨드 naming clarity: `set-status` rename, `release` canonical for single-task recovery (`force-status`/`release-stale --task-id` deprecated alias).
- **P4 (PR #693)** — duration parser `d`/`w` 단위 지원 (`mo`/`y` 는 가변 길이라 의도적 미지원).

### Watch-unified 롤업 (W1–W4)

- **W1 (PR #701)** — `autopilot watch` daemon이 SQLite events 테이블을 폴링하여 `TASK_READY` / `STALE_WIP` / `EPIC_DONE` 이벤트를 stdout 으로 emit. `--stale-threshold <D>` 신규 (default `1h`). idempotency state 는 `watch.json` 의 `last_event_at` / `seen_keys` / `epics_done` / `stale_seen`.
- **W2 (PR #700)** — gap-watch 를 ledger-only writer 로 전환 (`gap-issue-creator` → `gap-ledger-writer` rename). GitHub issue 생성 제거 — gap 발견은 SQLite 내부 큐로만 보관. (W2 의 stagnation/persona 로직은 disabled — issue body 파싱 의존이라 follow-up.)
- **W3 (PR #699)** — qa-boost 를 ledger-only writer 로 전환. `{label_prefix}qa-suggestion` issue 흐름 제거 — work-ledger reader 가 task 를 claim 하여 PR 직접 발행.
- **W4 (PR #702)** — Monitor 디스패치 표에 ledger 이벤트 3 행 추가 (`TASK_READY` → `work-ledger --epic <E>`, `STALE_WIP` → `stale-task-review --candidates <JSON>`, `EPIC_DONE` → 로그만). hybrid 모드의 `work_ledger` / `release_stale` cron 제거. `EPIC_DONE` 의 `epic-rollup` 디스패치는 feat-epic-flow epic 의 follow-up.

### E2E Smoke Verification

릴리스 바이너리(`plugins/github-autopilot/cli/target/release/autopilot`) 빌드 후 다음 명령으로 ledger 통합을 검증할 수 있습니다 (전체 시나리오는 [`RUNBOOK.md`](./RUNBOOK.md) 참조):

```bash
BIN=plugins/github-autopilot/cli/target/release/autopilot
export AUTOPILOT_DB_PATH=/tmp/autopilot-smoke.db
rm -f $AUTOPILOT_DB_PATH

# 1) 3개 backlog epic 부트스트랩 (멱등)
for E in gap-backlog qa-backlog ci-backlog; do
  $BIN epic create --name "$E" --spec "spec/$E.md" --idempotent
done

# 2) writer 시뮬레이션 (gap-watch / qa-boost / ci-watch가 내부적으로 호출하는 형태)
FP="gap:spec/auth.md:token-refresh"
TID=$(printf '%s' "$FP" | shasum -a 256 | cut -c1-12)
$BIN task add "$TID" --epic gap-backlog \
  --title "spec gap: token-refresh in spec/auth.md" \
  --fingerprint "$FP" --source gap-watch

# 3) reader 시뮬레이션 (work-ledger가 호출하는 claim → fail → escalate)
$BIN task claim --epic gap-backlog --json
$BIN task fail "$TID"            # {"outcome":"retried","attempts":1}

# 4) pr-merger close-the-loop 시뮬레이션
$BIN task claim --epic gap-backlog --json > /dev/null
$BIN task complete "$TID" --pr 999
$BIN task find-by-pr 999          # status=done, pr_number=999

# 5) ledger 상태 스냅샷 (autopilot Step 2.5와 동일한 호출)
$BIN epic status
$BIN events list --limit 5
```

각 명령의 기대 출력은 `RUNBOOK.md` Sections C/D 참조.

## 설정

`github-autopilot.local.md` YAML frontmatter:

```yaml
---
branch_strategy: "draft-main"
work_branch: ""                      # 에이전트 작업 base 브랜치 (비어있으면 branch_strategy에 따라 결정)
auto_promote: true
label_prefix: "autopilot:"
spec_paths:
  - "spec/"
  - "docs/spec/"
default_intervals:
  gap_watch: "30m"
  analyze_issue: "20m"
  build_issues: "15m"
  merge_prs: "10m"
  ci_watch: "20m"
  ci_fix: "15m"
  qa_boost: "1h"
notification: ""
---
```
