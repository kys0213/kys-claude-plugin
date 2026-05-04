# 01. Use Cases

> Epic 기반 Task Store 가 만족해야 하는 시나리오를 페르소나 관점에서 정의한다. Ledger 모델에서 autopilot 은 상태 관리만 담당하고, 탐지 / 결정 / 구현은 외부 **Agent** 가 한다. 본 문서는 그 책임 분리를 전제로 시나리오를 기술한다.
>
> 각 시나리오는 다음 항목으로 구성된다:
>
> - **트리거** — 시나리오를 시작시키는 외부 이벤트
> - **선행 조건** — 시나리오 시작 시점의 시스템 상태
> - **주 흐름** — 정상 경로의 단계 (Agent ↔ Autopilot CLI)
> - **대안 흐름** — 분기 / 실패 경로
> - **사후 조건** — 시나리오 종료 시점에 보장되어야 하는 상태
> - **관찰 가능 결과** — 외부에서 검증 가능한 부수 효과 (DB 행, git ref, GitHub 이슈/PR)

## 페르소나

| 코드 | 이름 | 역할 |
|------|------|------|
| **OP** | 운영자 (Operator) | autopilot 을 호스트하고 Agent 를 트리거하는 사람 |
| **AG** | 에이전트 (Agent) | Claude Code 세션 / autodev / GitHub Actions 등. monitor 호출 / autopilot CLI 호출 / 구현 / PR 을 직접 수행 |
| **MN** | 모니터 (Monitor) | spec ↔ code 갭을 탐지하는 외부 도구 (예: gap-detector, qa-boost, ci-watch). Agent 가 호출 |
| **AP** | 오토파일럿 (Autopilot) | ledger + CLI. CRUD + 결정적 상태 전이만 담당 |
| **CO** | 협업자 (Collaborator) | 같은 레포에서 일하는 다른 사람 |
| **RV** | 리뷰어 (Reviewer) | PR 을 검토하는 사람 |

페르소나 간 통신 경로:

```
OP → AG : 자연어 요청 ("auth.md 로 epic 시작해줘")
AG → MN : 도구 호출 (spec 분해 / 갭 탐지)
AG → AP : CLI 호출 (epic create / task add / claim / complete / fail / ...)
AP → AG : 종료 코드 + JSON 출력 (claim 한 task / fail 의 outcome / ...)
AG ↔ git remote / GitHub : 직접 git 조작 + gh CLI / API
AG → OP : 알림 (epic 완료 / escalation 필요)
CO → GitHub : 이슈, PR 코멘트 (사람의 통상 흐름)
CO ← AG : escalation 이슈
```

**중요**: AP 는 git / GitHub 를 직접 만지지 않는다. MN 호출도 하지 않는다. 모든 외부 효과는 AG 의 책임.

## UC-1. Epic 시작

> 운영자가 새 spec 으로부터 자율 작업 권한을 부여한다.

- **트리거**: OP 가 AG 에게 "spec/auth.md 로 epic 시작" 요청
- **선행 조건**:
  - 현재 working tree 가 깨끗 (default branch 위치)
  - `spec/auth.md` 파일이 존재
  - 같은 이름의 active epic 이 없음
- **주 흐름**:
  1. AG 가 MN 호출 → 결정적 task_id + 의존성 정보 획득
  2. AG 가 git checkout -b epic/auth-token-refresh && git push
  3. AG 가 `autopilot epic create --name auth-token-refresh --spec spec/auth.md --branch epic/auth-token-refresh`
  4. AG 가 `autopilot task add-batch --epic auth-token-refresh --from <jsonl>` (task + deps 일괄 적재)
  5. AP 가 의존성 검증 (cycle / unknown target) + 진입점 task 들의 status=ready 자동 승격
- **대안 흐름**:
  - 같은 이름 epic 이 active → AP 가 `EpicAlreadyExists` 반환. AG 는 OP 에게 `epic resume` 안내
  - 분해 실패 → AG 가 epic create 호출 전에 중단. AP 상태 변경 없음
  - working tree dirty → AG 의 git 단계에서 실패. AG 가 OP 에게 정리 요청
  - cycle 감지 → AP 의 `add-batch` 가 `DepCycle` 반환. AG 는 epic 을 abandon 하거나 사람에게 escalate
