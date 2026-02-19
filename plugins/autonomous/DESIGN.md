# Autonomous Plugin - 상세 설계

## 1. 개요

기존 플러그인 생태계(`develop-workflow`, `git-utils`, `external-llm`)를 이벤트 기반으로 자동 실행하는 오케스트레이션 레이어.

```
autonomous (오케스트레이터)
  ├── develop-workflow  → /develop, /multi-review
  ├── git-utils         → /merge-pr, /commit-and-pr
  └── external-llm      → /invoke-codex, /invoke-gemini
```

### 핵심 원칙

- **Monitor 자체는 얇게**: 이벤트 감지, 큐 관리, 세션 실행만 담당
- **분석/구현 품질은 기존 플러그인에 위임**: 플러그인이 진화하면 자동으로 품질 향상
- **레포별 독립 설정**: concurrency, 스캔 주기, 워크플로우 선택 가능
- **단일 바이너리**: Rust 데몬 + 내장 대시보드, 추가 의존성 없음
- **사람과 동일한 환경**: `claude -p`는 워크트리 cwd에서 실행하여 해당 레포의 `.claude/`, `CLAUDE.md`, 설치된 플러그인이 그대로 적용됨. `--plugin-dir` 등 별도 지정 없음. 사람이 직접 레포를 열어 작업하는 것과 100% 동일한 환경에서 동작하여 디버깅 및 이슈 재현이 용이

---

## 2. 플러그인 디렉토리 구조

```
plugins/autonomous/
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
├── cli/                         # Rust 코어 엔진
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # CLI 진입점
│       ├── lib.rs               # 모듈 export
│       ├── daemon/
│       │   ├── mod.rs           # 데몬 시작/중지/상태
│       │   └── pid.rs           # PID 파일 관리
│       ├── scanner/
│       │   ├── mod.rs           # GitHub API 스캐너
│       │   ├── issues.rs        # 이슈 감지
│       │   └── pulls.rs         # PR 감지
│       ├── queue/
│       │   ├── mod.rs           # 큐 매니저
│       │   ├── schema.rs        # SQLite 스키마 초기화
│       │   └── models.rs        # 큐 아이템 모델
│       ├── consumer/
│       │   ├── mod.rs           # Consumer 매니저 (워커 풀)
│       │   ├── issue.rs         # Issue Consumer
│       │   ├── pr.rs            # PR Consumer
│       │   └── merge.rs         # Merge Consumer
│       ├── workspace/
│       │   ├── mod.rs           # 워크스페이스 매니저
│       │   └── worktree.rs      # git worktree 관리
│       ├── session/
│       │   ├── mod.rs           # claude -p 세션 실행
│       │   └── output.rs        # 세션 출력 파싱
│       ├── dashboard/
│       │   ├── mod.rs           # axum 라우터
│       │   ├── api.rs           # REST API 핸들러
│       │   ├── ws.rs            # WebSocket (실시간 로그)
│       │   └── assets.rs        # 정적 파일 서빙 (include_bytes!)
│       └── config/
│           ├── mod.rs           # 설정 로드/저장
│           └── models.rs        # 설정 모델
│
├── dashboard-ui/                # 내장 프론트엔드
│   ├── index.html
│   ├── app.js                   # vanilla JS (빌드 도구 불필요)
│   └── style.css
│
└── README.md
```

---

## 3. Cargo.toml

```toml
[package]
name = "autonomous"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "autonomous"
path = "src/main.rs"

[dependencies]
# CLI
clap = { version = "4", features = ["derive"] }

# Async runtime
tokio = { version = "1", features = ["full"] }

# HTTP client (GitHub API)
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }

# Web server (dashboard)
axum = { version = "0.8", features = ["ws"] }
tower-http = { version = "0.6", features = ["cors", "fs"] }

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

DB 경로: `~/.autonomous/autonomous.db`

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

## 5. CLI 서브커맨드

```
autonomous start              # 데몬 시작 (백그라운드)
autonomous stop               # 데몬 중지
autonomous status             # 상태 요약 출력
autonomous dashboard          # 대시보드 서버 시작 (localhost:9800)

autonomous repo add <url>     # 레포 등록 (대화형 설정)
autonomous repo list          # 등록된 레포 목록
autonomous repo config <name> # 레포 설정 변경
autonomous repo remove <name> # 레포 제거

