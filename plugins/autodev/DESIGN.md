# Autonomous Plugin - 상세 설계

## 1. 개요

기존 플러그인 생태계(`develop-workflow`, `git-utils`, `external-llm`)를 이벤트 기반으로 자동 실행하는 오케스트레이션 레이어.

```
autodev (오케스트레이터)
  ├── develop-workflow  → /develop, /multi-review
  ├── git-utils         → /merge-pr, /commit-and-pr
  └── external-llm      → /invoke-codex, /invoke-gemini
```

### 핵심 원칙

- **Monitor 자체는 얇게**: 이벤트 감지, 라벨 관리, 세션 실행만 담당
- **분석/구현 품질은 기존 플러그인에 위임**: 플러그인이 진화하면 자동으로 품질 향상
- **GitHub 라벨 = SSOT**: 작업 완료 상태의 유일한 영속 마커 (`autodev:done`, `autodev:skip`)
- **SQLite = 영속 관리**: 레포 등록, scan 커서(API 최적화), 실행 로그(감사) — 작업 큐는 저장하지 않음
- **In-Memory StateQueue = 작업 큐**: 상태별 큐로 이벤트 드리븐 처리, 휘발성 (재시작 시 bounded reconciliation으로 자동 복구)
- **단일 바이너리**: Rust 데몬 + TUI 대시보드, 추가 의존성 없음
- **사람과 동일한 환경**: `claude -p`는 워크트리 cwd에서 실행하여 해당 레포의 `.claude/`, `CLAUDE.md`, 설치된 플러그인이 그대로 적용됨

---

## 2. 아키텍처

### 3-Tier 상태 관리

```
┌─────────────────────────────────────────────┐
│        GitHub Labels (SSOT, 영속)            │
│  autodev:done — 완료 (유일한 영속 완료 마커)   │
│  autodev:skip — HITL 대기                    │
│  autodev:wip  — 처리중                       │
│  (없음)       — 미처리 → scan 대상            │
│                                              │
│  역할: 재시작 시 reconciliation의 기준         │
│        "라벨 없는 open 건 = 미처리"            │
└──────────────────┬──────────────────────────┘
                   │ gh api
┌──────────────────▼──────────────────────────┐
│            SQLite (영속 관리)                 │
│  repositories   — 레포 등록/활성화 관리        │
│  scan_cursors   — incremental scan 최적화용   │
│                   (일관성 보장 아님, 순수 최적화)│
│  consumer_logs  — 실행 로그/감사 추적          │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────▼──────────────────────────┐
│       In-Memory StateQueue (휘발)            │
│  issues:  StateQueue<IssueItem>              │
│  prs:     StateQueue<PrItem>                 │
│  merges:  StateQueue<MergeItem>              │
│                                              │
│  index: HashMap<WorkId, State>               │
│  — scan 시 중복 적재 방지 (O(1) lookup)       │
│  — consumer는 상태별 큐를 watch하여 처리       │
│                                              │
│  재시작 시: bounded reconciliation으로 복구    │
└──────────────────────────────────────────────┘
```

### GitHub 라벨

| 라벨 | 의미 | 영속성 |
|------|------|--------|
| (없음) | 미처리 → scan 대상 | - |
| `autodev:wip` | 데몬이 처리중 | 크래시 시 orphan → recovery() 정리 |
| `autodev:done` | 처리 완료 | **영속 완료 마커** (reconciliation 기준) |
| `autodev:skip` | clarify/wontfix 등으로 건너뜀 | **영속 제외 마커** |

### 라벨 상태 전이

```
(없음) ──scan──→ autodev:wip ──success──→ autodev:done
                     │
                     ├──skip────→ autodev:skip
                     │
                     ├──failure──→ (없음)  ← 재시도
                     │
                     └──crash────→ autodev:wip (orphan)
                                     │
                                  recovery()
                                     │
                                     ▼
                                   (없음)  ← 재시도
```

### SQLite Schema

```sql
-- 레포 등록/관리 (CLI로 관리, 영속 필요)
CREATE TABLE repositories (
    id          TEXT PRIMARY KEY,
    url         TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,        -- "org/repo"
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- Scan 커서 (API 최적화 전용, 일관성 보장 아님)
-- 정상 운영: since 파라미터로 API 응답 크기 축소
-- 재시작 시: reconcile_window_hours만큼 되감아서 사용
CREATE TABLE scan_cursors (
    repo_id     TEXT NOT NULL REFERENCES repositories(id),
    target      TEXT NOT NULL,           -- "issues" | "pulls"
    last_seen   TEXT NOT NULL,           -- RFC3339: 마지막으로 본 항목의 updated_at
    last_scan   TEXT NOT NULL,           -- RFC3339: 마지막 scan 실행 시점
    PRIMARY KEY (repo_id, target)
);

-- 실행 로그 (감사/디버깅/knowledge extraction 소스)
CREATE TABLE consumer_logs (
    id          TEXT PRIMARY KEY,
    repo_id     TEXT NOT NULL REFERENCES repositories(id),
    queue_type  TEXT NOT NULL,           -- "issue" | "pr" | "merge"
    item_key    TEXT NOT NULL,           -- "issue:org/repo:42"
    worker_id   TEXT NOT NULL,
    command     TEXT NOT NULL,
    stdout      TEXT,
    stderr      TEXT,
    exit_code   INTEGER,
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    duration_ms INTEGER
);
CREATE INDEX idx_consumer_logs_repo ON consumer_logs(repo_id, started_at);
```

> **작업 큐 테이블 없음**: issue_queue, pr_queue, merge_queue는 In-Memory StateQueue로 대체.
> SQLite는 레포 관리 + API 최적화 + 로그 기록만 담당한다.

### In-Memory StateQueue

