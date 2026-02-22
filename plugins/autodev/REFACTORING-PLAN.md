# Refactoring Plan: SQLite Queue → In-Memory StateQueue + Label SSOT

> **Date**: 2026-02-22
> **Scope**: `plugins/autodev/cli/src/` 전체
> **목표**: DESIGN.md의 3-Tier 상태 관리로 구현 정렬

---

## 변경 요약

| Phase | 내용 | 영향 파일 수 |
|-------|------|-------------|
| **Phase 1** | StateQueue + TaskQueues 구현 | 신규 2 + 수정 2 |
| **Phase 2** | Gh trait에 label_add 추가 | 수정 3 |
| **Phase 3** | Scanner → StateQueue 전환 | 수정 3 |
| **Phase 4** | Pipeline → StateQueue 전환 + 라벨 관리 | 수정 4 |
| **Phase 5** | Daemon 루프 + startup_reconcile | 수정 2 |
| **Phase 6** | SQLite 큐 테이블 제거 + client 수정 | 수정 3 |
| **Phase 7** | PR 피드백 루프 구현 | 수정 2 |
| **Phase 8** | 테스트 작성 | 신규 2 + 수정 1 |

---

## Phase 1: StateQueue + TaskQueues 구현

### 신규 파일: `queue/state_queue.rs`

```rust
use std::collections::{HashMap, VecDeque};

/// 상태별 큐
pub struct StateQueue<T: HasWorkId> {
    queues: HashMap<String, VecDeque<T>>,
}

impl<T: HasWorkId> StateQueue<T> {
    pub fn push(&mut self, state: &str, item: T);
    pub fn pop(&mut self, state: &str) -> Option<T>;
    pub fn transit(&mut self, id: &str, from: &str, to: &str) -> bool;
    pub fn remove(&mut self, id: &str) -> Option<T>;
    pub fn len(&self, state: &str) -> usize;
    pub fn find(&self, id: &str) -> Option<(&str, &T)>;  // (state, item)
}
```

### 신규 파일: `queue/task_queues.rs`

```rust
/// 전체 작업 큐 (dedup index 포함)
pub struct TaskQueues {
    pub issues: StateQueue<IssueItem>,
    pub prs: StateQueue<PrItem>,
    pub merges: StateQueue<MergeItem>,
    index: HashMap<WorkId, String>,  // WorkId → current state
}

impl TaskQueues {
    pub fn contains(&self, id: &WorkId) -> bool;
    pub fn state_of(&self, id: &WorkId) -> Option<&str>;
    pub fn push_issue(&mut self, state: &str, item: IssueItem);
    pub fn push_pr(&mut self, state: &str, item: PrItem);
    pub fn push_merge(&mut self, state: &str, item: MergeItem);
    pub fn remove_issue(&mut self, id: &str) -> Option<IssueItem>;
    // ... etc
}
```

### 수정: `queue/models.rs`

현재 DB용 모델을 인메모리용으로 교체:

```rust
// 기존 IssueQueueItem (DB 전체 row) → 제거
// 새로운 인메모리 모델:

pub trait HasWorkId {
    fn work_id(&self) -> WorkId;
}

#[derive(Clone)]
pub struct WorkId(pub String);  // "issue:org/repo:42"

#[derive(Clone)]
pub struct IssueItem {
    pub work_id: WorkId,
    pub repo_id: String,
    pub repo_name: String,
    pub repo_url: String,
    pub github_number: i64,
    pub title: String,
    pub body: Option<String>,
    pub labels: Vec<String>,
    pub author: String,
    pub analysis_report: Option<String>,
}

#[derive(Clone)]
pub struct PrItem {
    pub work_id: WorkId,
    pub repo_id: String,
    pub repo_name: String,
    pub repo_url: String,
    pub github_number: i64,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
    pub review_comment: Option<String>,
}

#[derive(Clone)]
pub struct MergeItem {
    pub work_id: WorkId,
    pub repo_id: String,
    pub repo_name: String,
    pub repo_url: String,
    pub pr_number: i64,
    pub head_branch: String,
    pub base_branch: String,
}
```

### 수정: `queue/mod.rs`

TaskQueues를 모듈로 export:
```rust
pub mod state_queue;
pub mod task_queues;
// ... 기존 schema, repository 유지 (repos, cursors, logs용)
```

