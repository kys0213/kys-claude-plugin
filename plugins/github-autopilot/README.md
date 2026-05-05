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
| `/github-autopilot:gap-watch [interval]` | 스펙 갭 탐지 → 이슈 발행 |
| `/github-autopilot:qa-boost [commit] [interval]` | 테스트 갭 탐지 → 이슈 발행 |
| `/github-autopilot:ci-watch [interval]` | CI 실패 감지 → 이슈 발행 |
| `/github-autopilot:build-issues [interval]` | `:ready` 이슈 구현 → PR |
| `/github-autopilot:merge-prs [interval]` | `:auto` PR 머지 |
| `/github-autopilot:analyze-issue [numbers]` | 이슈 분석 (인자 없으면 라벨 없는 이슈 자동 탐색) |
| `/github-autopilot:work-ledger` | ledger Ready task를 epic당 1개씩 claim → issue-implementer 디스패치 → PR open (첫 reader) |

## 에이전트

| 에이전트 | model | 호출 위치 | 역할 |
|----------|-------|----------|------|
| `gap-detector` | - | gap-watch | 스펙 파싱 → 구조 매핑 → call chain 갭 분석 |
| `gap-issue-creator` | haiku | gap-watch | 갭 리포트 → GitHub 이슈 생성 (fingerprint 중복 검사 포함) |
| `issue-analyzer` | sonnet | analyze-issue | 이슈 분석 → ready/skip 판정 (HITL) |
| `issue-dependency-analyzer` | - | build-issues | 이슈 간 의존성 → 배치 분류 |
| `issue-implementer` | opus | build-issues | worktree에서 이슈 구현 |
| `branch-promoter` | haiku | build-issues | draft → feature 브랜치 승격 + PR (:auto 라벨) |
| `pr-merger` | - | merge-prs | PR 문제 해결 (conflict, CI 실패) |
| `ci-failure-analyzer` | - | ci-watch | CI 로그 분석 → 실패 원인 리포트 |

## Ledger Integration

GitHub 이슈 파이프라인과 별도로, 결정적 SQLite ledger(`autopilot` CLI의 `epic`/`task`/`events` 서브커맨드)를 운영합니다. **ledger-followups 롤업 (PR #684–#688) 머지 이후 lifecycle 은 완전 자동입니다**: writer cron 이 task 를 기록하고, work-ledger reader cron (10m) 이 claim 하고, issue-implementer → branch-promoter 가 구현 + PR 을 열고, pr-merger 가 머지 시 close-the-loop 을 닫고, release-stale cron (30m) 이 worker crash / ctrl-C 발생 시 stale Wip 을 회수합니다. 운영자는 cron 등록 후 자리를 비울 수 있습니다.

| Backlog Epic | Writer | 역할 |
|--------------|--------|------|
| `gap-backlog` | `/github-autopilot:gap-watch` (cron) | 스펙 갭 발견 시 GitHub issue와 동시에 ledger task 기록 (observer) |
| `qa-backlog` | `/github-autopilot:qa-boost` (cron) | 테스트 갭 발견 시 GitHub issue와 동시에 ledger task 기록 (observer) |
| `ci-backlog` | `/github-autopilot:ci-watch` (cron) | CI 실패 발견 시 GitHub issue와 동시에 ledger task 기록 (observer) |
| (모든 epic) | `pr-merger` 에이전트 + `merge-prs` Step 4 fast-path | PR 머지 후 `task complete --pr <N>` 호출 (Wip→Done). PR #666 + PR #686 (F1) |
| (모든 epic) | `/github-autopilot:work-ledger` (**10m cron**, PR #684 F2) | reader — Ready task를 epic당 1개씩 round-robin claim → `issue-implementer` 디스패치 → `branch-promoter` (Closes #N suppress when missing, PR #685 F3) → PR open |
| (모든 epic) | `autopilot task release-stale --before 1h` (**30m cron**, PR #688 F5) | stale Wip 자동 회수 — worker crash / ctrl-C / worktree 파괴 시 attempts 감소 후 Ready 로 복귀 |

자동화된 lifecycle:

```
gap/qa/ci-watch (writer cron) ──task add──> Ready
                                              │  work-ledger cron (10m)
                                              ▼
                                             Wip ──fail (retried)──> Ready (attempts++)
                                              │   fail (escalated, attempts >= max)──> Escalated
                                              │   complete --pr <N>──> Done   (merge-prs Step 4/5)
                                              │   release-stale (30m)──> Ready (worker crash 복구)
                                              └─ release ──> Ready (transient infra failures only)
```

`/github-autopilot:autopilot` 시작 시 Step 2.5 (PR #681)에서 epic 상태 스냅샷과 최근 이벤트 5건을 출력합니다 (best-effort). `autopilot stats update --command work-ledger` 도 canonical 목록에 포함되어 (PR #687 F4) 모든 loop의 통계가 일관되게 수집됩니다.

Ledger 쓰기는 GitHub issue 흐름의 **보조 observer**입니다. ledger CLI 실패는 `|| echo WARN ...` 패턴으로 격리되어, GitHub issue 생성/PR 머지 결과를 절대 무효화하지 않습니다.

### Follow-up 처리 현황 (모두 해소)

- **F1 (PR #686)** — `merge-prs` Step 4 fast-path 도 `find-by-pr` → `task complete --pr` inline 호출 (best-effort).
- **F2 (PR #684)** — `/github-autopilot:work-ledger` 가 autopilot Step 2 에서 10m cron 으로 자동 등록.
- **F3 (PR #685)** — `branch-promoter` 가 `issue_number` 누락 시 PR body 에서 `Closes #N` 줄 suppress (깨진 링크 방지).
- **F4 (PR #687)** — `autopilot stats update --command work-ledger` 가 canonical 목록에 포함 (`--command` 는 free string 유지).
- **F5 (PR #688)** — `autopilot task release-stale --before <duration>` + 30m cron 으로 stale Wip 자동 회수 (idempotent).

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
