# 04. Test Scenarios

> 03 의 인터페이스가 01 의 13개 UC 를 만족함을 블랙박스 시나리오로 검증한다. CLAUDE.md 의 TDD 원칙에 따라 본 문서의 시나리오는 **구현 전에** 작성되며, 인메모리/fake 어댑터로 실행 가능해야 한다.

## 1. 테스트 레이어와 격리

| 레이어 | 대상 | 어댑터 | 격리 단위 |
|--------|------|--------|----------|
| **L1. Domain** | pure 함수 / 타입 (TaskId, TaskGraph, deps cycle) | 없음 | 함수별 단위 |
| **L2. Store conformance** | TaskStore 의 트랜잭션 의미 (LSP) | `InMemoryTaskStore` + `SqliteTaskStore` 양쪽에 동일 suite 실행 | 메서드 + tx 경계 |
| **L3. Orchestration** | EpicManager / BuildLoop / WatchDispatcher / MergeLoop / Reconciler / Escalator | 모든 포트는 fake (InMemoryTaskStore, FakeGit, FakeGitHub, FakeDecomposer, FakeNotifier, FixedClock) | UC 1건 = 1 시나리오 |
| **L4. Property** | 결정적 ID 충돌, reconcile 멱등성, 동시 claim 의 원자성 | proptest 기반 임의 입력 | 불변식별 |

CLI 진입점 / SQLite 어댑터의 실 SQL 동작은 L2 가 담당한다. 별도 e2e (실제 git/GitHub) 테스트는 본 문서 범위 밖 — CI 환경에서 수동 smoke 만 수행.

## 2. 공통 fixture 형식

각 시나리오는 다음 YAML-like 구조로 표기한다 (실제 구현에서는 builder 함수 권장):

```yaml
fixture:
  clock:
    now: "2026-04-28T09:00:00Z"
  epic:
    name: "auth-token-refresh"
    spec_path: "spec/auth.md"
    branch: "epic/auth-token-refresh"
  decomposer:
    tasks:
      - { section: "## 인증", req: "토큰 갱신",   id: "<derived>" }
      - { section: "## 인증", req: "401 재시도", id: "<derived>" }
    deps: []
  git:
    initial_branches: ["main"]
    push_rejects: []
  github:
    issues: []
    prs: []
```

`<derived>` 는 결정적 ID 함수의 결과 (테스트 setup 에서 계산하여 매칭).

## 3. L1: Domain 테스트

### 3.1 TaskId 결정성

```
test: task_id_is_stable_across_runs
  for (epic, section, req) in samples:
    a = TaskId::new_deterministic(epic, section, req)
    b = TaskId::new_deterministic(epic, section, req)
    assert_eq!(a, b)
    assert_eq!(a.as_str().len(), 12)
```

### 3.2 TaskId 정규화

```
test: task_id_normalizes_section_and_requirement
  same_id = TaskId::new_deterministic("e", "## 인증", "토큰 갱신")
  also_same = TaskId::new_deterministic("e", "##  인증  ", "토큰  갱신")  -- 공백 차이
  also_same2 = TaskId::new_deterministic("e", "## 인증", "토큰\u{00A0}갱신") -- NFKC
  assert_eq!(same_id, also_same)
  assert_eq!(same_id, also_same2)

test: task_id_differs_on_meaningful_change
  a = TaskId::new_deterministic("e", "## 인증", "토큰 갱신")
  b = TaskId::new_deterministic("e", "## 인증", "토큰 발급")
  assert_ne!(a, b)
```

### 3.3 Dep cycle 검출

```
test: graph_with_no_cycle_passes
  graph = TaskGraph::build([(B, A), (C, B)])
  assert!(graph.detect_cycle().is_none())

test: graph_with_self_loop_fails
  graph = TaskGraph::build([(A, A)])
  assert_eq!(graph.detect_cycle(), Some(vec![A]))

test: graph_with_2cycle_fails
  graph = TaskGraph::build([(A, B), (B, A)])
  assert!(graph.detect_cycle().is_some())

test: dependents_of_finds_transitive
  graph = TaskGraph::build([(B, A), (C, B)])
  -- A 가 완료되면 B 가 다음 후보. C 는 B 가 done 일 때만
  assert_eq!(graph.dependents_of(&A), vec![&B])
```