```rust
/// 상태별 큐: consumer가 특정 상태의 아이템을 pop하여 처리
struct StateQueue<T: HasId> {
    queues: HashMap<State, VecDeque<T>>,
}

impl<T: HasId> StateQueue<T> {
    fn push(&mut self, state: State, item: T);
    fn pop(&mut self, state: State) -> Option<T>;        // consumer가 꺼냄
    fn transit(&mut self, from: State, to: State, id: &str); // 상태 전이
    fn remove(&mut self, id: &str) -> Option<T>;         // done/HITL 시 제거
    fn len(&self, state: State) -> usize;                // 큐 깊이
}

/// 전체 작업 큐 (dedup index 포함)
struct TaskQueues {
    issues: StateQueue<IssueItem>,
    prs:    StateQueue<PrItem>,
    merges: StateQueue<MergeItem>,

    // Dedup + state lookup: O(1)
    // scan 시 이미 큐에 있는 아이템은 skip
    index: HashMap<WorkId, State>,
}

// WorkId = "{type}:{repo}:{number}"
// 예: "issue:org/repo:42", "pr:org/repo:15", "merge:org/repo:15"
```

### Phase 정의

```
Issue Phase:
  Pending       → scan에서 등록됨
  Analyzing     → 분석 프롬프트 실행중
  Ready         → 분석 완료, 구현 대기
  Implementing  → 구현 프롬프트 실행중

PR Phase (리뷰):
  Pending       → scan에서 등록됨
  Reviewing     → PR 리뷰 실행중
  ReviewDone    → 리뷰 완료, 개선 대기
  Improving     → 리뷰 피드백 반영 구현중
  Improved      → 개선 완료, 재리뷰 대기

Merge Phase:
  Pending       → merge scan에서 등록됨
  Merging       → 머지 실행중
  Conflict      → 충돌 해결 시도중
```

### 큐에서 제거되는 시점

| 조건 | 동작 | 이유 |
|------|------|------|
| **done** | queue.remove() + `autodev:done` 라벨 | 작업 완료 |
| **skip** | queue.remove() + `autodev:skip` 라벨 | HITL 대기 (clarify/wontfix) |
| **failure** | queue.remove() + wip 라벨 제거 | 다음 scan에서 재발견 → 재시도 |

> done/skip 시에만 영속 라벨이 붙으므로, 재시작 시 reconciliation에서 자연스럽게 필터링된다.

---

## 3. 플러그인 디렉토리 구조

```
plugins/autodev/
├── .claude-plugin/
│   └── plugin.json
│
├── commands/
│   ├── auto.md                  # /auto - 시작/중지/상태
│   ├── auto-setup.md            # /auto-setup - 레포 등록 위자드
│   ├── auto-config.md           # /auto-config - 설정 변경
│   └── auto-dashboard.md        # /auto-dashboard - 대시보드 열기
│
├── agents/
│   ├── issue-analyzer.md        # 이슈 분석 리포트 생성
│   ├── pr-reviewer.md           # PR 코드 리뷰 (multi-LLM)
│   └── conflict-resolver.md     # 머지 충돌 해결
│
├── cli/                         # Rust 단일 바이너리 (daemon + CLI)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # CLI 진입점 (clap subcommands)
│       ├── daemon/
│       │   ├── mod.rs           # 데몬 시작/중지 (단일 인스턴스 보장)
│       │   └── pid.rs           # PID 파일 관리 (~/.autodev/daemon.pid)
│       ├── scanner/
│       │   └── mod.rs           # GitHub 라벨 기반 스캐너
│       ├── processor/
│       │   ├── mod.rs           # Phase별 작업 실행
│       │   ├── issue.rs         # Issue 처리 (분석 → 구현)
│       │   ├── pr.rs            # PR 처리 (리뷰 → 개선)
│       │   └── merge.rs         # Merge 처리
│       ├── queue/
│       │   ├── mod.rs           # TaskQueues (StateQueue + dedup index)
│       │   ├── schema.rs        # SQLite 스키마 (repositories, scan_cursors, consumer_logs)
│       │   └── repository.rs    # 레포/커서/로그 DB 쿼리
│       ├── github/
│       │   └── mod.rs           # GitHub API + 라벨 관리
│       ├── workspace/
│       │   └── mod.rs           # 워크스페이스 매니저 (git worktree)
│       ├── session/
│       │   ├── mod.rs           # claude -p 세션 실행
│       │   └── output.rs        # 세션 출력 파싱
│       ├── tui/
│       │   ├── mod.rs           # TUI 앱 루프
│       │   ├── views.rs         # 화면 레이아웃
│       │   └── events.rs        # 키보드/마우스 이벤트 처리
│       └── config/
│           ├── mod.rs           # 설정 로드/저장
│           └── models.rs        # 설정 모델
│
└── README.md
```

---

## 4. Cargo.toml

```toml
[package]
name = "autodev"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "autodev"
path = "src/main.rs"

[dependencies]
# CLI
clap = { version = "4", features = ["derive"] }

# Async runtime
tokio = { version = "1", features = ["full"] }

# HTTP client (GitHub API)
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }

# SQLite (레포 관리 + scan 커서 + 실행 로그)
rusqlite = { version = "0.32", features = ["bundled"] }

# TUI
ratatui = "0.29"
crossterm = "0.28"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Utils
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

> SQLite는 **레포 관리 + scan 커서(최적화) + 실행 로그(감사)** 전용.
> 작업 큐는 In-Memory StateQueue로 처리하며 SQLite에 저장하지 않는다.

---

## 5. 메인 루프

```
daemon start
│
├─ 0. startup_reconcile()        ← 최초 1회만 실행
│    SQLite에서 enabled 레포 로드
│    scan_cursors.last_seen - reconcile_window_hours (기본 24h) 이후의
│    open 이슈/PR을 GitHub API로 조회
│    autodev:done/skip 라벨이 있는 건 skip
│    나머지 → memory StateQueue[Pending]에 적재
│    cursor를 현재 시점으로 갱신
│
└─ loop (매 tick)
    │
    ├─ 1. recovery()
    │    "autodev:wip 라벨 + queue에 없는 이슈" 조회
    │    → autodev:wip 라벨 제거
    │    (다음 tick의 scan에서 자연스럽게 재발견)
    │
    ├─ 2. scan()                  ← scan_interval 경과 시에만
    │    cursor 기반 incremental scan (since 파라미터)
    │    이미 queue.index에 있는 아이템은 skip (O(1) dedup)
    │    신규 아이템만 → wip 라벨 + queue.push(Pending)
    │
    ├─ 3. consume()               ← 매 tick 실행
    │    queue[Pending]에 아이템 있으면 pop → 처리 시작
    │    queue[Ready]에 아이템 있으면 pop → 구현 시작
    │    (상태별 큐를 순회하며 이벤트 드리븐 처리)
    │    처리 완료 → queue.remove() + 라벨 전이
    │    pre-flight API 호출 불필요 (scan 시점에 open 확인 완료)
    │
    └─ 4. sleep(tick_interval)
