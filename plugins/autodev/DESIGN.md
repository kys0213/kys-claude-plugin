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

- **Monitor 자체는 얇게**: 이벤트 감지, 큐 관리, 세션 실행만 담당
- **분석/구현 품질은 기존 플러그인에 위임**: 플러그인이 진화하면 자동으로 품질 향상
- **레포별 독립 설정**: concurrency, 스캔 주기, 워크플로우 선택 가능
- **단일 바이너리**: Rust 데몬 + TUI 대시보드, 추가 의존성 없음
- **사람과 동일한 환경**: `claude -p`는 워크트리 cwd에서 실행하여 해당 레포의 `.claude/`, `CLAUDE.md`, 설치된 플러그인이 그대로 적용됨. `--plugin-dir` 등 별도 지정 없음. 사람이 직접 레포를 열어 작업하는 것과 100% 동일한 환경에서 동작하여 디버깅 및 이슈 재현이 용이

---

## 2. 플러그인 디렉토리 구조

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
│       ├── client/
│       │   └── mod.rs           # CLI 조회/관리 (SQLite 직접 접근)
│       ├── scanner/
│       │   ├── mod.rs           # GitHub API 스캐너
│       │   ├── issues.rs        # 이슈 감지
│       │   └── pulls.rs         # PR 감지
│       ├── queue/
│       │   ├── mod.rs           # Database 구조체 + re-exports
│       │   ├── schema.rs        # SQLite 스키마 초기화
│       │   ├── models.rs        # 데이터 모델 (입출력 타입 포함)
│       │   └── repository.rs    # Repository trait 정의 + SQLite 구현
│       ├── consumer/
│       │   ├── mod.rs           # Consumer 매니저 (워커 풀)
│       │   ├── issue.rs         # Issue Consumer
│       │   ├── pr.rs            # PR Consumer
│       │   └── merge.rs         # Merge Consumer
│       ├── workspace/
│       │   └── mod.rs           # 워크스페이스 매니저 (git worktree)
│       ├── session/
│       │   ├── mod.rs           # claude -p 세션 실행
│       │   └── output.rs        # 세션 출력 파싱
│       ├── tui/
│       │   ├── mod.rs           # TUI 앱 루프
│       │   ├── views.rs         # 화면 레이아웃 (repos, queues, logs)
│       │   └── events.rs        # 키보드/마우스 이벤트 처리
│       └── config/
│           ├── mod.rs           # 설정 로드/저장
│           └── models.rs        # 설정 모델
│
└── README.md
```

---

## 3. Cargo.toml

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

# TUI
ratatui = "0.29"
crossterm = "0.28"

# Database
rusqlite = { version = "0.32", features = ["bundled"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Utils
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["v4"] }

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

---

## 4. SQLite 스키마

DB 경로: `~/.autodev/autodev.db`

```sql
-- 등록된 레포지토리
CREATE TABLE repositories (
    id          TEXT PRIMARY KEY,          -- UUID
    url         TEXT NOT NULL UNIQUE,      -- https://github.com/org/repo
    name        TEXT NOT NULL,             -- org/repo
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- 레포별 설정
CREATE TABLE repo_configs (
    repo_id             TEXT PRIMARY KEY REFERENCES repositories(id),
    scan_interval_secs  INTEGER NOT NULL DEFAULT 300,
    scan_targets        TEXT NOT NULL DEFAULT '["issues","pulls"]',  -- JSON array
    issue_concurrency   INTEGER NOT NULL DEFAULT 1,
    pr_concurrency      INTEGER NOT NULL DEFAULT 1,
    merge_concurrency   INTEGER NOT NULL DEFAULT 1,
    model               TEXT NOT NULL DEFAULT 'sonnet',
    issue_workflow       TEXT NOT NULL DEFAULT 'multi-llm',  -- multi-llm | single
    pr_workflow          TEXT NOT NULL DEFAULT '/multi-review',
    filter_labels       TEXT DEFAULT NULL,       -- JSON array, NULL = 전체
    ignore_authors      TEXT DEFAULT '["dependabot","renovate"]',  -- JSON array
    workspace_strategy  TEXT NOT NULL DEFAULT 'worktree'  -- worktree | clone
);

-- 스캔 이력 (마지막 스캔 시점 추적)
CREATE TABLE scan_cursors (
    repo_id     TEXT NOT NULL REFERENCES repositories(id),
    target      TEXT NOT NULL,               -- 'issues' | 'pulls'
    last_seen   TEXT NOT NULL,               -- ISO-8601 (마지막으로 본 updated_at)
    last_scan   TEXT NOT NULL,               -- ISO-8601 (마지막 스캔 시각)
    PRIMARY KEY (repo_id, target)
);

-- 이슈 큐
CREATE TABLE issue_queue (
    id              TEXT PRIMARY KEY,
    repo_id         TEXT NOT NULL REFERENCES repositories(id),
    github_number   INTEGER NOT NULL,
    title           TEXT NOT NULL,
    body            TEXT,
    labels          TEXT,                     -- JSON array
    author          TEXT NOT NULL,
    analysis_report TEXT,                     -- 분석 완료 시 리포트
    status          TEXT NOT NULL DEFAULT 'pending',
        -- pending → analyzing → ready → processing → done | failed
    worker_id       TEXT,                     -- 처리 중인 워커 ID
    branch_name     TEXT,                     -- 작업 브랜치
    pr_number       INTEGER,                  -- 생성된 PR 번호
    error_message   TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- PR 큐
CREATE TABLE pr_queue (
    id              TEXT PRIMARY KEY,
    repo_id         TEXT NOT NULL REFERENCES repositories(id),
    github_number   INTEGER NOT NULL,
    title           TEXT NOT NULL,
    body            TEXT,
    author          TEXT NOT NULL,
    head_branch     TEXT NOT NULL,
    base_branch     TEXT NOT NULL,
    review_comment  TEXT,                     -- 리뷰 완료 시 코멘트 내용
    status          TEXT NOT NULL DEFAULT 'pending',
        -- pending → reviewing → review_done → improving → improved → merge_ready | failed
    worker_id       TEXT,
    error_message   TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- 머지 큐
CREATE TABLE merge_queue (
    id              TEXT PRIMARY KEY,
    repo_id         TEXT NOT NULL REFERENCES repositories(id),
    pr_number       INTEGER NOT NULL,
    title           TEXT NOT NULL,
    head_branch     TEXT NOT NULL,
    base_branch     TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
        -- pending → merging → done | conflict | failed
    conflict_files  TEXT,                     -- JSON array (충돌 파일 목록)
    worker_id       TEXT,
    error_message   TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- Consumer 실행 로그
CREATE TABLE consumer_logs (
    id          TEXT PRIMARY KEY,
    repo_id     TEXT NOT NULL REFERENCES repositories(id),
    queue_type  TEXT NOT NULL,               -- 'issue' | 'pr' | 'merge'
    queue_item_id TEXT NOT NULL,
    worker_id   TEXT NOT NULL,
    command     TEXT NOT NULL,               -- 실행한 claude -p 커맨드
    stdout      TEXT,
    stderr      TEXT,
    exit_code   INTEGER,
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    duration_ms INTEGER
);

-- 인덱스
CREATE INDEX idx_issue_queue_status ON issue_queue(repo_id, status);
CREATE INDEX idx_pr_queue_status ON pr_queue(repo_id, status);
CREATE INDEX idx_merge_queue_status ON merge_queue(repo_id, status);
CREATE INDEX idx_consumer_logs_repo ON consumer_logs(repo_id, started_at);
```

---

## 5. CLI 아키텍처 (단일 바이너리, daemon + CLI)

### 동작 모델

```
단일 바이너리 `autodev` 가 daemon 모드와 CLI 모드를 모두 수행:

  autodev start     → daemon 모드 (단일 인스턴스, 포그라운드)
  autodev stop      → PID 파일로 SIGTERM 전송
  autodev status    → SQLite 직접 읽기
  autodev dashboard → SQLite 직접 읽기 + TUI 표시
```

### 공유 상태 (소켓 없음)

```
~/.autodev/
├── autodev.db       # SQLite WAL (daemon + CLI 공유, 동시 읽기 허용)
└── daemon.pid          # PID 파일 (단일 인스턴스 보장 + stop 시 kill)
```

- **모든 조회 (status, queue, logs, dashboard)**: SQLite 직접 읽기
- **모든 데이터 변경 (repo add/remove, config, retry, clear)**: SQLite 직접 쓰기
- **데몬 중지 (stop)**: PID 파일 읽기 → `kill <pid>` (SIGTERM)
- **소켓 불필요**: SQLite WAL 모드가 reader/writer 동시 접근을 보장

### 서브커맨드

```
# 데몬 제어
autodev start              # 데몬 시작 (포그라운드, 단일 인스턴스)
autodev stop               # 데몬 중지 (PID → SIGTERM)
autodev restart             # 데몬 재시작

# 상태 조회 (→ SQLite 직접 읽기)
autodev status             # 데몬 상태 + 전체 레포 요약
autodev dashboard          # TUI 대시보드

# 레포 관리 (→ SQLite 직접 쓰기)
autodev repo add <url>     # 레포 등록 (대화형 설정)
autodev repo list          # 등록된 레포 목록
autodev repo config <name> # 레포 설정 변경
autodev repo remove <name> # 레포 제거

# 큐 관리 (→ SQLite 직접 읽기/쓰기)
autodev queue list <repo>  # 큐 상태 확인
autodev queue retry <id>   # 실패 항목 재시도
autodev queue clear <repo> # 큐 비우기

# 로그 조회 (→ SQLite 직접 읽기)
autodev logs <repo>        # 실행 로그 조회
```

### 셸 환경 등록 (/auto-setup 시 자동)

`/auto-setup` 실행 시 셸 프로필에 환경변수 + alias 등록:

```bash
# ~/.bashrc 또는 ~/.zshrc에 추가
export AUTONOMOUS_HOME="$HOME/.autodev"
export PATH="$HOME/.local/bin:$PATH"

# 단축 명령어
alias auto="autodev"
alias auto-s="autodev status"
alias auto-d="autodev dashboard"
alias auto-q="autodev queue list"
```

등록 후 터미널에서 바로 사용 가능:
```bash
auto-s                    # 상태 확인
auto-d                    # TUI 대시보드
auto-q org/my-repo        # 큐 확인
auto start                # 데몬 시작
auto stop                 # 데몬 중지
```

---

## 5.1. Repository 패턴

DB 접근 로직을 trait으로 추상화하여 consumer/scanner/client에서 raw SQL을 직접 사용하지 않도록 분리.

### trait 정의 (`queue/repository.rs`)

```rust
/// 레포지토리 관리
pub trait RepoRepository {
    fn add(&self, url: &str, name: &str, config: &RepoConfig) -> Result<String>;
    fn remove(&self, name: &str) -> Result<()>;
    fn list_with_config(&self) -> Result<Vec<RepoWithConfig>>;
    fn find_enabled_with_config(&self) -> Result<Vec<EnabledRepo>>;
    fn update_config(&self, name: &str, config: &RepoConfig) -> Result<()>;
    fn get_config(&self, name: &str) -> Result<String>;
    fn count(&self) -> Result<i64>;
    fn status_summary(&self) -> Result<Vec<RepoStatusRow>>;
}

/// 이슈 큐
pub trait IssueQueueRepository {
    fn insert(&self, item: &NewIssueItem) -> Result<String>;
    fn exists(&self, repo_id: &str, github_number: i64) -> Result<bool>;
    fn find_pending(&self, limit: u32) -> Result<Vec<PendingIssue>>;
    fn update_status(&self, id: &str, status: &str, fields: StatusFields) -> Result<()>;
    fn mark_failed(&self, id: &str, error: &str) -> Result<()>;
    fn count_active(&self) -> Result<i64>;
}

/// PR 큐
pub trait PrQueueRepository {
    fn insert(&self, item: &NewPrItem) -> Result<String>;
    fn exists(&self, repo_id: &str, github_number: i64) -> Result<bool>;
    fn find_pending(&self, limit: u32) -> Result<Vec<PendingPr>>;
    fn update_status(&self, id: &str, status: &str, fields: StatusFields) -> Result<()>;
    fn mark_failed(&self, id: &str, error: &str) -> Result<()>;
    fn count_active(&self) -> Result<i64>;
}

/// 머지 큐
pub trait MergeQueueRepository {
    fn insert(&self, item: &NewMergeItem) -> Result<String>;
    fn find_pending(&self, limit: u32) -> Result<Vec<PendingMerge>>;
    fn update_status(&self, id: &str, status: &str, fields: StatusFields) -> Result<()>;
    fn mark_failed(&self, id: &str, error: &str) -> Result<()>;
    fn count_active(&self) -> Result<i64>;
}

/// 스캔 커서
pub trait ScanCursorRepository {
    fn get_last_seen(&self, repo_id: &str, target: &str) -> Result<Option<String>>;
    fn upsert(&self, repo_id: &str, target: &str, last_seen: &str) -> Result<()>;
    fn should_scan(&self, repo_id: &str, interval_secs: i64) -> Result<bool>;
}

/// Consumer 실행 로그
pub trait ConsumerLogRepository {
    fn insert(&self, log: &NewConsumerLog) -> Result<()>;
    fn recent(&self, repo: Option<&str>, limit: usize) -> Result<Vec<LogEntry>>;
}
```

### 설계 원칙

- **Database 구조체가 모든 trait을 구현** (`impl RepoRepository for Database { ... }`)
- **Consumer/Scanner/Client는 trait 메서드만 호출** → SQL 로직에 의존하지 않음
- **입력용/출력용 모델 분리**: `NewIssueItem` (insert용) vs `PendingIssue` (query용) vs `IssueQueueItem` (full model)
- **in-memory SQLite (`:memory:`)로 블랙박스 테스트 가능** → mock 불필요

---

## 5.2. 테스트 전략 (블랙박스)

in-memory SQLite DB를 사용하여 repository 레이어를 블랙박스 테스트.

### 테스트 카테고리

| 카테고리 | 시나리오 |
|---------|---------|
| **레포 CRUD** | 등록/중복URL/이름추출/삭제/설정변경 |
| **이슈 큐** | 삽입/중복감지/상태전이(pending→analyzing→ready→done)/실패처리 |
| **PR 큐** | 삽입/중복감지/상태전이(pending→reviewing→review_done)/실패처리 |
| **머지 큐** | 삽입/상태전이(pending→merging→done\|conflict)/충돌→재시도 |
| **스캔 커서** | 초기스캔(이력없음)/간격미달시skip/간격초과시scan/커서업데이트 |
| **Consumer 로그** | 삽입/조회(레포필터)/limit제한/시간순정렬 |
| **경계값** | 빈큐에서find_pending/limit=0/초장문title/NULL body/음수github_number |
| **동시성** | count_active 정확도/pending+active 혼재 시 조회 |
| **재시도** | failed→pending 전환/done항목은retry불가/존재하지않는ID |
| **큐 비우기** | done+failed만 삭제/pending+active는 유지 |

### 테스트 파일 위치

```
cli/tests/
└── repository_tests.rs    # 통합 테스트 (in-memory SQLite)
```

---

## 6. 슬래시 커맨드 설계

### /auto-setup (위자드)

> **이 플러그인은 반드시 User Scope로 설치**해야 합니다.
> 사용자가 모니터링하려는 레포 디렉토리에서 `/auto-setup`을 호출하면 위자드가 시작됩니다.

```yaml
---
description: 현재 레포를 자율 개발 모니터링 대상으로 등록합니다
allowed-tools: ["AskUserQuestion", "Bash"]
---
```

**흐름:**
1. `git remote get-url origin`으로 현재 레포 URL 자동 감지
2. **필수 플러그인 의존성 체크** → `kys-claude-plugin` 마켓플레이스에서 User Scope로 설치 여부 확인
   - 필수: `develop-workflow`, `git-utils`
   - 권장: `external-llm` (multi-LLM 사용 시)
   - 미설치 시 경고 + 설치 안내, 계속 진행 여부 확인
3. AskUserQuestion → 감시 대상 (Issues / PRs / 둘 다)
4. AskUserQuestion → 스캔 주기 (1분/5분/15분/커스텀)
5. AskUserQuestion → Consumer 처리량 (Issue/PR/Merge 각각)
6. AskUserQuestion → 워크플로우 선택 (Issue 분석: multi-LLM or 단일, PR 리뷰: /multi-review or 단일)
7. AskUserQuestion → 필터 (전체 / 특정 라벨 / 작성자 제외)
8. Bash → `autodev repo add <url> --config '<json>'`
9. 설정 요약 출력

### /auto

```yaml
---
description: 자율 개발 데몬 제어 - 시작, 중지, 상태 확인
argument-hint: "[start|stop|status]"
allowed-tools: ["Bash"]
---
```

**흐름:**
- 인자 없음 → 현재 상태 요약 + 시작/중지 제안
- `start` → `autodev start`
- `stop` → `autodev stop`
- `status` → `autodev status` (각 레포별 큐 현황)

### /auto-config

```yaml
---
description: 등록된 레포의 자율 모니터링 설정을 변경합니다
argument-hint: "[repo-name]"
allowed-tools: ["AskUserQuestion", "Bash"]
---
```

### /auto-dashboard

```yaml
---
description: 자율 개발 TUI 대시보드를 터미널에서 엽니다
allowed-tools: ["Bash"]
---
```

**흐름:**
1. `autodev dashboard` (TUI 실행)

---

## 7. 에이전트 설계

### issue-analyzer.md

```yaml
---
description: (internal) Issue Consumer가 호출 - Multi-LLM 병렬 분석으로 이슈 리포트 생성
model: sonnet
tools: ["Read", "Glob", "Grep", "Bash", "Task"]
---
```

역할: Claude + Codex + Gemini를 병렬 호출하여 이슈를 다각도로 분석
- Claude: `claude -p`로 코드베이스 기반 분석
- Codex: `common/scripts/call-codex.sh`로 병렬 분석
- Gemini: `common/scripts/call-gemini.sh`로 병렬 분석
- 3개 결과를 종합하여 구조화된 리포트 생성 (공통 의견, 상충 의견, 영향 범위, 체크포인트)

### pr-reviewer.md

```yaml
---
description: (internal) PR Consumer가 호출 - PR 코드 리뷰 수행 (multi-LLM 병렬)
model: sonnet
tools: ["Read", "Glob", "Grep", "Bash", "Task"]
---
```

역할: `/multi-review` 호출을 통해 Sonnet + Codex + Gemini 병렬 리뷰 수행

### conflict-resolver.md

```yaml
---
description: (internal) Merge Consumer가 호출 - 머지 충돌을 분석하고 해결
model: opus
tools: ["Read", "Glob", "Grep", "Edit", "Bash"]
---
```

역할: conflict 파일 분석, 양측 의도 파악, 자동 해결

---

## 8. Consumer 상세 흐름

### Issue Consumer

```
[issue_queue: pending] → 가져오기
  │
  ├── 1단계: Multi-LLM 병렬 분석 (status: analyzing)
  │   워크트리: ~/.autodev/workspaces/<repo>/issue-<num>
  │   병렬 실행:
  │     ├── Claude:  claude -p "Analyze issue #<num>: <title>\n<body>" \
  │     │            --output-format json
  │     ├── Codex:   common/scripts/call-codex.sh <분석 프롬프트>
  │     └── Gemini:  common/scripts/call-gemini.sh <분석 프롬프트>
  │   결과: 3개 분석 결과를 종합하여 analysis_report에 저장
  │     - 공통 의견 → 높은 확신도
  │     - 상충 의견 → 플래그 표시
  │     - 영향 범위, 구현 방향, 체크포인트 구조화
  │   → status: ready
  │
  ├── 2단계: 구현 (status: processing)
  │   실행: cd <worktree> && claude -p \
  │         "/develop implement based on analysis: <report>"
  │   → 레포에 설치된 플러그인이 cwd 기반으로 자동 적용
  │   결과: PR 생성됨 → pr_number 저장
  │   → status: done
  │
  └── 실패 시: status: failed, error_message 기록
```

### PR Consumer

```
[pr_queue: pending] → 가져오기
  │
  ├── 1단계: 리뷰 (status: reviewing)
  │   워크트리: ~/.autodev/workspaces/<repo>/pr-<num>
  │   실행: cd <worktree> && git checkout <head_branch>
  │   실행: claude -p "/multi-review" --output-format json
  │   → 레포에 설치된 플러그인이 cwd 기반으로 자동 적용
  │   결과: GitHub에 리뷰 코멘트 게시, review_comment 저장
  │   → status: review_done
  │
  ├── 2단계: 개선 (status: improving)
  │   실행: claude -p "/develop implement review feedback: <review_comment>"
  │   → status: improved
  │
  ├── 3단계: 재리뷰 → 머지 판단
  │   실행: claude -p "/multi-review" (2차 리뷰)
  │   판단: approve → merge_queue에 삽입, status: merge_ready
  │         request_changes → status: reviewing (반복)
  │
  └── 실패 시: status: failed
```

### Merge Consumer

```
[merge_queue: pending] → 가져오기
  │
  ├── 실행 (status: merging)
  │   실행: cd <worktree> && claude -p "/merge-pr <pr_number>"
  │
  ├── 충돌 시 (status: conflict)
  │   실행: claude -p "Resolve merge conflicts for PR #<num>"
  │   → 해결 후 재시도
  │
  ├── 성공: status: done
  └── 실패: status: failed
```

---

## 9. TUI 대시보드

`autodev dashboard` 실행 시 ratatui 기반 터미널 UI 표시.

### 키바인딩
```
Tab       - 패널 전환 (Repos → Queues → Logs)
j/k       - 목록 상/하 이동
Enter     - 상세 보기
r         - 실패 항목 재시도
q         - 종료
?         - 도움말
```

### 레이아웃
```
┌─────────────────────────────────────────────────────────┐
│  autodev v0.1.0          ● daemon running    [?]help │
├──────────┬──────────────────────────────────────────────┤
│          │                                              │
│ Repos    │  Queues: org/repo-a              ● enabled   │
│          │  ┌─────────────────────────────────────┐     │
│ > repo-a │  │ Issues  ██░░░  3 pending  1 active  │     │
│   repo-b │  │ PRs     █░░░░  2 pending  0 active  │     │
│          │  │ Merges  ░░░░░  1 pending             │     │
│          │  └─────────────────────────────────────┘     │
│          │  Scan: 5m | Issue×2 PR×1 Merge×1 | sonnet   │
│          │                                              │
│          ├──────────────────────────────────────────────┤
│          │  Activity Log (실시간)                        │
│          │  14:32 issue-42  analyzing → ready            │
│          │  14:30 pr-15    reviewing                     │
│          │  14:28 pr-12    merged ✓                      │
│          │  14:25 issue-41 done ✓ → PR #18               │
│          │  14:20 issue-40 failed ✗ "timeout"            │
│          │                                              │
└──────────┴──────────────────────────────────────────────┘
```

### 장점 (vs 웹 대시보드)
- 별도 서버/포트 불필요
- 바이너리 크기 최소화 (axum, tower-http, HTML/JS/CSS 제거)
- 터미널 환경에서 바로 확인 가능
- SQLite에서 직접 읽어 표시 (데몬 프로세스와 DB 공유)

---

## 11. 설치 & 배포

### 로컬 개발
```bash
cd plugins/autodev/cli
cargo build --release
cp target/release/autodev ~/.local/bin/
```

### 플러그인 설치
```bash
# marketplace에서 설치
/plugin install autodev@kys-claude-plugin

# 바이너리는 /auto-setup 실행 시 자동 빌드 or GitHub Release에서 다운로드
```

### CI/CD (rust-binary.yml 확장)
```yaml
# 기존 suggest-workflow 빌드에 autodev 추가
- name: Build autodev
  run: |
    cd plugins/autodev/cli
    cargo build --release
    tar czf autodev-${{ matrix.target }}.tar.gz autodev
```

### marketplace.json 추가
```json
{
  "name": "autodev",
  "version": "0.1.0",
  "source": "./plugins/autodev",
  "category": "automation",
  "description": "이벤트 기반 자율 개발 오케스트레이터 - Issue 분석, PR 리뷰, 자동 머지",
  "keywords": ["autodev", "monitor", "queue", "dashboard", "automation"]
}
```

### 의존성 체크 (/auto-setup 커맨드 내 LLM 지침)

`/auto-setup` 커맨드에서 LLM이 마켓플레이스명 + 플러그인명으로 설치 여부를 확인:

```markdown
## 의존성 검증 (Step 2)

다음 플러그인이 `kys-claude-plugin` 마켓플레이스에서 User Scope로 설치되어 있는지 확인하세요:

| 구분 | 플러그인 | 마켓플레이스 |
|------|---------|-------------|
| 필수 | `develop-workflow` | `kys-claude-plugin` |
| 필수 | `git-utils` | `kys-claude-plugin` |
| 권장 | `external-llm` | `kys-claude-plugin` |

미설치 시 안내:
- 필수 → 경고 + `/plugin install <name>@kys-claude-plugin`
- 권장 → multi-LLM 분석이 Claude 단일 모델로 fallback됨을 안내
```

---

## 12. 사이드이펙트 & 의존성

### 기존 코드 영향
- **marketplace.json**: 플러그인 1개 추가 (신규)
- **rust-binary.yml**: autodev 빌드 step 추가
- **validate.yml**: autodev 경로 추가 (자동 감지될 수 있음)
- **기존 플러그인 코드 변경 없음**: autodev는 레포 워크트리의 cwd에서 `claude -p`를 실행하여 기존 커맨드를 호출 (레포에 설치된 플러그인이 자동 적용)

### 외부 의존성
- **GitHub Personal Access Token**: Scanner가 GitHub API 호출 시 필요
- **Claude CLI**: `claude` 명령이 PATH에 있어야 함
- **git**: worktree 생성에 필요
- **디스크 공간**: 레포별 worktree (base clone + worktree per task)

### 보안 고려
- GitHub token은 환경변수(`GITHUB_TOKEN`) 또는 `gh auth` 활용
- `claude -p`의 `--dangerously-skip-permissions`는 사용하지 않음
- Dashboard는 TUI (터미널 내) → 네트워크 노출 없음

---

## 13. 구현 우선순위

### Phase 1: 코어 (MVP)
1. Cargo 프로젝트 초기화 + CLI 프레임워크
2. SQLite 스키마 + 큐 CRUD
3. Scanner (GitHub API polling)
4. Workspace manager (git worktree)
5. Session runner (claude -p 실행)
6. Issue Consumer (단일 워커)

### Phase 2: 확장
7. PR Consumer + Merge Consumer
8. Consumer 워커 풀 (concurrency 설정)
9. 슬래시 커맨드 (auto-setup, auto, auto-config)
10. 에이전트 파일

### Phase 3: TUI 대시보드
11. ratatui 기본 레이아웃 (repos, queues, logs 패널)
12. 실시간 로그 스트리밍 (SQLite polling)
13. 키바인딩 (탐색, 재시도, 상세 보기)

### Phase 4: 배포
14. CI/CD 통합 (rust-binary.yml)
15. marketplace.json 등록
16. README 문서화
