# Task Internals — 컴포넌트 구조

> Task 내부의 컴포넌트 조립, resolve() 분기, 책임 경계를 정의한다.
> 시스템 레벨 플로우는 [class-diagram.md](./class-diagram.md) 참조.

---

## Task 내부 공통 패턴

모든 Task는 `run(agent)` 내부에서 5단계를 따른다.
이 단계들은 Task 외부(TaskRunner)에 노출되지 않는 **구현 세부사항**이다.

```
  Task.run(agent) {

      ┌──────────────────────────────────────────────────────────┐
      │  1. preflight()                                          │
      │     Notifier: is_open / is_reviewable / is_mergeable     │
      │     Workspace: clone, worktree                           │
      │     실패 시 → early return (라벨 정리 + TaskOutput)       │
      └────────────────────────┬─────────────────────────────────┘
                               ▼
      ┌──────────────────────────────────────────────────────────┐
      │  2. agent 호출 (1회 또는 N회)                             │
      │     agent.run_session(prompt, opts)                      │
      └────────────────────────┬─────────────────────────────────┘
                               ▼
      ┌──────────────────────────────────────────────────────────┐
      │  3. resolve()  ← private 순수 함수                       │
      │     agent 산출물 → typed verdict                         │
      │     Mock 불필요, 단위 테스트 핵심 대상                     │
      └────────────────────────┬─────────────────────────────────┘
                               ▼
      ┌──────────────────────────────────────────────────────────┐
      │  4. apply(verdict)                                       │
      │     verdict에 따른 결정론적 처리                           │
      │     Gh: 라벨, 코멘트, PR review                           │
      │     큐: queue_ops 구성                                    │
      └────────────────────────┬─────────────────────────────────┘
                               ▼
      ┌──────────────────────────────────────────────────────────┐
      │  5. cleanup()                                            │
      │     Workspace: worktree 제거                             │
      └────────────────────────┬─────────────────────────────────┘
                               ▼
      return TaskOutput { queue_ops, logs }
  }
```

---

## 컴포넌트 구조도

```
  ┌──────────────────────────────────────────────────────────────────────────┐
  │                           Task 내부 컴포넌트                             │
  │                                                                          │
  │  ┌──────────────────────────────────────────┐                            │
  │  │           Workspace                      │                            │
  │  │  clone, worktree 생성/제거                │                            │
  │  │  preflight + cleanup에서 사용             │                            │
  │  │  의존: «Git», «Env»                      │                            │
  │  └──────────────────────────────────────────┘                            │
  │                                                                          │
  │  ┌──────────────────────────────────────────┐                            │
  │  │           Notifier                       │                            │
  │  │  pre-flight 검증 + 코멘트 게시            │                            │
  │  │  is_issue_open / is_pr_reviewable         │                            │
  │  │  is_pr_mergeable / post_issue_comment     │                            │
  │  │  의존: «Gh»                               │                            │
  │  └──────────────────────────────────────────┘                            │
  │                                                                          │
  │  ┌──────────────────────────────────────────┐  ┌────────────────────┐    │
  │  │           Analyzer                       │  │    Reviewer        │    │
  │  │  이슈 분석 agent 호출                     │  │  PR 리뷰 agent 호출│    │
  │  │  AnalyzeTask에서 사용                     │  │  Review/ReReview   │    │
  │  │  의존: «Agent»                            │  │  의존: «Agent»     │    │
  │  └──────────────────────────────────────────┘  └────────────────────┘    │
  │                                                                          │
  │  ┌──────────────────────────────────────────┐  ┌────────────────────┐    │
  │  │           Merger                         │  │ KnowledgeExtractor │    │
  │  │  merge 실행 + 충돌 해결                   │  │  학습 포인트 추출   │    │
  │  │  MergeTask에서 사용                       │  │  best-effort       │    │
  │  │  의존: «Agent»                            │  │  의존: «Agent»     │    │
  │  └──────────────────────────────────────────┘  └────────────────────┘    │
  │                                                                          │
  │  Task가 커지면 내부 컴포넌트를 더 분리하면 된다.                           │
  └──────────────────────────────────────────────────────────────────────────┘
```

### 컴포넌트 → Task 매핑