```

### Startup Reconciliation (Bounded Recovery)

재시작 시 메모리 큐를 복구하는 메커니즘.
cursor를 일관성 보장에 사용하지 않고, **GitHub 라벨을 source of truth**로 삼는다.

```
startup_reconcile()
  │
  ▼
repos = db.repo_find_enabled()

for repo in repos:
  │
  ├─ 1. safe_since 계산
  │    last_seen = db.cursor_get_last_seen(repo.id, target)
  │    safe_since = last_seen - reconcile_window_hours  (기본 24h)
  │    (cursor가 없으면 now - 24h)
  │
  ├─ 2. GitHub API 조회 (bounded)
  │    gh api repos/{repo}/issues?state=open&since={safe_since}
  │    gh api repos/{repo}/pulls?state=open&since={safe_since}
  │
  ├─ 3. 라벨 기반 필터
  │    autodev:done 라벨 → skip (이미 완료)
  │    autodev:skip 라벨 → skip (HITL 대기)
  │    autodev:wip 라벨  → wip 제거 후 큐 적재 (orphan 정리 겸용)
  │    라벨 없음          → 큐 적재 (미처리)
  │
  ├─ 4. memory queue 적재
  │    queue.push(Pending, item)
  │    queue.index.insert(work_id, Pending)
  │
  └─ 5. cursor 갱신
       db.cursor_upsert(repo.id, target, now)
```

**왜 bounded (24h) 인가:**

| 방식 | API 비용 | 안전성 | 적합성 |
|------|---------|--------|--------|
| Full scan (since 없음) | 높음 (전체 open 건) | 완벽 | 이슈가 많은 레포에서 비효율 |
| Cursor 그대로 사용 | 낮음 | **갭 발생 가능** | 위험 |
| **Cursor - 24h (bounded)** | 중간 (24h 윈도우) | 충분 | 대부분의 crash-restart 커버 |

```yaml
# 설정으로 조절 가능
daemon:
  reconcile_window_hours: 24   # 재시작 시 복구 윈도우 (기본 24h)
```

> 24h 이상 데몬이 죽어있었다면 운영 이슈로, 별도 알림/모니터링이 필요하다.

### 타이밍 예시 (scan_interval_secs: 300)

```
startup:   reconcile (since=cursor-24h) + queue 복구

tick  0s:  recovery + scan ✓ (첫 실행)  + consume
tick 10s:  recovery + scan SKIP         + consume
tick 20s:  recovery + scan SKIP         + consume
...
tick 300s: recovery + scan ✓ (5분 경과) + consume
```

### Cursor의 역할 (최적화 전용)

```
┌────────────────────────────────────────────────────────┐
│ cursor는 "일관성 보장"이 아니라 "API 최적화"만 담당한다  │
│                                                        │
│ 정상 운영:                                              │
│   scan(since=cursor) → API 응답 크기 축소               │
│   cursor 전진 → 다음 scan에서 중복 응답 방지             │
│                                                        │
│ 재시작:                                                 │
│   reconcile(since=cursor-24h) → 안전 마진으로 복구       │
│   GitHub 라벨로 실제 완료 여부 판별                       │
│   cursor가 틀려도 라벨이 맞으면 OK                        │
│                                                        │
│ 일관성 보장 = GitHub 라벨 (SSOT)                         │
│ API 최적화 = scan_cursors (보조)                         │
└────────────────────────────────────────────────────────┘
```

---

## 6. 상세 흐름

### recovery()

데몬 시작 시 또는 매 tick에서, 크래시로 인해 남아있는 orphan `autodev:wip` 라벨을 정리한다.

```
recovery()
  │
  ▼
gh api: "autodev:wip" 라벨이 있는 open 이슈/PR 조회
  │
  for each item:
    id = "{type}:{repo}:{number}"
    │
    ├─ active.contains(id)?
    │    YES → 정상 (데몬이 처리중) → skip
    │    NO  → orphan (크래시 잔여물)
    │           → gh label remove "autodev:wip"
    │           (다음 scan에서 자연스럽게 재발견 → 재처리)
```

### scan()

cursor 기반 incremental scan으로 신규 아이템만 발견하여 memory queue에 적재한다.
**pre-flight API 호출 없음** — scan 시점에 open 상태가 확인되므로 consumer에서 재확인 불필요.

```
scan()
  │
  repos = db.repo_find_enabled()
  │
  for repo in repos:
  │
  ├─ should_scan = db.cursor_should_scan(repo.id, scan_interval_secs)
  │  NO → skip (아직 interval 미경과)
  │
  ├── 2a. issue/pr scan (incremental)
  │   since = db.cursor_get_last_seen(repo.id, "issues")
  │   gh api repos/{repo}/issues?state=open&since={since}&sort=updated
  │   gh api repos/{repo}/pulls?state=open&since={since}&sort=updated
  │   │
  │   필터:
  │     • queue.index에 이미 있으면 skip (O(1) dedup)
  │     • autodev 라벨 있으면 skip (done/skip/wip 모두)
  │     • filter_labels 매칭 (설정된 경우)
  │     • ignore_authors 제외
  │   │
  │   for each new_item:
  │     id = "{type}:{repo}:{number}"
  │     gh label add "autodev:wip"
  │     queue.push(Pending, item)
  │     queue.index.insert(id, Pending)
  │   │
  │   cursor 전진: db.cursor_upsert(repo.id, "issues", latest_updated_at)
  │
  └── 2b. merge scan
      gh api repos/{repo}/pulls?state=open
      │
      필터:
        • approved 상태 (사람 or autodev가 approve)
        • CI checks 통과 (설정된 경우)
        • queue.index에 이미 있으면 skip
        • autodev 라벨 없는 것만
        • auto_merge 설정이 활성화된 레포만
      │
      for each item:
        id = "merge:{repo}:{number}"
        gh label add "autodev:wip"
        queue.merges.push(Pending, item)
        queue.index.insert(id, Pending)
