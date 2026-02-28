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

`_one()` 내부 로직을 **before / resolve** 로 분리한다.
- Task는 **판단만** 한다 (순수 함수로 데이터 반환)
- TaskRunner가 **실행**을 담당한다 (Agent 호출, side effect 실행)
- cleanup hook은 **선택적** — 필요한 task만 구현 (worktree 정리 등)

```
                     ┌──────────────────────────────────────────────┐
                     │                «trait» Task                   │
                     ├──────────────────────────────────────────────┤
                     │ + before_invoke() → Result<Invocation,       │  필수: pre-flight + 요청 구성
                     │                           SkipReason>        │
                     │ + resolve(output) → TaskResult               │  필수: 결과 해석 (순수 함수)
                     │ + cleanup()  { }              // default nop │  선택: 필요시만 override
                     └────────────────────┬─────────────────────────┘
                                          │
          ┌───────────────┬───────────────┼──────────────┬──────────────┬──────────────┐
          │               │               │              │              │              │
  ┌───────┴──────┐ ┌──────┴──────┐ ┌──────┴─────┐ ┌─────┴──────┐ ┌────┴───────┐ ┌────┴──────┐
  │ AnalyzeTask  │ │ImplementTask│ │ ReviewTask  │ │ImproveTask │ │ReReviewTask│ │ MergeTask │
  │              │ │             │ │             │ │            │ │            │ │           │
  │ cleanup: ✓   │ │ cleanup: ✓  │ │ cleanup: ✓  │ │ cleanup: ✓ │ │ cleanup: ✓  │ │ cleanup: ✓│
  │ (worktree)   │ │ (worktree)  │ │ (worktree)  │ │ (worktree) │ │ (worktree)  │ │ (worktree)│
  └──────────────┘ └─────────────┘ └─────────────┘ └────────────┘ └────────────┘ └───────────┘

  ┌─────────────────────────────────────────────────────────────────────────────────────────┐
  │                              TaskRunner (orchestrator)                                  │
  │                                                                                         │
  │  pub async fn execute(task, agent, gh) -> TaskOutput {                                  │
  │                                                                                         │
  │      // 1. Pre-flight + request 구성 (Task 판단)                                        │
  │      let invocation = match task.before_invoke().await {                                │
  │          Ok(inv) => inv,                                                                │
  │          Err(skip) => {                                                                 │
  │              self.run_side_effects(&skip.side_effects, gh).await;                       │
  │              return skip.into_task_output();                                            │
  │          }                                                                              │
  │      };                                                                                 │
  │                                                                                         │
  │      // 2. Agent 호출 (TaskRunner 책임)                                                  │
  │      let agent_output = agent.run_session(                                              │
  │          &invocation.cwd, &invocation.prompt, &invocation.opts                          │
  │      ).await;                                                                           │
  │                                                                                         │
  │      // 3. 결과 해석 (Task 판단 — 순수 함수)                                              │
  │      let result = task.resolve(agent_output);                                           │
  │                                                                                         │
  │      // 4. Side effects 실행 (TaskRunner 책임)                                           │
  │      self.run_side_effects(&result.side_effects, gh).await;                             │
  │                                                                                         │
  │      // 5. Cleanup hook (선택적)                                                         │
  │      task.cleanup().await;                                                              │
  │                                                                                         │
  │      result.into_task_output()                                                          │
  │  }                                                                                      │
  │                                                                                         │
  │  async fn run_side_effects(&self, effects: &[SideEffect], gh: &dyn Gh) {               │
  │      for effect in effects {                                                            │
  │          match effect {                                                                 │
  │              LabelRemove { .. }     => gh.label_remove(..).await,                       │
  │              LabelAdd { .. }        => gh.label_add(..).await,                          │
  │              PostComment { .. }     => gh.issue_comment(..).await,                      │
  │              PrReview { .. }        => gh.pr_review(..).await,                          │
  │              ExtractKnowledge { .. } => /* best-effort */,                              │
  │          }                                                                              │
  │      }                                                                                  │
  │  }                                                                                      │
  └─────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 핵심 타입 정의

```
  ┌──────────────────────────────────────┐
  │            Invocation                │
  ├──────────────────────────────────────┤  before_invoke() 성공 시 반환
  │ + cwd: PathBuf                       │  Agent에게 전달할 요청을 기술
  │ + prompt: String                     │
  │ + opts: SessionOptions               │
  └──────────────────────────────────────┘

  ┌──────────────────────────────────────┐
  │            SkipReason                │
  ├──────────────────────────────────────┤  before_invoke() 실패 시 반환
  │ + work_id: String                    │  Agent 호출 없이 바로 종료
  │ + repo_name: String                  │
  │ + reason: SkipKind                   │
  │ + queue_ops: Vec<QueueOp>            │  SkipReason도 side_effects를 갖는다
  │ + side_effects: Vec<SideEffect>      │  (예: IssueClosed → WIP제거 + DONE추가)
  │ + logs: Vec<NewConsumerLog>          │
  │ + into_task_output() → TaskOutput    │
  └──────────────────────────────────────┘

  ┌──────────────────────────────────────┐
  │     «enum» SkipKind                  │
  ├──────────────────────────────────────┤
  │   IssueClosed                        │
  │   PrNotReviewable                    │
  │   PrNotMergeable                     │
  │   CloneFailed(String)                │
  │   WorktreeCreationFailed(String)     │
  └──────────────────────────────────────┘

  ┌──────────────────────────────────────┐
  │            TaskResult                │
  ├──────────────────────────────────────┤  resolve() 반환값 — 순수 데이터
  │ + work_id: String                    │  Agent 결과를 해석한 판단 결과
  │ + repo_name: String                  │
  │ + queue_ops: Vec<QueueOp>            │  큐 조작 (main loop에서 실행)
  │ + side_effects: Vec<SideEffect>      │  외부 호출 (TaskRunner가 실행)
  │ + logs: Vec<NewConsumerLog>          │  DB 로그
  │ + into_task_output() → TaskOutput    │
  └──────────────────────────────────────┘

  ┌──────────────────────────────────────┐
  │     «enum» SideEffect                │
  ├──────────────────────────────────────┤  Task가 "무엇을 해야 하는지" 기술
  │   LabelRemove { repo, num, label }   │  TaskRunner가 실행을 담당
  │   LabelAdd { repo, num, label }      │
  │   PostComment { repo, num, body }    │
  │   PrReview { repo, num, event, body }│
  │   ExtractKnowledge { repo, num, .. } │
  └──────────────────────────────────────┘
