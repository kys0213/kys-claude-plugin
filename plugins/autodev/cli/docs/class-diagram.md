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

## 미결 설계 포인트

### 1. ImplementTask의 find_existing_pr fallback

현재 `implement_one()`은 stdout에서 PR 번호 추출 실패 시
`gh.api_paginate()`로 fallback 조회한다.

resolve()를 순수 함수로 유지하려면:
- **방안 A**: `FindPr { head_branch }` SideEffect 추가 → TaskRunner가 조회 후 queue_ops 결정
- **방안 B**: resolve()를 async로 두고 Gh를 주입 (순수성 포기)
- **방안 C**: before_invoke에서 head_branch 기록, resolve에서 stdout 파싱만 하고
             실패 시 `NeedsPrLookup` 상태 반환 → TaskRunner가 2차 처리

### 2. MergeTask의 conflict resolution

현재 `merge_one()`은 conflict 발생 시 `merger.resolve_conflicts()`를
추가 Agent 호출로 시도한다.

resolve()가 단일 Agent 응답만 받으므로:
- **방안 A**: Conflict → `ResolveConflict` SideEffect → TaskRunner가 2차 Agent 호출
- **방안 B**: MergeTask를 2-phase task로 설계 (merge → conflict resolution)
- **방안 C**: Merger가 내부적으로 2회 호출하고 최종 결과만 반환