```

> **merge scan의 소스**: autodev가 리뷰하여 approve한 PR + 사람이 approve한 PR 모두 대상.
> `auto_merge: true` 설정이 있는 레포에서만 동작한다.

### API 호출 비용 비교

```
Before (DB 큐 방식):
  scan:     N repos × 2 API calls / 300초       (incremental)
  consumer: M items × 1~2 pre-flight calls / 10초 (매 tick마다!)

After (Memory StateQueue 방식):
  scan:     N repos × 2 API calls / 300초       (incremental, 동일)
  consumer: 0 API calls                          (메모리에서 직접 처리)
  startup:  N repos × 2 API calls × 1회          (bounded reconciliation)
```

### Issue Flow

```
process() — Issue
  │
  ├─ Pending
  │    워크트리 준비
  │    run_claude(analysis_prompt, json)
  │    phase → Analyzing
  │
  ├─ Analyzing (완료 대기)
  │    │
  │    ▼
  │  AnalysisResult {
  │    verdict: "implement" | "needs_clarification" | "wontfix"
  │    confidence: 0.0 ~ 1.0
  │    summary, affected_files, implementation_plan
  │    questions: [...]   ← confidence 낮을 때
  │  }
  │    │
  │    ├─ implement (high confidence)
  │    │    GitHub 댓글: "분석 완료, 구현 진행합니다"
  │    │    phase → Ready
  │    │
  │    ├─ needs_clarification
  │    │    GitHub 댓글: 분석 레포트 + 질문
  │    │    wip → autodev:skip
  │    │    active.remove(id)
  │    │    (사람이 답변 후 skip 라벨 제거하면 다시 처리)
  │    │
  │    └─ wontfix
  │         GitHub 댓글: 사유 설명
  │         wip → autodev:skip
  │         active.remove(id)
  │
  ├─ Ready
  │    run_claude(implement_prompt + analysis_context)
  │    phase → Implementing
  │
  └─ Implementing (완료 대기)
       │
       ├─ success
       │    GitHub PR 생성
       │    wip → autodev:done
       │    active.remove(id)
       │
       └─ failure
            wip 라벨 제거 (라벨 없음)
            active.remove(id)
            (다음 tick에 재발견 → 재시도)
```

### PR Flow - 리뷰 → 개선 → 재리뷰 사이클

리뷰 결과를 JSON으로 받아 verdict에 따라 결정적으로 분기한다.
`request_changes` 시 자동으로 피드백을 반영하고, 재리뷰 후 approve되면 리뷰 완료.
머지는 별도 Merge Flow가 담당한다.

```
process() — PR (리뷰 전용)
  │
  ├─ Pending
  │    워크트리 준비 (head_branch checkout)
  │    run_claude(/multi-review, json)
  │    phase → Reviewing
  │
  ├─ Reviewing (완료 대기)
  │    │
  │    ▼
  │  ReviewResult {
  │    verdict: "approve" | "request_changes"
  │    summary, comments: [{path, line, body}]
  │  }
  │    │
  │    ├─ approve
  │    │    gh pr review --approve -b "{summary}"
  │    │    wip → autodev:done
  │    │    active.remove("pr:...", id)
  │    │    (다음 tick의 merge scan이 이 PR을 발견 → 머지 큐 진입)
  │    │
  │    └─ request_changes
  │         POST /pulls/{N}/reviews
  │           event: REQUEST_CHANGES
  │           body: "{summary}"
  │           comments: [{path, line, body}]
  │         phase → ReviewDone
  │
  ├─ ReviewDone
  │    run_claude(
  │      "/develop implement review feedback:"
  │      + review_comment
  │    )
  │    phase → Improving
  │
  ├─ Improving (완료 대기)
  │    │
  │    ├─ success → phase → Improved
  │    └─ failure → wip 제거, active.remove(id) → 재시도
  │
  └─ Improved
       재리뷰: run_claude(/multi-review, json)
       phase → Reviewing (반복)
       (Reviewing에서 approve 나올 때까지 사이클 반복)
```

### Merge Flow - 별도 큐

approved 상태의 PR을 발견하여 순차적으로 머지한다.
autodev가 approve한 PR, 사람이 approve한 PR 모두 대상.

```
merge scan에서 발견된 approved PR
  │
  ▼
process() — Merge
  │
  ├─ Pending
  │    워크트리 준비
  │    CI checks 통과 확인 (설정된 경우)
  │    run_claude(/merge-pr {N})
  │    phase → Merging
  │
  ├─ Merging (완료 대기)
  │    │
  │    ├─ success
  │    │    wip → autodev:done
  │    │    active.remove(id)
  │    │
  │    ├─ conflict
  │    │    run_claude(conflict resolution)
  │    │    phase → Conflict
  │    │
  │    └─ other failure
  │         wip 제거, active.remove(id) → 재시도
  │
  └─ Conflict (충돌 해결 대기)
       │
       ├─ 해결 성공 → 재머지 시도 → phase → Merging
       └─ 해결 실패 → wip 제거, active.remove(id) → 재시도