```

### 역할 분리 원칙

```
  Task (판단)                         TaskRunner (실행)
  ─────────────────────               ─────────────────────────
  "issue가 closed니까                 "Task가 말한 대로
   WIP 라벨 빼고                       gh.label_remove() 호출하고
   DONE 라벨 붙여야 해"                gh.label_add() 호출한다"
       ↓                                  ↓
  SideEffect 데이터 반환              SideEffect 데이터 받아서 실행
  (순수 함수, mock 불필요)            (Gh trait 의존)
```

---

## Concrete Task 내부 구조 (before / resolve 분리)

### AnalyzeTask

```
  ┌─────────────────────────────────────────────────────────────┐
  │                      AnalyzeTask                            │
  ├─────────────────────────────────────────────────────────────┤
  │ - item: IssueItem                                           │
  │ - workspace: Workspace                                      │
  │ - notifier: Notifier                                        │
  │ - config: ConsumerConfig                                    │
  │ - wt_path: Option<PathBuf>                                  │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ notifier.is_issue_open()  → Err(IssueClosed)          │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt, opts: json_schema })     │
  │                                                             │
  │ resolve(agent_output) → TaskResult:         ← 순수 함수     │
  │   ├─ exit_code != 0                                         │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects: [LabelRemove(WIP)]                      │
  │   ├─ parse analysis JSON                                    │
  │   │   ├─ Wontfix                                            │
  │   │   │   queue_ops: [Remove]                               │
  │   │   │   side_effects: [LabelRemove(WIP), LabelAdd(SKIP), │
  │   │   │                  PostComment(사유)]                  │
  │   │   ├─ NeedsClarification | confidence < threshold        │
  │   │   │   queue_ops: [Remove]                               │
  │   │   │   side_effects: [LabelRemove(WIP), LabelAdd(SKIP), │
  │   │   │                  PostComment(질문)]                  │
  │   │   ├─ Implement                                          │
  │   │   │   queue_ops: [Remove]                               │
  │   │   │   side_effects: [LabelRemove(WIP),                  │
  │   │   │                  LabelAdd(ANALYZED),                 │
  │   │   │                  PostComment(리포트)]                │
  │   │   └─ parse 실패                                         │
  │   │       queue_ops: [Remove]                               │
  │   │       side_effects: [LabelRemove(WIP),                  │
  │   │                      LabelAdd(ANALYZED),                 │
  │   │                      PostComment(fallback)]              │
  │   └─ Err                                                    │
  │       queue_ops: [Remove]                                   │
  │       side_effects: [LabelRemove(WIP)]                      │
  │                                                             │
  │ cleanup():  workspace.remove_worktree()                     │
  └─────────────────────────────────────────────────────────────┘