## 4. L2: TaskStore Conformance Suite

다음 suite 는 `InMemoryTaskStore` 와 `SqliteTaskStore` 양쪽에 매크로로 적용. **두 구현이 동일한 결과를 내야 한다 (LSP).**

### 4.1 insert_epic_with_tasks

```
test: insert_creates_epic_tasks_deps_and_promotes_entry_points
  given: empty store
  when:  insert_epic_with_tasks(plan with 3 tasks, deps=[(B,A),(C,A)])
  then:
    list_epics() == [plan.epic with status='active']
    list_tasks_by_epic("e") in {A.status='ready', B.status='pending', C.status='pending'}
    list_deps(B) == [A]
    events: epic_started + 3x task_inserted

test: insert_rejects_dep_cycle
  when:  insert_epic_with_tasks(plan with deps=[(A,B),(B,A)])
  then:  Err(DomainError::DepCycle(_))
         store unchanged

test: insert_is_atomic_on_partial_failure
  given: store with epic 'e' already active
  when:  insert_epic_with_tasks(plan { name='e' })
  then:  Err(EpicAlreadyExists)
         no tasks inserted, events untouched
```

### 4.2 claim_next_task

```
test: claim_returns_none_when_no_ready_task
  given: 0 tasks in epic
  when:  claim_next_task("e")
  then:  Ok(None)

test: claim_returns_oldest_ready_task
  given: tasks = [A(ready, created=t0), B(ready, created=t1)]   # t0 < t1
  when:  claim_next_task("e")
  then:  task.id == A
         get_task(A).status == 'wip'
         get_task(A).attempts == 1

test: claim_skips_tasks_with_unsatisfied_deps
  given: A(done), B(ready, deps=[A]), C(pending, deps=[B])
  when:  claim_next_task("e")
  then:  task.id == B          # C 는 deps 미충족이라 후보 아님

test: claim_is_atomic_under_concurrent_callers
  given: 1 ready task A
  when:  두 호출자가 동시에 claim_next_task   # tokio::spawn or std::thread
  then:  정확히 한쪽만 Some(A) 를 받고 다른 쪽은 None
         get_task(A).attempts == 1   # 한 번만 증가

test: claim_increments_attempts_only_on_real_attempt
  given: 1 ready task A
  when:  claim → release_claim → claim
  then:  attempts == 1
         -- release_claim 은 시도조차 못 한 경우(UC-11)에 사용되며,
         -- claim 의 +1 을 차감하여 max_attempts 카운트에 영향 없음

test: mark_task_failed_preserves_attempts_for_retry
  given: 1 ready task A, max_attempts=3
  when:  claim → mark_task_failed (Retried) → claim → mark_task_failed (Retried)
  then:  attempts == 2
         -- 시도 후 실패는 attempts 보존 (escalation 카운트에 반영)
```

### 4.3 complete_task_and_unblock

```
test: completing_a_task_unblocks_dependents_with_satisfied_deps
  given: A(wip), B(pending, deps=[A]), C(pending, deps=[A,B])
  when:  complete_task_and_unblock(A, pr=42)
  then:  A.status='done', A.pr_number=42
         B.status='ready'      # B 의 deps 가 모두 done
         C.status='pending'    # C 는 B 가 아직 done 아님
         report.newly_ready == [B]

test: complete_emits_unblocked_event_per_promoted_task
  given: A(wip), B(pending, deps=[A]), D(pending, deps=[A])
  when:  complete_task_and_unblock(A, pr=1)
  then:  events kind='task_unblocked' for B and D

test: complete_rejects_when_status_not_wip
  given: A(ready)
  when:  complete_task_and_unblock(A, pr=1)
  then:  Err(IllegalTransition(A, _, _))
```

### 4.4 mark_task_failed