- **사후 조건**:
  - 리모트에 `epic/<name>` 브랜치 존재
  - DB 에 epic 1개 + task N개 (≥1 ready) 존재
- **관찰 가능 결과**:
  - `git ls-remote origin "refs/heads/epic/auth-token-refresh"` 결과 1줄
  - `autopilot events list --epic auth-token-refresh --kind epic_started` 1행

## UC-2. Task 자동 구현 사이클

> 활성 epic 에서 ready task 가 PR 머지까지 진행된다.

- **트리거**: AG 의 build cycle tick (cron / loop / 수동)
- **선행 조건**:
  - 활성 epic ≥1
  - 해당 epic 에 status=ready task ≥1
  - AG 가 자체 정한 동시 구현 한도 미만
- **주 흐름**:
  1. AG 가 `autopilot task claim --epic <name> --json` → ready task 1건 (원자적 wip 전환)
  2. AG 가 worktree 생성 + 코드 작성 + `epic/<name>/<task_id>` 브랜치 push
  3. AG 가 PR 생성 (target=epic 브랜치, 라벨 자유)
  4. AG 가 `autopilot task complete <id> --pr <n>` (또는 PR 머지 후 호출)
  5. AP 가 task.status=done, pr_number 기록 + 의존하던 blocked/pending 의 deps 재평가하여 ready 승격
- **대안 흐름**:
  - claim 시 동시에 다른 호출자가 가져감 → AP 가 None 반환, AG 는 다음 tick 으로 skip
  - push 실패 (UC-11 reject) → AG 가 `autopilot task release <id>` (attempts 차감)
  - 구현/CI 실패 → AG 가 `autopilot task fail <id>` → outcome=Retried 면 다시 ready, Escalated 면 UC-8
- **사후 조건**: 해당 task.status=done, epic 브랜치에 코드 반영
- **관찰 가능 결과**:
  - `events`: task_claimed → task_completed (+ task_unblocked × N)
  - GitHub PR closed-merged

## UC-3. 의존성 있는 Task

> Task B 가 Task A 에 의존할 때, A 완료 전에는 B 가 claim 되지 않는다.

- **트리거**: 분해 결과 deps 가 있음
- **선행 조건**: A.status=ready, B.status=pending, B → A 의존성 등록
- **주 흐름**:
  1. AG 가 `task claim --epic <name>` → AP 가 deps 미충족인 B 는 후보에서 제외, A 만 반환
  2. A 가 done 으로 전이됨 (UC-2)
  3. AP 의 `complete_task_and_unblock` 이 B 의 deps 를 재평가 → 모두 done 이면 B.status=ready (자동, 같은 트랜잭션)
  4. 다음 claim 에서 B 반환 가능
- **대안 흐름**:
  - A 가 escalated → AP 가 자동으로 B 를 blocked 로 전이 (mark_task_failed 의 escalate 분기에서 처리)
  - A → B → A 사이클 발견 → AP 가 `add-batch` 시점에 `DepCycle` 반환
- **사후 조건**: 모든 task 가 deps 순서대로만 wip 진입
- **관찰 가능 결과**: `events` 의 시간순으로 task_completed(A) 가 task_claimed(B) 보다 먼저

## UC-4. Epic 이어받기 (다른 사람 / 다른 머신)

> CO 가 OP 의 휴가 중에 같은 epic 을 이어서 진행한다.

- **트리거**: CO 의 AG 가 epic 재개를 시도
- **선행 조건**:
  - 리모트에 `epic/auth-token-refresh` 브랜치 존재
  - CO 의 로컬 DB 에는 해당 epic 행이 없음 (또는 stale)
- **주 흐름**:
  1. AG 가 git fetch origin
  2. AG 가 MN 호출 → 결정적 task_id 재도출 (= 동일 ID)
  3. AG 가 ls-remote 로 `epic/<name>` + `epic/<name>/*` 브랜치 스캔
  4. AG 가 PR 스캔 (target=epic 브랜치)
  5. AG 가 plan.jsonl 작성: tasks + deps + remote_state + orphan_branches
  6. AG 가 `autopilot epic reconcile --name auth-token-refresh --plan plan.jsonl`
  7. AP 가 plan 을 idempotent 하게 적용:
     - PR merged → done
     - feature 브랜치 + open PR → wip
     - feature 브랜치만 + PR 없음 → wip (stale 후보, events 기록)
     - 브랜치 없음 → ready (deps 만족) / pending