```

### ImplementTask

```
  ┌─────────────────────────────────────────────────────────────┐
  │                     ImplementTask                           │
  ├─────────────────────────────────────────────────────────────┤
  │ - item: IssueItem                                           │
  │ - workspace: Workspace                                      │
  │ - wt_path: Option<PathBuf>                                  │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt, opts: default })         │
  │                                                             │
  │ resolve(agent_output) → TaskResult:         ← 순수 함수     │
  │   ├─ exit_code != 0                                         │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects: [LabelRemove(IMPLEMENTING)]             │
  │   ├─ exit_code == 0                                         │
  │   │   ├─ extract_pr_number(stdout) → Some(pr)               │
  │   │   │   queue_ops: [Remove, PushPr(PENDING, pr_item)]     │
  │   │   │   side_effects: [LabelAdd(WIP, pr)]                 │
  │   │   └─ extract 실패                                       │
  │   │       queue_ops: [Remove]                               │
  │   │       side_effects: [LabelRemove(IMPLEMENTING)]         │
  │   └─ Err                                                    │
  │       queue_ops: [Remove]                                   │
  │       side_effects: [LabelRemove(IMPLEMENTING)]             │
  │                                                             │
  │ cleanup():  workspace.remove_worktree()                     │
  │                                                             │
  │ NOTE: find_existing_pr() fallback는 before_invoke에서       │
  │       head_branch를 미리 기록해두고, resolve에서             │
  │       stdout 파싱 실패 시 FindPr side_effect로 위임.        │
  │       또는 resolve를 async로 두고 직접 gh 조회.              │
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
  │ - wt_path: Option<PathBuf>                                  │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ notifier.is_pr_reviewable() → Err(PrNotReviewable)    │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt, opts: json_schema })     │
  │                                                             │
  │ resolve(agent_output) → TaskResult:         ← 순수 함수     │
  │   ├─ exit_code != 0                                         │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects: [LabelRemove(WIP)]                      │
  │   ├─ Approve                                                │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects:                                         │
  │   │     [PrReview(APPROVE),                                 │
  │   │      PostComment(리뷰 요약),                             │
  │   │      LabelRemove(WIP), LabelAdd(DONE, pr),             │
  │   │      LabelRemove(IMPLEMENTING, issue),   ← if linked   │
  │   │      LabelAdd(DONE, issue),              ← if linked   │
  │   │      ExtractKnowledge(..)]               ← if enabled  │
  │   ├─ RequestChanges + linked issue                          │
  │   │   queue_ops: [Remove, PushPr(REVIEW_DONE, updated)]    │
  │   │   side_effects:                                         │
  │   │     [PrReview(REQUEST_CHANGES),                         │
  │   │      PostComment(리뷰 피드백)]                           │
  │   ├─ RequestChanges + external PR                           │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects:                                         │
  │   │     [PostComment(리뷰 피드백),                           │
  │   │      LabelRemove(WIP), LabelAdd(DONE)]                 │
  │   └─ Err                                                    │
  │       queue_ops: [Remove]                                   │
  │       side_effects: [LabelRemove(WIP)]                      │
  │                                                             │
  │ cleanup():  workspace.remove_worktree()                     │
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
  │ resolve(agent_output) → TaskResult:         ← 순수 함수     │
  │   ├─ exit_code != 0                                         │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects: [LabelRemove(WIP)]                      │
  │   ├─ exit_code == 0                                         │
  │   │   queue_ops: [Remove, PushPr(IMPROVED, updated)]        │
  │   │   side_effects:                                         │
  │   │     [LabelRemove(iteration/N),     ← if iteration > 0  │
  │   │      LabelAdd(iteration/N+1)]                           │
  │   └─ Err                                                    │
  │       queue_ops: [Remove]                                   │
  │       side_effects: [LabelRemove(WIP)]                      │
  │                                                             │
  │ cleanup():  workspace.remove_worktree()                     │
  └─────────────────────────────────────────────────────────────┘