```
test: failure_below_max_returns_to_ready
  given: A(wip, attempts=1), max_attempts=3
  when:  mark_task_failed(A, max_attempts=3)
  then:  Ok(Retried{attempts:1})
         A.status='ready'

test: failure_at_max_escalates_and_blocks_dependents
  given: A(wip, attempts=3), B(ready, deps=[A]), max_attempts=3
  when:  mark_task_failed(A, max_attempts=3)
  then:  Ok(Escalated{attempts:3})
         A.status='escalated'
         B.status='blocked'
         events kind='task_escalated' + dep B 의 'task_blocked' 가 표현
```

### 4.5 apply_reconciliation 멱등성

```
test: reconcile_is_idempotent
  given: empty store
  when:  apply_reconciliation(plan) 두 번 호출 (동일 입력)
  then:  두 번째 호출 후 store 상태 == 첫 번째 호출 후 상태
         단, events 는 누적 (kind='reconciled' 가 2건)

test: reconcile_preserves_attempts_counter
  given: tasks=[A(wip, attempts=2)]
  when:  apply_reconciliation(plan with same tasks, remote_state shows A still wip)
  then:  A.attempts == 2     # 카운터는 보존

test: reconcile_overrides_status_from_remote_truth
  given: tasks=[A(ready, attempts=0)]
  when:  apply_reconciliation(plan, remote_state=[A: pr_merged=true])
  then:  A.status='done'
         A.pr_number=<from remote>
```

### 4.6 fingerprint 중복 검사

```
test: upsert_watch_task_inserts_new_when_no_fingerprint_match
  given: epic with no tasks
  when:  upsert_watch_task(NewWatchTask { fingerprint: "fp-1", ... })
  then:  Ok(Inserted(_))

test: upsert_watch_task_returns_duplicate_on_existing_fingerprint
  given: task A with fingerprint='fp-1'
  when:  upsert_watch_task(NewWatchTask { fingerprint: "fp-1", ... })
  then:  Ok(DuplicateFingerprint(A.id))
         no new task inserted
```

### 4.7 lookup helpers

```
test: find_task_by_pr_returns_owning_task
  given: A(done, pr_number=42), B(wip, pr_number=None)
  when:  find_task_by_pr(42)
  then:  Ok(Some(A))

test: find_task_by_pr_returns_none_when_unknown
  when:  find_task_by_pr(9999)
  then:  Ok(None)

test: find_active_by_spec_path_matches_active_only
  given: epic e1(active, spec="spec/auth.md"), e2(abandoned, spec="spec/auth.md")
  when:  find_active_by_spec_path("spec/auth.md")
  then:  Ok(Some(e1))   # abandoned 는 제외

test: find_active_by_spec_path_returns_none_when_no_match
  given: epic e1(active, spec="spec/payments.md")
  when:  find_active_by_spec_path("spec/auth.md")
  then:  Ok(None)

test: find_active_by_spec_path_rejects_invariant_violation
  given: epic e1(active, spec="spec/auth.md"), e2(active, spec="spec/auth.md")
         # 정상 흐름에서는 만들 수 없으나 직접 삽입했다고 가정
  when:  find_active_by_spec_path("spec/auth.md")
  then:  Err(DomainError::Inconsistency(_))
```

### 4.8 force_status

```
test: force_status_bypasses_normal_transition
  given: A(escalated)
  when:  force_status(A, target=Pending, reason="manual reset")
  then:  A.status == 'pending'
         events kind 에 reason 이 payload 로 기록됨

test: force_status_does_not_unblock_dependents
  given: A(escalated), B(blocked, deps=[A])
  when:  force_status(A, target=Done, reason="human fixed")
  then:  A.status == 'done'
         B.status == 'blocked'   # force_status 는 자식 unblock 안 함
         # → 호출자가 별도로 reconcile / 메서드를 호출해야 함

test: force_status_records_event_with_reason
  given: A(wip)
  when:  force_status(A, target=Ready, reason="rollback")
  then:  events 에 force_status 기록, payload.reason == "rollback"
```

### 4.9 suppression

