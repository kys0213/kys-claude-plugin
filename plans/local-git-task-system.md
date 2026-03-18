# Local Git Task System - 설계 문서

## 개요

autodev의 GitHub 의존성을 추상화하여 **로컬 Git + SQLite**로도 동일한 워크플로우를 수행할 수 있게 만든다.
GitHub과 로컬 모드를 repo별로 선택 가능하게 하여, 폐쇄망 환경이나 비개발 문서 작업에서도 autonomous 워크플로우를 사용할 수 있도록 한다.

### 대상 유스케이스

- 대규모 RFP 문서 작성/개선
- 규격화된 장애보고서 자동 생성/검토
- 정부기관 대응 문서 관리
- Spec 기반 반복 개선이 필요한 모든 문서/코드 작업
- 여러 머신에 걸친 수평 확장 (bare git repo 공유)

## 아키텍처

### 현재 구조의 문제점

```
Task → Gh trait → RealGh (gh CLI) → GitHub API
                                      ↑ 유일한 플랫폼
```

- `Gh` trait이 GitHub REST API에 특화 (`api_get_field`, `api_paginate`)
- `RepoIssue`, `RepoPull`이 GitHub JSON 응답에서 직접 파싱
- Label 기반 상태 관리가 GitHub에 종속

### 변경 후 구조

```
Task → Platform trait ─┬→ GitHubPlatform (기존 Gh 래핑)
                       └→ LocalPlatform  (SQLite + bare git)
```

## Phase 1: Platform Trait 정의

### 1.1 Platform Trait

`Gh` trait의 9개 메서드를 **도메인 중심**으로 재설계한다.

```rust
// core/platform.rs

#[async_trait]
pub trait Platform: Send + Sync {
    // === Issue 관리 ===
    async fn list_issues(
        &self, repo: &str, filter: &IssueFilter,
    ) -> Result<Vec<PlatformIssue>>;

    async fn get_issue_state(
        &self, repo: &str, number: i64,
    ) -> Option<ItemState>;  // Open | Closed

    async fn create_issue(
        &self, repo: &str, title: &str, body: &str,
    ) -> Option<i64>;  // Returns issue number

    async fn comment(
        &self, repo: &str, number: i64, body: &str,
    ) -> bool;

    // === Label 관리 ===
    async fn label_add(
        &self, repo: &str, number: i64, label: &str,
    ) -> bool;

    async fn label_remove(
        &self, repo: &str, number: i64, label: &str,
    ) -> bool;

    // === PR 관리 ===
    async fn list_pulls(
        &self, repo: &str, filter: &PullFilter,
    ) -> Result<Vec<PlatformPull>>;

    async fn create_pr(
        &self, repo: &str, head: &str, base: &str,
        title: &str, body: &str,
    ) -> Option<i64>;

    async fn pr_review(
        &self, repo: &str, number: i64,
        event: ReviewEvent, body: &str,
    ) -> bool;

    // === 플랫폼 식별 ===
    fn platform_type(&self) -> PlatformType;  // GitHub | Local
}
```

### 1.2 도메인 모델

기존 `RepoIssue`, `RepoPull`은 `from_json()`으로 GitHub에 결합되어 있다.
Platform에서 반환하는 통합 모델을 정의한다.

```rust
// core/platform.rs

pub struct PlatformIssue {
    pub number: i64,
    pub title: String,
    pub body: Option<String>,
    pub author: String,
    pub labels: Vec<String>,
    pub state: ItemState,
}

pub struct PlatformPull {
    pub number: i64,
    pub title: String,
    pub body: Option<String>,
    pub author: String,
    pub labels: Vec<String>,
    pub head_branch: String,
    pub base_branch: String,
    pub state: ItemState,
}

pub enum ItemState { Open, Closed, Merged }
pub enum ReviewEvent { Approve, RequestChanges, Comment }
pub enum PlatformType { GitHub, Local }

pub struct IssueFilter {
    pub labels: Option<Vec<String>>,
    pub state: Option<ItemState>,
}

pub struct PullFilter {
    pub state: Option<ItemState>,
    pub head_branch: Option<String>,
}
```