```

### ReReviewTask

```
  ┌─────────────────────────────────────────────────────────────┐
  │                     ReReviewTask                            │
  ├─────────────────────────────────────────────────────────────┤
  │ - item: PrItem                                              │
  │ - workspace: Workspace                                      │
  │ - config: DevelopConfig                                     │
  │ - wt_path: Option<PathBuf>                                  │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt, opts: json_schema })     │
  │                                                             │
  │ resolve(agent_output) → TaskResult:         ← 순수 함수     │
  │   ├─ exit_code != 0                                         │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects: [LabelRemove(WIP)]                      │
  │   ├─ Approve                                                │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects:                                         │
  │   │     [PrReview(APPROVE),                                 │
  │   │      LabelRemove(WIP), LabelAdd(DONE, pr),             │
  │   │      LabelRemove(iteration/N),                          │
  │   │      LabelRemove(IMPLEMENTING, issue),   ← if linked   │
  │   │      LabelAdd(DONE, issue),              ← if linked   │
  │   │      ExtractKnowledge(..)]               ← if enabled  │
  │   ├─ RequestChanges + iteration < max                       │
  │   │   queue_ops: [Remove, PushPr(REVIEW_DONE, updated)]    │
  │   │   side_effects:                                         │
  │   │     [PrReview(REQUEST_CHANGES),                         │
  │   │      PostComment(리뷰 피드백)]                           │
  │   ├─ RequestChanges + iteration >= max     ← CRITICAL       │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects:                                         │
  │   │     [LabelRemove(WIP), LabelAdd(SKIP),                 │
  │   │      LabelRemove(iteration/N),                          │
  │   │      PostComment("iteration limit reached")]            │
  │   └─ Err                                                    │
  │       queue_ops: [Remove]                                   │
  │       side_effects: [LabelRemove(WIP)]                      │
  │                                                             │
  │ cleanup():  workspace.remove_worktree()                     │
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
  │ - wt_path: Option<PathBuf>                                  │
  ├─────────────────────────────────────────────────────────────┤
  │ before_invoke():                                            │
  │   ├─ notifier.is_pr_mergeable() → Err(PrNotMergeable)      │
  │   ├─ workspace.ensure_cloned() → Err(CloneFailed)          │
  │   ├─ workspace.create_worktree() → Err(WorktreeFailed)     │
  │   └─ Ok(Invocation { cwd, prompt: merge_pr, opts })        │
  │                                                             │
  │ resolve(agent_output) → TaskResult:         ← 순수 함수     │
  │   ├─ Success                                                │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects: [LabelRemove(WIP), LabelAdd(DONE)]     │
  │   ├─ Conflict                                               │
  │   │   (conflict resolution은 별도 Agent 호출이 필요하므로    │
  │   │    ResolveConflict side_effect로 위임하거나              │
  │   │    MergeTask를 2-phase로 설계)                           │
  │   ├─ Failed                                                 │
  │   │   queue_ops: [Remove]                                   │
  │   │   side_effects: [LabelRemove(WIP)]                      │
  │   └─ Error                                                  │
  │       queue_ops: [Remove]                                   │
  │       side_effects: [LabelRemove(WIP)]                      │
  │                                                             │
  │ cleanup():  workspace.remove_worktree()                     │
  └─────────────────────────────────────────────────────────────┘
