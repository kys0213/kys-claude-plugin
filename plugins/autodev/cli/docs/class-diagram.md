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

`_one()` 내부 로직을 **before / after** 로 분리하여,
Agent 호출 없이도 전처리·후처리를 독립 테스트할 수 있게 한다.

```
                     ┌──────────────────────────────────────────┐
                     │              «trait» Task                 │
                     ├──────────────────────────────────────────┤
                     │ + before_invoke() → Result<Invocation,   │
                     │                           SkipReason>    │
                     │ + after_invoke(output) → TaskResult      │
                     │ + cleanup()                               │
                     └────────────────────┬─────────────────────┘
                                          │
          ┌───────────────┬───────────────┼──────────────┬──────────────┬──────────────┐
          │               │               │              │              │              │
  ┌───────┴──────┐ ┌──────┴──────┐ ┌──────┴─────┐ ┌─────┴──────┐ ┌────┴───────┐ ┌────┴──────┐
  │ AnalyzeTask  │ │ImplementTask│ │ ReviewTask  │ │ImproveTask │ │ReReviewTask│ │ MergeTask │
  └───────┬──────┘ └──────┬──────┘ └──────┬─────┘ └─────┬──────┘ └────┬───────┘ └────┬──────┘
          │               │               │              │              │              │
          ▼               ▼               ▼              ▼              ▼              ▼

  ┌─────────────────────────────────────────────────────────────────────────────────────────┐
  │                              TaskRunner (orchestrator)                                  │
  │                                                                                         │
  │  pub async fn execute(task: &mut dyn Task, agent: &dyn Agent) -> TaskOutput {           │
  │      // 1. Pre-flight + request 구성                                                    │
  │      let invocation = match task.before_invoke() {                                      │
  │          Ok(inv) => inv,                                                                │
  │          Err(skip) => return skip.into_task_output(),   // Agent 호출 없이 종료          │
  │      };                                                                                 │
  │                                                                                         │
  │      // 2. Agent 호출 (유일한 외부 호출 지점)                                              │
  │      let agent_output = agent.run_session(                                              │
  │          &invocation.cwd, &invocation.prompt, &invocation.opts                          │
  │      ).await;                                                                           │
  │                                                                                         │
  │      // 3. 후처리 + queue ops/labels/comments 결정                                       │
  │      let result = task.after_invoke(agent_output);                                      │
  │                                                                                         │
  │      // 4. Cleanup (항상 실행)                                                            │
  │      task.cleanup();                                                                    │
  │                                                                                         │
  │      result.into_task_output()                                                          │
  │  }                                                                                      │
  └─────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 핵심 타입 정의

```
  ┌─────────────────────────────────────┐
  │            Invocation               │
  ├─────────────────────────────────────┤  before_invoke() 성공 시 반환
  │ + cwd: PathBuf                      │  Agent에게 전달할 요청을 기술
  │ + prompt: String                    │
  │ + opts: SessionOptions              │
  └─────────────────────────────────────┘

  ┌─────────────────────────────────────┐
  │            SkipReason               │
  ├─────────────────────────────────────┤  before_invoke() 실패 시 반환
  │ + work_id: String                   │  Agent 호출 없이 바로 종료
  │ + repo_name: String                 │
  │ + reason: SkipKind                  │
  │ + queue_ops: Vec<QueueOp>           │
  │ + logs: Vec<NewConsumerLog>         │
  │ + into_task_output() → TaskOutput   │
  └─────────────────────────────────────┘

  ┌─────────────────────────────────────┐
  │     «enum» SkipKind                 │
  ├─────────────────────────────────────┤
  │   IssueClosed                       │
  │   PrNotReviewable                   │
  │   PrNotMergeable                    │
  │   CloneFailed(String)               │
  │   WorktreeCreationFailed(String)    │
  └─────────────────────────────────────┘

  ┌─────────────────────────────────────┐
  │            TaskResult               │
  ├─────────────────────────────────────┤  after_invoke() 반환값
  │ + work_id: String                   │  Agent 결과 해석 후 queue ops 결정
  │ + repo_name: String                 │
  │ + queue_ops: Vec<QueueOp>           │
  │ + logs: Vec<NewConsumerLog>         │
  │ + side_effects: Vec<SideEffect>     │
  │ + into_task_output() → TaskOutput   │
  └─────────────────────────────────────┘

  ┌─────────────────────────────────────┐
  │     «enum» SideEffect               │
  ├─────────────────────────────────────┤  후처리에서 발생하는 외부 호출 기술
  │   LabelRemove { repo, num, label }  │  (실행은 TaskRunner가 담당)
  │   LabelAdd { repo, num, label }     │
  │   PostComment { repo, num, body }   │
  │   PrReview { repo, num, event, .. } │
  │   ExtractKnowledge { ... }          │
  └─────────────────────────────────────┘