- **대안 흐름**:
  - 분해 결과에 없는 task_id 의 브랜치 → orphan_branches 로 plan 에 포함, AP 가 reconciled.payload 에 기록
  - epic 브랜치 자체가 없음 → AG 가 epic 시작 (UC-1) 안내
- **사후 조건**: CO 의 로컬 DB 가 OP 의 직전 상태와 의미적으로 동일 (캐시 무손실 복구)
- **관찰 가능 결과**: `epic status` 출력이 OP 와 일치

## UC-5. 로컬 DB 손실 후 복구

> OP 의 머신에서 `.autopilot/state.db` 가 삭제됐다.

- **트리거**: DB 파일 부재 상태에서 AG 의 다음 tick
- **선행 조건**: 리모트의 epic 브랜치/PR 은 그대로 존재
- **주 흐름**: UC-4 와 동일 (`epic reconcile` 이 단일 진입점)
- **사후 조건**: DB 가 재구성. 진행 중이던 task 의 attempts 카운터는 0 으로 초기화 (캐시 손실의 허용 손실)
- **관찰 가능 결과**: `events` 에 kind='reconciled' 1행, 직전 attempts 정보는 손실됨

## UC-6. 갭 발견 (활성 epic 매칭 가능)

> AG 가 호출한 MN 이 spec ↔ 코드 갭을 발견했고, 그 spec 은 활성 epic 의 spec 과 일치.

- **트리거**: AG 의 watch tick 에서 MN 호출
- **선행 조건**: 발견된 갭의 spec 경로가 활성 epic 의 `spec_path` 와 일치
- **주 흐름**:
  1. AG 가 `autopilot epic find-by-spec-path <path>` → 매칭 epic 획득
  2. AG 가 `autopilot task add --epic <name> --id <det_id> --fingerprint <fp> --source gap-watch ...`
  3. AP 가 fingerprint 중복 검사 → 신규면 insert, 중복이면 `DuplicateFingerprint(existing_id)` 반환
- **대안 흐름**:
  - 동일 fingerprint task 존재 → AG 가 결과 보고 skip (멱등)
  - spec 이 어떤 활성 epic 에도 매칭 안 됨 → UC-7
- **사후 조건**: 매칭 epic 산하 task 1건 추가 (또는 멱등 skip)
- **관찰 가능 결과**: `tasks` 행 추가, GitHub 이슈 미발행

## UC-7. 갭 발견 (활성 epic 없음)

> 발견된 갭이 어떤 활성 epic 에도 속하지 않는다.

- **트리거**: AG 의 watch tick 에서 MN 호출
- **선행 조건**: 갭의 spec 경로가 활성 epic 의 spec_path 와 매칭 안 됨
- **주 흐름**:
  1. AG 가 `autopilot suppress check --fingerprint <fp> --reason unmatched_watch`
  2. exit 1 (미억제) 이면 AG 가 GitHub 이슈 발행 (`autopilot:hitl-needed` 라벨)
  3. AG 가 `autopilot suppress add --fingerprint <fp> --reason unmatched_watch --until <now+24h>`
  4. AP 는 task 생성 안 함 — epic 미할당 task 는 존재할 수 없다
- **대안 흐름**: suppress check exit 0 (억제 중) → AG 가 이슈 발행 skip
- **사후 조건**: GitHub 이슈 1건 신규, suppression 행 1건 신규
- **관찰 가능 결과**: `events` 에 kind='escalated' (선택), GitHub 이슈 / suppression 테이블

## UC-8. Task 반복 실패 → Escalate

> 한 task 가 max_attempts 까지 실패했다.

