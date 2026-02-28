# Pipeline Class Diagram

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
  ├──────────────┤ ├───────────┤ ├──────────┤ ├────────────┤ ├───────────┤ ├──────────┤
  │ item         │ │ item      │ │ item     │ │ item       │ │ item      │ │ item     │
  │ env,gh,git   │ │ env,gh,git│ │ env,gh,  │ │ env,gh,git │ │ env,gh,   │ │ env,gh,  │
  │ agent        │ │ agent     │ │ git,agent│ │ agent      │ │ git,agent │ │ git,agent│
  │              │ │           │ │ sw       │ │            │ │ sw        │ │          │
  ├──────────────┤ ├───────────┤ ├──────────┤ ├────────────┤ ├───────────┤ ├──────────┤
  │ run():       │ │ run():    │ │ run():   │ │ run():     │ │ run():    │ │ run():   │
  │  delegate to │ │  delegate │ │  delegate│ │  delegate  │ │  delegate │ │  delegate│
  │  analyze_one │ │  impl_one │ │  rev_one │ │  impr_one  │ │  re_rev   │ │  merge   │
  └──────────────┘ └───────────┘ └──────────┘ └────────────┘ └───────────┘ └──────────┘
          │                │           │           │              │              │
          ▼                ▼           ▼           ▼              ▼              ▼
  ┌─────────────────────────────────────────────────────────────────────────────────────┐
  │                         Monolithic _one() Functions                                 │
  │                                                                                     │
  │  analyze_one()  implement_one()  review_one()  improve_one()  re_review_one()  merge│
  │                                                                                     │
  │  각 함수 내부:                                                                       │
  │  ┌──────────────────────────────────────────────────────────┐                       │
  │  │ 1. Pre-flight check (is_open, is_reviewable, ...)       │  ← 테스트 불가 분리    │
  │  │ 2. Workspace setup (clone, create_worktree)             │                       │
  │  │ 3. Agent 호출 (analyze, run_session, review_pr, merge)  │  ← mock 필요          │
  │  │ 4. 결과 파싱 (verdict, PR number, merge outcome)        │  ← 테스트 불가 분리    │
  │  │ 5. Label 전이 (WIP→DONE, WIP→SKIP, iteration 관리)     │                       │
  │  │ 6. GitHub 코멘트 (post_issue_comment, pr_review)        │                       │
  │  │ 7. Knowledge extraction (best-effort)                   │                       │
  │  │ 8. QueueOp 생성 (Remove, PushPr, PushMerge)            │  ← 테스트 불가 분리    │
  │  │ 9. Worktree cleanup (always)                            │                       │
  │  └──────────────────────────────────────────────────────────┘                       │
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
- TaskRunner는 **스케줄러** — 유휴 Agent를 Task에 할당하고 `run()` 호출, 그 다음은 모른다
- Task는 내부에서 **컴포넌트를 조립**하여 SRP를 지킨다
- 새 Task 타입 추가 시 TaskRunner 변경 없음 (OCP)

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

  ┌─────────────────────────────────────────────────────────────────────────────────────────┐
  │                           TaskRunner (scheduler)                                       │
  │                                                                                         │
  │  pub fn spawn(join_set, task, agent) {                                                  │
  │      join_set.spawn(async move {                                                        │
  │          task.run(agent).await         // Task에 Agent 할당, 그 다음은 모른다            │
  │      });                                                                                │
  │  }                                                                                      │
  │                                                                                         │
  │  // TaskRunner는 Task 내부 워크플로우를 알지 못한다                                       │
  │  // Agent 몇 번 호출하는지, 라벨을 어떻게 바꾸는지 — 전부 Task의 구현 세부사항             │
  └─────────────────────────────────────────────────────────────────────────────────────────┘
```

### Task 내부 구조: 컴포넌트 조립

각 Task는 `run()` 내부에서 관심사별 컴포넌트를 조립한다.
이것은 Task 외부에서 알 필요 없는 **구현 세부사항**이다.

```
  ┌─────────────────────────────────────────────────────────────┐
  │ Task.run(agent) 내부 흐름:                                   │
  │                                                              │
  │  ┌──────────────────┐                                        │
  │  │ 1. preflight()   │ ← Notifier: is_open / is_reviewable   │
  │  │    환경 준비       │   Workspace: clone, worktree          │
  │  └────────┬─────────┘                                        │
  │           │ 실패 시 → early return (라벨 정리 + TaskOutput)   │
  │           ▼                                                  │
  │  ┌──────────────────┐                                        │
  │  │ 2. agent 호출     │ ← agent.run_session(prompt, opts)     │
  │  │    (필요 시 N회)  │   MergeTask: merge → conflict → 재호출│
  │  └────────┬─────────┘   ReviewTask: review → knowledge 추출  │
  │           ▼                                                  │
  │  ┌──────────────────┐                                        │
  │  │ 3. resolve()      │ ← private 순수 함수                   │
  │  │    verdict 해석    │   agent 산출물 → typed verdict         │
  │  └────────┬─────────┘                                        │
  │           ▼                                                  │
  │  ┌──────────────────┐                                        │
  │  │ 4. apply()        │ ← Gh: 라벨, 코멘트, PR review         │
  │  │    상태 전이 실행  │   verdict에 따른 결정론적 처리          │
  │  └────────┬─────────┘                                        │
  │           ▼                                                  │
  │  ┌──────────────────┐                                        │
  │  │ 5. cleanup()      │ ← Workspace: worktree 제거            │
  │  └────────┬─────────┘                                        │
  │           ▼                                                  │
  │  return TaskOutput { queue_ops, logs }                       │
  └─────────────────────────────────────────────────────────────┘