```
test: suppression_blocks_until_window_expires
  fixture: clock at t0
  when:  suppress(fp="fp-1", reason="unmatched_watch", until=t0+1h)
         is_suppressed(fp-1, "unmatched_watch", at=t0+30m) -> true
         is_suppressed(fp-1, "unmatched_watch", at=t0+2h)  -> false

test: suppression_is_scoped_by_reason
  when:  suppress(fp="fp-1", reason="unmatched_watch", ...)
         is_suppressed(fp-1, "rejected_by_human", now) -> false

test: suppression_clear_unblocks_immediately
  when:  suppress(fp-1, "r", until=far_future)
         clear(fp-1, "r")
         is_suppressed(fp-1, "r", now) -> false
```

### 4.10 schema_version 일관성 (SQLite 전용)

```
test: sqlite_initializes_schema_v1_on_empty_db
  given: 빈 DB 파일
  when:  SqliteTaskStore::open()
  then:  meta.schema_version == "1"

test: sqlite_rejects_unknown_higher_version
  given: meta.schema_version = "999"
  when:  SqliteTaskStore::open()
  then:  Err(SchemaMismatch{found:999, expected:1})
```

## 5. L3: Orchestration 시나리오 (UC ↔ 테스트)

각 시나리오는 fake 어댑터로 wiring 한 orchestration 컴포넌트를 호출하고 외부 관찰점 (DB, FakeGit ref 그래프, FakeGitHub 이슈/PR, FakeNotifier 메시지) 으로 사후 조건을 검증한다.

### UC-1. Epic 시작

```
test: uc1_epic_start_creates_branch_and_seeds_tasks
  fixture:
    git: branches=[main], working_tree_clean=true
    decomposer: tasks=[(s1,r1), (s2,r2)], deps=[]
  when:  EpicManager::start({ name:"e", spec:"spec/auth.md" })
  then:
    git.branches contains "epic/e"
    git.pushed contains "epic/e"
    store.list_epics() == [Epic{name:"e", status='active', ...}]
    store.list_tasks_by_epic("e") -> 2 tasks, 둘 다 status='ready'
    store.events: epic_started + 2x task_inserted

test: uc1_epic_start_rolls_back_on_decomposer_failure
  fixture: decomposer.fail = SpecNotFound
  when:  EpicManager::start({ name:"e", spec:"missing.md" })
  then:  Err(_)
         store.list_epics() empty
         git.branches unchanged

test: uc1_epic_start_rejects_dirty_working_tree
  fixture: git.working_tree_clean=false
  when:  EpicManager::start(...)
  then:  Err(GitError::DirtyWorkingTree)

test: uc1_epic_start_rejects_when_active_epic_with_same_name
  fixture: store has epic "e" with status='active'
  when:  EpicManager::start({ name:"e" })
  then:  Err(EpicAlreadyExists)
```

### UC-2. Task 자동 구현 사이클

```
test: uc2_full_cycle_claim_to_merged
  fixture:
    epic "e" with 1 ready task A
    fake_implementer: on_invoke push branch successfully
    github: empty
  when:
    BuildLoop::tick()           -> claim A, IM push, branch_promoter create PR #100
    MergeLoop::tick()           -> CI ok 가정, merge PR #100
  then:
    A.status == 'done'
    A.pr_number == 100
    events sequence: task_claimed, task_started, task_completed
    github.prs[100].merged == true

test: uc2_push_failure_returns_task_to_ready
  fixture: fake_implementer pushes successfully but branch_promoter fails to create PR
  when:  BuildLoop::tick()
  then:  A.status == 'ready'
         events: task_failed (non-final)
```

### UC-3. 의존성 있는 Task

```
test: uc3_dependent_task_not_claimed_until_parent_done
  fixture: tasks A(ready), B(pending, deps=[A])
  when:
    claim_next_task -> A
    complete_task_and_unblock(A, pr=1)
    claim_next_task -> ?
  then:  세 번째 결과는 Some(B)
         events: A.completed before B.claimed

test: uc3_escalated_parent_blocks_child
  fixture: A(wip, attempts=max), B(pending, deps=[A])
  when:  mark_task_failed(A, max_attempts) → Escalated
  then:  B.status == 'blocked'
```