```

---

## Concrete Task 내부 구조 (before / after 분리)

### AnalyzeTask

```
  ┌─────────────────────────────────────────────────────────────┐
  │                      AnalyzeTask                            │
  ├─────────────────────────────────────────────────────────────┤
  │ - item: IssueItem                                           │
  │ - workspace: Workspace                                      │
  │ - notifier: Notifier                                        │
  │ - config: ConsumerConfig                                    │
  │ - wt_path: Option<PathBuf>          // cleanup용            │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ notifier.is_issue_open()  → Err(IssueClosed)          │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt, opts: json_schema })     │
  │                                                             │
  │ after_invoke(agent_output):                                 │
  │   ├─ exit_code != 0 → Remove + WIP 제거                    │
  │   ├─ parse analysis JSON                                    │
  │   │   ├─ Wontfix → Remove + SKIP + 사유 코멘트             │
  │   │   ├─ NeedsClarification | low confidence → SKIP         │
  │   │   ├─ Implement → ANALYZED + 분석 리포트 코멘트          │
  │   │   └─ parse 실패 → fallback ANALYZED                    │
  │   └─ Err → Remove + WIP 제거                               │
  │                                                             │
  │ cleanup():                                                  │
  │   └─ workspace.remove_worktree()                            │
  └─────────────────────────────────────────────────────────────┘
```

### ImplementTask

```
  ┌─────────────────────────────────────────────────────────────┐
  │                     ImplementTask                           │
  ├─────────────────────────────────────────────────────────────┤
  │ - item: IssueItem                                           │
  │ - workspace: Workspace                                      │
  │ - gh: Arc<dyn Gh>                   // PR fallback 조회용   │
  │ - wt_path: Option<PathBuf>                                  │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt, opts: default })         │
  │                                                             │
  │ after_invoke(agent_output):                                 │
  │   ├─ exit_code != 0 → Remove + IMPLEMENTING 제거           │
  │   ├─ extract_pr_number(stdout)                              │
  │   │   ├─ Some(pr) → Remove + PushPr(PENDING)               │
  │   │   ├─ None → find_existing_pr(gh) fallback              │
  │   │   │   ├─ Some(pr) → Remove + PushPr(PENDING)           │
  │   │   │   └─ None → Remove + IMPLEMENTING 제거             │
  │   └─ Err → Remove + IMPLEMENTING 제거                      │
  │                                                             │
  │ cleanup():                                                  │
  │   └─ workspace.remove_worktree()                            │
  └─────────────────────────────────────────────────────────────┘
```

### ReviewTask

```
  ┌─────────────────────────────────────────────────────────────┐
  │                      ReviewTask                             │
  ├─────────────────────────────────────────────────────────────┤
  │ - item: PrItem                                              │
  │ - workspace: Workspace                                      │
  │ - notifier: Notifier                                        │
  │ - config: ConsumerConfig                                    │
  │ - sw: Arc<dyn SuggestWorkflow>                              │
  │ - wt_path: Option<PathBuf>                                  │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ notifier.is_pr_reviewable() → Err(PrNotReviewable)    │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt, opts: json_schema })     │
  │                                                             │
  │ after_invoke(agent_output):                                 │
  │   ├─ exit_code != 0 → Remove + WIP 제거                    │
  │   ├─ parse review verdict                                   │
  │   │   ├─ Approve                                            │
  │   │   │   ├─ PrReview(APPROVE)                              │
  │   │   │   ├─ knowledge extraction (if enabled)              │
  │   │   │   ├─ linked issue → issue DONE                      │
  │   │   │   └─ PR DONE                                        │
  │   │   ├─ RequestChanges + linked issue                      │
  │   │   │   ├─ PrReview(REQUEST_CHANGES)                      │
  │   │   │   └─ PushPr(REVIEW_DONE) + review_comment 보존     │
  │   │   ├─ RequestChanges + external PR                       │
  │   │   │   ├─ PostComment (코멘트만)                          │
  │   │   │   └─ PR DONE                                        │
  │   │   └─ None → RequestChanges와 동일 처리                  │
  │   └─ Err → Remove + WIP 제거                               │
  │                                                             │
  │ cleanup():                                                  │
  │   └─ workspace.remove_worktree()                            │
  └─────────────────────────────────────────────────────────────┘