```

---

## 의존성 구조 (Dependency Graph)

```
  ┌─────────────────────────────────────────────────────────────────────────┐
  │                          DAEMON (Orchestrator)                         │
  │                                                                         │
  │  loop {                                                                 │
  │    scan → pop from queues → TaskRunner.execute(task, agent, gh)         │
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
        │   agent, gh)      │               │ prs: StateQueue   │
        │  → TaskOutput     │               │ merges: StateQueue│
        │                   │               └───────────────────┘
        │ run_side_effects()│
        │  (Gh 의존)        │
        └────────┬──────────┘
                 │
    ┌────────────┼─────────────────────────┐
    ▼            ▼                         ▼
  Task         Agent                    TaskRunner
  .before()    .run_session()           .run_side_effects()
    │                                      │
    ▼            │                         ▼
  Invocation ────┘                      for effect in side_effects {
  or SkipReason ──→ run_side_effects()      gh.label_remove()
                                            gh.label_add()
    Task                                    gh.issue_comment()
    .resolve(output)                        gh.pr_review()
    │                                   }
    ▼
  TaskResult { queue_ops, side_effects } ──→ run_side_effects()
```

### 의존성 방향 정리

```
  Task (순수 판단)              TaskRunner (실행)
  ─────────────────────         ─────────────────────────
  의존: item, config            의존: Agent, Gh
  before: + Workspace, Notifier
  resolve: 의존 없음 (순수)
  cleanup: + Workspace

  ※ resolve()는 외부 의존성 없는 순수 함수
  ※ side_effects 실행은 TaskRunner가 Gh에 위임
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

## 테스트 포인트 매핑

resolve()가 순수 함수이므로, 테스트 레이어가 명확히 분리된다.

```
  ┌─────────────────────────────────────────────────────────────────────┐
  │                         TEST BOUNDARIES                             │
  │                                                                     │
  │  ┌─── before_invoke() 단위 테스트 ────────────────────────────┐    │
  │  │                                                              │    │
  │  │  Mock: Notifier (pre-flight), Workspace (clone/worktree)    │    │
  │  │  검증: Invocation 내용 or SkipReason { kind, side_effects } │    │
  │  │  Agent: 불필요                                               │    │
  │  │                                                              │    │
  │  └──────────────────────────────────────────────────────────────┘    │
  │                                                                     │
  │  ┌─── resolve() 단위 테스트 ──────────────────────────────────┐    │
  │  │                                                              │    │
  │  │  Input: 미리 구성한 SessionResult (exit_code, stdout JSON)  │    │
  │  │  Mock: 없음 (순수 함수)                                      │    │
  │  │  검증: assert_eq!(result.queue_ops, expected_ops)           │    │
  │  │        assert_eq!(result.side_effects, expected_effects)    │    │
  │  │  Agent: 불필요, Gh: 불필요                                   │    │
  │  │                                                              │    │
  │  └──────────────────────────────────────────────────────────────┘    │
  │                                                                     │
  │  ┌─── TaskRunner 통합 테스트 ──────────────────────────────────┐   │
  │  │                                                              │    │
  │  │  Mock: Agent (응답 주입), Gh (side effect 실행 검증)        │    │
  │  │  검증: before→agent→resolve→side_effects→cleanup 순서 보장  │    │
  │  │        SkipReason 시 Agent 호출 안 됨                       │    │
  │  │        SkipReason의 side_effects도 실행됨                   │    │
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

### 테스트 난이도 비교 (AS-IS vs TO-BE)

```
  AS-IS: _one() 전체 호출
  ──────────────────────────────────────────────────────
  테스트 하나에 필요한 mock:  Agent + Gh + Git + Env
  verdict 분기 검증하려면:    Agent mock 응답 조작 필수
  label 검증하려면:           MockGh의 호출 기록 확인

  TO-BE: resolve() 단독 호출
  ──────────────────────────────────────────────────────
  테스트 하나에 필요한 mock:  없음 (순수 함수)
  verdict 분기 검증:          SessionResult 직접 구성
  label 검증:                 assert_eq!(side_effects, [...])
