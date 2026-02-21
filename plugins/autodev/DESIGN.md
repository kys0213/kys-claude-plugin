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
- **GitHub = SSOT**: 라벨이 영속 상태, 인메모리는 현재 처리중인 작업만 추적
- **SQLite 없음**: 상태는 GitHub 라벨, 추적은 인메모리 HashMap, 로그는 파일
- **단일 바이너리**: Rust 데몬 + TUI 대시보드, 추가 의존성 없음
- **사람과 동일한 환경**: `claude -p`는 워크트리 cwd에서 실행하여 해당 레포의 `.claude/`, `CLAUDE.md`, 설치된 플러그인이 그대로 적용됨

---

## 2. 아키텍처

### 상태 관리: GitHub 라벨 + 인메모리

```
GitHub (SSOT, 영속)
  │
  │  gh api (조회/댓글/라벨/PR)
  ▼
daemon process
  │
  ├─ ActiveItems: HashMap<WorkId, Phase>   ← 인메모리, 휘발
  │    "issue:owner/repo:42" → Analyzing
  │    "issue:owner/repo:99" → Ready
  │    "pr:owner/repo:10"   → Processing
  │
  └─ 로그: append-only 파일 (~/.autodev/daemon.log)
```

### GitHub 라벨

| 라벨 | 의미 |
|------|------|
| (없음) | 미처리 → scan 대상 |
| `autodev:wip` | 데몬이 처리중 |
| `autodev:done` | 처리 완료 |
| `autodev:skip` | clarify/wontfix 등으로 건너뜀 |

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

### 인메모리 ActiveItems

```
ActiveItems: HashMap<String, Phase>

key = "{type}:{repo}:{number}"
예: "issue:org/repo:42", "pr:org/repo:15"

Phase:
  Pending       → scan에서 등록됨, 아직 처리 시작 안함
  Analyzing     → 분석 프롬프트 실행중
  Ready         → 분석 완료, 구현 대기
  Implementing  → 구현 프롬프트 실행중
  Reviewing     → PR 리뷰 실행중
  ReviewDone    → 리뷰 완료, 개선 대기
  Improving     → 리뷰 피드백 반영 구현중
  Improved      → 개선 완료, 재리뷰 대기
  MergeReady    → approve 완료, 머지 대기
  Merging       → 머지 실행중
```

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
│       ├── active/
│       │   └── mod.rs           # ActiveItems (HashMap 관리)
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

# TUI
ratatui = "0.29"
crossterm = "0.28"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Utils
chrono = { version = "0.4", features = ["serde"] }
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

> SQLite(rusqlite), uuid 의존성 제거. GitHub 라벨이 SSOT이므로 로컬 DB 불필요.

---

## 5. 메인 루프

```
daemon start
│
└─ loop (매 tick)
    │
    ├─ 1. recovery()
    │    "autodev:wip 라벨 + active에 없는 이슈" 조회
    │    → autodev:wip 라벨 제거
    │    (다음 tick의 scan에서 자연스럽게 재발견)
    │
    ├─ 2. scan()
    │    "open + autodev 라벨 없는 이슈/PR" 조회
    │    for each item:
    │      gh label add "autodev:wip"
    │      active.insert(id, Pending)
    │
    ├─ 3. process()
    │    for each (id, phase) in active:
    │
    │      Pending ─────────────────────────────┐
    │        분석 프롬프트 실행                    │
    │        phase → Analyzing                   │
    │                                            │
    │      Analyzing ───────────────────────────┐│
    │        완료 대기                            ││
    │        verdict:                            ││
    │        ├─ implement → phase → Ready        ││
    │        ├─ clarify   ──┐                    ││
    │        └─ wontfix   ──┤                    ││
    │                       ▼                    ││
    │                   GitHub 댓글               ││
    │                   wip → autodev:skip       ││
    │                   active.remove(id)        ││
    │                                            ││
    │      Ready ───────────────────────────────┐││
    │        구현 프롬프트 실행                    │││
    │        phase → Implementing               │││
    │                                           │││
    │      Implementing ────────────────────────┘││
    │        완료 대기                             ││
    │        result:                              ││
    │        ├─ success                           ││
    │        │    GitHub PR 생성                   ││
    │        │    wip → autodev:done              ││
    │        │    active.remove(id)               ││
    │        └─ failure                           ││
    │             wip 라벨 제거 (라벨 없음)         ││
    │             active.remove(id)               ││
    │             (다음 tick에 재시도)              ││
    │                                             ││
    └─ 4. sleep(interval)                         ││
                                                  ▼▼
                                        다음 tick으로
```

### 타이밍 예시 (scan_interval_secs: 300)