### UC-4. Epic 이어받기

```
test: uc4_resume_reconstructs_state_from_remote
  fixture:
    store: empty (다른 머신 또는 DB 손실 가정)
    git.remote_branches: ["epic/e", "epic/e/<idA>", "epic/e/<idB>"]
    github.prs: [{head:"epic/e/<idA>", base:"epic/e", merged:true, number:10}]
    decomposer: tasks=[A,B,C]   # idA,idB,idC 결정적
  when:  EpicManager::resume("e")
  then:
    A.status == 'done', A.pr_number == 10
    B.status == 'wip'           # 브랜치만 있고 PR 없음
    C.status == 'ready'         # 브랜치 없음, deps 만족
    events: kind='reconciled'

test: uc4_resume_reports_orphan_branch
  fixture: remote has "epic/e/<unknown_id>"
  when:  EpicManager::resume("e")
  then:  events 에 reconciled.payload.orphan_branch=="epic/e/<unknown_id>" 1건
```

### UC-5. DB 손실 후 복구

```
test: uc5_lost_db_recovers_via_resume
  fixture: 같은 fixture 를 두 번 실행 — 첫 번째는 실 DB, 두 번째는 DB 파일 삭제 후 resume
  then:  두 번째 실행 후 store 상태가 첫 번째의 의미적 상태와 일치
         단, attempts 카운터는 0 으로 재설정 (허용 손실)
```

### UC-6. Watch 매칭 task append

```
test: uc6_gap_watch_appends_task_when_spec_matches_active_epic
  fixture: active epic "e" with spec_path="spec/auth.md"
  when:  WatchDispatcher::dispatch(GapFinding {
           spec_path:"spec/auth.md",
           section:"## 인증", req:"새로운 갭",
           fingerprint:"fp-1"
         })
  then:  store.list_tasks_by_epic("e") 가 1건 증가
         새 task.source == 'gap-watch'
         github.issues empty   # 매칭됐으므로 이슈 발행 없음

test: uc6_duplicate_fingerprint_is_no_op
  fixture: epic "e" 에 이미 fingerprint="fp-1" 인 task
  when:  WatchDispatcher::dispatch(finding with fp-1)
  then:  store.tasks count unchanged
         events kind='watch_duplicate' 1건
```

### UC-7. Watch 미매칭 escalation

```
test: uc7_unmatched_watch_creates_escalation_issue
  fixture: 활성 epic 의 spec_path 와 다른 path
  when:  WatchDispatcher::dispatch(finding)
  then:  github.issues count == 1
         issue.labels contains "autopilot:hitl-needed"
         store.tasks unchanged (미매칭 watch 는 task 미생성)
         escalation_suppression 에 fingerprint 기록

test: uc7_suppressed_fingerprint_skips_issue
  fixture: escalation_suppression 에 fp 가 활성 (now < suppress_until)
  when:  WatchDispatcher::dispatch(finding with fp)
  then:  github.issues count unchanged
```

### UC-8. 반복 실패 escalation

```
test: uc8_max_attempts_creates_escalation_issue
  fixture: A(wip, attempts=2), max_attempts=3
  when:
    mark_task_failed(A, max=3)        # → Retried{attempts:2}
    re-claim → mark_task_failed(A, max=3)   # 이제 attempts=3 → Escalated
    Escalator::escalate(A)             # GitHub 이슈 발행
  then:  github.issues count == 1
         A.escalated_issue == issue.number
         A.status == 'escalated'
```

### UC-9. 사람이 escalation 처리