### 1.3 기존 코드와의 호환

- `RepoIssue` → `impl From<PlatformIssue> for RepoIssue`
- `RepoPull` → `impl From<PlatformPull> for RepoPull`
- 기존 `Gh` trait은 유지, `GitHubPlatform`이 내부에서 래핑

## Phase 2: GitHubPlatform 구현

기존 `Gh` trait을 래핑하여 `Platform` trait을 구현한다.

```rust
// infra/platform/github.rs

pub struct GitHubPlatform {
    gh: Arc<dyn Gh>,
    host: Option<String>,
}

#[async_trait]
impl Platform for GitHubPlatform {
    async fn list_issues(&self, repo: &str, filter: &IssueFilter) -> Result<Vec<PlatformIssue>> {
        // 기존 api_paginate 래핑
        let endpoint = "issues";
        let mut params = vec![("state", "open")];
        if let Some(labels) = &filter.labels {
            params.push(("labels", &labels.join(",")));
        }
        let bytes = self.gh.api_paginate(repo, endpoint, &params, self.host()).await?;
        // JSON → PlatformIssue 변환
    }

    async fn get_issue_state(&self, repo: &str, number: i64) -> Option<ItemState> {
        // 기존 api_get_field(.state) 래핑
        let path = format!("issues/{number}");
        let state = self.gh.api_get_field(repo, &path, ".state", self.host()).await?;
        Some(match state.as_str() {
            "open" | "OPEN" => ItemState::Open,
            _ => ItemState::Closed,
        })
    }

    // ... 나머지 메서드도 기존 Gh 메서드 래핑
}
```

**변경 범위**: Task 코드에서 `self.gh` → `self.platform`으로 교체.
메서드 시그니처가 더 단순해지므로 (host 파라미터 제거) 코드가 깔끔해진다.

## Phase 3: LocalPlatform 구현 (핵심)

### 3.1 로컬 DB 스키마 확장

```sql
-- local_issues: GitHub Issue 대체
CREATE TABLE IF NOT EXISTS local_issues (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id TEXT NOT NULL REFERENCES repositories(id),
    number INTEGER NOT NULL,  -- repo 내 순번
    title TEXT NOT NULL,
    body TEXT,
    author TEXT NOT NULL DEFAULT 'system',
    state TEXT NOT NULL DEFAULT 'open',  -- open | closed
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(repo_id, number)
);

-- local_labels: Label 관리
CREATE TABLE IF NOT EXISTS local_labels (
    repo_id TEXT NOT NULL,
    item_number INTEGER NOT NULL,
    item_type TEXT NOT NULL,  -- issue | pr
    label TEXT NOT NULL,
    PRIMARY KEY (repo_id, item_number, item_type, label)
);

-- local_pulls: GitHub PR 대체
CREATE TABLE IF NOT EXISTS local_pulls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id TEXT NOT NULL REFERENCES repositories(id),
    number INTEGER NOT NULL,
    title TEXT NOT NULL,
    body TEXT,
    author TEXT NOT NULL DEFAULT 'system',
    head_branch TEXT NOT NULL,
    base_branch TEXT NOT NULL DEFAULT 'main',
    state TEXT NOT NULL DEFAULT 'open',  -- open | closed | merged
    source_issue_number INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(repo_id, number)
);

-- local_comments: Issue/PR 코멘트
CREATE TABLE IF NOT EXISTS local_comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id TEXT NOT NULL,
    item_number INTEGER NOT NULL,
    item_type TEXT NOT NULL,  -- issue | pr
    author TEXT NOT NULL DEFAULT 'system',
    body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- local_reviews: PR 리뷰
CREATE TABLE IF NOT EXISTS local_reviews (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id TEXT NOT NULL,
    pr_number INTEGER NOT NULL,
    event TEXT NOT NULL,  -- APPROVE | REQUEST_CHANGES | COMMENT
    body TEXT NOT NULL,
    author TEXT NOT NULL DEFAULT 'system',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- local_number_seq: issue/pr 번호 시퀀스
CREATE TABLE IF NOT EXISTS local_number_seq (
    repo_id TEXT NOT NULL,
    item_type TEXT NOT NULL,  -- issue | pr
    next_number INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (repo_id, item_type)
);
```