```

- **preflight**: 환경 검증 + 워크스페이스 준비 (Notifier, Workspace)
- **agent 호출**: LLM에게 작업 위임 (Agent)
- **resolve()**: agent 산출물 → typed verdict (순수 함수, **private**)
- **apply()**: verdict에 따른 라벨/코멘트/큐 처리 (Gh)
- **cleanup()**: 리소스 정리 (Workspace)

각 단계가 별도 관심사이므로 SRP 충족. 하지만 이 분리는 Task 외부에 노출되지 않는다.
Task가 커지면 내부 컴포넌트를 더 분리하면 된다.

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
  │ + work_id: String                    │  main loop가 큐 조작 + DB 로그 처리
  │ + repo_name: String                  │
  │ + queue_ops: Vec<QueueOp>            │
  │ + logs: Vec<NewConsumerLog>          │
  └──────────────────────────────────────┘

  ※ SideEffect, Invocation, SkipReason 등은
    Task 내부 구현의 세부사항 — trait 수준에서 노출하지 않는다.
    각 Task가 자유롭게 내부 타입을 정의할 수 있다.
```

### 디미터 법칙 적용

```
  TaskRunner (스케줄러)
  ─────────────────────────────
  알아야 하는 것:
    • Task trait (run → TaskOutput)
    • Agent trait (Task에 주입할 대상)

  알지 않아도 되는 것:
    • Task 내부 워크플로우 (몇 단계?)
    • Agent 호출 횟수 (1회? 2회?)
    • 라벨 전이 규칙
    • resolve() 함수의 존재
    • SideEffect 타입의 존재
```

```
  Task (캡슐화된 실행 단위)
  ─────────────────────────────
  소유하는 것:
    • 자기 워크플로우 전체
    • preflight → agent → resolve → apply → cleanup
    • Agent 몇 번 호출할지
    • 어떤 라벨을 붙이고 뗄지

  반환하는 것:
    • TaskOutput { queue_ops, logs } — 최소한의 인터페이스
```

---

## Concrete Task 내부 설계

각 Task의 `run()` 내부는 **구현 세부사항**이다.
resolve()는 trait 메서드가 아닌 **private 메서드**로, 순수 함수 단위 테스트가 가능하다.

### resolve()의 위치와 테스트

```
  impl AnalyzeTask {
      fn resolve(&self, result: &SessionResult) -> AnalyzeVerdict { ... }
  }

  impl ReviewTask {
      fn resolve(&self, result: &ReviewOutput) -> ReviewVerdict { ... }
  }

  // 테스트: Mock 불필요 (순수 함수)
  #[test]
  fn resolve_wontfix() {
      let task = AnalyzeTask::new(item, config);
      let result = fake_session_result(wontfix_json);
      assert_eq!(task.resolve(&result), AnalyzeVerdict::Wontfix { .. });
  }
```

### 에이전트 호출이 여러 번 필요한 Task

Task가 워크플로우를 소유하므로, 내부에서 Agent를 N회 호출할 수 있다.
TaskRunner가 알 필요 없는 구현 세부사항이다.

```
  MergeTask.run(agent):
      merger.merge_pr(agent, &wt_path)        ← 1차 Agent 호출
      if conflict →
          merger.resolve_conflicts(agent)      ← 2차 Agent 호출
      apply(최종 결과)

  ReviewTask.run(agent):
      reviewer.review_pr(agent, &wt_path)     ← 1차 Agent 호출
      verdict = resolve(result)
      apply(verdict)
      if approve && knowledge_enabled →
          extractor.extract(agent, &wt_path)  ← 2차 (best-effort)

  ImplementTask.run(agent):
      agent.run_session(&wt_path, &prompt)    ← 1차 Agent 호출
      verdict = resolve(result)
      if PR번호 없음 →
          gh.api_paginate(...)                 ← GitHub API fallback
      apply(verdict)
```

---

## 의존성 구조