```
  ┌──────────────┬────────────┬──────────┬──────────┬──────────┬────────────┐
  │              │ Workspace  │ Notifier │ Analyzer │ Reviewer │   Merger   │
  ├──────────────┼────────────┼──────────┼──────────┼──────────┼────────────┤
  │ AnalyzeTask  │     ✓      │    ✓     │    ✓     │          │            │
  │ ImplementTask│     ✓      │          │          │          │            │
  │ ReviewTask   │     ✓      │    ✓     │          │    ✓     │            │
  │ ImproveTask  │     ✓      │          │          │          │            │
  │ ReReviewTask │     ✓      │          │          │    ✓     │            │
  │ MergeTask    │     ✓      │    ✓     │          │          │     ✓      │
  └──────────────┴────────────┴──────────┴──────────┴──────────┴────────────┘
```

---

## resolve(): 각 Task별 분기 매트릭스

### resolve()의 위치

```
  impl AnalyzeTask {
      fn resolve(&self, result: &SessionResult) -> AnalyzeVerdict { ... }
  }
  impl ReviewTask {
      fn resolve(&self, result: &SessionResult) -> ReviewVerdict { ... }
  }

  • trait 메서드 아님 — 각 Task의 private 메서드
  • 순수 함수 — 외부 의존성 없음
  • 단위 테스트 핵심 대상
```

### AnalyzeTask.resolve()

```
  Input: SessionResult { exit_code, stdout (JSON) }

  ┌─────────────────────────────┬────────────────────────────────────────┐
  │  조건                        │  결과                                  │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ exit_code != 0              │ Error                                  │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label_remove(WIP)                 │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ verdict: implement          │ Implement                              │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label(WIP→ANALYZED) + 분석 코멘트  │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ verdict: wontfix            │ Wontfix                                │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label(WIP→SKIP) + 사유 코멘트     │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ verdict: needs_clarification│ NeedsClarification                     │
  │ or confidence < threshold   │  queue: [Remove]                       │
  │                             │  gh: label(WIP→SKIP) + 질문 코멘트     │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ JSON parse 실패              │ ParseFailed                            │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label(WIP→ANALYZED) + fallback    │
  └─────────────────────────────┴────────────────────────────────────────┘
```

### ImplementTask.resolve()

```
  Input: SessionResult { exit_code, stdout }

  ┌─────────────────────────────┬────────────────────────────────────────┐
  │  조건                        │  결과                                  │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ exit_code == 0 + PR번호 있음 │ Success(pr_number)                     │
  │                             │  queue: [Remove, PushPr(PENDING)]      │
  │                             │  gh: label_add(WIP, pr) + pr-link 코멘트│
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ exit_code == 0 + PR번호 없음 │ NoPrFound                              │
  │                             │  → gh.api_paginate() fallback 시도     │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label_remove(IMPLEMENTING)        │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ exit_code != 0              │ Failed                                 │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label_remove(IMPLEMENTING)        │
  └─────────────────────────────┴────────────────────────────────────────┘
```

### ReviewTask.resolve()

```
  Input: SessionResult { exit_code, stdout (JSON) }

  ┌─────────────────────────────┬────────────────────────────────────────┐
  │  조건                        │  결과                                  │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ verdict: approve            │ Approve                                │
  │                             │  queue: [Remove]                       │
  │                             │  gh: pr_review(APPROVE) + 리뷰 코멘트   │
  │                             │      label(WIP→DONE, pr)               │
  │                             │      label(IMPL→DONE, issue) ← linked │
  │                             │      knowledge_extract ← best-effort  │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ verdict: request_changes    │ RequestChanges                         │
  │ + linked issue 있음         │  queue: [Remove, PushPr(REVIEW_DONE)]  │
  │                             │  gh: pr_review(REQ_CHANGES) + 피드백   │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ verdict: request_changes    │ RequestChangesExternal                 │
  │ + external PR              │  queue: [Remove]                       │
  │                             │  gh: label(WIP→DONE) + 피드백 코멘트   │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ exit_code != 0              │ Error                                  │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label_remove(WIP)                 │
  └─────────────────────────────┴────────────────────────────────────────┘
```

### ImproveTask.resolve()

```
  Input: SessionResult { exit_code }

  ┌─────────────────────────────┬────────────────────────────────────────┐
  │  조건                        │  결과                                  │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ exit_code == 0              │ Success                                │
  │                             │  queue: [Remove, PushPr(IMPROVED)]     │
  │                             │  gh: iteration 라벨 갱신 (N → N+1)     │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ exit_code != 0              │ Failed                                 │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label_remove(WIP)                 │
  └─────────────────────────────┴────────────────────────────────────────┘
```

### ReReviewTask.resolve()