- **트리거**: AG 가 IM 의 push 실패 / PR CI 실패를 인지
- **선행 조건**: task.attempts == max_attempts (직전 실패 처리 시점)
- **주 흐름**:
  1. AG 가 `autopilot task fail <id>` → AP 가 outcome 결정:
     - attempts < max → `Retried`. AP 가 status=ready 로 복귀, attempts 보존
     - attempts ≥ max → `Escalated`. AP 가 status=escalated 로 전이 + 의존 task 들 자동 blocked
  2. outcome=Escalated 면 AG 가 GitHub 이슈 발행
  3. AG 가 `autopilot task escalate <id> --issue <n>` → AP 가 escalated_issue 기록 + event
- **대안 흐름**: attempts < max → AG 는 다음 tick 에 재시도
- **사후 조건**: task.status=escalated, 의존 task 들 blocked
- **관찰 가능 결과**: `events`: task_failed → task_escalated, GitHub 이슈 1건

## UC-9. 사람이 Escalation 이슈 처리

> OP/CO 가 escalation 이슈를 받고 결정을 내린다.

- **트리거**: 사람이 GitHub 에서 escalation 이슈를 close 또는 라벨 변경
- **선행 조건**: 이슈 label=`autopilot:hitl-needed` + 본문에 epic/task 메타 포함
- **주 흐름** (이슈 처리 패턴별):
  - **(a) 새 epic 으로 승격**: 사람이 OP 에게 요청 → AG 가 UC-1 흐름. 기존 escalation 이슈는 사람이 close
  - **(b) 기존 epic 에 흡수**: AG 가 issue 분석 후 spec 매칭 → `task add --source human` (UC-6 와 같은 흐름). 이슈 close + 코멘트
  - **(c) 거부 / 무시**: 사람이 close → AG 의 다음 escalation polling 에서 close 감지:
    - reconcile 시도 → status 변화 없음 → AG 가 `autopilot suppress add --reason rejected_by_human --until <now+30d>`
  - **(d) 사람이 직접 코드 수정**: 사람이 코드 push + PR merge + escalation 이슈 close → AG 의 polling 이 close 감지 → `epic reconcile` 호출 → AP 가 task.status=done 으로 전이
- **사후 조건**: 이슈 closed, 결과에 따라 새 task / 기존 task 상태 갱신
- **관찰 가능 결과**: `events` 에 kind='escalation_resolved', payload.resolution=(a|b|c|d 에 해당하는 값)

## UC-10. Epic 완료 → main 머지 (반자동)

> 한 epic 의 모든 task 가 done 이 되었다.

- **트리거**: AG 의 cycle 에서 `autopilot epic status` 가 all-done 보고
- **선행 조건**: epic 산하 task 의 status 가 모두 done (escalated 가 남아 있으면 UC-9 의 처리 후 done 또는 force-status 필요)
- **주 흐름**:
  1. AG 가 `autopilot epic complete <name>` → AP 가 status=completed, completed_at 기록
  2. AG 가 사용자 알림 (notification 채널은 AG 의 책임)
  3. **OP 가 직접** epic → main PR 생성 — 자동화 안 함
- **대안 흐름**:
  - 일부 task 가 escalated 인 채로 남아 있고 사람이 close 만 했음 → AG 가 reconcile 또는 `task force-status <id> --to done --reason "rejected resolved"` 후 epic complete
- **사후 조건**: epic.status=completed
- **관찰 가능 결과**: `events`: epic_completed, AG 의 알림 송신 로그

## UC-11. 두 머신에서 같은 Epic 동시 진행 (충돌 자연 차단)

> OP 와 CO 가 모르는 사이에 같은 epic 을 동시에 resume 하여 같은 task 를 구현하려 한다.

- **트리거**: 두 머신의 AG 가 거의 동시에 동일 task 를 claim
- **선행 조건**: 둘 다 reconcile 후 동일 task 가 ready 로 보임
- **주 흐름**:
  1. AP 의 `claim_next_task` 의 원자적 UPDATE 가 한 쪽만 winner — 다른 쪽은 None 반환 (같은 머신 내 race)
  2. 다른 머신 케이스에선 둘 다 winner 처럼 보일 수 있음 → 하지만 git push 단계에서 한 쪽이 reject
  3. reject 받은 머신의 AG 가 `autopilot task release <id>` 호출 (attempts 차감)
  4. 동일 task 의 PR 은 머신 A 의 것 1개만 존재