### 3.2 LocalPlatform 구현

```rust
// infra/platform/local.rs

pub struct LocalPlatform {
    db: Database,
}

#[async_trait]
impl Platform for LocalPlatform {
    async fn create_issue(&self, repo: &str, title: &str, body: &str) -> Option<i64> {
        let repo_id = self.resolve_repo_id(repo)?;
        let number = self.next_number(&repo_id, "issue")?;
        self.db.conn().execute(
            "INSERT INTO local_issues (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
            params![repo_id, number, title, body],
        ).ok()?;
        Some(number)
    }

    async fn label_add(&self, repo: &str, number: i64, label: &str) -> bool {
        let repo_id = match self.resolve_repo_id(repo) {
            Some(id) => id,
            None => return false,
        };
        self.db.conn().execute(
            "INSERT OR IGNORE INTO local_labels (repo_id, item_number, item_type, label)
             VALUES (?1, ?2, ?3, ?4)",
            params![repo_id, number, self.detect_item_type(&repo_id, number), label],
        ).is_ok()
    }

    async fn create_pr(&self, repo: &str, head: &str, base: &str,
                       title: &str, body: &str) -> Option<i64> {
        let repo_id = self.resolve_repo_id(repo)?;
        let number = self.next_number(&repo_id, "pr")?;
        self.db.conn().execute(
            "INSERT INTO local_pulls (repo_id, number, title, body, head_branch, base_branch)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![repo_id, number, title, body, head, base],
        ).ok()?;
        Some(number)
    }

    async fn pr_review(&self, repo: &str, number: i64,
                       event: ReviewEvent, body: &str) -> bool {
        let repo_id = match self.resolve_repo_id(repo) {
            Some(id) => id,
            None => return false,
        };
        let event_str = match event {
            ReviewEvent::Approve => "APPROVE",
            ReviewEvent::RequestChanges => "REQUEST_CHANGES",
            ReviewEvent::Comment => "COMMENT",
        };

        // PR 리뷰 저장
        let ok = self.db.conn().execute(
            "INSERT INTO local_reviews (repo_id, pr_number, event, body) VALUES (?1, ?2, ?3, ?4)",
            params![repo_id, number, event_str, body],
        ).is_ok();

        // APPROVE면 자동 merge 가능 (config에 따라)
        if matches!(event, ReviewEvent::Approve) {
            self.try_auto_merge(&repo_id, number).await;
        }

        ok
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Local
    }
}
```

### 3.3 로컬 PR Merge

GitHub에서는 웹 UI나 API로 merge하지만, 로컬에서는 `git merge`를 직접 수행한다.

```rust
impl LocalPlatform {
    async fn try_auto_merge(&self, repo_id: &str, pr_number: i64) -> bool {
        let pr = self.get_pull(repo_id, pr_number);
        if pr.is_none() { return false; }
        let pr = pr.unwrap();

        // git merge: head_branch → base_branch
        let repo_path = self.workspace_path(repo_id);
        let result = Command::new("git")
            .args(["merge", "--no-ff", &pr.head_branch, "-m",
                   &format!("Merge PR #{}: {}", pr_number, pr.title)])
            .current_dir(&repo_path)
            .output().await;

        match result {
            Ok(output) if output.status.success() => {
                // PR 상태를 merged로 변경
                self.db.conn().execute(
                    "UPDATE local_pulls SET state = 'merged', updated_at = datetime('now')
                     WHERE repo_id = ?1 AND number = ?2",
                    params![repo_id, pr_number],
                ).ok();

                // worktree 정리
                let _ = Command::new("git")
                    .args(["branch", "-d", &pr.head_branch])
                    .current_dir(&repo_path)
                    .output().await;

                true
            }
            _ => false, // conflict 발생 시 수동 해결 필요
        }
    }
}
```