```
test: uc9a_resolved_by_starting_new_epic
  fixture: open escalation issue with epic="none"
  when:  사람이 EpicManager::start(...) 로 새 epic 시작
  then:  새 epic 활성화. 기존 escalation 이슈는 사람이 close 해야 닫힘 (자동 close 안함)

test: uc9b_resolved_by_absorbing_into_existing_epic
  fixture: 활성 epic "e" + open escalation issue
  when:  analyze-issue 가 이슈 본문 → spec 매칭 후 task append
  then:  store 에 새 task 1건, source='human', body 에 issue_number 메타
         이슈는 코멘트 추가 후 close

test: uc9d_human_fixed_directly_marks_done_on_resume
  fixture: A.status='escalated', 사람이 직접 코드 push 후 PR merged
  when:  EpicManager::resume(...)
  then:  reconcile 로 A.status='done' 으로 전이

test: uc9_escalation_watcher_picks_up_human_close_without_resume
  fixture:
    epic "e" active, A.status='escalated', A.escalated_issue=#42
    github.issues[42].closed = true
    git.remote: PR head=epic/e/<A_id> base=epic/e merged=true
    decomposer: tasks 에 A 포함
  when:  EscalationWatcher::tick()
  then:  A.status == 'done'
         events kind='escalation_resolved', payload.resolution=='human_fixed'
         # resume 호출 없이 자동 인식

test: uc9_escalation_watcher_records_rejection_on_close_without_code
  fixture:
    epic "e" active, A.status='escalated', A.escalated_issue=#42, fingerprint='fp-A'
    github.issues[42].closed = true
    git.remote: 해당 task 의 feature 브랜치 / PR 없음
  when:  EscalationWatcher::tick()
  then:  A.status == 'escalated' 유지   # reconcile 결과 done 아님
         suppression(fp='fp-A', reason='rejected_by_human') 활성
         events kind='escalation_resolved', payload.resolution=='rejected'
```

### UC-10. Epic 완료

```
test: uc10_all_tasks_done_completes_epic_and_notifies
  fixture: epic "e" 의 마지막 task A 가 wip → done 으로 전이 직후
  when:  MergeLoop::tick() 가 마지막 PR 머지 후 epic 완료 판정
  then:  epic.status == 'completed'
         notifier.sent contains EpicCompleted{ epic:"e" }
         no automatic main-merge PR 생성  # main 머지는 사람의 몫

test: uc10_does_not_complete_with_open_escalations
  fixture: epic 의 task 들 중 하나가 escalated 이고 issue 가 open
  when:  MergeLoop::tick()
  then:  epic.status == 'active' 유지
         notifier.sent 에 EpicCompleted 없음
```

### UC-11. 동시 진행 자연 차단

```
test: uc11_push_reject_releases_claim_without_attempt_charge
  fixture: BuildLoop 가 task A 를 claim, IM 이 push 시도하나 git.push_rejects 로 reject
  when:  IM 결과 처리
  then:  A.status == 'ready'
         events kind='claim_lost'
         A.attempts == 0   # release_claim 으로 차감되어 다음 cycle 에서 새 시도

test: uc11_concurrent_claim_yields_one_winner
  given: store 에 ready task A 1건
  when:  두 BuildLoop 인스턴스가 동시에 claim
  then:  한 쪽만 Some(A), 다른 쪽 None
         tasks.attempts == 1
         events kind='task_claimed' 1건
```

### UC-12. Epic 강제 중단

```
test: uc12_stop_marks_abandoned_and_keeps_branches
  fixture: epic "e" with task A(wip)
  when:  EpicManager::stop("e", purge_branches=false)
  then:  epic.status == 'abandoned'
         A.status == 'wip' (보존)
         git.remote_branches contains "epic/e" (삭제 안 함)

test: uc12_stop_with_purge_deletes_unmerged_branches
  fixture: epic with feature branches, 일부 PR merged
  when:  EpicManager::stop("e", purge_branches=true)
  then:  머지된 PR 의 feature 브랜치만 삭제
         미머지 feature 브랜치는 보존 (작업 손실 방지)
```

### UC-13. 마이그레이션