```
tick  0s:  recovery + scan ✓ (첫 실행)  + process
tick 10s:  recovery + scan SKIP         + process
tick 20s:  recovery + scan SKIP         + process
...
tick 300s: recovery + scan ✓ (5분 경과) + process
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

GitHub API에서 라벨이 없는 새 이슈/PR을 발견하여 active에 등록한다.

```
scan()
  │
  ▼
gh api repos/{repo}/issues?state=open
  │
  ▼
필터:
  • autodev 라벨 없는 것만 (wip/done/skip 모두 제외)
  • filter_labels 매칭 (설정된 경우)
  • ignore_authors 제외
  │
  for each item:
    id = "{type}:{repo}:{number}"
    │
    ├─ active.contains(id)?
    │    YES → skip
    │    NO  → gh label add "autodev:wip"
    │          active.insert(id, Pending)
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
`request_changes` 시 자동으로 피드백을 반영하고, 재리뷰 후 approve되면 머지로 진행한다.

```
process() — PR
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
  │    │    phase → MergeReady
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
  ├─ Improved
  │    재리뷰: run_claude(/multi-review, json)
  │    phase → Reviewing (반복)
  │    │
  │    (Reviewing에서 approve 나올 때까지 사이클 반복)
  │
  ├─ MergeReady
  │    run_claude(/merge-pr {N})
  │    phase → Merging
  │
  └─ Merging (완료 대기)
       │
       ├─ success
       │    wip → autodev:done
       │    active.remove(id)
       │
       ├─ conflict
       │    run_claude(conflict resolution)
       │    ├─ 해결 → wip → autodev:done
       │    └─ 실패 → wip 제거, active.remove(id) → 재시도
       │
       └─ failure
            wip 제거, active.remove(id) → 재시도
```

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

# 상태 조회 (→ GitHub API 직접 조회)
autodev status             # 데몬 상태 + active items 요약
autodev dashboard          # TUI 대시보드

# 설정 관리 (→ YAML 파일)
autodev config show        # 현재 설정 표시
autodev config edit        # 설정 편집
```

### 공유 상태

```
~/.autodev/
├── config.yaml          # 설정 파일
├── daemon.pid           # PID 파일 (단일 인스턴스 보장)
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

daemon:
  tick_interval_secs: 10
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
│                          DAEMON LOOP                              │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ 1. RECOVERY                                                 │ │
│  │    autodev:wip + active에 없음 → wip 라벨 제거               │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                            │                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ 2. SCAN                                                     │ │
│  │    gh api: open + autodev 라벨 없는 이슈/PR 조회             │ │
│  │    → autodev:wip 라벨 추가                                   │ │
│  │    → active.insert(id, Pending)                             │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                            │                                      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ 3. PROCESS                                                  │ │
│  │                                                             │ │
│  │  Issues:                                                    │ │
│  │    Pending → Analyzing → verdict 분기                        │ │
│  │      implement → Ready → Implementing → PR 생성              │ │
│  │      clarify/wontfix → 댓글 + autodev:skip                  │ │
│  │    success → autodev:done → [Knowledge Extraction]          │ │
│  │    failure → 라벨 제거 (재시도)                                │ │
│  │                                                             │ │
│  │  PRs:                                                       │ │
│  │    Pending → Reviewing → verdict 분기                        │ │
│  │      approve → MergeReady → Merging → done                  │ │
│  │      request_changes → ReviewDone → Improving → Improved    │ │
│  │        → 재리뷰 (Reviewing 반복)                              │ │
│  │    success → autodev:done → [Knowledge Extraction]          │ │
│  │    failure → 라벨 제거 (재시도)                                │ │
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
| PR | `Pending → Reviewing → MergeReady → Merging → done` | `(없음) → wip → done` |
| PR | `Pending → Reviewing → ReviewDone → Improving → Improved → Reviewing (반복)` | `wip` 유지 |

---

## Summary

| 구성요소 | 역할 |
|---------|------|
| **GitHub 라벨** | 영속 상태 (SSOT) — `autodev:wip`, `autodev:done`, `autodev:skip` |
| **ActiveItems (HashMap)** | 현재 처리중인 작업 + Phase 추적 (휘발) |
| **recovery()** | 크래시 후 orphan wip 정리 |
| **scan()** | 라벨 없는 이슈/PR 발견 → wip 라벨 + active 등록 |
| **process()** | Phase별 작업 실행 → done/skip/재시도 |
| **Knowledge Extraction (per-task)** | done 전이 시 해당 세션 분석 → 즉시 피드백 |
| **Knowledge Extraction (daily)** | 매일 전일 로그 전체 분석 → 일일 리포트 + 크로스 태스크 패턴 |
| **daemon.YYYY-MM-DD.log** | 일자별 롤링 로그 (30일 보존) |