- **대안 흐름**: 두 IM 의 코드가 다른 경우에도 git 차원에서 한 쪽만 살아남음. 머신 B 는 작업 결과 폐기
- **사후 조건**: task 1건 = PR 1건. 중복 PR 없음
- **관찰 가능 결과**: 머신 B 의 `events` 에 kind='claim_lost'

## UC-12. 운영자가 Epic 강제 중단

> OP 가 진행 중 epic 을 더 이상 진행하지 않기로 결정한다.

- **트리거**: OP → AG 에게 중단 요청
- **선행 조건**: epic.status=active
- **주 흐름**:
  1. AG 가 `autopilot epic abandon <name>` → AP 가 status=abandoned 로 전이
  2. AG 는 다음 cycle 부터 해당 epic 에 대해 task claim 시도 안 함
  3. 진행 중이던 task 들 중 wip 상태는 그대로 둠 (현재 push 된 코드는 보존)
- **대안 흐름**:
  - OP 가 `--purge-branches` 같은 정책을 원하면 AG 가 ls-remote + `git push --delete` 직접 수행 (autopilot 은 git 미관여)
- **사후 조건**: 해당 epic 의 새 작업이 진행되지 않음. 리모트의 코드는 보존
- **관찰 가능 결과**: `events`: epic_abandoned

## UC-13. 라벨 기반 → DB 기반 마이그레이션

> 기존 `:ready` 라벨 기반으로 운영하던 레포에서 ledger 기반으로 전환한다.

- **트리거**: OP → AG 에게 마이그레이션 요청
- **선행 조건**: 활성 epic ≥1, 대상 이슈가 `:ready` 상태
- **주 흐름**:
  1. AG 가 `gh issue view <#>` 로 본문 / 제목 분석
  2. AG 가 spec 매칭 (어느 epic 에 속할지)
  3. 매칭 epic 의 `task add --source human --body "<원본 issue #N>"`
  4. AG 가 이슈에 코멘트 추가: "task <id> 로 흡수됨"
  5. AG 가 `:ready` 라벨 제거
- **대안 흐름**:
  - 매칭 실패 → UC-7 의 escalation 형태로 유지 또는 사람이 수동 mapping
- **사후 조건**: 이슈는 살아 있되 라벨이 정리됨, 새 task 행 1건
- **관찰 가능 결과**: 새 task 의 source='human', body 에 원본 이슈 번호 메타

## 시나리오 ↔ Autopilot 트레이트 메서드 매핑 (요약)

| UC | Agent CLI 호출 | Autopilot 트레이트 메서드 |
|----|--------------|------------------------|
| 1 | `epic create`, `task add-batch` | `EpicRepo::upsert_epic`, `TaskRepo::insert_epic_with_tasks` |
| 2 | `task claim`, `task complete \| fail \| release`, `task escalate` | `claim_next_task`, `complete_task_and_unblock`, `mark_task_failed`, `release_claim`, `escalate_task` |
| 3 | (UC-2 와 동일) | `claim_next_task` 의 deps 필터, `complete_task_and_unblock` 의 unblock |
| 4-5 | `epic reconcile --plan ...` | `apply_reconciliation` |
| 6 | `epic find-by-spec-path`, `task add` | `find_active_by_spec_path`, `upsert_watch_task` |
| 7 | `suppress check`, `suppress add` | `is_suppressed`, `suppress` |
| 8 | `task fail`, `task escalate` | `mark_task_failed`, `escalate_task` |
| 9 | `epic reconcile`, `suppress add`, `task force-status` | `apply_reconciliation`, `suppress`, `force_status` |
| 10 | `epic status`, `epic complete` | `list_epics`, `set_epic_status` |
| 11 | `task release` | `release_claim` |
| 12 | `epic abandon` | `set_epic_status` |
| 13 | `task add --source human` | `upsert_watch_task` (source=Human) |

이 매핑은 02 (아키텍처) 의 모듈 경계와 03 (상세 스펙) 의 trait 시그니처가 충족해야 하는 최소 요구사항이다. **Agent 측의 워크플로 통합 검증은 본 spec 의 책임이 아니다** — 04 §1 참조.