```

> **순서 보장**: merge scan에서 발견된 PR은 `created_at` 순으로 처리하여
> 먼저 approve된 PR부터 머지한다. 선행 PR 머지 후 base branch가 변경되면
> 후속 PR은 자연스럽게 conflict → 자동 해결 or 재시도.

---

## 7. Workspace 관리 (git worktree)

각 태스크는 격리된 worktree에서 실행된다.

```
~/.autodev/workspaces/
└── {sanitized-repo-name}/
    │
    ├── main/                    ← base clone (장기 유지)
    │   └── (전체 레포)            git clone --single-branch
    │                              git pull (scan 시 갱신)
    │
    ├── issue-42/                ← worktree (태스크 시작 시 생성)
    │   └── (분석 + 구현 작업)
    │
    ├── pr-15/                   ← worktree (head_branch checkout)
    │   └── (리뷰 + 개선 작업)
    │
    └── merge-pr-12/             ← worktree (merge 시도)

워크트리 생명주기:
  태스크 시작 → ensure_cloned() → create_worktree()
  태스크 완료 → remove_worktree() (done/skip/failure 시 정리)

장점:
  • 태스크 간 완전 격리 (동시 issue-42 + pr-15 가능)
  • base clone 재사용 (네트워크 비용 최소화)
  • claude -p는 worktree cwd에서 실행
    → 레포의 .claude/, CLAUDE.md, 설치된 플러그인 자동 적용
    → 사람이 직접 레포 열어 작업하는 것과 100% 동일한 환경
```

---

## 8. Session Runner (claude -p)

```
run_claude(cwd, prompt, output_format)
  │
  │  claude -p "{prompt}" --output-format json
  │  cwd = worktree 경로
  │  env = GITHUB_TOKEN, ANTHROPIC_API_KEY 등
  │
  ▼
SessionResult {
  stdout: String       ← JSON 파싱 대상
  stderr: String       ← 디버깅용
  exit_code: i32       ← 0=성공, else=실패
  duration_ms: u64
}
  │
  ├─ exit_code == 0 → stdout → serde_json::from_str::<T>()
  └─ exit_code != 0 → failure 처리 + 로그 기록
```

---

## 9. CLI 서브커맨드

```
# 데몬 제어
autodev start              # 데몬 시작 (포그라운드, 단일 인스턴스)
autodev stop               # 데몬 중지 (PID → SIGTERM)
autodev restart            # 데몬 재시작

# 레포 관리 (→ SQLite repositories 테이블)
autodev repo add <url>     # 레포 등록 (URL에서 name 자동 추출)
autodev repo list          # 등록된 레포 목록 (enabled/disabled 표시)
autodev repo config <name> # 레포별 설정 확인 (글로벌 + 워크스페이스 merge 결과)
autodev repo remove <name> # 레포 제거

# 상태 조회
autodev status             # 데몬 상태 + 큐 깊이 요약 + 레포별 통계
autodev dashboard          # TUI 대시보드

# 설정 관리 (→ YAML 파일)
autodev config show        # 현재 설정 표시
autodev config edit        # 설정 편집
```

### 공유 상태

```
~/.autodev/
├── config.yaml          # 글로벌 설정 파일
├── autodev.db           # SQLite (repositories, scan_cursors, consumer_logs)
├── daemon.pid           # PID 파일 (단일 인스턴스 보장)
├── workspaces/          # 레포별 워크스페이스
│   └── {org}/{repo}/
│       ├── main/        # base clone
│       └── issue-42/    # worktree
└── logs/
    ├── daemon.2026-02-21.log   # 일자별 롤링
    ├── daemon.2026-02-20.log
    └── ...                     # retention_days 이후 자동 삭제
```

### 로그 롤링 정책

- **일자별 롤링**: `tracing-appender::rolling::daily()` 사용
- **보존 기간**: `log_retention_days` 설정 (기본 30일)
- **자동 정리**: 데몬 시작 시 + 매일 자정에 보존 기간 초과 파일 삭제
- **파일명 형식**: `daemon.YYYY-MM-DD.log`

---

## 10. TUI 대시보드

`autodev dashboard` 실행 시 ratatui 기반 터미널 UI 표시.

### 키바인딩
```
Tab       - 패널 전환 (Active → Logs)
j/k       - 목록 상/하 이동
r         - 실패 항목 재시도
q         - 종료
?         - 도움말
```

### 레이아웃
```
┌─────────────────────────────────────────────────────────┐
│  autodev v0.1.0          ● daemon running    [?]help    │
├──────────┬──────────────────────────────────────────────┤
│          │                                              │
│ Active   │  issue:org/repo:42     Analyzing             │
│ Items    │  issue:org/repo:99     Ready                 │
│          │  pr:org/repo:10        Reviewing             │
│          │                                              │
│          ├──────────────────────────────────────────────┤
│ Labels   │  GitHub Label Summary                        │
│          │  autodev:wip   3                             │
│          │  autodev:done  28                            │
│          │  autodev:skip  5                             │
│          │                                              │
│          ├──────────────────────────────────────────────┤
│          │  Activity Log (당일 로그 tail)              │
│          │  14:32 issue-42  Pending → Analyzing         │
│          │  14:30 pr-15    Pending → Reviewing          │
│          │  14:28 pr-12    autodev:done ✓               │
│          │  14:25 issue-41 autodev:done ✓ → PR #18     │
│          │                                              │
└──────────┴──────────────────────────────────────────────┘
```

---

## 11. 에이전트 설계

### issue-analyzer.md

```yaml
---
description: (internal) Issue 분석 - Multi-LLM 병렬 분석으로 리포트 생성
model: sonnet
tools: ["Read", "Glob", "Grep", "Bash", "Task"]
---
```

역할: Claude + Codex + Gemini를 병렬 호출하여 이슈를 다각도로 분석

### pr-reviewer.md

```yaml
---
description: (internal) PR 코드 리뷰 수행 (multi-LLM 병렬)
model: sonnet
tools: ["Read", "Glob", "Grep", "Bash", "Task"]
---
```

역할: `/multi-review` 호출을 통해 Sonnet + Codex + Gemini 병렬 리뷰

### conflict-resolver.md

```yaml
---
description: (internal) 머지 충돌을 분석하고 해결
model: opus
tools: ["Read", "Glob", "Grep", "Edit", "Bash"]
---
```

---

## 12. Configuration

```yaml
# ~/.autodev/config.yaml