autonomous queue list <repo>  # 큐 상태 확인
autonomous queue retry <id>   # 실패 항목 재시도
autonomous queue clear <repo> # 큐 비우기

autonomous logs <repo>        # 실행 로그 조회
```

---

## 6. 슬래시 커맨드 설계

### /auto-setup (위자드)

```yaml
---
description: 자율 개발 모니터링을 위한 레포 등록 및 설정 위자드
argument-hint: "[repository-url]"
allowed-tools: ["AskUserQuestion", "Bash"]
---
```

**흐름:**
1. AskUserQuestion → 레포 URL 입력 (또는 인자에서 받기)
2. **필수 플러그인 의존성 체크** → user 레벨에 설치되어 있는지 검증
   - 필수: `develop-workflow`, `git-utils`
   - 권장: `external-llm` (multi-LLM 사용 시)
   - 미설치 시 경고 + 설치 안내 출력, 계속 진행 여부 확인
3. AskUserQuestion → 감시 대상 (Issues / PRs / 둘 다)
4. AskUserQuestion → 스캔 주기 (1분/5분/15분/커스텀)
5. AskUserQuestion → Consumer 처리량 (Issue/PR/Merge 각각)
6. AskUserQuestion → 워크플로우 선택 (Issue 분석: multi-LLM or 단일, PR 리뷰: /multi-review or 단일)
7. AskUserQuestion → 필터 (전체 / 특정 라벨 / 작성자 제외)
8. Bash → `autonomous repo add <url> --config '<json>'`
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
- `start` → `autonomous start`
- `stop` → `autonomous stop`
- `status` → `autonomous status` (각 레포별 큐 현황)

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
description: 자율 개발 대시보드를 브라우저에서 엽니다
allowed-tools: ["Bash"]
---
```

**흐름:**
1. `autonomous dashboard` (백그라운드 시작)
2. `open http://localhost:9800` (브라우저 열기)

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
  │   워크트리: ~/.autonomous/workspaces/<repo>/issue-<num>
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
  │   워크트리: ~/.autonomous/workspaces/<repo>/pr-<num>
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

## 9. Dashboard REST API

Base: `http://localhost:9800/api`

### 레포 관리
```
GET    /repos                    # 레포 목록 + 큐 카운트 요약
POST   /repos                    # 레포 등록
GET    /repos/:id                # 레포 상세
PUT    /repos/:id/config         # 설정 변경
DELETE /repos/:id                # 레포 제거
POST   /repos/:id/toggle         # 활성화/비활성화 토글
```

### 큐 조회
```
GET    /repos/:id/queues/issues  # 이슈 큐 목록 (status 필터)
GET    /repos/:id/queues/prs     # PR 큐 목록
GET    /repos/:id/queues/merges  # 머지 큐 목록
POST   /queues/:queue_type/:item_id/retry  # 재시도
```

### 로그
```
GET    /repos/:id/logs           # 실행 로그 (페이지네이션)
GET    /logs/:id                 # 로그 상세 (stdout/stderr)
```

### 데몬 상태
```
GET    /status                   # 데몬 상태 + 전체 통계
POST   /start                    # 데몬 시작
POST   /stop                     # 데몬 중지
```

### WebSocket
```
WS     /ws/logs                  # 실시간 로그 스트리밍
WS     /ws/events                # 큐 상태 변경 이벤트
```

---

## 10. Dashboard UI

단일 페이지, vanilla JS (빌드 도구 없음), `include_bytes!`로 바이너리에 임베딩.

```
┌─────────────────────────────────────────────────────────┐
│  Autonomous Dashboard                    [Start] [Stop] │
├──────────┬──────────────────────────────────────────────┤
│          │                                              │
│ Repos    │  org/repo-a                    ● Running     │
│          │  ┌─────────────────────────────────────┐     │
│ ● repo-a │  │ Issues  [3 pending] [1 processing]  │     │
│ ○ repo-b │  │ PRs     [2 pending] [0 processing]  │     │
│ + Add    │  │ Merges  [1 pending]                  │     │
│          │  └─────────────────────────────────────┘     │
│          │                                              │
│          │  Recent Activity                             │
│          │  ┌─────────────────────────────────────┐     │
│          │  │ 14:32 issue-42 analyzing → ready     │     │
│          │  │ 14:30 pr-15   reviewing              │     │
│          │  │ 14:28 pr-12   merged ✓               │     │
│          │  │ 14:25 issue-41 done ✓ → PR #18       │     │
│          │  └─────────────────────────────────────┘     │
│          │                                              │
│          │  Consumer Logs (실시간)                       │
│          │  ┌─────────────────────────────────────┐     │
│          │  │ [issue-worker-1] claude -p "/devel…  │     │
│          │  │ [pr-worker-1] claude -p "/multi-r…   │     │
│          │  └─────────────────────────────────────┘     │
│          │                                              │
│          │  Settings                    [Edit Config]   │
│          │  Scan: 5m | Issue×2 PR×1 Merge×1 | sonnet  │
└──────────┴──────────────────────────────────────────────┘
```