```
test: uc13_import_issue_creates_task_and_strips_label
  fixture:
    epic "e" active, spec_path="spec/auth.md"
    github.issues = [{ number:5, title:"...", labels:[":ready"], body:"...spec/auth.md..." }]
  when:  MigrateCommand::import_issue(5)
  then:
    store.list_tasks_by_epic("e") 1건 증가
    new_task.source == 'human'
    new_task.body contains "issue #5"
    github.issues[5].labels does NOT contain ":ready"
    github.issues[5].comments contains "task <id> 로 흡수됨"

test: uc13_unmatched_issue_falls_back_to_escalation
  fixture: 이슈의 spec 매칭 실패
  when:  MigrateCommand::import_issue(5)
  then:  task 생성 안 됨
         이슈는 그대로 유지 (사람이 수동 처리)
```

## 6. L4: Property 테스트

```
proptest: deterministic_task_id_no_collision_in_realistic_corpus
  generator: 1000개 spec section/requirement 쌍 (Korean+English mix, 길이 1..200)
  property: { TaskId(...) } 가 모두 unique 하거나 입력이 정규화 후 동일

proptest: claim_invariant_attempts_monotonic
  for any sequence of (claim → mark_failed | revert) operations:
    task.attempts 는 단조 증가 (감소하지 않음)
    final status ∈ {ready, wip, escalated, done}

proptest: reconcile_idempotent_under_arbitrary_remote_state
  generator: 임의 remote_state (브랜치 + PR 조합)
  property: apply_reconciliation(plan) 두 번 적용 결과의 store 상태 동일
            (events 누적은 허용)

proptest: dep_graph_topological_order_respects_constraints
  for any DAG:
    topological_order() 결과 [a, b, c, ...] 에서
    각 (x, y) ∈ deps 에 대해 y 가 x 보다 앞서 등장
```

## 7. Fixture 빌더와 fake 동작 명세

### 7.1 Fake 동작 규약

| Fake | 보장 동작 |
|------|----------|
| `InMemoryTaskStore` | 단일 `Mutex<State>` 로 모든 메서드 atomic. SqliteTaskStore 와 같은 트랜잭션 의미 |
| `FakeGitClient` | 가상 ref 그래프. `push_rejects` 리스트에 등록된 브랜치는 항상 `Rejected` 반환. push 성공 시 ref 추가 |
| `FakeGitHubClient` | 이슈/PR 의 in-memory store. PR.merged 는 명시적 `merge_pr` 호출 시에만 true |
| `FakeDecomposer` | fixture 의 tasks/deps 를 그대로 반환. `fail` 옵션 시 지정된 에러 반환 |
| `FakeNotifier` | 호출 시 메시지를 `sent: Vec<NotificationEvent>` 에 누적 |
| `FixedClock` | `now()` 가 고정. 테스트가 명시적으로 advance 가능 |

### 7.2 Builder 함수 (예)

```rust
let world = TestWorld::new()
    .with_clock_at("2026-04-28T09:00:00Z")
    .with_active_epic("e", spec="spec/auth.md")
    .with_tasks(&["A", "B"])
    .with_deps(&[("B", "A")])
    .with_remote_branches(&["epic/e", "epic/e/<A_id>"])
    .with_merged_pr(number=10, head="epic/e/<A_id>", base="epic/e");

let outcome = world.epic_manager().resume("e")?;
world.assert_task_status("A", TaskStatus::Done);
world.assert_task_status("B", TaskStatus::Wip);
```

빌더는 03 의 도메인 타입 + 결정적 ID 함수를 사용하여 fixture 의 `<A_id>` 를 자동 계산한다.

## 8. 카버리지 목표

- L1: 100% (pure 함수 — 작성 즉시 모두 다룰 수 있음)
- L2: TaskStore trait 의 모든 public 메서드에 ≥1 시나리오 + LSP suite 전체 양 구현체 통과
- L3: 01 의 13개 UC 마다 ≥1 happy path + ≥1 대안 흐름
- L4: 본 문서에 명시된 4개 property 모두 통과

CI 게이트: 위 목표 미달 시 PR 차단.

## 9. 본 문서의 변경 정책

- UC 가 추가/변경되면 (01 갱신) 본 문서의 L3 시나리오도 함께 추가
- 03 의 시그니처가 변경되면 본 문서의 코드 단편도 함께 갱신
- 신규 시나리오는 가능한 한 기존 fixture builder 를 재사용