## Phase 4: LocalTaskSource 구현

기존 `GitHubTaskSource`에 대응하는 `LocalTaskSource`.
GitHub API 스캔 대신 SQLite를 직접 쿼리한다.

```rust
// service/daemon/collectors/local.rs

pub struct LocalTaskSource<DB> {
    platform: Arc<LocalPlatform>,
    workspace: Arc<dyn WorkspaceOps>,
    config: Arc<dyn ConfigLoader>,
    git: Arc<dyn Git>,
    db: DB,
    repos: HashMap<String, GitRepository>,
}

#[async_trait(?Send)]
impl<DB: RepoRepository + ScanCursorRepository + QueueRepository> Collector for LocalTaskSource<DB> {
    async fn poll(&mut self) -> Vec<Box<dyn Task>> {
        let mut tasks = vec![];

        for (repo_id, git_repo) in &self.repos {
            // 1. open issues with autodev:analyze label → AnalyzeTask
            let issues = self.platform.list_issues(
                &git_repo.name,
                &IssueFilter { labels: Some(vec!["autodev:analyze".into()]), state: Some(ItemState::Open) },
            ).await.unwrap_or_default();

            for issue in issues {
                // QueueItem 생성 → Task 변환 (기존 로직과 동일)
            }

            // 2. open PRs → ReviewTask
            let pulls = self.platform.list_pulls(
                &git_repo.name,
                &PullFilter { state: Some(ItemState::Open), head_branch: None },
            ).await.unwrap_or_default();

            // ... 기존 스캔 로직과 동일한 패턴
        }

        tasks
    }
}
```

## Phase 5: Repo Config 확장

### 5.1 Config 모델

```rust
// core/config/models.rs

#[derive(Deserialize, Clone)]
pub struct RepoConfig {
    pub platform: PlatformKind,  // 추가
    pub sources: SourcesConfig,
    // ... 기존 필드
}

#[derive(Deserialize, Clone, Default)]
pub enum PlatformKind {
    #[default]
    GitHub,
    Local,
}
```

### 5.2 Repo 등록 CLI

```bash
# GitHub repo (기존)
autodev repo add https://github.com/org/repo

# Local repo (신규)
autodev repo add --local /path/to/git/repo
autodev repo add --local --init /path/to/new/repo  # git init 포함
```

### 5.3 Daemon 초기화 분기

```rust
// service/daemon/mod.rs - start()

for repo in enabled_repos {
    match repo.platform_kind() {
        PlatformKind::GitHub => {
            // 기존 GitHubTaskSource에 추가
            github_source.add_repo(repo);
        }
        PlatformKind::Local => {
            // LocalTaskSource에 추가
            local_source.add_repo(repo);
        }
    }
}

// 두 source를 모두 TaskManager에 등록
let collectors: Vec<Box<dyn Collector>> = vec![
    Box::new(github_source),
    Box::new(local_source),
];
```

## Phase 6: 수평 확장 (Sync)

### 6.1 Git 기반 동기화

각 머신이 독립적으로 운영되되, bare git repo를 통해 작업물을 공유한다.

```
Machine A                    Shared Bare Repo              Machine B
  local git ──push/fetch──→  /shared/repo.git  ←──push/fetch── local git
  SQLite A                                                   SQLite B
```

### 6.2 SQLite 상태 동기화 방안

