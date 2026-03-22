# DESIGN v5

> **Date**: 2026-03-22
> **Status**: Draft
> **기준**: v4 운영 피드백 + 열린 이슈 + 설계 논의 반영

---

## 목표

코어를 단순하게 유지하면서, DataSource가 자기 시스템의 언어로 자동화 워크플로우를 정의할 수 있게 한다.

```
코어가 아는 것     = 큐 상태 머신 (Pending → Ready → Running → Done)
DataSource가 아는 것 = 어떤 조건에서 수집하고, 어떻게 처리하고, 다음에 뭘 트리거하는지
Claw가 아는 것      = 결과가 충분한지, 사람이 봐야 하는지
```

---

## 설계 철학

### 1. 컨베이어 벨트

아이템은 한 방향으로 흐른다. 되돌아가지 않는다.

```
투입 → 처리 → 판정 → 완료
```

부족하면 Cron이 새 아이템을 만들어서 다시 벨트에 태운다. 같은 아이템이 되돌아가는 게 아니라 새 아이템이 생긴다.

### 2. DataSource가 워크플로우를 소유

각 DataSource는 자기 시스템의 상태 표현으로 워크플로우를 정의한다.

```
GitHub = 라벨로 상태 전이     (autodev:analyze → autodev:implement → autodev:review)
```

> **향후 확장**: Jira(티켓 status), Slack(리액션) 등도 DataSource impl 추가만으로 지원 가능. v5는 GitHub에 집중.

코어는 DataSource의 내부 상태를 모른다. collect() 조건을 만족하면 큐에 넣고, Done이면 on_done 액션을 실행할 뿐.

### 3. 코어는 큐만 돌린다

코어의 유일한 책임은 큐 상태 머신을 관리하는 것.

```
QueuePhase: Pending → Ready → Running → Done | Skipped
```

무엇을 실행할지, 어떤 라벨을 붙일지, 다음 단계가 뭔지 — 전부 DataSource와 설정의 영역.

### 4. DataSource가 상태 축적기

DataSource의 외부 시스템이 **상태 저장소** 역할을 겸한다.

```
GitHub issue = 상태 축적기
  - analyze handler → 분석 결과를 issue comment로 남김
  - implement handler → PR을 생성하고 issue에 링크
  - review handler → PR review comment로 결과를 남김
```

handler는 별도의 output 저장소가 필요 없다. 외부 시스템(GitHub issue/PR) 자체에 산출물이 누적되고, 다음 단계 handler가 DataSource를 통해 이전 컨텍스트를 조회한다.

### 5. 코어가 출구에서 분류한다

코어의 evaluate 함수가 **출구의 분류기**. 품질 판단이 아니라 **완료 가능 여부만 판별**.

```
투입 = 자동 (DataSource.collect() → Pending → Ready → Running)
처리 = 자동 (handler 배열 순차 실행)
분류 = 코어 evaluate ("완료 처리해도 되나, 사람이 봐야 하나?" → Done or HITL)
```

evaluate의 판단 입력:
- `DataSource.get_context(item)` → 해당 아이템의 리뷰, 피드백, 코멘트 등 외부 시스템 컨텍스트
- handler 실행 결과 (exit code, stdout 요약)
- AgentRuntime을 통해 LLM이 컨텍스트를 읽고 분류

스펙 적합성, 코드 품질, gap 검출은 **Cron 품질 루프**가 담당한다. evaluate는 토큰을 최소로 쓰고 분류에만 집중.

### 6. 아이템 계보 (Lineage)