```

---

## 에이전트 vs 어플리케이션 책임 경계

### 원칙

> **Agent가 내리는 건 "판정"이다. 판정 이후의 상태 전이는 모두 결정론적이다.**

```
  ┌─────────────────────────────┐    ┌──────────────────────────────────┐
  │     에이전트 위임 영역       │    │      어플리케이션 제어 영역       │
  │  (LLM 지능이 필요한 것)      │    │   (결정론적, LLM 불필요)         │
  ├─────────────────────────────┤    ├──────────────────────────────────┤
  │                             │    │                                  │
  │  • 코드 분석 → 판정         │    │  • Pre-flight 검증               │
  │    (implement/wontfix/skip) │    │    (is_open, is_reviewable, ...) │
  │                             │    │                                  │
  │  • 코드 리뷰 → 판정         │    │  • Workspace 생명주기            │
  │    (approve/request_changes)│    │    (clone, worktree, cleanup)    │
  │                             │    │                                  │
  │  • 코드 구현                │    │  • 라벨 상태 전이                 │
  │    (브랜치/커밋/PR 생성)     │    │    (WIP→DONE, WIP→SKIP, ...)    │
  │                             │    │                                  │
  │  • 머지 실행                │    │  • 큐 상태 전이                   │
  │    (git merge 수행)         │    │    (Remove, PushPr, PushMerge)   │
  │                             │    │                                  │
  │  • 충돌 해결                │    │  • GitHub API 호출               │
  │    (conflict markers 처리)  │    │    (comment, pr_review, label)   │
  │                             │    │                                  │
  │  • 지식 추출 (best-effort)  │    │  • 결과 파싱                     │
  │    (학습 포인트 요약)        │    │    (JSON verdict, PR번호 추출)   │
  │                             │    │                                  │
  │                             │    │  • 코멘트 포맷팅                  │
  │                             │    │    (format_review_comment, ...)  │
  │                             │    │                                  │
  │                             │    │  • 설정 기반 분기                 │
  │                             │    │    (confidence, max_iterations)  │
  │                             │    │                                  │
  │                             │    │  • 로그 기록                     │
  │                             │    │    (ConsumerLog 생성)            │
  └─────────────────────────────┘    └──────────────────────────────────┘
           │                                      │
           ▼                                      ▼
    Agent.run_session()                   Task.resolve() (순수 함수)
    결과: 자유 형식 텍스트 +              결과: {queue_ops, side_effects}
          구조화 JSON                     → TaskRunner가 실행
```

### Task별 책임 매트릭스

각 Task에서 에이전트에게 **무엇을 요청**하고, 어플리케이션이 **무엇을 제어**하는지:

```
  ┌──────────────────────────────────────────────────────────────────────────────┐
  │  Task          │ 에이전트 위임 (Prompt)   │ 에이전트 산출물      │ 비고      │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ AnalyzeTask    │ 이슈 분석 + JSON 응답    │ { verdict,          │ JSON      │
  │                │ (verdict/confidence/      │   confidence,       │ schema    │
  │                │  report/questions/reason) │   summary, report,  │ 강제      │
  │                │                           │   questions, reason }│          │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ ImplementTask  │ 이슈 구현 + PR 생성      │ SessionResult       │ 자유형    │
  │                │ (워크플로우 프롬프트)      │ { exit_code,        │ stdout에  │
  │                │                           │   stdout, stderr }  │ PR번호    │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ ReviewTask     │ PR 코드 리뷰 + JSON 응답  │ { verdict,          │ JSON      │
  │                │ (verdict/summary)         │   summary }         │ schema    │
  │                │                           │                     │ 강제      │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ ImproveTask    │ 리뷰 피드백 반영          │ SessionResult       │ 자유형    │
  │                │ (코드 수정 + push)        │ { exit_code }       │ exit_code │
  │                │                           │                     │ 만 사용   │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ ReReviewTask   │ 수정된 PR 재리뷰         │ { verdict,          │ JSON      │
  │                │ (verdict/summary)         │   summary }         │ schema    │
  │                │                           │                     │ 강제      │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ MergeTask      │ PR 머지 실행             │ MergeOutcome        │ exit_code │
  │                │ (/git-utils:merge-pr)     │ { Success|Conflict  │ + conflict│
  │                │                           │   |Failed|Error }   │ 키워드    │
  └──────────────────────────────────────────────────────────────────────────────┘