repos:
  - name: org/my-repo
    url: https://github.com/org/my-repo
    enabled: true
    scan_interval_secs: 300
    scan_targets: [issues, pulls]
    filter_labels: []              # 빈 배열 = 전체 대상
    ignore_authors: [dependabot, renovate]
    model: sonnet
    confidence_threshold: 0.7
    auto_merge: true               # approved PR 자동 머지
    merge_require_ci: true         # CI checks 통과 필수

daemon:
  tick_interval_secs: 10
  reconcile_window_hours: 24     # 재시작 시 복구 윈도우 (기본 24h)
  log_dir: ~/.autodev/logs
  log_retention_days: 30
  daily_report_hour: 6           # 매일 06:00에 일일 리포트 생성
```

---

## 13. Knowledge Extraction (Agent-Driven)

두 가지 트리거로 인사이트를 도출한다:

| 트리거 | 시점 | 분석 범위 | 용도 |
|--------|------|----------|------|
| **Per-task** | done 전이 시 | 해당 태스크 1건 | 즉시 피드백 (규칙 제안, 개선점) |
| **Daily** | 매일 `daily_report_hour` | 전일 로그 전체 | 크로스 태스크 패턴, 일일 요약 리포트 |

```
Per-task: 이슈 #42 done → 해당 세션만 분석 → 즉시 제안
Daily:    06:00 → daemon.2026-02-20.log 전체 분석 → 일일 리포트
```

### 설계 원칙: Data-Only + LLM 해석

```
daemon.log + suggest-workflow = 사실만 반환   "무엇이 일어났는가"
Agent (LLM)                   = 의미를 해석   "그래서 무슨 의미인가"
```

```
❌ Rule-based (엣지케이스 누적)
   if error.contains("timeout")  → suggest "increase timeout"
   ... (끝없이 규칙 추가)

✅ Data-only + LLM 해석
   Log: "timeout 3건, 모두 external API 호출 시점"
   Agent: "→ API 클라이언트에 retry/backoff 설정 필요"
```

### 데이터 소스

autodev의 인사이트는 **두 개의 독립적인 데이터 소스**에서 나온다.

```
┌───────────────────────────────┐    ┌──────────────────────────────────┐
│ A. daemon.YYYY-MM-DD.log      │    │ B. suggest-workflow index.db     │
│    (~/.autodev/logs/)         │    │    (~/.claude/suggest-workflow-   │
│                               │    │     index/{project}/)            │
│ 일자별 롤링 (30일 보존):       │    │                                  │
│ • phase 전이 이벤트            │    │ 세션 실행 이력:                     │
│ • 라벨 변경 이력               │    │ • sessions (+ first_prompt_      │
│ • 에러 메시지                  │    │   snippet)                       │
│ • 소요 시간                   │    │ • prompts                        │
│                               │    │ • tool_uses (classified)         │
│ "무엇을 처리했는가"             │    │ • file_edits                     │
│ (상태 전이, 에러, 시간)         │    │                                  │
│                               │    │ "어떻게 실행했는가"                │
│ per-task: 당일 로그에서 1건     │    │ (도구 사용, 파일 수정, 프롬프트)     │
│ daily:   전일 로그 파일 전체     │    │                                  │
└───────────────────────────────┘    └──────────────────────────────────┘
```

### 세션 식별: `[autodev]` 마커 컨벤션

autodev processor가 `claude -p` 실행 시, 첫 프롬프트에 마커를 삽입한다:

```
claude -p "[autodev] fix: resolve login timeout issue in auth module"
```

suggest-workflow는 인덱싱 시 `first_prompt_snippet` (첫 500자)을 저장한다.
이후 `--session-filter`로 autodev 세션만 조회 가능:

```bash
# autodev 세션 목록 조회
suggest-workflow query \
  --perspective filtered-sessions \
  --param prompt_pattern="[autodev]"

# autodev 세션의 도구 사용 패턴
suggest-workflow query \
  --perspective tool-frequency \
  --session-filter "first_prompt_snippet LIKE '[autodev]%'"

# autodev 세션의 파일 수정 이상치
suggest-workflow query \
  --perspective repetition \
  --session-filter "first_prompt_snippet LIKE '[autodev]%'"
```

### Per-task 추출 (done 전이 시)

해당 태스크 1건에 대한 즉시 피드백.

```
done 전이 시
  │
  ▼
knowledge-extractor agent (per-task mode)
  │
  │  1. 당일 daemon.log에서 해당 태스크 로그만 추출
  │     → phase 전이, 소요 시간, 에러
  │
  │  2. suggest-workflow: 해당 세션 1건 분석
  │     suggest-workflow query \
  │       --perspective tool-frequency \
  │       --session-filter "session_id = '<session_id>'"
  │     → 도구 사용 패턴, 이상치 여부
  │
  │  3. 인사이트 종합
  │     "이 이슈에서 Bash:test 12회 반복 → 테스트 전략 개선 필요"
  │
  ▼
KnowledgeSuggestion → 이슈 코멘트로 즉시 피드백
```

### Daily 리포트 (일일 배치 분석)

전일 처리된 전체 태스크를 종합 분석하여 크로스 태스크 패턴을 발견한다.

```
매일 daily_report_hour (기본 06:00)
  │
  ▼
