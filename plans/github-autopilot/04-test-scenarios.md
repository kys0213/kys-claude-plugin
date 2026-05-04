# 04. Test Scenarios

> 03 의 인터페이스가 01 의 13개 UC 를 만족함을 블랙박스 시나리오로 검증한다. CLAUDE.md 의 TDD 원칙에 따라 본 문서의 시나리오는 **구현 전에** 작성되며, 인메모리/fake 어댑터로 실행 가능해야 한다.

## 1. 테스트 레이어와 격리

Ledger 모델에서 autopilot 의 책임은 TaskStore + CLI 까지다. 그 위의 워크플로 (UC-1~13 의 시나리오 통합) 검증은 **agent 측의 책임** 이다 — 본 spec 의 범위 밖.

| 레이어 | 대상 | 어댑터 | 격리 단위 |
|--------|------|--------|----------|
| **L1. Domain** | pure 함수 / 타입 (TaskId, TaskGraph, deps cycle) | 없음 | 함수별 단위 |
| **L2. Store conformance** | TaskStore 의 트랜잭션 의미 (LSP) | `InMemoryTaskStore` + `SqliteTaskStore` 양쪽에 동일 suite 실행 | 메서드 + tx 경계 |
| **L3. CLI 통합** | clap 진입점 ↔ TaskStore 연결, JSON 출력 / exit code | `InMemoryTaskStore` + `FixedClock` 주입 | 명령 1건 |
| **L4. Property** | 결정적 ID 충돌, attempts 단조성, reconcile 멱등성, 동시 claim 원자성 | proptest 기반 임의 입력 | 불변식별 |

E2E (실제 git / GitHub) 테스트와 agent 측 워크플로 통합은 본 문서 범위 밖.

## 2. 공통 fixture 형식

L2 / L3 / L4 모두 in-memory store 와 fixed clock 을 기본 fixture 로 사용. agent 와의 통합 fixture (FakeGit / FakeGitHub / FakeDecomposer / FakeNotifier) 는 더 이상 본 spec 의 책임이 아니다.

```yaml
fixture:
  clock:
    now: "2026-04-28T09:00:00Z"
  store: in-memory   # 또는 sqlite (conformance 비교)
  epics:
    - { name: "auth-token-refresh", spec_path: "spec/auth.md", status: "active" }
  tasks:
    - { id: "<derived>", epic: "auth-token-refresh", status: "ready", title: "..." }
  deps: []
  events: []
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


## 5. L3: CLI 통합

CLI 진입점이 trait 메서드를 올바르게 디스패치하고 JSON 출력 / exit code 가 안정적인지 검증한다. `InMemoryTaskStore` + `FixedClock` 을 직접 주입하고 `assert_cmd` 또는 동등 도구로 stdout / exit code 를 검사한다.

```
test: cli_epic_create_persists_and_emits_event
  given: empty store
  when:  `autopilot epic create --name e --spec spec/e.md`
  then:  exit 0
         store.list_epics() == [Epic{name="e", status=active, ...}]
         events: epic_started

test: cli_task_claim_outputs_next_ready_task_as_json
  given: epic e with task A(ready)
  when:  `autopilot task claim --epic e --json`
  then:  exit 0
         stdout JSON: {"id":"...A...","status":"wip","attempts":1,...}
         get_task(A).status == Wip

test: cli_task_claim_signals_no_ready_via_exit_code
  given: epic e with no ready task
  when:  `autopilot task claim --epic e`
  then:  exit 1   # "no ready task" 신호 (사용자 에러 아님)

test: cli_task_complete_updates_pr_and_unblocks
  given: A(wip), B(pending, deps=[A])
  when:  `autopilot task complete <A.id> --pr 42`
  then:  A.status == Done, A.pr_number == 42
         B.status == Ready

test: cli_task_fail_reports_outcome
  given: A(wip, attempts=2), max_attempts=3
  when:  `autopilot task fail <A.id>`
  then:  exit 0
         stdout JSON: {"outcome":"retried","attempts":2}
         A.status == Ready

  when (한번 더 claim → fail): 동일 호출
  then:  stdout JSON: {"outcome":"escalated","attempts":3}
         A.status == Escalated