```

### ImproveTask

```
  ┌─────────────────────────────────────────────────────────────┐
  │                     ImproveTask                             │
  ├─────────────────────────────────────────────────────────────┤
  │ - item: PrItem                                              │
  │ - workspace: Workspace                                      │
  │ - wt_path: Option<PathBuf>                                  │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt, opts: default })         │
  │                                                             │
  │ after_invoke(agent_output):                                 │
  │   ├─ exit_code != 0 → Remove + WIP 제거                    │
  │   ├─ exit_code == 0                                         │
  │   │   ├─ iteration > 0 → 이전 iteration 라벨 제거          │
  │   │   ├─ iteration++ → 새 iteration 라벨 추가              │
  │   │   └─ PushPr(IMPROVED)                                   │
  │   └─ Err → Remove + WIP 제거                               │
  │                                                             │
  │ cleanup():                                                  │
  │   └─ workspace.remove_worktree()                            │
  └─────────────────────────────────────────────────────────────┘
```

### ReReviewTask

```
  ┌─────────────────────────────────────────────────────────────┐
  │                     ReReviewTask                            │
  ├─────────────────────────────────────────────────────────────┤
  │ - item: PrItem                                              │
  │ - workspace: Workspace                                      │
  │ - notifier: Notifier                                        │
  │ - config: DevelopConfig                                     │
  │ - sw: Arc<dyn SuggestWorkflow>                              │
  │ - wt_path: Option<PathBuf>                                  │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt, opts: json_schema })     │
  │                                                             │
  │ after_invoke(agent_output):                                 │
  │   ├─ exit_code != 0 → Remove + WIP 제거                    │
  │   ├─ parse review verdict                                   │
  │   │   ├─ Approve                                            │
  │   │   │   ├─ PrReview(APPROVE)                              │
  │   │   │   ├─ knowledge extraction (if enabled)              │
  │   │   │   ├─ linked issue → issue DONE                      │
  │   │   │   ├─ iteration 라벨 제거                             │
  │   │   │   └─ PR DONE                                        │
  │   │   ├─ RequestChanges + iteration < max                   │
  │   │   │   ├─ PrReview(REQUEST_CHANGES)                      │
  │   │   │   └─ PushPr(REVIEW_DONE) + review_comment 보존     │
  │   │   ├─ RequestChanges + iteration >= max ← CRITICAL       │
  │   │   │   ├─ SKIP + iteration 라벨 제거                     │
  │   │   │   └─ PostComment("iteration limit reached")         │
  │   │   └─ None → RequestChanges와 동일 처리                  │
  │   └─ Err → Remove + WIP 제거                               │
  │                                                             │
  │ cleanup():                                                  │
  │   └─ workspace.remove_worktree()                            │
  └─────────────────────────────────────────────────────────────┘