---

## Phase 2: Gh trait에 label_add 추가

### 수정: `infrastructure/gh/mod.rs`

```rust
#[async_trait]
pub trait Gh: Send + Sync {
    // 기존 메서드 유지
    async fn api_get_field(...) -> Option<String>;
    async fn api_paginate(...) -> Result<Vec<u8>>;
    async fn issue_comment(...) -> bool;
    async fn label_remove(...) -> bool;

    // 추가:
    async fn label_add(
        &self,
        repo_name: &str,
        number: i64,
        label: &str,
        host: Option<&str>,
    ) -> bool;
}
```

### 수정: `infrastructure/gh/real.rs`

```rust
async fn label_add(&self, repo_name: &str, number: i64, label: &str, host: Option<&str>) -> bool {
    // gh api repos/{repo}/issues/{number}/labels --method POST -f labels[]={label}
}
```

### 수정: `infrastructure/gh/mock.rs`

```rust
pub added_labels: Mutex<Vec<(String, i64, String)>>,  // (repo_name, number, label)

async fn label_add(&self, repo_name: &str, number: i64, label: &str, _host: Option<&str>) -> bool {
    self.added_labels.lock().unwrap().push((repo_name.to_string(), number, label.to_string()));
    true
}
```

---

## Phase 3: Scanner → StateQueue 전환

### 수정: `scanner/mod.rs`

시그니처 변경:
```rust
pub async fn scan_all(
    db: &Database,         // repos + cursors만 사용
    env: &dyn Env,
    gh: &dyn Gh,
    queues: &mut TaskQueues,  // ActiveItems 대신 TaskQueues
) -> Result<()>
```

### 수정: `scanner/issues.rs`

```rust
pub async fn scan(
    db: &Database,
    gh: &dyn Gh,
    repo_id: &str,
    repo_name: &str,
    repo_url: &str,
    ignore_authors: &[String],
    filter_labels: &Option<Vec<String>>,
    gh_host: Option<&str>,
    queues: &mut TaskQueues,
) -> Result<()> {
    // 변경점:
    // 1. active.contains() → queues.contains(&work_id)
    // 2. db.issue_exists() → 제거 (queues.contains로 대체)
    // 3. db.issue_insert() → queues.push_issue("Pending", item)
    // 4. active.insert() → 제거 (push 시 index에 자동 등록)
    // 5. 추가: gh.label_add(repo_name, number, "autodev:wip", gh_host)
    // 6. autodev:done/skip/wip 라벨 있으면 skip (라벨 기반 필터)
}
```

### 수정: `scanner/pulls.rs`

동일한 패턴으로 변경.

---

## Phase 4: Pipeline → StateQueue 전환 + 라벨 관리

### 수정: `pipeline/mod.rs`

```rust
pub async fn process_all(
    db: &Database,         // consumer_logs만 사용
    env: &dyn Env,
    workspace: &Workspace<'_>,
    notifier: &Notifier<'_>,
    gh: &dyn Gh,           // 추가: 라벨 관리용
    claude: &dyn Claude,
    queues: &mut TaskQueues,
) -> Result<()> {
    // issues StateQueue[Pending] pop → analyze
    // issues StateQueue[Ready] pop → implement
    // prs StateQueue[Pending] pop → review
    // merges StateQueue[Pending] pop → merge
}
```

### 수정: `pipeline/issue.rs`

```rust
// 핵심 변경:
// 1. db.issue_find_pending() → queues.issues.pop("Pending")
// 2. db.issue_update_status("analyzing") → 제거 (pop 했으므로 큐에서 이미 빠짐)
// 3. 처리 중 상태 = 변수로 관리 (process 함수 로컬)
// 4. 완료 시:
//    - implement → queues.push_issue("Ready", item)
//    - needs_clarification → queues.remove_issue() + gh.label_add("autodev:skip") + gh.label_remove("autodev:wip")
//    - wontfix → queues.remove_issue() + gh.label_add("autodev:skip") + gh.label_remove("autodev:wip")
// 5. process_ready:
//    - queues.issues.pop("Ready") → 구현 실행
//    - success → queues.remove_issue() + gh.label_add("autodev:done") + gh.label_remove("autodev:wip")
//    - failure → queues.remove_issue() + gh.label_remove("autodev:wip")  // 다음 scan에서 재발견
// 6. pre-flight check 제거 (scan에서 open 확인 완료)
```