**Option A: Git-tracked 상태 파일**
```
.autodev/
  state.json        ← git add로 추적
  specs/             ← spec 정의 파일
  cron/              ← cron 정의 파일
```

머신간 `git pull`로 spec/cron 정의를 공유하고, 각 머신이 자체 SQLite에 import.

**Option B: Litestream 복제 (고급)**
- SQLite WAL을 S3/NFS로 실시간 복제
- 읽기 전용 복제본으로 다른 머신에서 상태 조회

**권장: Option A** (git-native, 추가 인프라 불필요)

### 6.3 Spec 동기화 플로우

```
1. Machine A: autodev spec add --title "Q1 보고서"
   → .autodev/specs/q1-report.yaml 생성
   → git commit + push to shared bare repo

2. Machine B: git pull
   → .autodev/specs/q1-report.yaml 감지
   → autodev spec import (SQLite에 등록)
   → Cron이 issue 생성 → 작업 시작
```

## 구현 순서

| 순서 | 작업 | 변경 대상 | 난이도 |
|------|------|-----------|--------|
| 1 | Platform trait + 도메인 모델 정의 | `core/platform.rs` (신규) | 낮음 |
| 2 | GitHubPlatform 구현 | `infra/platform/github.rs` (신규) | 중간 |
| 3 | Task 코드에서 Gh → Platform 교체 | `service/tasks/*.rs` | 중간 |
| 4 | 로컬 DB 스키마 추가 | `infra/db/schema.rs` | 낮음 |
| 5 | LocalPlatform 구현 | `infra/platform/local.rs` (신규) | 중간 |
| 6 | LocalTaskSource 구현 | `service/daemon/collectors/local.rs` (신규) | 중간 |
| 7 | Repo config에 platform 필드 추가 | `core/config/models.rs` | 낮음 |
| 8 | Daemon 초기화에서 platform별 분기 | `service/daemon/mod.rs` | 중간 |
| 9 | CLI: `repo add --local` 지원 | `cli/repo.rs` | 낮음 |
| 10 | Spec 파일 기반 동기화 | `cli/spec.rs`, `service/sync/` (신규) | 높음 |

## 사이드이펙트 분석

### 영향받는 코드

1. **Task 코드 (analyze, implement, review, improve)**
   - `self.gh` → `self.platform` 교체
   - `gh_host` 파라미터 제거 (Platform 내부에서 관리)
   - 메서드 시그니처 단순화

2. **GitHubTaskSource**
   - 기존 코드 유지, `Collector` trait으로 동작 변화 없음
   - `Platform` trait 대신 기존 `Gh` 직접 사용 가능 (래핑 불필요하면)

3. **QueueItem**
   - `github_number` → `item_number`로 리네임 고려 (breaking)
   - 또는 그대로 유지하고 의미만 확장 (number가 로컬 seq를 가리킴)

4. **DB 스키마**
   - 새 테이블 추가 (기존 테이블 변경 없음)
   - Migration v5로 추가

5. **Config**
   - `GitHubSourceConfig` 외에 `LocalSourceConfig` 추가
   - 또는 `SourceConfig`를 enum으로 통합

### 영향 없는 코드

- `Claude` trait/구현 (작업 실행은 동일)
- `Git` trait/구현 (git 명령어는 동일)
- `CronEngine` (플랫폼 무관)
- `TUI` (QueueItem 기반이므로 변경 불필요)
- `SuggestWorkflow` (별도 플러그인)

## 미래 확장

- **Web UI**: daemon이 HTTP 서버를 띄워서 diff viewer, kanban, spec 진행률 시각화
- **Webhook 대체**: `inotify`/`fswatch`로 로컬 git repo 변경 감지
- **Multi-repo Spec**: 하나의 spec이 여러 repo에 걸쳐 작업 생성
- **Template Library**: RFP, 장애보고서, 정부문서 등 양식 라이브러리
