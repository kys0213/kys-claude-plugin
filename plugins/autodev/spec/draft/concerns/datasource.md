# DataSource — 외부 시스템 추상화 + 워크플로우 정의

> 외부 시스템(GitHub, Slack, Jira, ...)을 추상화하고, 각 시스템의 언어로 자동화 워크플로우를 정의한다.
> 새 외부 시스템 추가 = 새 DataSource impl + yaml 설정, 코어 변경 0 (OCP).

---

## 역할

```
DataSource가 소유하는 것:
  1. 수집 — 어떤 조건에서 아이템을 감지하는가 (trigger)
  2. 처리 — 감지된 아이템을 어떻게 처리하는가 (handlers)
  3. 전이 — 처리 완료 후 다음에 뭘 트리거하는가 (on_done)
  4. 동기화 — 외부 시스템에 상태를 어떻게 반영하는가 (라벨, 코멘트 등)
  5. 실패 정책 — 실패 시 어떻게 escalation하는가

코어는 DataSource 내부를 모른다. collect() 결과를 큐에 넣고, 상태 전이만 관리.
```

---

## 상태 기반 워크플로우

각 DataSource는 자기 시스템의 상태 표현으로 워크플로우를 정의한다. v5는 GitHub에 집중한다.

### GitHub (라벨 기반)

```yaml
sources:
  github:
    url: https://github.com/org/repo
    scan_interval_secs: 300
    concurrency: 1

    states:
      analyze:
        trigger: { label: "autodev:analyze" }
        handlers:
          - prompt: "이슈를 분석하고 구현 가능 여부를 판단해줘"
        on_done: { label: "autodev:implement" }
        on_hitl: { label: "autodev:needs-decision" }

      implement:
        trigger: { label: "autodev:implement" }
        handlers:
          - prompt: "이슈를 구현해줘"
        on_done: { label: "autodev:review" }

      review:
        trigger: { label: "autodev:review" }
        handlers:
          - prompt: "PR을 리뷰하고 품질을 평가해줘"
        on_done: { label: "autodev:done" }
```

### 향후 확장 (v6+)

DataSource trait을 구현하면 코어 변경 없이 새 외부 시스템을 추가할 수 있다.

| 시스템 | 상태 표현 | trigger 예시 |
|--------|----------|-------------|
| Jira | 티켓 status | `{ status: "To Analyze" }` |
| Slack | 리액션 | `{ reaction: "robot_face" }` |
| Linear | 라벨/status | `{ label: "autodev" }` |

---

## Handler

handler는 **prompt 단일 타입**. 자연어 문자열을 AgentRuntime.invoke()로 실행한다. prompt는 순수 작업 지시만 담당하고, 린트/컨벤션은 hooks와 rules가 단계 진입 시 자동 보장.

handler 배열은 Running 상태에서 순차 실행. 하나라도 실패 시 escalation.

---

## 아이템 흐름

```
1. DataSource.collect(): trigger 조건 매칭 → QueueItem 생성
2. Pending → Ready → Running (자동)
3. handlers 순차 실행
4. 코어 evaluate: "완료? 추가 검토?" → Done or HITL
5. Done → on_done 액션 실행
   → 다음 state의 trigger 활성화
   → 다음 collect() 턴에서 새 아이템으로 감지
```

---

## Escalation 정책

DataSource가 실패 정책을 결정하고, 코어가 실행한다.

```yaml
sources:
  github:
    escalation:
      1: retry
      2: retry_with_comment
      3: hitl
      4: skip
      5: replan
```

DataSource마다 다른 정책이 가능한 구조. v5는 GitHub 정책만 구현.

---

## trait 개요

```rust
pub trait DataSource: Send + Sync {
    fn name(&self) -> &str;
    async fn collect(&mut self, workspace: &WorkspaceConfig) -> Result<Vec<QueueItem>>;
    async fn on_phase_enter(&self, phase: QueuePhase, item: &QueueItem) -> Result<()>;
    async fn on_failed(&self, item: &QueueItem, failure_count: u32) -> Result<EscalationAction>;
}
```

v4 대비 대폭 축소. `before_task`, `after_task`, `on_done`, `on_skip`, `on_phase_exit` 제거.
on_done 액션은 yaml 설정에서 선언적으로 처리.