```

```
  ┌──────────────────────────────────────────────────────────────────────────────┐
  │  Task          │ 어플리케이션 제어 (resolve → 결정론적 처리)                 │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ AnalyzeTask    │ verdict 기준 분기:                                         │
  │                │  implement  → Remove + label(WIP→ANALYZED) + 분석 코멘트   │
  │                │  wontfix    → Remove + label(WIP→SKIP) + 사유 코멘트       │
  │                │  needs_clar → Remove + label(WIP→SKIP) + 질문 코멘트       │
  │                │  confidence < threshold → needs_clar과 동일                │
  │                │  parse 실패 → Remove + label(WIP→ANALYZED) + fallback 코멘트│
  │                │  exit≠0/err → Remove + label(WIP 제거)                     │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ ImplementTask  │ exit_code 기준 분기:                                       │
  │                │  exit=0 + PR번호 → Remove + PushPr(PENDING) + label(WIP,pr)│
  │                │                   + issue 코멘트(pr-link)                  │
  │                │  exit=0 + PR없음 → Remove + label(IMPLEMENTING 제거)       │
  │                │  exit≠0/err      → Remove + label(IMPLEMENTING 제거)       │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ ReviewTask     │ verdict 기준 분기:                                         │
  │                │  Approve → Remove + pr_review(APPROVE) + 리뷰 코멘트       │
  │                │           + label(WIP→DONE,pr) + label(IMPL→DONE,issue)    │
  │                │           + knowledge(best-effort)                         │
  │                │  ReqChanges + linked  → Remove + PushPr(REVIEW_DONE)       │
  │                │                        + pr_review(REQ_CHANGES)            │
  │                │  ReqChanges + external → Remove + label(WIP→DONE) + 코멘트  │
  │                │  exit≠0/err → Remove + label(WIP 제거)                     │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ ImproveTask    │ exit_code 기준 분기:                                       │
  │                │  exit=0 → Remove + PushPr(IMPROVED) + iteration 라벨 갱신  │
  │                │  exit≠0/err → Remove + label(WIP 제거)                     │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ ReReviewTask   │ verdict + iteration 기준 분기:                             │
  │                │  Approve → Remove + pr_review(APPROVE) + label(WIP→DONE)   │
  │                │           + iteration 라벨 제거                             │
  │                │           + label(IMPL→DONE,issue) + knowledge             │
  │                │  ReqChanges + iter<max → Remove + PushPr(REVIEW_DONE)      │
  │                │                         + pr_review(REQ_CHANGES)           │
  │                │  ReqChanges + iter≥max → Remove + label(WIP→SKIP)          │
  │                │                         + iteration 라벨 제거 + 한계 코멘트 │
  │                │  exit≠0/err → Remove + label(WIP 제거)                     │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │ MergeTask      │ outcome 기준 분기:                                         │
  │                │  Success          → Remove + label(WIP→DONE)               │
  │                │  Conflict→해결성공 → Remove + label(WIP→DONE)              │
  │                │  Conflict→해결실패 → Remove + label(WIP 제거)              │
  │                │  Failed/Error     → Remove + label(WIP 제거)               │
  └──────────────────────────────────────────────────────────────────────────────┘
```

### 단일 에이전트 호출 원칙을 깨는 경우

대부분의 Task는 `before → Agent 1회 → resolve` 패턴을 따르지만,
3가지 예외가 존재한다:

```
  ┌─────────────────────────────────────────────────────────────────────────┐
  │ 예외 케이스          │ 현재 구현                │ 왜 단일 호출이 안 되는가│
  ├─────────────────────────────────────────────────────────────────────────┤
  │ ① ImplementTask     │ stdout PR번호 파싱 실패 시│ Agent가 PR 번호를      │
  │    find_existing_pr  │ gh.api_paginate() 로     │ stdout에 안 남길 수    │
  │                      │ fallback 조회             │ 있음 → API 조회 필요   │
  │                      │                           │                       │
  │                      │ 특성: Agent 재호출 아님,  │                       │
  │                      │       GitHub API 조회     │                       │
  ├─────────────────────────────────────────────────────────────────────────┤
  │ ② MergeTask         │ merge 실패 + conflict     │ merge와 conflict      │
  │    resolve_conflicts │ 감지 시 Agent 2차 호출    │ resolution은 별개의    │
  │                      │ (Merger.resolve_conflicts)│ 프롬프트가 필요        │
  │                      │                           │                       │
  │                      │ 특성: Agent 2차 호출      │                       │
  ├─────────────────────────────────────────────────────────────────────────┤
  │ ③ Review/ReReview   │ Approve 판정 후           │ 리뷰 결과 + 코드를    │
  │    knowledge_extract │ Agent 추가 호출로         │ 종합 분석해 학습 포인트│
  │                      │ 학습 포인트 추출          │ 추출 → 별도 프롬프트   │
  │                      │ (best-effort, 실패 무시)  │                       │
  │                      │                           │                       │
  │                      │ 특성: Agent 2차 호출,     │                       │
  │                      │       best-effort         │                       │
  └─────────────────────────────────────────────────────────────────────────┘
