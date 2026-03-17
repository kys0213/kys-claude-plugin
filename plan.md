# Phase A 설계: Critical 자율 루프 (C1-C4)

> **Date**: 2026-03-17
> **기준**: REMAINING-WORK.md C1-C4 항목 + 코드 대조

---

## 요구사항 정리

### C1. Daemon drain 제거 (Collector 수집 전용화)

**현재**: `GitHubTaskSource::poll()` → `sync_queue_phases()` → `drain_queue_items()`
- `claw_enabled == true` → `from_phase = Ready` → Ready→Running만 drain
- `claw_enabled == false` → `from_phase = Pending` → Pending→Running 직행
- `sync_queue_phases()`가 DB의 Ready 상태를 in-memory에 반영

**재검토 결과**: 이미 `claw_enabled` 분기로 DESIGN.md의 의도대로 동작 중
- Claw 모드: Collector는 Pending까지만, Claw가 `queue advance`로 Ready 전이, Daemon이 Ready→Running drain
- Issue 모드: Pending→Running 직행 (Claw 미개입)

→ **C1은 구현 완료 상태**. REMAINING-WORK.md에서 완료로 이동.

---

### C2. Notifier를 Daemon에 연결

**현재**: NotificationDispatcher + GitHub/Webhook 구현체 존재, daemon에서 미사용
**목표**: Task 완료/실패, HITL 생성 시 notify() 호출

**설계**:

1. **TaskResult에 notifications 필드 추가**
   ```rust
   // core/task.rs
   pub struct TaskResult {
       // ... existing fields
       pub notifications: Vec<NotificationEvent>,  // NEW
   }
   ```
   - Task가 HITL 생성 시 NotificationEvent도 함께 구성
   - Task가 context를 가장 잘 알고 있으므로 적합

2. **Daemon struct에 dispatcher 추가**
   ```rust
   // daemon/mod.rs
   pub struct Daemon {
       // ... existing fields
       dispatcher: Option<NotificationDispatcher>,
   }
   ```
   - `with_dispatcher()` builder 메서드 추가

3. **start()에서 Dispatcher 초기화**
   - config에서 webhook URL 읽기
   - GitHubCommentNotifier + WebhookNotifier(URL 설정 시) 구성

4. **run() task completion arm에서 dispatch**
   ```rust
   if let Some(ref dispatcher) = self.dispatcher {
       for event in &task_result.notifications {
           let _ = dispatcher.dispatch(event).await;
       }
   }
   ```

**사이드이펙트**:
- TaskResult 변경 → 모든 Task 구현체 + 테스트의 TaskResult 생성 코드 수정 필요
- notifications 빈 Vec이면 동작 변경 없음 → 기존 코드 호환

---

### C3. Force trigger (이벤트 → claw-evaluate 즉시 실행)

**현재**: claw-evaluate는 cron 60초 주기에만 실행
**목표**: 특정 이벤트에서 즉시 실행

**트리거 시점**: 스펙 등록, Task 실패, 스펙 연관 Task 완료, HITL 응답

**설계**:

1. **CronEngine에 force_trigger 메서드 추가**
   ```rust
   impl CronEngine {
       pub async fn force_trigger(&mut self, name: &str, repo_id: Option<&str>)
           -> Option<CronExecResult>
   }
   ```

2. **Daemon 내부 이벤트** (task 실패/완료):
   - `run()` task completion arm에서 Failed 시 → `self.cron_engine.force_trigger("claw-evaluate")` 호출

3. **CLI 이벤트** (spec add, hitl respond):
   - CLI는 별도 프로세스 → `cron_trigger()` 직접 호출 (기존 함수 활용)
   - `spec_add()` 마지막에 cron_trigger 호출 추가
   - `hitl::respond()` 마지막에 cron_trigger 호출 추가

**사이드이펙트**:
- `spec_add()`, `respond()`에 `env` 파라미터 추가 필요
- cron_trigger는 스크립트를 동기 실행 → CLI 응답 약간 지연 (guard check로 빠르게 종료)

---

### C4. Failure escalation 5단계

**현재**: retry(제한적) + skip(review만)
**목표**: 모든 Task에 5단계 에스컬레이션

**설계**:

1. **DB 마이그레이션**
   ```sql
   ALTER TABLE queue_items ADD COLUMN failure_count INTEGER NOT NULL DEFAULT 0;
   ```

2. **Escalation helper 모듈** (NEW: `service/tasks/helpers/escalation.rs`)
   ```rust
   pub enum EscalationLevel {
       Retry,      // Level 1: 같은 Task 재실행
       Comment,    // Level 2: GitHub 이슈에 실패 원인 코멘트 + retry
       Hitl,       // Level 3: HITL 이벤트 생성
       Skip,       // Level 4: autodev:skip 라벨 + 큐 제거
       Replan,     // Level 5: DecisionType::Replan + /update-spec 제안
   }

   pub fn determine_level(failure_count: u32, config: &EscalationConfig) -> EscalationLevel;
   pub fn escalate(item: &QueueItem, failure_count: u32, msg: &str, config: &EscalationConfig)
       -> EscalationResult;
   ```

3. **EscalationConfig** (core/config/models.rs)
   ```rust
   pub struct EscalationConfig {
       pub max_retries: u32,         // default: 2
       pub max_before_hitl: u32,     // default: 3
       pub max_before_skip: u32,     // default: 5
       pub max_before_replan: u32,   // default: 7
   }
   ```

4. **각 Task에 적용**: after_invoke() 실패 시 escalate() 호출
   - QueueOp::Push로 같은 phase에 재삽입 (retry 시)
   - NotificationEvent 생성 (C2 활용)
   - HITL 생성 (Level 3+)
   - DecisionType::Replan 기록 (Level 5)

**사이드이펙트**:
- ReviewTask의 기존 max_iterations 로직과 통합 필요
- queue_items DB 마이그레이션 필요 (failure_count 컬럼)

---

## 구현 순서 (의존성 기반)

```
1. C1 — REMAINING-WORK.md 업데이트만 (이미 구현 완료)

2. C2 (Notifier 연결) — C3, C4의 알림 기반
   a. TaskResult에 notifications 필드 추가 + 기존 코드 수정
   b. Daemon에 dispatcher 필드/builder 추가
   c. start()에서 초기화
   d. run()에서 dispatch 호출
   e. 테스트

3. C3 (Force trigger) — C2와 독립적이나 순서상 후순위
   a. CronEngine.force_trigger() 추가
   b. Daemon run()에서 task 실패 시 trigger
   c. CLI spec_add/hitl respond에 trigger 추가
   d. 테스트

4. C4 (Failure escalation) — C2 notifications 활용
   a. DB 마이그레이션 (failure_count)
   b. EscalationConfig 추가
   c. escalation.rs helper 모듈 생성
   d. 각 Task에 escalation 적용
   e. 테스트
```

---

## 테스트 전략

### C2
- MockNotifier로 dispatch 호출 검증
- TaskResult에 notifications 포함 시 dispatch 확인
- 빈 notifications → dispatch 미호출 확인

### C3
- force_trigger: 존재하는 job 트리거 확인
- 이미 실행 중인 job → skip 확인
- spec_add 후 cron_trigger 호출 확인

### C4
- determine_level: failure_count별 올바른 level 반환
- escalate: 각 level별 올바른 QueueOp + notification 생성
- DB migration: failure_count 컬럼 존재 확인

---

## 변경하지 않는 것

- Collector trait 시그니처 (poll → Vec<Task> 유지)
- TaskManager trait 시그니처
- 기존 Issue 모드 (claw_enabled == false) 동작
- GitHub label 상수 및 패턴
