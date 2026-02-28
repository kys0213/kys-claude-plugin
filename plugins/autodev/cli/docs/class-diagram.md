# Pipeline Design v2 — System Flow

> 시스템 레벨의 플로우와 타입 관계를 정의한다.
> Task 내부 컴포넌트 구조는 [task-internals.md](./task-internals.md) 참조.

---

## AS-IS: 현재 구조

Task trait이 존재하지만, 모든 로직이 `_one()` 함수에 집중되어 있어
Task 구현체는 단순 위임(delegation)에 불과하다.

```
                          ┌─────────────────────┐
                          │    «trait» Task      │
                          ├─────────────────────┤
                          │ + run() → TaskOutput │
                          └──────────┬──────────┘
                                     │ implements
          ┌──────────────┬───────────┼───────────┬──────────────┬──────────────┐
          │              │           │           │              │              │
  ┌───────┴──────┐ ┌─────┴─────┐ ┌──┴───────┐ ┌─┴──────────┐ ┌┴──────────┐ ┌─┴────────┐
  │ AnalyzeTask  │ │Implement- │ │ReviewTask│ │ImproveTask │ │ReReview-  │ │MergeTask │
  │              │ │Task       │ │          │ │            │ │Task       │ │          │
  └──────┬───────┘ └─────┬─────┘ └────┬─────┘ └─────┬──────┘ └─────┬─────┘ └────┬─────┘
         │               │            │              │              │             │
         ▼               ▼            ▼              ▼              ▼             ▼
  ┌─────────────────────────────────────────────────────────────────────────────────────┐
  │                         Monolithic _one() Functions                                 │
  │                                                                                     │
  │  각 함수 내부에 9단계가 하나로 혼재:                                                  │
  │  pre-flight → workspace → agent 호출 → 결과 파싱 → label 전이                       │
  │  → GitHub 코멘트 → knowledge extraction → QueueOp 생성 → cleanup                    │
  └─────────────────────────────────────────────────────────────────────────────────────┘
```

### 문제점

1. **테스트 단위가 너무 큼**: `_one()` 전체를 호출해야만 내부 분기를 검증 가능
2. **Agent 호출이 끼어 있음**: pre-flight/post-processing만 테스트하고 싶어도 Agent mock 필수
3. **SRP 위반**: workspace/notifier/label/comment/knowledge가 하나의 함수에 혼재
4. **Task trait 무의미**: 단순 위임이므로 trait의 추상화 가치 없음

---

## TO-BE: 목표 구조

**Task가 자기 워크플로우를 캡슐화한다.**

```
  AS-IS _one():  모든 관심사가 하나의 함수에 혼재 (SRP 위반)
  TO-BE Task:    run() 하나지만 내부적으로 컴포넌트 조립 (SRP 준수)
                 새 Task 타입 추가 시 TaskRunner 변경 없음 (OCP)
```

```
                     ┌────────────────────────────────────┐
                     │           «trait» Task              │
                     ├────────────────────────────────────┤
                     │ + run(agent) → TaskOutput           │  유일한 public 인터페이스
                     └──────────────┬─────────────────────┘
                                    │
          ┌───────────────┬─────────┼──────────┬──────────────┬──────────────┐
          │               │         │          │              │              │
  ┌───────┴──────┐ ┌──────┴──────┐ ┌┴─────────┐┌─────┴──────┐┌────┴───────┐┌────┴──────┐
  │ AnalyzeTask  │ │ImplementTask│ │ReviewTask ││ImproveTask ││ReReviewTask││ MergeTask │
  │ (내부 조립)   │ │ (내부 조립)  │ │(내부 조립)││(내부 조립)  ││(내부 조립)  ││(내부 조립) │
  └──────────────┘ └─────────────┘ └───────────┘└────────────┘└────────────┘└───────────┘
```

---

## 전체 시스템 플로우

