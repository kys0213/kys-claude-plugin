# github-autopilot Acceptance Runbook

ledger-integration 시리즈(PR #662 / #663 / #664 / #665 / #666 / #674 / #681) 머지 이후, **신규 사용자가 플러그인을 설치하고 기존 autopilot 흐름과 새 ledger 흐름을 모두 검증할 수 있도록** 단계별로 정리한 runbook입니다.

이 문서의 모든 출력은 실제로 `plugins/github-autopilot/cli/target/release/autopilot` 바이너리(v0.22.0)에 대해 실행한 결과입니다. 임의로 만들어낸 출력이 아닙니다.

---

## Section A: Pre-flight

### A.1 바이너리 빌드

레포지토리 루트에서:

```bash
cargo build --release --manifest-path plugins/github-autopilot/cli/Cargo.toml
./plugins/github-autopilot/cli/target/release/autopilot --version
# autopilot 0.22.0
```

이후 모든 섹션은 레포 루트에서 `BIN=plugins/github-autopilot/cli/target/release/autopilot` 를 가정합니다.

### A.2 플러그인 설정 초기화

```bash
/github-autopilot:setup
```

기대 동작:
- 프로젝트 루트에 `github-autopilot.local.md` (frontmatter YAML) 생성
- user scope (`~/.claude/settings.json`) 에 hook 설치
- GitHub 라벨 일괄 생성 (`autopilot:ready`, `autopilot:wip`, `autopilot:ci-failure`, `autopilot:auto`)

### A.3 환경 변수

ledger DB 경로는 기본적으로 `~/.local/share/autopilot/autopilot.db` (또는 `autopilot.toml`의 설정값) 입니다. 운영 환경과 격리하여 검증하려면:

```bash
export AUTOPILOT_DB_PATH=/tmp/autopilot-runbook.db
```

> 본 runbook의 Sections C/D는 격리된 DB 경로를 가정합니다.

---

## Section B: Existing Autopilot Regression Smoke

ledger-integration 6개 PR이 머지된 이후에도 기존 7개 watcher와 setup/autopilot/test-watch 가 동일하게 동작해야 합니다. 각 커맨드를 1회씩 실행하여 회귀 없는지 확인합니다.

| # | 커맨드 | 변경 영향 (T1-T6) | 기대 동작 |
|---|--------|--------------------|----------|
| 1 | `/github-autopilot:setup` | 없음 | 설정 파일 생성 + 라벨 생성 (A.2와 동일) |
| 2 | `/github-autopilot:autopilot` | **Step 2.5 추가** (PR #681) — cron 등록 직후 ledger 상태 스냅샷 출력 (best-effort) | 기존 7개 cron 등록 + 새 Step 2.5 출력. 등록 실패 없음 |
| 3 | `/github-autopilot:gap-watch` | **Step 5a 추가** (PR #662, #663) — ledger epic 부트스트랩 + per-issue ledger task 쓰기 (observer) | 기존 GitHub issue 생성 흐름 그대로. ledger 실패 시 `WARN: ...` 로그만 |
| 4 | `/github-autopilot:ci-watch` | **Step 5a/5c 추가** (PR #664) — `ci-backlog` epic 부트스트랩 + per-failure ledger task 쓰기 (observer) | 기존 CI 분석 + issue 생성 그대로 |
| 5 | `/github-autopilot:qa-boost` | **Step 5.5 추가** (PR #665) — `qa-backlog` epic 부트스트랩 + per-finding ledger task 쓰기 (observer) | 기존 테스트 갭 분석 + issue 생성 그대로 |
| 6 | `/github-autopilot:build-issues` | 없음 | 기존 ready 이슈 → draft → PR 흐름 그대로 |
| 7 | `/github-autopilot:merge-prs` | **간접 영향** — Step 5의 pr-merger 에이전트가 머지 후 ledger close-the-loop 호출 (PR #666). Step 4 (all-green fast path)는 변경 없음 | all-green PR은 기존 squash merge. 문제 PR은 pr-merger가 처리 후 ledger 닫기 시도 |
| 8 | `/github-autopilot:analyze-issue` | 없음 | 기존 라벨 부여 흐름 그대로 |
| 9 | `/github-autopilot:ci-fix` | 없음 | 기존 tick 단위 CI 수정 그대로 |
| 10 | `/github-autopilot:test-watch <suite>` | 없음 | 기존 테스트 스위트 실행 그대로 |
| 11 | `/github-autopilot:work-ledger` | **신규 명령** (PR #674) — 첫 reader. 기존 명령에는 영향 없음 | round-robin claim → issue-implementer → branch-promoter → PR open |

**Section B 회귀 판정**: 모든 기존 커맨드는 ledger 호출 실패와 무관하게 정상 종료해야 합니다. 모든 ledger 추가 단계는 best-effort observer 패턴 (`|| echo WARN ...`) 으로 격리되어 있습니다.

### B.1 알려진 갭 (회귀 아님, follow-up)

- **`merge-prs.md` Step 4 fast-path는 ledger close-the-loop을 호출하지 않습니다**. all-green PR을 `gh pr merge` 로 직접 머지하므로, 해당 PR을 소유한 ledger task 는 Wip로 남게 됩니다. PR #666의 follow-up 항목 — Step 4도 pr-merger 경유로 통일하거나 inline ledger close 호출을 추가해야 함.
- `branch-promoter` 가 `issue_number` 누락 시 PR body에 `Closes #N` 을 어떻게 처리하는지 명시되지 않았습니다 (PR #674 follow-up #1).
- `autopilot stats update --command work-ledger` 의 허용 여부가 미확인 (PR #674 follow-up #2).

---

## Section C: Ledger Lifecycle Smoke

기존 흐름과 독립적으로 ledger lifecycle (epic create → task add → claim → fail/escalate → complete → events) 을 직접 검증합니다. **모든 출력은 실제 v0.22.0 바이너리 실행 결과입니다.**

### C.0 격리된 DB 준비

```bash
BIN=plugins/github-autopilot/cli/target/release/autopilot
export AUTOPILOT_DB_PATH=/tmp/runbook-smoke.db
rm -f $AUTOPILOT_DB_PATH
```

### C.1 `epic create --idempotent` (PR #663)

**(a) 신규 생성**

```bash
$BIN epic create --name gap-backlog --spec spec/gap-backlog.md --idempotent
```

```
epic 'gap-backlog' created
exit=0
```

**(b) 동일 spec_path 재실행 → 멱등 (exit 0)**

```bash
$BIN epic create --name gap-backlog --spec spec/gap-backlog.md --idempotent
```

```
epic 'gap-backlog' already exists (idempotent)
exit=0
```

**(c) 다른 spec_path 재실행 → 의미 충돌이므로 exit 1**

```bash
$BIN epic create --name gap-backlog --spec spec/different.md --idempotent
```

```
epic 'gap-backlog' already exists with different spec_path: spec/gap-backlog.md (requested spec/different.md)
exit=1
```

**(d) 나머지 backlog 부트스트랩**

```bash
$BIN epic create --name qa-backlog --spec spec/qa-backlog.md --idempotent
$BIN epic create --name ci-backlog --spec spec/ci-backlog.md --idempotent
```

```
epic 'qa-backlog' created
epic 'ci-backlog' created
```

### C.2 `task add` (writer 시뮬레이션)

writer 3종이 내부적으로 호출하는 형태 — fingerprint로부터 결정적 12-hex id를 만들고 task를 기록합니다.

```bash
FP1="gap:spec/auth.md:token-refresh"; TID1=$(printf '%s' "$FP1" | shasum -a 256 | cut -c1-12)
$BIN task add "$TID1" --epic gap-backlog --title "spec gap: token-refresh in spec/auth.md" --fingerprint "$FP1" --source gap-watch

FP2="qa:src/auth/refresh.rs:unit"; TID2=$(printf '%s' "$FP2" | shasum -a 256 | cut -c1-12)
$BIN task add "$TID2" --epic qa-backlog --title "test(auth): add unit tests for refresh.rs" --fingerprint "$FP2" --source qa-boost

FP3="ci:validate.yml:main:test-failure"; TID3=$(printf '%s' "$FP3" | shasum -a 256 | cut -c1-12)
$BIN task add "$TID3" --epic ci-backlog --title "ci: validate.yml test-failure on main" --fingerprint "$FP3" --source ci-watch
```

```
inserted task 905ad8424947
inserted task 45ca5bfe4667
inserted task 5a4f4a5482ee
```

### C.3 `task list`

```bash
$BIN task list --epic gap-backlog
```

```
ID            STATUS     ATTEMPTS  TITLE
905ad8424947  ready             0  spec gap: token-refresh in spec/auth.md
```

(qa-backlog / ci-backlog 도 동일하게 ready / 0 / 1건씩.)

### C.4 `task claim` (Ready → Wip)

```bash
$BIN task claim --epic gap-backlog --json
```

```json
{"id":"905ad8424947","epic_name":"gap-backlog","source":"gap-watch","fingerprint":"gap:spec/auth.md:token-refresh","title":"spec gap: token-refresh in spec/auth.md","body":null,"status":"wip","attempts":1,"branch":null,"pr_number":null,"escalated_issue":null,"created_at":"2026-05-05T01:26:57.564510Z","updated_at":"2026-05-05T01:27:03.900113Z"}
```

### C.5 `task fail` 사이클 — retried → escalated

`fail`은 attempts를 증가시키며, max_attempts(기본 3) 도달 시 `escalated`로 자동 전환합니다.

```bash
# 1차
$BIN task fail 905ad8424947
# {"outcome":"retried","attempts":1}

$BIN task list --epic gap-backlog
# 905ad8424947  ready             1  spec gap: ...

# 2차 (claim → fail)
$BIN task claim --epic gap-backlog --json > /dev/null
$BIN task fail 905ad8424947
# {"outcome":"retried","attempts":2}

# 3차 (claim → fail → 자동 escalate)
$BIN task claim --epic gap-backlog --json > /dev/null
$BIN task fail 905ad8424947
# {"outcome":"escalated","attempts":3}

$BIN task list --epic gap-backlog
# 905ad8424947  escalated         3  spec gap: ...
```

### C.6 `task complete --pr <N>` (pr-merger close-the-loop 시뮬레이션)

```bash
$BIN task claim --epic qa-backlog --json > /dev/null
$BIN task complete 45ca5bfe4667 --pr 999
```

```
completed task 45ca5bfe4667 (PR #999)
newly ready: (none)
```

### C.7 `task find-by-pr` (pr-merger Step 4의 핵심)

```bash
$BIN task find-by-pr 999
```

```
id:              45ca5bfe4667
epic:            qa-backlog
status:          done
source:          qa-boost
attempts:        1
title:           test(auth): add unit tests for refresh.rs
pr_number:       999
fingerprint:     qa:src/auth/refresh.rs:unit
```

존재하지 않는 PR은 exit 1:

```bash
$BIN task find-by-pr 12345; echo "exit=$?"
# no task owns PR #12345
# exit=1
```

### C.8 `epic status` 및 `events list`

```bash
$BIN epic status
```

```
EPIC               STATUS     PEND READY  WIP  BLK DONE  ESC TOTAL
ci-backlog         active        0     1    0    0    0    0     1
gap-backlog        active        0     0    0    0    0    1     1
qa-backlog         active        0     0    0    0    1    0     1
```

`autopilot.md` Step 2.5 (PR #681) 이 사용하는 JSON 모드:

```bash
$BIN epic status gap-backlog --json
```

```json
[{"epic":"gap-backlog","status":"active","total":1,"counts":{"pending":0,"ready":0,"wip":0,"blocked":0,"done":0,"escalated":1}}]
```

최근 이벤트 5건:

```bash
$BIN events list --limit 5
```

```
AT                            KIND                  EPIC                TASK          PAYLOAD-SUMMARY
2026-05-05T01:26:49.702408+00:00  epic_started          gap-backlog         -             {}
2026-05-05T01:26:49.716329+00:00  epic_started          qa-backlog          -             {}
2026-05-05T01:26:49.721379+00:00  epic_started          ci-backlog          -             {}
2026-05-05T01:26:57.564510+00:00  task_inserted         gap-backlog         905ad8424947  {"fingerprint":"gap:spec/auth.md:token-r…
2026-05-05T01:26:57.580799+00:00  task_inserted         qa-backlog          45ca5bfe4667  {"fingerprint":"qa:src/auth/refresh.rs:u…
```

---

## Section D: Ledger Reader Smoke (`/github-autopilot:work-ledger`)

`work-ledger`는 첫 reader 입니다. epic당 1개씩 round-robin claim 한 후 `issue-implementer` 에 디스패치합니다. 본 섹션은 CLI 호출만 시뮬레이션합니다 (실제 에이전트 디스패치는 Claude Code 세션에서 `/github-autopilot:work-ledger` 실행).

### D.0 격리된 DB + 1개 epic만 task 보유 시드

```bash
export AUTOPILOT_DB_PATH=/tmp/runbook-reader.db
rm -f $AUTOPILOT_DB_PATH

for E in gap-backlog qa-backlog ci-backlog; do
  $BIN epic create --name "$E" --spec "spec/$E.md" --idempotent
done

$BIN task add d1seed --epic gap-backlog \
  --title "Add /healthz endpoint" \
  --source gap-watch \
  --fingerprint "gap:spec/api.md:healthz" \
  --body "Spec requires /healthz returning 200"
```

```
epic 'gap-backlog' created
epic 'qa-backlog' created
epic 'ci-backlog' created
inserted task d1seed
```

### D.1 Round-robin claim across 3 epics

work-ledger Step 4가 수행하는 `gap-backlog → qa-backlog → ci-backlog` 순서. 빈 epic은 exit 1 (정상) 후 다음 epic 진행.

```bash
for E in gap-backlog qa-backlog ci-backlog; do
  echo "--- claim --epic $E ---"
  $BIN task claim --epic "$E" --json
  echo "  exit=$?"
done
```

```
--- claim --epic gap-backlog ---
{"id":"d1seed","epic_name":"gap-backlog","source":"gap-watch","fingerprint":"gap:spec/api.md:healthz","title":"Add /healthz endpoint","body":"Spec requires /healthz returning 200","status":"wip","attempts":1,"branch":null,"pr_number":null,"escalated_issue":null,"created_at":"2026-05-05T01:27:18.358061Z","updated_at":"2026-05-05T01:27:18.363016Z"}
  exit=0
--- claim --epic qa-backlog ---
(no ready tasks on epic 'qa-backlog')
  exit=1
--- claim --epic ci-backlog ---
(no ready tasks on epic 'ci-backlog')
  exit=1
```

### D.2 epic status (after claim)

```bash
$BIN epic status
```

```
EPIC               STATUS     PEND READY  WIP  BLK DONE  ESC TOTAL
ci-backlog         active        0     0    0    0    0    0     0
gap-backlog        active        0     0    1    0    0    0     1
qa-backlog         active        0     0    0    0    0    0     0
```

`gap-backlog` 의 `WIP` 카운트가 1로 증가한 것이 reader 가 task를 점유했음을 보여줍니다. 이 상태에서 `issue-implementer` 가 구현을 마치고 `branch-promoter` 가 PR을 열면 `pr-merger` 의 close-the-loop 단계 (Section C.6) 가 PR 머지 시 `task complete --pr <N>` 으로 Wip→Done 전환을 닫습니다.

### D.3 work-ledger 명령 자체 실행

Claude Code 세션에서:

```
/github-autopilot:work-ledger
```

기대 출력 형태 (실제 환경에 따라 다름):

```
[STEP 3] epic 부트스트랩 완료: gap-backlog, qa-backlog, ci-backlog
[STEP 4] claimed: gap-backlog/d1seed
[STEP 4] claimed: (qa-backlog empty — skip)
[STEP 4] claimed: (ci-backlog empty — skip)
[STEP 5] dispatching 1 task (max_parallel_agents=3, single subgroup)
[STEP 6] d1seed → success → PR #142 (Wip)
```

빈 큐인 경우:

```
[STEP 3] epic 부트스트랩 완료
[STEP 4] claimed: (none — all 3 epics empty)
[STEP 7] Ready task 없음 — cycle 종료
```

---

## Section E: Troubleshooting

### E.1 DB 경로 / 환경변수

| 상황 | 해결 |
|------|------|
| ledger 명령이 `~/.local/share/autopilot/autopilot.db` 를 쓴다 | 기본값. `AUTOPILOT_DB_PATH` 환경변수 또는 `autopilot.toml` 의 설정값으로 변경 가능 |
| 여러 프로젝트 병행 시 ledger가 섞임 | 프로젝트 루트에 `autopilot.toml` 을 두고 DB 경로를 분리 |
| 검증/실험 중 운영 DB 오염 우려 | `export AUTOPILOT_DB_PATH=/tmp/autopilot-smoke.db` 로 격리 (Section C.0 / D.0 참고) |

### E.2 멱등 재부트스트랩

`epic create --idempotent` (PR #663) 는 동일 `(name, spec_path)` 가 이미 있으면 exit 0. 쉘 한 줄로 안전하게 재실행 가능합니다:

```bash
for E in gap-backlog qa-backlog ci-backlog; do
  $BIN epic create --name "$E" --spec "spec/$E.md" --idempotent || \
    echo "WARN: $E 부트스트랩 실패 — 다음 cycle에서 재시도"
done
```

`task add` 는 동일 id가 이미 있으면 exit 1 + `task '<id>' already exists` — `|| echo WARN ...` 로 흡수합니다 (PR #662 / #664 / #665 의 writer 패턴).

### E.3 stale Wip 복구

`work-ledger` 가 task 를 claim 한 후 `issue-implementer` 가 크래시했거나 PR 을 열지 못해 `task fail` 호출조차 실패한 경우, task 는 Wip 로 남습니다. 운영자 수동 복구:

```bash
# 어떤 task가 stale Wip 인지 확인
$BIN task list --epic gap-backlog | awk '$2=="wip"'

# Wip → Ready 강제 전환 (attempts 보존)
$BIN task release <task_id>

# Wip → 명시적 status 강제 변경 (operator override)
$BIN task force-status <task_id> ready
```

자동 stale-Wip 감지는 PR #674 의 follow-up 항목입니다 (lease/heartbeat 도입 필요).

### E.4 ledger ↔ GitHub issue 정합성 점검

writer 3종은 동일 fingerprint 로 GitHub issue 와 ledger task 를 동시에 생성하지만, 한쪽만 성공하는 경우가 발생할 수 있습니다 (네트워크 / SQLite 락 등):

```bash
# fingerprint 로 ledger task 존재 확인
FP="gap:spec/auth.md:token-refresh"
TID=$(printf '%s' "$FP" | shasum -a 256 | cut -c1-12)
$BIN task list --epic gap-backlog | grep "$TID"

# 같은 fingerprint 의 GitHub issue
gh issue list --search "$FP" --label "autopilot:ready"
```

ledger 만 있고 issue 가 없으면 다음 writer cycle 에서 동일 fingerprint 로 새 issue 가 생성되며, ledger task add 는 duplicate id 로 흡수됩니다 (안전).

### E.5 `merge-prs` Step 4 fast-path 미지원

all-green PR 은 `merge-prs` Step 4 에서 직접 `gh pr merge` 되므로 ledger close-the-loop 이 호출되지 않습니다 (B.1 follow-up). 임시 우회:

```bash
# 머지된 PR 번호로 수동 close
$BIN task find-by-pr <PR_NUMBER> --json | jq -r '.id' \
  | xargs -I{} $BIN task complete {} --pr <PR_NUMBER>
```

근본 해결은 `merge-prs.md` Step 4 도 pr-merger 경유로 통일하거나 inline ledger 호출을 추가하는 follow-up PR 입니다.

---

## 참고 PR

| PR | 변경 |
|----|------|
| #662 | gap-watch ledger writer (gap-backlog, 첫 pilot) |
| #663 | `epic create --idempotent` 플래그 (CLI) |
| #664 | ci-watch ledger writer (ci-backlog) |
| #665 | qa-boost ledger writer (qa-backlog) |
| #666 | pr-merger close-the-loop (`task find-by-pr` → `task complete --pr`) |
| #674 | `/github-autopilot:work-ledger` (첫 reader) |
| #681 | `autopilot.md` Step 2.5 ledger 상태 스냅샷 |