```

### MergeTask

```
  ┌─────────────────────────────────────────────────────────────┐
  │                      MergeTask                              │
  ├─────────────────────────────────────────────────────────────┤
  │ - item: MergeItem                                           │
  │ - workspace: Workspace                                      │
  │ - notifier: Notifier                                        │
  │ - merger: Merger                    // merge 전용 컴포넌트  │
  │ - wt_path: Option<PathBuf>                                  │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ notifier.is_pr_mergeable() → Err(PrNotMergeable)      │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt: merge_pr, opts })        │
  │                                                             │
  │ after_invoke(agent_output):                                 │
  │   ├─ parse MergeOutcome                                     │
  │   │   ├─ Success → PR DONE                                  │
  │   │   ├─ Conflict → resolve_conflicts()                     │
  │   │   │   ├─ resolve 성공 → PR DONE                         │
  │   │   │   └─ resolve 실패 → Remove + WIP 제거              │
  │   │   ├─ Failed → Remove + WIP 제거                         │
  │   │   └─ Error → Remove + WIP 제거                          │
  │   └─ (MergeTask는 Agent 호출 대신 Merger 사용)              │
  │                                                             │
  │ cleanup():                                                  │
  │   └─ workspace.remove_worktree()                            │
  └─────────────────────────────────────────────────────────────┘
```

---

## 의존성 구조 (Dependency Graph)

```
  ┌─────────────────────────────────────────────────────────────────────────┐
  │                          DAEMON (Orchestrator)                         │
  │                                                                         │
  │  loop {                                                                 │
  │    scan → pop from queues → TaskRunner.execute(task, agent)             │
  │    handle_task_output(queues, db, output)                               │
  │  }                                                                      │
  └───────────────────────────────────┬─────────────────────────────────────┘
                                      │ uses
                    ┌─────────────────┴─────────────────┐
                    ▼                                   ▼
        ┌───────────────────┐               ┌───────────────────┐
        │    TaskRunner     │               │    TaskQueues     │
        ├───────────────────┤               ├───────────────────┤
        │ execute(task,     │               │ issues: StateQueue│
        │         agent)    │               │ prs: StateQueue   │
        │  → TaskOutput     │               │ merges: StateQueue│
        └────────┬──────────┘               └───────────────────┘
                 │ calls
      ┌──────────┼──────────────┐
      ▼          ▼              ▼
  before_     Agent          after_
  invoke()  .run_session()   invoke()
      │                        │
      │          │              │
      ▼          │              ▼
  Invocation ────┘          TaskResult
  or SkipReason             { queue_ops, side_effects, logs }
```

### Infrastructure Traits (변경 없음)

```
  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
  │  «trait» Agent   │  │   «trait» Gh     │  │   «trait» Git    │
  ├──────────────────┤  ├──────────────────┤  ├──────────────────┤
  │ run_session()    │  │ api_get_field()  │  │ clone()          │
  │                  │  │ api_paginate()   │  │ pull_ff_only()   │
  │ ┌────────────┐   │  │ issue_comment()  │  │ worktree_add()   │
  │ │ ClaudeAgent│   │  │ label_remove()   │  │ worktree_remove()│
  │ │ MockAgent  │   │  │ label_add()      │  │ checkout_branch()│
  │ └────────────┘   │  │ create_pr()      │  │ add_commit_push()│
  └──────────────────┘  │ pr_review()      │  │                  │
                        │                  │  │ ┌────────────┐   │
  ┌──────────────────┐  │ ┌────────────┐   │  │ │ RealGit    │   │
  │  «trait» Env     │  │ │ RealGh     │   │  │ │ MockGit    │   │
  ├──────────────────┤  │ │ MockGh     │   │  │ └────────────┘   │
  │ var()            │  │ └────────────┘   │  └──────────────────┘
  │                  │  └──────────────────┘
  │ ┌────────────┐   │  ┌──────────────────────┐
  │ │ OsEnv      │   │  │«trait» SuggestWorkflow│
  │ │ TestEnv    │   │  ├──────────────────────┤
  │ └────────────┘   │  │ query_tool_frequency()│
  └──────────────────┘  │ query_filtered_sessions│
                        │ query_repetition()    │
                        └──────────────────────┘