```
  Input: SessionResult { exit_code, stdout (JSON) }, iteration

  ┌─────────────────────────────┬────────────────────────────────────────┐
  │  조건                        │  결과                                  │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ verdict: approve            │ Approve                                │
  │                             │  queue: [Remove]                       │
  │                             │  gh: pr_review(APPROVE)                │
  │                             │      label(WIP→DONE, pr)               │
  │                             │      iteration 라벨 제거               │
  │                             │      label(IMPL→DONE, issue) ← linked │
  │                             │      knowledge_extract ← best-effort  │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ verdict: request_changes    │ RequestChanges                         │
  │ + iteration < max           │  queue: [Remove, PushPr(REVIEW_DONE)]  │
  │                             │  gh: pr_review(REQ_CHANGES) + 피드백   │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ verdict: request_changes    │ IterationLimitReached  ← CRITICAL     │
  │ + iteration >= max          │  queue: [Remove]                       │
  │                             │  gh: label(WIP→SKIP)                   │
  │                             │      iteration 라벨 제거               │
  │                             │      한계 도달 코멘트                    │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ exit_code != 0              │ Error                                  │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label_remove(WIP)                 │
  └─────────────────────────────┴────────────────────────────────────────┘
```

### MergeTask.resolve()

```
  Input: MergeOutcome { Success | Conflict | Failed | Error }

  ┌─────────────────────────────┬────────────────────────────────────────┐
  │  조건                        │  결과                                  │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ Success                     │ MergeSuccess                           │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label(WIP→DONE)                   │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ Conflict → 해결 성공         │ ConflictResolved                       │
  │ (2차 agent 호출 후)         │  queue: [Remove]                       │
  │                             │  gh: label(WIP→DONE)                   │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ Conflict → 해결 실패         │ ConflictUnresolved                     │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label_remove(WIP)                 │
  ├─────────────────────────────┼────────────────────────────────────────┤
  │ Failed / Error              │ MergeFailed                            │
  │                             │  queue: [Remove]                       │
  │                             │  gh: label_remove(WIP)                 │
  └─────────────────────────────┴────────────────────────────────────────┘
```

---

## 에이전트 호출 패턴

대부분의 Task는 Agent 1회 호출이지만, 3가지 예외가 존재한다.
Task가 워크플로우를 소유하므로 내부에서 자유롭게 N회 호출 가능하다.

```
  ┌──────────────┬──────────────────────────────────────────────────────┐
  │  Task        │  Agent 호출 패턴                                     │
  ├──────────────┼──────────────────────────────────────────────────────┤
  │ AnalyzeTask  │  1회: analyze(issue)                                │
  │ ImplementTask│  1회: implement(issue)  + gh API fallback (PR 조회)  │
  │ ReviewTask   │  1회: review(PR)  + 선택적 2차: knowledge extract    │
  │ ImproveTask  │  1회: improve(PR, feedback)                         │
  │ ReReviewTask │  1회: re-review(PR)  + 선택적 2차: knowledge extract │
  │ MergeTask    │  1회: merge(PR)  + 조건부 2차: resolve conflicts     │
  └──────────────┴──────────────────────────────────────────────────────┘
```

### 다중 호출 상세

```
  ImplementTask.run(agent):
  ─────────────────────────────────────────────
      agent.run_session(&wt, &prompt)        ← 1차
      verdict = resolve(result)
      if verdict == NoPrFound →
          gh.api_paginate(head_branch)        ← GitHub API (Agent 아님)
      apply(verdict)

  MergeTask.run(agent):
  ─────────────────────────────────────────────
      merger.merge_pr(agent, &wt)            ← 1차
      if outcome == Conflict →
          merger.resolve_conflicts(agent, &wt) ← 2차 (필수)
      apply(최종 결과)

  ReviewTask.run(agent):
  ─────────────────────────────────────────────
      reviewer.review_pr(agent, &wt)         ← 1차
      verdict = resolve(result)
      apply(verdict)
      if verdict == Approve && knowledge_enabled →
          extractor.extract(agent, &wt)      ← 2차 (best-effort, 실패 무시)
```

---

## 에이전트 vs 어플리케이션 책임 경계

> **Agent가 내리는 건 "판정"이다. 판정 이후의 상태 전이는 모두 결정론적이다.**
> **이 경계선은 Task 내부에서 관리된다. TaskRunner는 이 경계를 알 필요가 없다.**