### 수정: `pipeline/pr.rs` & `pipeline/merge.rs`

동일한 패턴.

---

## Phase 5: Daemon 루프 + startup_reconcile

### 수정: `daemon/mod.rs`

```rust
pub async fn start(...) -> Result<()> {
    // ...
    let mut queues = TaskQueues::new();

    // 0. Startup Reconcile (기존 stuck/retry 대신)
    startup_reconcile(&db, gh, &mut queues, &cfg).await?;

    // 메인 루프
    loop {
        // 1. Recovery: orphan wip → queues 기준 검사
        recovery::recover_orphan_wip(&repos, gh, &queues, gh_host).await?;

        // 2. Scan (interval 체크 포함)
        scanner::scan_all(&db, env, gh, &mut queues).await?;

        // 3. Consume
        pipeline::process_all(&db, env, &workspace, &notifier, gh, claude, &mut queues).await?;

        sleep(tick_interval).await;
    }
}

/// Bounded reconciliation (재시작 시 메모리 큐 복구)
async fn startup_reconcile(
    db: &Database,
    gh: &dyn Gh,
    queues: &mut TaskQueues,
    cfg: &WorkflowConfig,
) -> Result<()> {
    let repos = db.repo_find_enabled()?;
    let window_hours = 24; // reconcile_window_hours

    for repo in repos {
        let safe_since = compute_safe_since(db, &repo.id, window_hours)?;

        // GitHub API 조회 (bounded)
        let issues = gh.api_paginate(&repo.name, "issues", &[
            ("state", "open"), ("since", &safe_since), ("sort", "updated"),
        ], cfg.consumer.gh_host.as_deref()).await?;

        let pulls = gh.api_paginate(&repo.name, "pulls", &[
            ("state", "open"), ("since", &safe_since), ("sort", "updated"),
        ], cfg.consumer.gh_host.as_deref()).await?;

        // 라벨 기반 필터:
        // autodev:done → skip
        // autodev:skip → skip
        // autodev:wip  → wip 제거 후 큐 적재 (orphan 정리 겸용)
        // 라벨 없음    → 큐 적재 (미처리)

        // cursor 갱신
        db.cursor_upsert(&repo.id, "issues", &now)?;
    }
}
```

### 수정: `daemon/recovery.rs`

시그니처 변경: `ActiveItems` → `TaskQueues`
```rust
pub async fn recover_orphan_wip(
    repos: &[EnabledRepo],
    gh: &dyn Gh,
    queues: &TaskQueues,  // queues.contains()로 확인
    gh_host: Option<&str>,
) -> Result<u64>
```

---

## Phase 6: SQLite 큐 테이블 제거 + client 수정

### 수정: `queue/schema.rs`

`issue_queue`, `pr_queue`, `merge_queue` 테이블 및 관련 인덱스 제거.
`repositories`, `scan_cursors`, `consumer_logs`만 유지.

### 수정: `queue/repository.rs`

제거:
- `IssueQueueRepository` trait 전체
- `PrQueueRepository` trait 전체
- `MergeQueueRepository` trait 전체
- `QueueAdmin` trait의 `queue_retry`, `queue_clear`, `queue_reset_stuck`, `queue_auto_retry_failed`

유지:
- `RepoRepository` (repos 관리)
- `ScanCursorRepository` (cursors 관리)
- `ConsumerLogRepository` (logs 관리)

### 수정: `client/mod.rs`

`queue list/retry/clear` → TaskQueues 기반으로 변경.
데몬이 실행 중일 때만 의미 있음 (메모리 큐이므로).

---

## Phase 7: PR 피드백 루프 구현

### 수정: `pipeline/pr.rs`