```

### Components (Task가 내부적으로 사용)

```
  ┌──────────────────────────────────────────────────────────────────┐
  │                        Components Layer                         │
  │                                                                  │
  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐           │
  │  │  Workspace   │  │  Notifier    │  │   Analyzer   │           │
  │  ├──────────────┤  ├──────────────┤  ├──────────────┤           │
  │  │ ensure_      │  │ is_issue_    │  │ analyze()    │           │
  │  │  cloned()    │  │  open()      │  └──────────────┘           │
  │  │ create_      │  │ is_pr_       │  ┌──────────────┐           │
  │  │  worktree()  │  │  reviewable()│  │  Reviewer    │           │
  │  │ remove_      │  │ is_pr_       │  ├──────────────┤           │
  │  │  worktree()  │  │  mergeable() │  │ review_pr()  │           │
  │  └──────────────┘  │ post_issue_  │  └──────────────┘           │
  │      │   │         │  comment()   │  ┌──────────────┐           │
  │      │   │         └──────────────┘  │   Merger     │           │
  │      ▼   ▼             │             ├──────────────┤           │
  │   «Git» «Env»         ▼             │ merge_pr()   │           │
  │                      «Gh»           │ resolve_     │           │
  │                                      │  conflicts() │           │
  │                                      └──────────────┘           │
  └──────────────────────────────────────────────────────────────────┘
```

---

## SideEffect 실행 전략

현재 `_one()` 함수는 label/comment/review를 즉시 실행한다.
리팩토링 후에는 **두 가지 전략**이 가능하다:

### Option A: after_invoke 내부에서 즉시 실행 (현재와 동일)

```
  after_invoke(output):
    gh.label_remove(...)     // 즉시 실행
    gh.label_add(...)        // 즉시 실행
    gh.post_issue_comment()  // 즉시 실행
    → TaskResult { queue_ops }
```

- 장점: 기존 코드 변경 최소화
- 단점: after_invoke에 Gh 의존성 필요, side effect 테스트 시 MockGh 필수

### Option B: SideEffect를 데이터로 반환하고 TaskRunner가 실행

```
  after_invoke(output):
    → TaskResult {
        queue_ops: [Remove, PushPr(PENDING)],
        side_effects: [LabelRemove(..), LabelAdd(..), PostComment(..)],
      }

  TaskRunner:
    for effect in result.side_effects {
        effect.execute(&gh).await;
    }
```

- 장점: after_invoke가 **순수 함수**, 테스트에서 assert_eq로 검증 가능
- 단점: SideEffect enum 정의 필요, 추가 추상화 레이어

---

## 테스트 포인트 매핑

```
  ┌─────────────────────────────────────────────────────────────────────┐
  │                         TEST BOUNDARIES                             │
  │                                                                     │
  │  ┌─── before_invoke() 단위 테스트 ────────────────────────────┐    │
  │  │                                                              │    │
  │  │  Mock: Notifier (pre-flight), Workspace (clone/worktree)    │    │
  │  │  검증: Invocation 내용 or SkipReason 종류                   │    │
  │  │  Agent: 불필요                                               │    │
  │  │                                                              │    │
  │  └──────────────────────────────────────────────────────────────┘    │
  │                                                                     │
  │  ┌─── after_invoke() 단위 테스트 ─────────────────────────────┐    │
  │  │                                                              │    │
  │  │  Input: 미리 구성한 SessionResult (exit_code, stdout)       │    │
  │  │  Mock: Gh (Option A) or 없음 (Option B)                     │    │
  │  │  검증: queue_ops 내용, side_effects 내용, logs              │    │
  │  │  Agent: 불필요                                               │    │
  │  │                                                              │    │
  │  └──────────────────────────────────────────────────────────────┘    │
  │                                                                     │
  │  ┌─── TaskRunner 통합 테스트 ──────────────────────────────────┐   │
  │  │                                                              │    │
  │  │  Mock: Agent (응답 주입), Task (before/after stub)          │    │
  │  │  검증: before→agent→after→cleanup 순서 보장                 │    │
  │  │        SkipReason 시 Agent 호출 안 됨                       │    │
  │  │                                                              │    │
  │  └──────────────────────────────────────────────────────────────┘    │
  │                                                                     │
  │  ┌─── E2E 테스트 (기존 pipeline_e2e_tests 계승) ──────────────┐   │
  │  │                                                              │    │
  │  │  Mock: Agent, Gh, Git, Env 전부                              │    │
  │  │  검증: 전체 flow (scan → pop → execute → handle_output)     │    │
  │  │                                                              │    │
  │  └──────────────────────────────────────────────────────────────┘    │
  └─────────────────────────────────────────────────────────────────────┘
```