knowledge-extractor agent (daily mode)
  │
  │  ══ 1차: 전일 daemon.log 전체 파싱 ══
  │
  │  daemon.YYYY-MM-DD.log 읽기
  │  → 완료 태스크 N건, 실패 M건, skip K건
  │  → phase별 평균 소요 시간
  │  → 에러 메시지별 빈도
  │  → 리뷰→개선 사이클 반복 횟수
  │
  │  ══ 2차: suggest-workflow 일일 세션 분석 ══
  │
  │  suggest-workflow query \
  │    --perspective filtered-sessions \
  │    --param prompt_pattern="[autodev]"
  │  → 전일 autodev 세션 목록
  │
  │  suggest-workflow query \
  │    --perspective tool-frequency \
  │    --session-filter "first_prompt_snippet LIKE '[autodev]%'"
  │  → 도구 사용 빈도 집계
  │
  │  suggest-workflow query \
  │    --perspective repetition \
  │    --session-filter "first_prompt_snippet LIKE '[autodev]%'"
  │  → 이상치 세션 발견
  │
  │  ══ 3차: 크로스 태스크 패턴 분석 ══
  │
  │  daemon.log + suggest-workflow 교차 조회:
  │  "어제 이슈 5건 중 3건이 src/api/ 수정,
  │   모두 Bash:test 평균 대비 3배 호출.
  │   테스트 실패 → 수정 → 재실행 루프 반복.
  │   → .claude/rules/api-testing.md 추가 제안"
  │
  │  "PR 리뷰 4건 중 3건에서 null check 지적 반복.
  │   → .claude/rules/null-safety.md 추가 제안"
  │
  │  ══ 4차: 일일 리포트 생성 ══
  │
  ▼
┌───────────────────────────────────────┐
│ DailyReport {                         │
│   date: "2026-02-20",                 │
│   summary: {                          │
│     issues_done: 5, prs_done: 4,      │
│     failed: 1, skipped: 2             │
│   },                                  │
│   patterns: [{                        │
│     type: "repeated_failure",         │
│     description: "src/api/ 테스트 루프",│
│     occurrences: 3,                   │
│     suggestion: "..."                 │
│   }],                                 │
│   suggestions: [KnowledgeSuggestion]  │
│ }                                     │
└──────────────┬────────────────────────┘
               │
               ▼
          GitHub 이슈로 일일 리포트 게시
          + KnowledgeSuggestion → PR 생성
```

### 트리거 비교

```
Per-task (즉시)                     Daily (배치)
──────────────                      ──────────────
태스크 1건 완료 시                   매일 daily_report_hour
해당 세션만 분석                     전일 로그 + 세션 전체
즉시 피드백 (이슈 코멘트)            일일 리포트 (GitHub 이슈)
"이 태스크에서 뭘 배웠나"            "오늘 하루 전체에서 뭘 배웠나"

per-task이 놓치는 것:               daily가 발견:
  • 태스크 간 공통 패턴               • 같은 모듈 반복 수정
  • 누적 에러 경향                    • 리뷰 지적사항 패턴
  • 리소스 사용 트렌드                 • 에러 빈도 추세