```rust
// StateQueue 상태:
// Pending → Reviewing → (approve → done) or (request_changes → ReviewDone)
// ReviewDone → Improving → Improved → Reviewing (반복)

pub async fn process_pending(queues, ...) {
    while let Some(item) = queues.prs.pop("Pending") {
        // review → verdict 분기
        // approve → remove + autodev:done
        // request_changes → push("ReviewDone", item_with_review)
    }
}

pub async fn process_review_done(queues, ...) {
    while let Some(item) = queues.prs.pop("ReviewDone") {
        // 피드백 반영 구현 실행
        // success → push("Improved", item)
        // failure → remove + wip 제거 (재시도)
    }
}

pub async fn process_improved(queues, ...) {
    while let Some(item) = queues.prs.pop("Improved") {
        // 재리뷰 실행 → push("Reviewing", item)
        // (Reviewing → approve/request_changes 반복)
    }
}
```

### 수정: `pipeline/mod.rs`

```rust
pub async fn process_all(...) {
    // Issue
    issue::process_pending(...).await?;
    issue::process_ready(...).await?;

    // PR (전체 사이클)
    pr::process_pending(...).await?;
    pr::process_review_done(...).await?;   // 추가
    pr::process_improved(...).await?;      // 추가

    // Merge
    merge::process_pending(...).await?;
}
```

---

## Phase 8: 테스트 작성

### 신규: `queue/state_queue.rs` (unit tests)

```rust
#[cfg(test)]
mod tests {
    // push/pop/transit/remove 기본 동작
    // dedup index 정합성
    // 빈 큐 pop → None
}
```

### 신규: `queue/task_queues.rs` (unit tests)

```rust
#[cfg(test)]
mod tests {
    // contains/state_of 검증
    // push → index 자동 등록
    // remove → index 자동 제거
}
```

### 수정: 기존 pipeline 테스트 (mock 패턴으로)

```rust
#[tokio::test]
async fn test_issue_flow_implement() {
    let gh = MockGh::new();
    let claude = MockClaude::new();
    let git = MockGit::new();
    // ...
    // queues.push_issue("Pending", item)
    // process_pending(&queues, ...)
    // assert queues.issues.len("Ready") == 1
    // assert gh.added_labels contains "autodev:wip"
}
```

---

## 삭제 대상

| 파일/코드 | 이유 |
|-----------|------|
| `active.rs` | TaskQueues.index로 대체 |
| `queue/schema.rs` 내 issue_queue, pr_queue, merge_queue DDL | 인메모리로 전환 |
| `queue/repository.rs` 내 Issue/Pr/Merge Queue traits + impl | 인메모리로 전환 |
| `queue/models.rs` 내 DB 전용 모델 (IssueQueueItem 등) | 인메모리 모델로 교체 |

---

## 유지 대상 (변경 없음)

| 파일 | 이유 |
|------|------|
| `infrastructure/claude/` | 변경 불필요 |
| `infrastructure/git/` | 변경 불필요 |
| `components/workspace.rs` | 변경 불필요 |
| `components/verdict.rs` | 변경 불필요 |
| `components/reviewer.rs` | 변경 불필요 |
| `components/merger.rs` | 변경 불필요 |
| `config/` | 변경 불필요 (reconcile_window_hours 추가만) |
| `tui/` | Phase 8 이후 별도 작업 |

---

## 사이드이펙트 정리

| 영역 | 영향 | 대응 |
|------|------|------|
| 데몬 재시작 | 큐 데이터 휘발 | startup_reconcile()로 복구 |
| client CLI (`queue list/retry`) | DB 직접 조회 불가 | 데몬 프로세스에 IPC 또는 제한된 기능 |
| consumer_logs | 유지 (SQLite) | 변경 없음 |
| retry_count | DB 필드 제거 | 인메모리 retry 카운터 or scan에서 자연 재시도 |
| stuck recovery | DB 기반 불가 | 불필요 (메모리 큐는 프로세스 종료 시 자동 정리) |

---

## 구현 순서 (의존성 기반)

```
Phase 1 (StateQueue) ← 독립, 먼저 구현 + 테스트
    ↓
Phase 2 (label_add) ← 독립, 병렬 가능
    ↓
Phase 3 (Scanner)   ← Phase 1, 2 필요
    ↓
Phase 4 (Pipeline)  ← Phase 1, 2 필요
    ↓
Phase 5 (Daemon)    ← Phase 3, 4 필요
    ↓
Phase 6 (Cleanup)   ← Phase 5 이후
    ↓
Phase 7 (PR Loop)   ← Phase 4 이후
    ↓
Phase 8 (Tests)     ← 각 Phase마다 TDD로 병행
```