test: cli_suppress_check_returns_proper_exit_code
  given: suppress(fp="x", reason="r", until=t0+1h)
  when:  `autopilot suppress check --fingerprint x --reason r`
  then:  exit 0   # 억제 중

  when (window 만료 후): 동일 호출
  then:  exit 1   # 억제 안 됨

test: cli_epic_reconcile_applies_plan_idempotently
  given: empty store
  when:  `autopilot epic reconcile --name e --plan <plan.jsonl>` 두 번
  then:  두 번째 호출 후 store 상태 == 첫 번째 호출 후 상태
         events 에 reconciled 가 2건
```

L3 의 의도는 **CLI 인자 파싱 / 트레이트 디스패치 / JSON 출력** 의 안정성. 비즈니스 로직 검증은 L2 가 담당하므로 L3 는 가벼운 분량으로 유지.

## 6. L4: Property 테스트

```
proptest: deterministic_task_id_no_collision_in_realistic_corpus
  generator: 1000개 spec section/requirement 쌍 (Korean+English mix, 길이 1..200)
  property: { TaskId(...) } 가 모두 unique 하거나 입력이 정규화 후 동일

proptest: attempts_bounded_by_real_attempts
  for any sequence of operations from {claim, mark_failed, release_claim, complete}:
    let real_attempts =
        (# of claim) - (# of release_claim immediately following the matching claim)
    final task.attempts == real_attempts
    final task.attempts 는 결코 0 미만이 되지 않음 (saturating_sub 보장)
    final status ∈ {Ready, Wip, Escalated, Done, Pending, Blocked}

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
| `InMemoryTaskStore` | 단일 `Mutex<State>` 로 모든 메서드 atomic. `SqliteTaskStore` 와 같은 트랜잭션 의미 |
| `FixedClock` | `now()` 가 고정. 테스트가 명시적으로 advance 가능 |

git / github / decompose / notifier 의 fake 는 **본 spec 의 책임이 아니다** — agent 측에서 자기 워크플로 통합 테스트에 필요할 때 만든다.

### 7.2 Builder 함수 (예)

```rust
let store = Arc::new(InMemoryTaskStore::new()) as Arc<dyn TaskStore>;
let clock = FixedClock::at("2026-04-28T09:00:00Z");

store.insert_epic_with_tasks(plan("e", vec![nt("A","a"), nt("B","b")],
                                  vec![("B","A")]), clock.now())?;
store.claim_next_task("e", clock.now())?;       // A → Wip
store.complete_task_and_unblock(&id("A"), 42, clock.now())?;
assert_eq!(store.get_task(&id("A"))?.unwrap().status, TaskStatus::Done);
assert_eq!(store.get_task(&id("B"))?.unwrap().status, TaskStatus::Ready);
```

빌더는 03 의 도메인 타입 + 결정적 ID 함수를 사용한다. CLI L3 테스트에서는 `assert_cmd::Command` 같은 도구로 실행 + stdout / exit code 검사.

## 8. 카버리지 목표

- L1: 100% (pure 함수 — 작성 즉시 모두 다룰 수 있음)
- L2: TaskStore trait 의 모든 public 메서드에 ≥1 시나리오 + LSP suite 전체 양 구현체 통과
- L3: 03 §6 의 ledger CLI 명령마다 ≥1 happy path 시나리오. 에러 / 빈 결과 / JSON 형식 검증 포함
- L4: 본 문서에 명시된 4개 property 모두 통과

CI 게이트: 위 목표 미달 시 PR 차단.

UC-1~13 시나리오 (01 의 사용자 흐름 통합 검증) 는 **agent 측의 책임** — 본 문서의 카버리지 항목 아님.

## 9. 본 문서의 변경 정책

- 03 의 시그니처가 변경되면 본 문서의 L2 / L3 / L4 코드 단편도 함께 갱신
- ledger CLI 명령이 추가되면 L3 에 happy path 시나리오 추가
- 신규 property 가 도출되면 L4 에 추가