---

## 11. 설치 & 배포

### 로컬 개발
```bash
cd plugins/autonomous/cli
cargo build --release
cp target/release/autonomous ~/.local/bin/
```

### 플러그인 설치
```bash
# marketplace에서 설치
/plugin install autonomous@kys-claude-plugin

# 바이너리는 /auto-setup 실행 시 자동 빌드 or GitHub Release에서 다운로드
```

### CI/CD (rust-binary.yml 확장)
```yaml
# 기존 suggest-workflow 빌드에 autonomous 추가
- name: Build autonomous
  run: |
    cd plugins/autonomous/cli
    cargo build --release
    tar czf autonomous-${{ matrix.target }}.tar.gz autonomous
```

### marketplace.json 추가
```json
{
  "name": "autonomous",
  "version": "0.1.0",
  "source": "./plugins/autonomous",
  "category": "automation",
  "description": "이벤트 기반 자율 개발 오케스트레이터 - Issue 분석, PR 리뷰, 자동 머지",
  "keywords": ["autonomous", "monitor", "queue", "dashboard", "automation"]
}
```

### 의존성 체크 (/auto-setup 커맨드 내 LLM 지침)

플러그인 설치 경로는 Claude 내부 구현에 의존하므로, Rust CLI가 아닌
**`/auto-setup` 슬래시 커맨드 실행 시 LLM이 직접 검증**:

```markdown
# /auto-setup 커맨드 내 지침 (commands/auto-setup.md)

## 의존성 검증 (Step 2)

레포 등록 전에 다음 플러그인이 User Scope로 설치되어 있는지 확인하세요:

- **필수**: `develop-workflow`, `git-utils`
  → 미설치 시: 사용자에게 경고하고 설치 명령어 안내
  → `/plugin install develop-workflow@kys-claude-plugin`
- **권장**: `external-llm` (multi-LLM 분석 사용 시)
  → 미설치 시: multi-LLM 분석이 Claude 단일 모델로 fallback됨을 안내

설치 확인이 완료되지 않으면 다음 단계로 진행하지 마세요.
```

이점:
- Claude 내부 플러그인 경로에 의존하지 않음
- LLM이 현재 세션에서 사용 가능한 커맨드를 직접 확인 가능
- Rust CLI는 플러그인 시스템을 전혀 알 필요 없음 (관심사 분리)

---

## 12. 사이드이펙트 & 의존성

### 기존 코드 영향
- **marketplace.json**: 플러그인 1개 추가 (신규)
- **rust-binary.yml**: autonomous 빌드 step 추가
- **validate.yml**: autonomous 경로 추가 (자동 감지될 수 있음)
- **기존 플러그인 코드 변경 없음**: autonomous는 레포 워크트리의 cwd에서 `claude -p`를 실행하여 기존 커맨드를 호출 (레포에 설치된 플러그인이 자동 적용)

### 외부 의존성
- **GitHub Personal Access Token**: Scanner가 GitHub API 호출 시 필요
- **Claude CLI**: `claude` 명령이 PATH에 있어야 함
- **git**: worktree 생성에 필요
- **디스크 공간**: 레포별 worktree (base clone + worktree per task)

### 보안 고려
- GitHub token은 환경변수(`GITHUB_TOKEN`) 또는 `gh auth` 활용
- `claude -p`의 `--dangerously-skip-permissions`는 사용하지 않음
- Dashboard는 localhost만 바인딩 (0.0.0.0 아님)

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

### Phase 3: 대시보드
11. axum REST API
12. WebSocket 실시간 로그
13. Dashboard UI (HTML/JS/CSS)

### Phase 4: 배포
14. CI/CD 통합 (rust-binary.yml)
15. marketplace.json 등록
16. README 문서화