```
  ┌─────────────────────────────────────────────────────────────────────┐
  │                        DAEMON (main loop)                          │
  │                                                                     │
  │  loop {                                                             │
  │      ┌─────────────┐                                                │
  │      │  1. SCAN     │ GitHub API로 이슈/PR 상태 조회                │
  │      │              │ → 새 작업 발견 시 큐에 추가                    │
  │      └──────┬───────┘                                               │
  │             ▼                                                       │
  │      ┌─────────────┐                                                │
  │      │  2. POP      │ TaskQueues에서 READY 상태 작업 꺼냄            │
  │      │              │ → Task 객체 생성 (AnalyzeTask, ReviewTask...) │
  │      └──────┬───────┘                                               │
  │             ▼                                                       │
  │      ┌─────────────┐                                                │
  │      │  3. SPAWN    │ TaskRunner가 유휴 Agent 할당                   │
  │      │              │ → task.run(agent) 호출                        │
  │      │              │ → TaskRunner는 여기까지만, 내부는 모름          │
  │      └──────┬───────┘                                               │
  │             ▼                                                       │
  │      ┌─────────────────────────────────────────────────┐            │
  │      │  4. Task.run(agent)    [캡슐화된 블랙박스]       │            │
  │      │                                                  │            │
  │      │  내부에서 알아서:                                 │            │
  │      │   • preflight (환경 검증 + 워크스페이스)          │            │
  │      │   • agent 호출 (1회 또는 N회)                    │            │
  │      │   • resolve (verdict 해석, 순수 함수)            │            │
  │      │   • apply (라벨/코멘트/PR review)                │            │
  │      │   • cleanup (worktree 정리)                     │            │
  │      │                                                  │            │
  │      │  반환: TaskOutput { queue_ops, logs }            │            │
  │      └──────────────────────┬───────────────────────────┘            │
  │                             ▼                                       │
  │      ┌─────────────┐                                                │
  │      │  5. HANDLE   │ TaskOutput 처리                               │
  │      │              │ → queue_ops 실행 (Remove, PushPr, PushMerge)  │
  │      │              │ → logs DB 기록                                 │
  │      └──────────────┘                                               │
  │  }                                                                  │
  └─────────────────────────────────────────────────────────────────────┘
```

---

## 핵심 타입

```
  ┌──────────────────────────────────────┐
  │         «trait» Task                 │
  ├──────────────────────────────────────┤
  │ + run(agent: &dyn Agent)             │
  │     → TaskOutput                     │  유일한 trait 메서드
  └──────────────────────────────────────┘

  ┌──────────────────────────────────────┐
  │           TaskOutput                 │
  ├──────────────────────────────────────┤  Task.run()의 반환값
  │ + work_id: String                    │  Daemon이 큐 조작 + DB 로그 처리
  │ + repo_name: String                  │
  │ + queue_ops: Vec<QueueOp>            │
  │ + logs: Vec<NewConsumerLog>          │
  └──────────────────────────────────────┘

  ┌──────────────────────────────────────┐
  │         «enum» QueueOp               │
  ├──────────────────────────────────────┤  Daemon이 실행하는 큐 조작
  │   Remove(work_id)                    │
  │   PushPr(state, PrItem)              │
  │   PushMerge(state, MergeItem)        │
  └──────────────────────────────────────┘

  ※ SideEffect, Invocation, SkipReason, Verdict 등은
    Task 내부 구현의 세부사항 — trait 수준에서 노출하지 않는다.
```

---

## 의존성 방향

```
  ┌──────────────────┐        ┌──────────────────┐
  │     Daemon       │───────▶│   TaskQueues     │
  │  (main loop)     │        │  issues / prs /  │
  └────────┬─────────┘        │  merges          │
           │                  └──────────────────┘
           │ spawn
           ▼
  ┌──────────────────┐
  │   TaskRunner     │
  │   (scheduler)    │
  ├──────────────────┤
  │ 알고 있는 것:     │
  │  • Task trait    │
  │  • Agent trait   │
  │                  │
  │ 모르는 것:       │
  │  • 내부 흐름     │
  │  • Agent 호출 횟수│
  │  • 라벨 전이 규칙│
  └────────┬─────────┘
           │ task.run(agent)
           ▼
  ┌──────────────────┐
  │      Task        │
  │ (캡슐화된 단위)   │
  ├──────────────────┤
  │ 소유:             │        ┌──────────────────┐
  │  • 전체 워크플로우 │───────▶│ Agent, Gh, Git,  │
  │  • resolve()     │        │ Env (주입받음)    │
  │  • apply()       │        └──────────────────┘
  │                  │
  │ 반환:            │
  │  • TaskOutput    │
  └──────────────────┘

  ※ 새 Task 추가 시 TaskRunner 변경 없음 (OCP)
  ※ Task 내부 변경 시 TaskRunner 영향 없음 (디미터 법칙)
```

---

## Infrastructure Traits

```
  «trait» Agent    │  «trait» Gh        │  «trait» Git      │  «trait» Env
  ─────────────    │  ──────────        │  ──────────       │  ──────────
  run_session()    │  label_add/remove  │  clone()          │  var()
                   │  issue_comment()   │  worktree_add()   │
  ClaudeAgent      │  pr_review()       │  worktree_remove()│  OsEnv
  MockAgent        │  api_paginate()    │                   │  TestEnv
                   │  RealGh / MockGh   │  RealGit / MockGit│
```

---

## 테스트 전략 (요약)

```
                       resolve() 단위 테스트
                       ━━━━━━━━━━━━━━━━━━━
                       Mock: 없음 (순수 함수)
                       가장 많은 케이스 집중
                              │
                    ┌─────────┴─────────┐
                    ▼                   ▼
          Task.run() 통합 테스트    E2E 테스트
          ━━━━━━━━━━━━━━━━━━━    ━━━━━━━━━━
          Mock: Agent+Gh+Git     Mock: 전부
          흐름 검증               전체 파이프라인
```

상세 테스트 전략은 [task-internals.md](./task-internals.md) 참조.
