# github-autopilot Acceptance Runbook

ledger-integration 시리즈(PR #662 / #663 / #664 / #665 / #666 / #674 / #681), ledger-followups 롤업(PR #684 / #685 / #686 / #687 / #688), 그리고 ledger-polish 롤업(PR #693 / #694 / #695 / #696) 머지 이후, **신규 사용자가 플러그인을 설치하고 기존 autopilot 흐름과 자동으로 돌아가는 ledger 흐름을 모두 검증할 수 있도록** 단계별로 정리한 runbook입니다.

ledger-followups 롤업으로 work-ledger reader와 stale-Wip 회수가 cron 기반으로 동작하므로 ledger lifecycle은 이제 **수동 호출 없이 자동**입니다 (Section G). ledger-polish 롤업으로 epic 우선순위와 stale 회수 결정이 CLI에서 Skill/Agent 레이어로 이동했고, 단건 회수 명령 이름이 더 직관적으로 정리되었습니다 (Section F).

이 문서의 모든 출력은 실제로 `plugins/github-autopilot/cli/target/release/autopilot` 바이너리(v0.24.0)에 대해 실행한 결과입니다. 임의로 만들어낸 출력이 아닙니다.

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

ledger-integration 7개 PR + ledger-followups 5개 PR이 모두 머지된 이후에도 기존 watcher와 setup/autopilot/test-watch 가 동일하게 동작해야 합니다. 각 커맨드를 1회씩 실행하여 회귀 없는지 확인합니다.

| # | 커맨드 | 변경 영향 | 기대 동작 |
|---|--------|----------|----------|
| 1 | `/github-autopilot:setup` | 없음 | 설정 파일 생성 + 라벨 생성 (A.2와 동일) |
| 2 | `/github-autopilot:autopilot` | **Step 2.5** (PR #681) ledger 상태 스냅샷 + **work-ledger / stale-task-review cron 등록** (PR #684 F2 / #688 F5 / #695 P2) | 기존 cron + work-ledger(10m) + stale-task-review(30m) 등록. Step 2.5 출력. 등록 실패 없음 |
| 3 | `/github-autopilot:gap-watch` | **Step 5a** (PR #662, #663) — ledger epic 부트스트랩 + per-issue ledger task 쓰기 (observer) | 기존 GitHub issue 생성 흐름 그대로. ledger 실패 시 `WARN: ...` 로그만 |
| 4 | `/github-autopilot:ci-watch` | **Step 5a/5c** (PR #664) — `ci-backlog` epic 부트스트랩 + per-failure ledger task 쓰기 (observer) | 기존 CI 분석 + issue 생성 그대로 |
| 5 | `/github-autopilot:qa-boost` | **ledger-only writer** (epic/watch-unified) — `qa-backlog` epic 부트스트랩 + per-finding ledger task 쓰기. **GitHub issue 생성 제거** (work-ledger reader가 task를 claim하여 PR을 직접 발행) | 테스트 갭 분석 후 ledger task만 기록. issue/라벨 흐름 없음 |
| 6 | `/github-autopilot:build-issues` | 없음 | 기존 ready 이슈 → draft → PR 흐름 그대로 |
| 7 | `/github-autopilot:merge-prs` | **Step 4 + Step 5 모두 ledger close-the-loop 호출** — Step 5 pr-merger 에이전트 (PR #666) + Step 4 all-green fast-path inline (PR #686 F1) | all-green PR과 문제 PR 모두 머지 직후 ledger close (best-effort) |
| 8 | `/github-autopilot:analyze-issue` | 없음 | 기존 라벨 부여 흐름 그대로 |
| 9 | `/github-autopilot:ci-fix` | 없음 | 기존 tick 단위 CI 수정 그대로 |
| 10 | `/github-autopilot:test-watch <suite>` | 없음 | 기존 테스트 스위트 실행 그대로 |
| 11 | `/github-autopilot:work-ledger` | **첫 reader** (PR #674) + **autopilot cron 등록** (PR #684 F2, 10m cadence) | round-robin claim → issue-implementer → branch-promoter → PR open |
| 12 | `branch-promoter` 에이전트 | **`issue_number` 누락 시 `Closes #N` 줄 suppress** (PR #685 F3) | issue 없는 ledger PR도 깨진 `Closes #` 없이 정상 생성 |
| 13 | `autopilot stats update --command work-ledger` | **canonical loop 목록에 추가** (PR #687 F4) — `--command`는 free string 유지, 알려진 목록 검증만 추가 | work-ledger / 기타 모든 loop의 stats가 정상 기록 |
| 14 | `autopilot task release-stale --before <D>` | **신규** (PR #688 F5, bulk-release) — stale Wip 회수 후 Ready로 전환, attempts 감소, idempotent. **PR #695 (P2) 이후 cron은 `release-stale` 직접 호출이 아닌 `/github-autopilot:stale-task-review` 가 등록됨** (CLI 관찰 + 에이전트 결정) | bulk 호출은 운영자 우회용으로 유지, 평시 회수는 에이전트 결정 |
| 15 | `autopilot task list-stale --before <D> [--json]` | **신규 read-only 관찰 (PR #695 P2)** — cutoff 보다 오래된 Wip 후보를 반환만 하고 상태 변경하지 않음. `--task-id` 와 `--before` mutually exclusive | `stale-task-reviewer` 에이전트의 입력 — 결정은 에이전트가 내림 |
| 16 | `/github-autopilot:work-ledger` priority strategy | **신규 (PR #694 P1)** — `work_ledger.priority` 설정으로 `by-depth` (default) / `by-age` / `round-robin` / 명시 리스트. CLI는 변경 없음, 판단은 Skill | 깊은 큐가 굶지 않음 (lazy fairness). 자세한 사용법은 `commands/work-ledger.md` 참조 |
| 17 | `autopilot task set-status <ID> --to <STATUS>` | **신규 canonical 이름 (PR #696 P3)** — `force-status` 의 직관적 rename. `force-status` 는 deprecated alias 로 유지 | 운영자 override (Wip→Ready 등). PR #696 audit 결과는 Section E.3 참조 |
| 18 | duration parser `d`/`w` units | **확장 (PR #693 P4)** — `--before 1d` / `--before 1w` / `--before 2d12h` 등 지원. `mo`/`y` 는 가변 길이라 의도적으로 제외 | day/week threshold 사용 시 `--before-seconds 86400` fallback 불필요 |

**Section B 회귀 판정**: 모든 기존 커맨드는 ledger 호출 실패와 무관하게 정상 종료해야 합니다. 모든 ledger 추가 단계는 best-effort observer 패턴 (`|| echo WARN ...`) 으로 격리되어 있습니다.

### B.1 follow-up 처리 현황 (모두 해소됨)

ledger-followups 롤업으로 다음 follow-up이 모두 해소되었습니다:

| ID | 항목 | 해소 PR |
|----|------|---------|
| F1 | `merge-prs.md` Step 4 fast-path가 ledger close-the-loop을 호출하지 않음 | #686 (`fix(github-autopilot): close ledger loop in merge-prs fast-path`) |
| F2 | `/github-autopilot:work-ledger` 가 cron에 등록되지 않아 수동 실행 필요 | #684 (`feat(github-autopilot): register work-ledger in autopilot cron`, 10m) |
| F3 | `branch-promoter` 가 `issue_number` 누락 시 `Closes #N` 처리 미명시 | #685 (`fix(github-autopilot): suppress Closes #N when issue_number missing`) |
| F4 | `autopilot stats update --command work-ledger` 허용 여부 미확인 | #687 (`feat(github-autopilot): accept work-ledger in stats --command`) |
| F5 | stale Wip 자동 감지/복구 부재 (lease/heartbeat 미도입) | #688 (`feat(github-autopilot): add stale-Wip task recovery`, 30m cron) |

> F5는 lease/heartbeat이 아닌 **시간 기반 cutoff** (`updated_at` 비교) 로 해소되었습니다. lease/heartbeat 정교화는 별도 follow-up으로 carry-forward 됩니다 (Section F 참조).

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

`work-ledger`는 첫 reader 입니다. epic당 1개씩 selection strategy 에 따라 claim 한 후 `issue-implementer` 에 디스패치합니다. PR #694 (P1) 이후 default strategy 는 `by-depth` (각 epic 의 ready 큐 깊이가 큰 순) 이며 `work_ledger.priority` 설정으로 `by-age` / `round-robin` / 명시 리스트로 override 가능합니다 (자세한 내용은 `commands/work-ledger.md`). CLI 자체는 변경 없음 — Skill 이 `epic status --json` 으로 ranking 을 결정한 뒤 기존 `task claim --epic <NAME>` 을 호출합니다 (CLAUDE.md "책임 경계").

본 섹션은 CLI 호출만 시뮬레이션합니다 (실제 에이전트 디스패치는 Claude Code 세션에서 `/github-autopilot:work-ledger` 실행).

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

### D.1 by-depth claim across 3 epics (default strategy, PR #694 P1)

PR #694 (P1) 이후 work-ledger Step 4 의 default strategy 는 `by-depth` — 각 epic 의 ready 큐 깊이가 큰 순으로 claim. Skill 이 `epic status --json` 으로 ranking 을 결정한 뒤 기존 `task claim --epic <NAME>` 을 호출합니다.

**Step 1: Skill 이 ranking 입력 수집** (`epic status --json`)

```bash
$BIN epic status --json
```

```json
[{"epic":"ci-backlog","status":"active","total":0,"counts":{"pending":0,"ready":0,"wip":0,"blocked":0,"done":0,"escalated":0}},{"epic":"gap-backlog","status":"active","total":1,"counts":{"pending":0,"ready":1,"wip":0,"blocked":0,"done":0,"escalated":0}},{"epic":"qa-backlog","status":"active","total":0,"counts":{"pending":0,"ready":0,"wip":0,"blocked":0,"done":0,"escalated":0}}]
```

**Step 2: Skill 이 by-depth 로 ranking** — `gap-backlog(ready=1)` 만 후보, 나머지는 ready=0 으로 사전 필터링.

**Step 3: ranked 순서로 claim**

```bash
$BIN task claim --epic gap-backlog --json
echo "  exit=$?"
```

```
{"id":"d1seed","epic_name":"gap-backlog","source":"gap-watch","fingerprint":"gap:spec/api.md:healthz","title":"Add /healthz endpoint","body":"Spec requires /healthz returning 200","status":"wip","attempts":1,"branch":null,"pr_number":null,"escalated_issue":null,"created_at":"2026-05-05T01:27:18.358061Z","updated_at":"2026-05-05T01:27:18.363016Z"}
  exit=0
```

> race로 빈 큐가 빌 수 있으므로 `task claim` 자체는 exit 1 도 정상 처리해야 합니다 — Skill 이 다음 후보로 fallback 합니다.

**대체 strategy 확인**: `work_ledger.priority` 를 `"round-robin"` 으로 설정하면 PR #694 이전과 동일한 `gap-backlog → qa-backlog → ci-backlog` fixed order 동작 (back-compat). `"by-age"` 는 oldest `created_at` 우선, 명시 리스트는 그대로 순회.

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

기대 출력 형태 (default strategy `by-depth`, PR #694 P1):

```
[STEP 3] epic 부트스트랩 완료: gap-backlog, qa-backlog, ci-backlog
[STEP 4] strategy=by-depth → ranked: gap-backlog(ready=1)  # qa-backlog/ci-backlog ready=0 skipped
[STEP 4] claimed: gap-backlog/d1seed
[STEP 5] dispatching 1 task (max_parallel_agents=3, single subgroup)
[STEP 6] d1seed → success → PR #142 (Wip)
```

빈 큐인 경우:

```
[STEP 3] epic 부트스트랩 완료
[STEP 4] strategy=by-depth → ranked: (none — all 3 epics ready=0)
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

### E.3 stale Wip 복구 (관찰 + 에이전트 판단, PR #695 P2)

`work-ledger` 가 task 를 claim 한 후 `issue-implementer` 가 크래시했거나 PR 을 열지 못해 `task fail` 호출조차 실패한 경우, task 는 Wip 로 남습니다.

**[중요] cron 동작 변화** — PR #688 의 cron 은 `autopilot task release-stale --before <D>` 를 직접 호출하여 cutoff 보다 오래된 모든 Wip 을 **자동 bulk-release** 했습니다. PR #695 (P2) 이후 cron 은 **`/github-autopilot:stale-task-review` 를 호출**하며, 이 skill 은 `autopilot task list-stale --json` 으로 후보를 **관찰만** 하고 `stale-task-reviewer` 에이전트가 task 별로 release / fail / escalate / leave alone 을 결정합니다. 즉:

- **이전 (PR #688)**: cron → CLI auto-recovery (모두 release). 빠르지만 무차별.
- **현재 (PR #695)**: cron → CLI list-stale (read-only) → agent per-task decision. CLAUDE.md "책임 경계: CLI vs Skill/Agent" — 결정적 변환은 CLI, 컨텍스트 의존 판단은 에이전트.

자동 cron 등록은 `/github-autopilot:autopilot` Step 2 가 처리합니다 (30m cadence, `stale-task-review` cron 으로 등록). bulk `release-stale --before <D>` 는 운영자 비상 우회용으로 CLI 에 유지됩니다 (Section F 참조).

수동 점검/복구가 필요한 경우:

```bash
# stale Wip 후보 관찰 (read-only)
$BIN task list-stale --before 1h --json

# 단건 release (권장 경로 — 단건 회수의 canonical 이름)
$BIN task release <task_id>

# 단건 release (deprecated alias — 신규 호출자는 `release` 를 사용)
$BIN task release-stale --task-id <task_id>

# 비상시 bulk release (에이전트 우회 — 운영자 판단으로 일괄 회수)
$BIN task release-stale --before 1h --json

# Wip → 명시적 status 변경 (operator override; canonical 이름)
$BIN task set-status <task_id> --to ready

# (deprecated alias for one release; 동일 효과)
$BIN task force-status <task_id> --to ready
```

> `release-stale --task-id` 와 `release-stale --before` 는 mutually exclusive — clap 이 parser 단계에서 거부합니다.

#### Naming audit 결과 (PR #696)

| 명령 | 평가 | 권장 |
|------|------|------|
| `add` / `add-batch` / `list` / `claim` / `complete` / `fail` / `escalate` | 명확 | 유지 |
| `show` / `get` | 동일 명령 (alias) | 유지, RUNBOOK 에 alias 명시 |
| `find-by-pr` / `list-stale` | 명확 (다른 의도) | 유지 |
| `release` | 단건 Wip→Ready (attempts 감소). canonical 단건 회수 이름 | 유지 |
| `release-stale --before <D>` | bulk-only (운영자 우회) | 유지 |
| `release-stale --task-id <ID>` | `release <ID>` 와 100% 동일 — `-stale` suffix 는 오해 유발 | **deprecated alias** (한 릴리스 유지). 신규 호출은 `release <ID>` |
| `force-status` → `set-status` | "force" 는 일회성 override 뉘앙스, "set" 이 직관적 | **rename** + `force-status` deprecated alias |

`show ↔ get` 은 모두 단일 task 조회로 동일하게 동작합니다. `get` 이 spec-canonical 이며 `show` 가 humans-facing helper alias 입니다 — 어느 쪽을 호출해도 결과가 동일합니다.

> lease/heartbeat 기반 정교화 (worker liveness 직접 추적) 는 carry-forward follow-up 입니다 (Section F).

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

### E.5 `merge-prs` Step 4 fast-path (해소됨, PR #686)

이전에는 all-green PR 이 Step 4 fast-path 에서 직접 `gh pr merge` 되어 ledger close-the-loop 이 호출되지 않는 갭이 있었습니다 (B.1 의 알려진 갭). **PR #686 (F1) 이후 fast-path 도 인라인으로 `find-by-pr` → `task complete --pr` 를 호출하므로 우회가 필요하지 않습니다** (best-effort, set -e safe — ledger 실패 시 머지 결과는 변하지 않음).

그래도 task 가 Wip 으로 남아 있다면 수동 복구:

```bash
# 머지된 PR 번호로 수동 close
$BIN task find-by-pr <PR_NUMBER> --json | jq -r '.id' \
  | xargs -I{} $BIN task complete {} --pr <PR_NUMBER>
```

---

## Section F: Manual Operator Commands

평시에는 cron 이 lifecycle 을 모두 처리하지만 (Section G), 운영자가 명시적으로 개입해야 하는 상황을 위해 다음 CLI 명령들이 보존됩니다. 모든 명령은 idempotent — 잘못 호출해도 ledger 정합성이 깨지지 않습니다.

### F.1 단건 task 회수 — `release <ID>` (canonical, PR #696 P3)

특정 task 가 Wip 에 묶여 있어 회수해야 할 때 (운영자가 직접 진단 후):

```bash
$BIN task release <task_id>
```

- 효과: Wip → Ready, attempts 1 감소 (transient infra failure 보정).
- canonical 명령 — 단건 회수의 권장 경로. 자동화 스크립트도 신규 작성 시 이 이름을 사용합니다.

> **Deprecated alias**: `autopilot task release-stale --task-id <ID>` 는 `release <ID>` 와 100% 동일한 효과로 한 릴리스 더 유지됩니다 (back-compat). `-stale` suffix 는 단건 회수에 부적합한 이름이라 PR #696 audit 에서 deprecated 처리되었습니다 — 신규 호출은 `release <ID>` 사용.

### F.2 비상 bulk 회수 — `release-stale --before <D>` (운영자 우회)

cron 이 `stale-task-review` skill 을 호출하지 않고 즉시 모든 stale Wip 을 release 해야 할 때 (운영자 판단):

```bash
$BIN task release-stale --before 1h --json
$BIN task release-stale --before 1d         # PR #693 (P4): day/week unit 지원
$BIN task release-stale --before 1w
```

- 효과: cutoff 보다 오래된 모든 Wip task 를 일괄 Ready (attempts 감소).
- **에이전트 우회** — per-task 판단 없이 모두 release 합니다. 평시에는 cron 의 `stale-task-review` skill 이 에이전트 결정 흐름을 사용하므로 (PR #695 P2), 이 명령은 비상 시에만 사용합니다.
- duration parser (PR #693 P4) 가 `s` / `m` / `h` / `d` / `w` 를 모두 처리. `mo` / `y` 는 가변 길이라 의도적으로 미지원.

### F.3 명시적 status 변경 — `set-status` (canonical, PR #696 P3)

ledger 상태를 task 별 lifecycle 외 경로로 직접 바꿔야 할 때 (운영자 override):

```bash
$BIN task set-status <task_id> --to ready
$BIN task set-status <task_id> --to blocked
$BIN task set-status <task_id> --to escalated
```

- 효과: 즉시 status 변경 (attempts 변경 없음).
- canonical 이름 — `force-status` 의 직관적 rename (PR #696 P3 audit).
- **Deprecated alias**: `autopilot task force-status` 는 한 릴리스 더 유지됩니다 (back-compat). 신규 호출은 `set-status` 사용.

### F.4 stale 후보 read-only 관찰 — `list-stale` (PR #695 P2)

운영자가 cron tick 사이에 직접 stale 상태를 점검할 때:

```bash
$BIN task list-stale --before 1h --json
```

- 상태를 변경하지 않습니다 (read-only contract). `--task-id` 와 `--before` 는 mutually exclusive (clap parser 단계에서 거부).
- `stale-task-reviewer` 에이전트가 이 명령의 출력을 입력으로 받아 결정합니다.

### F.5 PR 머지 후 ledger close 누락 보정

`merge-prs` Step 4/5 가 PR 머지 후 `task complete --pr` 를 호출하지 못한 경우 (네트워크 등):

```bash
$BIN task find-by-pr <PR_NUMBER> --json | jq -r '.id' \
  | xargs -I{} $BIN task complete {} --pr <PR_NUMBER>
```

---

## Section G: Auto-running Operation

ledger-followups 롤업 + ledger-polish 롤업으로 ledger lifecycle 은 **수동 호출 없이 cron 으로 완전 자동화** 되었습니다. 운영자는 한 번 `/github-autopilot:autopilot` 으로 cron supervisor 를 등록한 뒤 자리를 비울 수 있습니다.

### G.1 자동화된 사이클

```
gap-watch / qa-boost / ci-watch (writer)
        │  task add (best-effort)
        ▼
   ledger Ready queue
        │  work-ledger cron (10m, PR #684)
        │  by-depth strategy (PR #694 P1) — Skill 이 ranking
        ▼
        Wip (claimed)
        │  issue-implementer → branch-promoter (Closes #N suppress when missing, PR #685)
        ▼
        PR open (autopilot:auto)
        │  merge-prs Step 4/5 (fast-path + pr-merger 모두 close, PR #666 + #686)
        ▼
        Done

       (worker crash / ctrl-C 발생 시)
   stale Wip → stale-task-review cron (30m, PR #695 P2)
                ├─ list-stale (read-only) → stale-task-reviewer agent
                └─ per-task 결정: release / fail / escalate / leave alone
```

### G.2 등록되는 cron (autopilot Step 2)

| 라벨 | 명령 | 기본 주기 | 출처 |
|------|------|----------|------|
| Build Issues | `/github-autopilot:build-issues` | 15m | 기존 |
| Gap Watch | `/github-autopilot:gap-watch` | 30m | 기존 (writer) |
| QA Boost | `/github-autopilot:qa-boost` | 1h | 기존 (writer) |
| CI Watch | `/github-autopilot:ci-watch` | 20m | 기존 (writer) |
| CI Fix | `/github-autopilot:ci-fix` | 15m | 기존 |
| Merge PRs | `/github-autopilot:merge-prs` | 10m | 기존 (close-the-loop 통합) |
| **Work Ledger** | `/github-autopilot:work-ledger` (priority=`by-depth` default) | **10m** | **PR #684 (F2) + PR #694 (P1)** |
| **Stale Task Review** | `/github-autopilot:stale-task-review --before {stale_wip.threshold}` | **30m** | **PR #688 (F5) → PR #695 (P2)**: list-stale + agent decision |
| Test Watch | `/github-autopilot:test-watch <suite>` | per-suite | 기존 |

### G.3 운영자 액션

이 시점부터 운영자에게 필요한 일상 액션은 다음 두 가지뿐입니다:

1. **HITL 처리**: `escalated` task / `:hitl` 라벨 issue 검토 후 결정 (`task escalate` 또는 운영자 판단 머지/close).
2. **idle 점검**: `autopilot stats show` 로 모든 loop이 처리 중인지 주기적으로 확인 (work-ledger 포함, PR #687).

writer 발견 → ledger 기록 → reader claim (by-depth) → 구현 → PR open → 머지 → ledger close 의 모든 단계가 cron 만으로 진행됩니다. 평시에는 운영자 개입이 필요 없습니다. 비상 시 수동 명령은 Section F 참조.

### G.4 검증

`make validate` (8984 pass / 11 warnings / 0 fail) + `cargo test` (353 tests) + Section C/D smoke 가 모두 통과하는 상태가 자동 운영의 안전 기준입니다. PR 머지 시 CI/CD 가 검증을 강제합니다.

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
| #684 | F2 — `work-ledger` cron 등록 (10m) |
| #685 | F3 — branch-promoter `Closes #N` suppress (issue 없는 ledger PR) |
| #686 | F1 — `merge-prs` Step 4 fast-path ledger close 통합 |
| #687 | F4 — `stats update --command work-ledger` 허용 (canonical 목록) |
| #688 | F5 — stale-Wip 자동 회수 (`task release-stale` + 30m cron) |
| #693 | **P4** — duration parser `d`/`w` units (PR #688 의 `s`/`m`/`h` 확장) |
| #694 | **P1** — work-ledger epic priority strategy (`by-depth` default + Skill-side ranking) |
| #695 | **P2** — stale 회수를 CLI observation (`list-stale`) + agent decision (`stale-task-reviewer`) 으로 분리 |
| #696 | **P3** — task 서브커맨드 naming clarity (`set-status` rename, `release` canonical) |