같은 외부 엔티티(예: GitHub issue #42)에서 파생된 아이템들은 `source_id`로 연결된다.

```
source_id = "github:org/repo#42"

QueueItem(state: analyze, source_id) → Done
QueueItem(state: implement, source_id) → Done   ← 같은 source_id
QueueItem(state: review, source_id) → Done       ← 같은 source_id
```

SQLite에 저장된 source_id 기반으로 이전 단계의 산출물과 이력을 조회한다.

### Claw는 대화형 에이전트

Claw = `/claw` 세션. 사용자가 자연어로 큐 상태를 조회하고, HITL에 응답하고, cron을 관리하는 대화형 인터페이스. 상세 설계는 [Claw 워크스페이스](./concerns/claw-workspace.md)에서 다룬다.

### 7. Cron은 품질 루프

파이프라인은 1회성. 품질은 Cron이 지속 감시.

```
Pipeline = "이 아이템을 처리한다" (단방향, 1회)
Cron     = "새로 할 일이 있는가?" (반복, 지속)
```

gap-detection이 스펙과 코드의 차이를 발견하면 새 이슈를 생성. 그 이슈가 다시 파이프라인에 진입. 루프가 스펙 완료까지 반복된다.

---

## 전체 구조

```
┌──────────────────────────────────────────────────────────────┐
│  사용자                                                       │
│                                                               │
│  /auto          /spec          /claw          dashboard       │
└───┬──────────────┬──────────────┬──────────────┬──────────────┘
    │              │              │              │
    ▼              ▼              ▼              ▼
┌──────────────────────────────────────────────────────────────┐
│  autodev CLI (SSOT)                                          │
└──────────────────────────┬───────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────┐
│  코어                                                         │
│                                                               │
│  큐 상태 머신 (Pending → Ready → Running → Done | Skipped)    │
│  Spec 상태 머신 (Draft → Active ↔ Paused → Completed)        │
│  HITL 시스템 (생성 → 응답 → 라우팅)                            │
│  Escalation (실패 횟수 → 재시도/HITL/스킵)                     │
└──────────────────────────┬───────────────────────────────────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│  DataSource  │  │ AgentRuntime │  │  Cron Engine │
│              │  │              │  │              │
│  수집        │  │  LLM 실행    │  │  주기 작업    │
│  상태 정의    │  │  추상화      │  │  품질 루프    │
│  워크플로우   │  │              │  │              │
│  외부 동기화  │  │              │  │              │
└──────────────┘  └──────────────┘  └──────────────┘
```

---

## 핵심 개념

### DataSource 상태 기반 워크플로우

```yaml
sources:
  github:
    states:
      analyze:
        trigger: { label: "autodev:analyze" }
        handlers:
          - prompt: "이슈를 분석하고 구현 가능 여부를 판단해줘"
        on_done: { label: "autodev:implement" }

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

각 state는 하나의 컨베이어 벨트 구간. Done이 되면 on_done 액션이 다음 state의 trigger를 활성화.

### Handler

handler는 **prompt 단일 타입**. 자연어 문자열을 AgentRuntime.invoke()로 실행한다.

prompt는 **순수 작업 지시만** 담당. 린트, 포맷, 컨벤션 준수는 handler가 아닌 인프라 레이어가 보장:

```
hooks (.claude/hooks/)  = 품질 게이트 (lint, format, test — 단계 진입 시 자동 실행)
rules (.claude/rules/)  = 컨벤션 (코딩 스타일, 아키텍처 원칙 — AgentRuntime이 참조)
```

handler prompt가 "린트 돌려줘"를 말할 필요 없다. 컨베이어 벨트의 각 구간에 진입하면 hooks와 rules가 알아서 적용된다.

### 큐 아이템 흐름

```
DataSource.collect(trigger 매칭)
    │
    ▼
  Pending → Ready → Running
    │
    │  handlers 순차 실행
    │  (prompt → AgentRuntime.invoke())
    │
    ├── 전부 성공 → 코어 evaluate: "완료? 추가 검토?"
    │                    │
    │              ┌─────┴─────┐
    │              ▼           ▼
    │            Done        HITL
    │              │
    │              ▼
    │   DataSource.on_done 액션
    │     → 다음 state trigger 활성화
    │     → DataSource.collect()가 다음 턴에 감지
    │
    └── 실패 → escalation (Retry / HITL / Skip)
```

### Cron 품질 루프

```
Pipeline (1회성)          Cron (반복)
─────────────────        ──────────────────
아이템 처리               gap-detection: 스펙 vs 코드
                         qa: 테스트 실행
                         knowledge: PR 지식 추출
                              │
                              ▼
                         gap/bug 발견 → 새 이슈 생성
                              │
                              ▼
                         DataSource.collect() → 파이프라인 재진입
```

---

## OCP 확장점

```
새 외부 시스템     = DataSource impl 추가      → 코어 변경 0  (v5: GitHub만, 나머지는 v6+)
새 LLM            = AgentRuntime impl 추가    → 코어 변경 0
새 파이프라인 단계  = DataSource config 수정    → 코어 변경 0
새 품질 검사       = Cron 등록                 → 코어 변경 0
```

---

## 관심사 분리

| 레이어 | 책임 | 토큰 |
|--------|------|------|
| Daemon | 수집, 큐 관리, handler 실행, cron 스케줄링 | 0 |
| 코어 | 상태 전이, 의존성, 스펙 링크, decision 기록 | 0 |
| DataSource | 워크플로우 정의, 외부 시스템 동기화 | 0 |
| AgentRuntime | LLM 실행 추상화 | handler별 |
| 코어 evaluate | 완료/추가검토 분류 (Done or HITL) | 분류 시 |
| Claw | `/claw` 대화형 에이전트 (자연어 → CLI) | 세션 시 |
| Cron | 주기 작업, 품질 루프 | job별 |