```

---

## 14. JSON Schemas

### DailyReport (Daily batch)

```json
{
  "date": "2026-02-20",
  "summary": {
    "issues_done": 5,
    "prs_done": 4,
    "failed": 1,
    "skipped": 2,
    "avg_duration_ms": 145000
  },
  "patterns": [
    {
      "type": "repeated_failure | review_cycle | test_loop | hotfile",
      "description": "src/api/ 수정 시 테스트 실패 루프 반복",
      "occurrences": 3,
      "affected_tasks": ["issue:42", "issue:45", "issue:48"]
    }
  ],
  "suggestions": ["<KnowledgeSuggestion 배열>"]
}
```

### KnowledgeSuggestion (Post-completion)

```json
{
  "suggestions": [
    {
      "type": "rule | claude_md | hook | skill | subagent",
      "target_file": ".claude/rules/error-handling.md",
      "content": "에러 핸들링시 반드시 anyhow context 사용",
      "reason": "이번 이슈에서 context 없는 에러로 디버깅에 30분 소요"
    }
  ]
}
```

### AnalysisResult (Issue)

```json
{
  "verdict": "implement | needs_clarification | wontfix",
  "confidence": 0.82,
  "summary": "분석 요약",
  "affected_files": ["src/foo.rs", "src/bar.rs"],
  "implementation_plan": "구현 방향 설명",
  "checkpoints": ["체크포인트1", "체크포인트2"],
  "risks": ["리스크1"],
  "questions": ["API v1 vs v2?", "리팩토링 범위?"]
}
```

### ReviewResult (PR)

```json
{
  "verdict": "approve | request_changes",
  "summary": "리뷰 요약",
  "comments": [
    {
      "path": "src/main.rs",
      "line": 42,
      "body": "null 체크가 필요합니다"
    }
  ]
}
```

---

## 15. 사이드이펙트 & 의존성

### 기존 코드 영향
- **marketplace.json**: 플러그인 1개 추가 (신규)
- **rust-binary.yml**: autodev 빌드 step 추가
- **기존 플러그인 코드 변경 없음**: autodev는 레포 워크트리의 cwd에서 `claude -p`를 실행

### 외부 의존성
- **GitHub Personal Access Token**: Scanner가 GitHub API 호출 시 필요
- **Claude CLI**: `claude` 명령이 PATH에 있어야 함
- **git**: worktree 생성에 필요

### 보안 고려
- GitHub token은 환경변수(`GITHUB_TOKEN`) 또는 `gh auth` 활용
- `claude -p`의 `--dangerously-skip-permissions`는 사용하지 않음
- Dashboard는 TUI (터미널 내) → 네트워크 노출 없음

---

## 16. 구현 우선순위

### Phase 1: 코어 (MVP)
1. Cargo 프로젝트 초기화 + CLI 프레임워크
2. GitHub API 모듈 (라벨 조회/추가/제거)
3. ActiveItems (HashMap)
4. Scanner (라벨 기반 필터링)
5. Workspace manager (git worktree)
6. Session runner (claude -p 실행)
7. Issue processor (분석 → 구현)

### Phase 2: 확장
8. PR processor + Merge processor
9. Recovery (orphan wip 정리)
10. 슬래시 커맨드 (auto-setup, auto, auto-config)
11. 에이전트 파일

### Phase 3: TUI 대시보드
12. ratatui 기본 레이아웃 (active items, labels, logs)
13. daemon.log tail 표시
14. 키바인딩

### Phase 4: 배포
15. CI/CD 통합 (rust-binary.yml)
16. marketplace.json 등록
17. README 문서화

---

## End-to-End

```
┌──────────────────────────────────────────────────────────────────┐
│                        DAEMON STARTUP                             │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ 0. STARTUP RECONCILE (최초 1회)                              │ │
│  │    SQLite에서 enabled 레포 로드                               │ │
│  │    since = cursor - reconcile_window_hours (24h)             │ │
│  │    GitHub API 조회 (bounded)                                 │ │
│  │    라벨 필터: done/skip → skip, wip → 정리 후 적재, 없음 → 적재│ │
│  │    → memory StateQueue[Pending] 복구 완료                     │ │
│  └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                          DAEMON LOOP                              │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ 1. RECOVERY                                                 │ │
│  │    autodev:wip + queue에 없음 → wip 라벨 제거                │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                            │                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ 2. SCAN (cursor 기반 incremental, interval 경과 시만)        │ │
│  │    since=cursor → GitHub API → 신규만 필터                   │ │
│  │    queue.index dedup (O(1)) → 중복 skip                     │ │
│  │    2a. issue/pr: 신규 → wip + queue.push(Pending)           │ │
│  │    2b. merge: approved + 신규 → wip + queue.push(Pending)   │ │
│  │    cursor 전진                                               │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                            │                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ 3. CONSUME (매 tick, 이벤트 드리븐)                          │ │
│  │    queue에서 pop → 처리 → 상태 전이 또는 완료                  │
│  │    pre-flight API 호출 없음 (scan에서 이미 확인)              │ │
│  │                                                             │ │
│  │  Issues:                                                    │ │
│  │    queue[Pending] pop → Analyzing → verdict 분기             │ │
│  │      implement → queue[Ready] → Implementing → PR 생성      │ │
│  │      clarify/wontfix → 댓글 + autodev:skip + queue.remove() │ │
│  │    success → autodev:done + queue.remove()                  │ │
│  │              → [Knowledge Extraction]                       │ │
│  │    failure → 라벨 제거 + queue.remove() (다음 scan에서 재발견) │ │
│  │                                                             │ │
│  │  PRs (리뷰):                                                │ │
│  │    queue[Pending] pop → Reviewing → verdict 분기             │ │
│  │      approve → autodev:done + queue.remove()                │ │
│  │      request_changes → ReviewDone → Improving → Improved    │ │
│  │        → 재리뷰 (Reviewing 반복)                              │ │
│  │    success → autodev:done + queue.remove()                  │ │
│  │              → [Knowledge Extraction]                       │ │
│  │    failure → 라벨 제거 + queue.remove() (재시도)              │ │
│  │                                                             │ │
│  │  Merges (별도 큐):                                           │ │
│  │    queue[Pending] pop → Merging → done | Conflict → 재시도   │ │
│  │    success → autodev:done + queue.remove()                  │ │
│  │                                                             │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                            │                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ 4. KNOWLEDGE EXTRACTION                                     │ │
│  │                                                             │ │
│  │  Per-task (done 전이 시):                                    │ │
│  │    해당 세션 1건 분석 → 즉시 피드백 (이슈 코멘트)               │ │
│  │                                                             │ │
│  │  Daily (매일 daily_report_hour):                             │ │
│  │    전일 daemon.YYYY-MM-DD.log 전체 분석                       │ │
│  │    + suggest-workflow 크로스 세션 분석                         │ │
│  │    → DailyReport (일일 리포트 이슈)                           │ │
│  │    → KnowledgeSuggestion (규칙 제안 PR)                      │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                            │                                      │
│                      sleep(tick)                                   │
│                            │                                      │
│                            └──→ loop                              │
└──────────────────────────────────────────────────────────────────┘
```

---

## Status Transitions

| Type | Phase Flow | 라벨 전이 |
|------|-----------|----------|
| Issue | `Pending → Analyzing → Ready → Implementing → done` | `(없음) → wip → done` |
| Issue | `Pending → Analyzing → skip` (clarify/wontfix) | `(없음) → wip → skip` |
| PR (리뷰) | `Pending → Reviewing → approve → done` | `(없음) → wip → done` |
| PR (리뷰) | `Pending → Reviewing → ReviewDone → Improving → Improved → Reviewing (반복)` | `wip` 유지 |
| Merge | `Pending → Merging → done` | `(없음) → wip → done` |
| Merge | `Pending → Merging → Conflict → Merging (재시도)` | `wip` 유지 |

---

## Summary

| 구성요소 | 역할 |
|---------|------|
| **GitHub 라벨** | 영속 상태 (SSOT) — `autodev:wip`, `autodev:done`, `autodev:skip` |
| **SQLite** | 영속 관리 — repositories, scan_cursors(최적화), consumer_logs(감사) |
| **In-Memory StateQueue** | 상태별 작업 큐 (이벤트 드리븐, 휘발) + dedup index |
| **startup_reconcile()** | 재시작 시 bounded window(24h)로 메모리 큐 복구, 라벨 기반 필터 |
| **recovery()** | 크래시 후 orphan wip 정리 |
| **scan()** | cursor 기반 incremental scan → 신규만 큐 적재 |
| **consume()** | 상태별 큐에서 pop → 처리 → 라벨 전이 (pre-flight API 불필요) |
| **Knowledge Extraction (per-task)** | done 전이 시 해당 세션 분석 → 즉시 피드백 |
| **Knowledge Extraction (daily)** | 매일 전일 로그 전체 분석 → 일일 리포트 + 크로스 태스크 패턴 |
| **daemon.YYYY-MM-DD.log** | 일자별 롤링 로그 (30일 보존) |