```
  ┌──── 에이전트 위임 ──────────────┐  ┌──── 어플리케이션 제어 ─────────┐
  │  LLM 지능이 필요한 것            │  │  결정론적, LLM 불필요           │
  ├──────────────────────────────────┤  ├──────────────────────────────────┤
  │  코드 분석 → verdict            │  │  preflight (검증 + 환경)        │
  │  코드 리뷰 → verdict            │  │  resolve() — 판정 해석 (순수)   │
  │  코드 구현 (브랜치/커밋/PR)     │  │  apply() — 라벨/코멘트/큐       │
  │  머지 실행                      │  │  cleanup() — 리소스 정리         │
  │  충돌 해결                      │  │  설정 분기 (confidence,          │
  │  지식 추출 (best-effort)        │  │    max_iterations 등)           │
  └──────────────────────────────────┘  └──────────────────────────────────┘
           │                                        │
           ▼                                        ▼
    agent.run_session()                      resolve() → verdict
    구조화 JSON 또는 자유형 텍스트            apply(verdict) → gh 호출
```

### Task별 에이전트 위임 상세

```
  ┌──────────────┬─────────────────────────┬──────────────────────────────┐
  │  Task        │  에이전트에게 요청       │  산출물                       │
  ├──────────────┼─────────────────────────┼──────────────────────────────┤
  │ AnalyzeTask  │ 이슈 분석 + JSON 응답   │ { verdict, confidence,       │
  │              │ (verdict/confidence/     │   summary, report,           │
  │              │  report/questions/reason)│   questions, reason }        │
  ├──────────────┼─────────────────────────┼──────────────────────────────┤
  │ ImplementTask│ 이슈 구현 + PR 생성     │ SessionResult                │
  │              │ (워크플로우 프롬프트)     │ { exit_code, stdout(PR번호) }│
  ├──────────────┼─────────────────────────┼──────────────────────────────┤
  │ ReviewTask   │ PR 코드 리뷰 + JSON     │ { verdict, summary }         │
  ├──────────────┼─────────────────────────┼──────────────────────────────┤
  │ ImproveTask  │ 리뷰 피드백 반영 + push │ SessionResult { exit_code }  │
  ├──────────────┼─────────────────────────┼──────────────────────────────┤
  │ ReReviewTask │ 수정된 PR 재리뷰 + JSON │ { verdict, summary }         │
  ├──────────────┼─────────────────────────┼──────────────────────────────┤
  │ MergeTask    │ PR 머지 실행            │ MergeOutcome                 │
  │              │ (/git-utils:merge-pr)   │ { Success|Conflict|Failed }  │
  └──────────────┴─────────────────────────┴──────────────────────────────┘
```

---

## 테스트 전략 상세

### resolve() 단위 테스트

```
  가장 많은 테스트 케이스가 여기에 집중됨.

  #[test]
  fn analyze_resolve_wontfix() {
      let task = AnalyzeTask::new(item, config);
      let result = fake_session(0, json!({ "verdict": "wontfix", "reason": "..." }));
      assert_eq!(task.resolve(&result), AnalyzeVerdict::Wontfix { .. });
  }

  #[test]
  fn review_resolve_approve() {
      let task = ReviewTask::new(item, config);
      let result = fake_session(0, json!({ "verdict": "approve", "summary": "..." }));
      assert_eq!(task.resolve(&result), ReviewVerdict::Approve { .. });
  }

  Mock 필요: 없음 (순수 함수)
  Agent: 불필요
  Gh: 불필요
```

### Task.run() 통합 테스트

```
  전체 워크플로우의 흐름을 검증.

  #[tokio::test]
  async fn analyze_task_wontfix_flow() {
      let agent = MockAgent::with_response(wontfix_json);
      let gh = MockGh::new();
      let task = AnalyzeTask::new(item, ...);

      let output = task.run(&agent).await;

      assert_eq!(output.queue_ops, vec![QueueOp::Remove(work_id)]);
      assert!(gh.label_removed("autodev/WIP/analyzing"));
      assert!(gh.label_added("autodev/SKIP"));
  }

  Mock: Agent (응답 주입) + Gh (호출 기록 검증)
  검증: TaskOutput, Gh 호출 기록, Agent 호출 횟수
```

### AS-IS vs TO-BE 비교

```
  AS-IS: _one() 전체 호출
  ────────────────────────────────────────────────
  테스트 하나에 필요한 mock:  Agent + Gh + Git + Env
  verdict 분기 검증하려면:    Agent mock 응답 조작 필수
  label 검증하려면:           MockGh의 호출 기록 확인
  문제: 모든 테스트가 통합 테스트 수준

  TO-BE: resolve() + run() 분리
  ────────────────────────────────────────────────
  resolve() 테스트:  Mock 없음 (순수 함수)
  run() 테스트:      Mock Agent + Gh (통합)
  핵심: 분기 로직(resolve)을 순수 함수로 테스트,
        통합 테스트는 흐름 검증에만 집중
```