```
  ┌─────────────────────────────────────────────────────────────────┐
  │                      DAEMON (Orchestrator)                     │
  │  loop {                                                         │
  │    scan → pop from queues → TaskRunner.spawn(task, agent)      │
  │    join completed → handle_task_output(queues, db, output)     │
  │  }                                                              │
  └─────────────────────────┬───────────────────────────────────────┘
                            │ uses
              ┌─────────────┴──────────────┐
              ▼                            ▼
  ┌─────────────────────┐     ┌───────────────────┐
  │  TaskRunner         │     │    TaskQueues     │
  │  (scheduler)        │     ├───────────────────┤
  ├─────────────────────┤     │ issues: StateQueue│
  │ spawn(task, agent)  │     │ prs: StateQueue   │
  │ → task.run(agent)   │     │ merges: StateQueue│
  │ 그 이후는 모름       │     └───────────────────┘
  └─────────┬───────────┘
            ▼
  ┌───────────────────────────────────────────┐
  │          Task.run(agent)                  │
  │  (캡슐화 — TaskRunner에 노출 안 됨)        │
  │                                            │
  │  내부에서 자유롭게:                         │
  │   • agent 호출 (1회 또는 N회)              │
  │   • gh 호출 (라벨, 코멘트, PR review)      │
  │   • git 호출 (worktree)                    │
  │   • resolve() (순수 함수, private)         │
  │                                            │
  │  반환: TaskOutput { queue_ops, logs }      │
  └───────────────────────────────────────────┘
```

### 의존성 방향

```
  TaskRunner (스케줄러)          Task (캡슐화된 실행 단위)
  ──────────────────────         ─────────────────────────
  의존: Task trait, Agent trait  의존: Agent, Gh, Git, Env (주입받음)
  역할: 할당 + spawn             역할: 워크플로우 전체 소유
  모름:                          소유:
   • 내부 흐름                    • preflight 로직
   • Agent 호출 횟수              • Agent 프롬프트 구성
   • 라벨 전이 규칙               • resolve() 판정 해석
   • resolve()의 존재             • apply() 상태 전이
                                  • cleanup() 리소스 정리

  ※ 새 Task 추가 시 TaskRunner 변경 없음 (OCP)
  ※ Task 내부 변경 시 TaskRunner 영향 없음 (디미터 법칙)
```

### Infrastructure Traits (변경 없음)

```
  «trait» Agent    │  «trait» Gh        │  «trait» Git      │  «trait» Env
  ─────────────    │  ──────────        │  ──────────       │  ──────────
  run_session()    │  label_add/remove  │  clone()          │  var()
                   │  issue_comment()   │  worktree_add()   │
  ClaudeAgent      │  pr_review()       │  worktree_remove()│  OsEnv
  MockAgent        │  api_paginate()    │                   │  TestEnv
                   │  RealGh / MockGh   │  RealGit / MockGit│
```

### Components (Task 내부에서 조립)

```
  Task가 내부적으로 사용하는 컴포넌트:

  Workspace   — clone, worktree 관리 (Git, Env 의존)
  Notifier    — pre-flight, comment 게시 (Gh 의존)
  Analyzer    — 이슈 분석 실행 (Agent 의존)
  Reviewer    — PR 리뷰 실행 (Agent 의존)
  Merger      — PR 머지 + 충돌 해결 (Agent 의존)

  Task가 커지면 내부 컴포넌트를 더 분리하면 된다.
  이것은 TaskRunner가 알 필요 없는 구현 세부사항이다.
```

---

## 테스트 전략

```
  ┌─── resolve() 단위 테스트 (private, 가장 많은 케이스) ─────┐
  │  Mock: 없음 (순수 함수)                                     │
  │  Input: 미리 구성한 SessionResult / ReviewOutput            │
  │  검증: verdict 값이 기대와 일치하는가                        │
  │  ※ 60개 분기 중 대부분이 여기서 테스트됨                     │
  └─────────────────────────────────────────────────────────────┘

  ┌─── Task.run() 통합 테스트 ──────────────────────────────────┐
  │  Mock: Agent + Gh + Git + Env                               │
  │  검증: TaskOutput의 queue_ops, Gh 호출 기록, Agent 호출 횟수 │
  └─────────────────────────────────────────────────────────────┘

  ┌─── E2E 테스트 (기존 pipeline_e2e_tests 계승) ──────────────┐
  │  Mock: 전부                                                 │
  │  검증: scan → pop → spawn → handle_output 전체 흐름         │
  └─────────────────────────────────────────────────────────────┘
```

---

## 에이전트 vs 어플리케이션 책임 경계

> **Agent가 내리는 건 "판정"이다. 판정 이후의 상태 전이는 모두 결정론적이다.**
> **이 경계선은 Task 내부에서 관리된다. TaskRunner는 이 경계를 알 필요가 없다.**

```
  Task.run(agent) 내부:

  ┌─── 에이전트 위임 ──────────┐  ┌─── 어플리케이션 제어 ──────┐
  │  코드 분석 → verdict       │  │  preflight (검증 + 환경)   │
  │  코드 리뷰 → verdict       │  │  resolve() — 판정 해석     │
  │  코드 구현                 │  │  apply() — 라벨/코멘트/큐  │
  │  머지 실행                 │  │  cleanup() — 리소스 정리   │
  │  충돌 해결                 │  │                            │
  │  지식 추출 (best-effort)   │  │  설정 분기 (confidence,    │
  └────────────────────────────┘  │   max_iterations 등)       │
                                  └────────────────────────────┘

  이 경계는 Task 내부의 관심사이며, TaskRunner에게는 보이지 않는다.
```