```

### 아키텍처 반영: Task trait 확장

위 예외를 수용하기 위해, Task trait에 **Agent 산출물의 성격**을 명시한다.

```
  «trait» Task {
      /// 에이전트 위임 정의: 무엇을 요청할 것인가
      fn invocation() → Result<Invocation, SkipReason>

      /// 에이전트 산출물 해석: 판정 추출 (순수 함수)
      fn resolve(agent_output) → TaskResult

      /// 후속 에이전트 호출이 필요한가 (기본: 없음)
      fn followup(result: &TaskResult) → Option<Invocation> { None }

      /// 리소스 정리 (기본: nop)
      fn cleanup() { }
  }
```

```
  TaskRunner.execute(task, agent, gh):

    1. invocation = task.invocation()?      ← 에이전트 위임 정의
    2. output = agent.run(invocation)        ← 에이전트 실행
    3. result = task.resolve(output)         ← 판정 추출 (순수)
    4. if let Some(inv) = task.followup(&result) {  ← 후속 호출 필요 시
           output2 = agent.run(inv)
           result = task.resolve_followup(output2, result)
       }
    5. run_side_effects(result.side_effects) ← 어플리케이션 실행
    6. task.cleanup()                        ← 리소스 정리
    7. return result.into_task_output()
```

### 예외 케이스별 적용

```
  ① ImplementTask.find_existing_pr:
     → resolve()에서 PR번호 파싱 실패 시
       side_effect에 FindPr { head_branch } 추가
     → TaskRunner.run_side_effects()에서 gh.api_paginate() 호출
     → 결과에 따라 queue_ops 보정
     ※ Agent 재호출이 아니므로 followup 불필요, SideEffect로 처리

  ② MergeTask.resolve_conflicts:
     → resolve()에서 outcome==Conflict 시
       followup() → Some(Invocation { prompt: resolve conflicts })
     → TaskRunner가 2차 Agent 호출
     → resolve_followup()에서 최종 판정

  ③ ReviewTask.knowledge_extraction:
     → resolve()에서 verdict==Approve 시
       followup() → Some(Invocation { prompt: extract knowledge })
     → TaskRunner가 2차 Agent 호출 (best-effort, 실패 무시)
     → resolve_followup()에서 knowledge log 추가
```

---

## 미결 설계 포인트

### 1. FindPr SideEffect의 queue_ops 보정

ImplementTask에서 `FindPr` SideEffect가 PR번호를 찾으면
`PushPr` QueueOp이 추가되어야 한다. 이를 처리하는 방법:

- **방안 A**: `FindPr` SideEffect가 `Option<QueueOp>`을 반환 → TaskRunner가 결과에 merge
- **방안 B**: resolve()를 2단계로 분리 — `resolve_partial()` → 외부 조회 → `resolve_complete()`
- **방안 C**: Invocation에 head_branch를 명시하고, Agent에게 PR번호를 반드시 반환하도록 스키마 강제

### 2. followup의 범용성

followup은 현재 MergeTask(conflict)와 Review/ReReviewTask(knowledge)만 사용한다.
두 케이스의 성격이 다르므로 (하나는 핵심 흐름, 하나는 best-effort):

- **방안 A**: `followup() → Option<FollowupSpec>` 에 `{ invocation, required: bool }` 포함
- **방안 B**: knowledge extraction을 SideEffect로 분리 (ExtractKnowledge),
             followup은 MergeTask conflict에만 사용
