# Autonomous Plugin

기존 플러그인 생태계(`develop-workflow`, `git-utils`, `external-llm`)를 polling 기반 이벤트 루프로 자동 실행하는 오케스트레이션 레이어.

```
autodev (오케스트레이터)
  ├── develop-workflow  → /develop, /multi-review
  ├── git-utils         → /merge-pr, /commit-and-pr
  └── external-llm      → /invoke-codex, /invoke-gemini
```

---

## Architecture

### Event Loop

```
setup (사용자 설정)
  │
  │  scan_interval_secs: 60 | 300 | 900 | custom
  │  scan_targets: [issues, pulls]
  │  filter_labels: ["autodev"]
  │  ...
  │
  ▼
~/.develop-workflow.yaml (또는 레포별 override)

daemon::start()
  │
  ▼
┌═══════════════════════════════════════════════════════════┐
║              DAEMON HEARTBEAT (내부 고정)                  ║
║                                                           ║
║  loop {                                                   ║
║                                                           ║
║    ┌───────────────────────────────────────────────────┐  ║
║    │ Phase 1: scan_all()                               │  ║
║    │                                                   │  ║
║    │  for repo in enabled_repos:                       │  ║
║    │    cfg = load_merged(global + repo yaml)          │  ║
║    │                                                   │  ║
║    │    cursor_should_scan(                            │  ║
║    │      scan_interval_secs   ← 유저 설정값            │  ║
║    │    )?                                             │  ║
║    │    │                                              │  ║
║    │    ├─ elapsed < interval ──→ SKIP                 │  ║
║    │    │                                              │  ║
║    │    └─ elapsed >= interval ──→ SCAN 실행            │  ║
║    │         gh api polling (since cursor)             │  ║
║    │         filter_labels / ignore_authors 적용        │  ║
║    │         신규 아이템 → queue INSERT (pending)        │  ║
║    │         cursor 갱신                                │  ║
║    └───────────────────────────────────────────────────┘  ║
║                         │                                 ║
║                         ▼                                 ║
║    ┌───────────────────────────────────────────────────┐  ║
║    │ Phase 2: process_all()          ← 매 tick 실행     │  ║
║    │                                                   │  ║
║    │  issue::process_pending()                         │  ║
║    │  issue::process_waiting_human()                   │  ║
║    │  pr::process_pending()                            │  ║
║    │  merge::process_pending()                         │  ║
║    └───────────────────────────────────────────────────┘  ║
║                         │                                 ║
║                   sleep(tick)                             ║
║                         │                                 ║
║                         └──→ loop                         ║
╚═══════════════════════════════════════════════════════════╝
```

scan은 유저가 setup에서 설정한 `scan_interval_secs` 간격으로만 실행된다.
`process_all()`은 매 tick 실행되지만, scan은 interval 게이트를 통과할 때만 GitHub API를 호출한다.

```
타이밍 예시 (scan_interval_secs: 300 = 5분):

tick  0s:  scan ✓ (첫 실행)  + process
tick 10s:  scan SKIP         + process
tick 20s:  scan SKIP         + process
...
tick 300s: scan ✓ (5분 경과) + process
tick 310s: scan SKIP         + process
```

---

## Flows

### Scanner → Queue 진입

```
scan 실행 (interval 경과 시에만)
  │
  ▼
gh api repos/{repo}/issues?state=open&since={cursor}
gh api repos/{repo}/pulls?state=open&since={cursor}
  │
  ▼
┌──────────────────────────┐
│ filter                    │
│ • filter_labels 매칭      │  ← "autodev" 등 특정 라벨
│ • ignore_authors 제외     │  ← "dependabot" 등
│ • active + DB 중복 체크    │
└────────────┬─────────────┘
             │
        신규 아이템만
             │
     ┌───────┴───────┐
     │               │
     ▼               ▼
issue_queue      pr_queue
(pending)        (pending)
```

---

### Issue Flow - Confidence 기반 분기

에이전트가 분석 결과의 confidence를 자체 판단하여,
명확하면 바로 구현하고 불확실하면 이슈 댓글로 질문을 남기고 사람 응답을 대기한다.

```
process_pending()
  │
  items = issue_find_pending(concurrency)
  │
  for each item:
  │
  ├─ is_issue_open? ──NO──→ done
  │
  ▼
analyzing (worker_id 할당)
  │
  │  workspace 준비 (clone + worktree)
  │  run_claude(analysis_prompt, json)
  │
  ├─ 실패 ──→ failed
  │
  ▼
┌──────────────────────────────────────────────────┐
│ AnalysisResult {                                 │
│   verdict:    "implement" | "needs_clarification"│
│               | "wontfix",                       │
│   confidence: 0.0 ~ 1.0,                        │
│   summary: String,                               │
│   affected_files: [String],                      │
│   implementation_plan: String,                   │
│   checkpoints: [String],                         │
│   risks: [String],                               │
│   questions: [String]    ← confidence 낮을 때     │
│ }                                                │
└──────────────────────┬───────────────────────────┘
                       │
          ┌────────────┼────────────┐
          │            │            │
       implement    needs_clari   wontfix
       + high conf  or low conf     │
          │            │            ▼
          │            │       POST comment
          │            │       (사유 설명)
          │            │       → done
          │            ▼
          │   POST issue comment:
          │   ┌─────────────────────────────┐
          │   │ ## 분석 레포트                │
          │   │ {summary}                   │
          │   │                             │
          │   │ ### 영향 파일                 │
          │   │ - src/foo.rs                │
          │   │ - src/bar.rs                │
          │   │                             │
          │   │ ### 확인 필요                 │
          │   │ 1. API v1 vs v2?            │
          │   │ 2. 리팩토링 범위?             │
          │   │                             │
          │   │ <!-- autodev:waiting     │
          │   │      item_id:xxx            │
          │   │      asked_at:2026-... -->  │
          │   └──────────────┬──────────────┘
          │                  │
          │                  ▼
          │           waiting_human
          │           (이번 tick 종료)
          │
          ▼
     POST comment
     ("분석 완료, 구현 진행합니다")
          │
          ▼
     processing
          │  run_claude(workflow.issue + analysis context)
          │  → 구현 + commit + push + PR 생성
          │
          ├─ 성공 ──→ done ──→ [Knowledge Extraction]
          └─ 실패 ──→ failed
```

---

### HITL (Human-in-the-Loop) 응답 처리

`waiting_human` 상태의 이슈는 매 tick마다 댓글을 확인한다.
별도 polling이 아니라 기존 `process_all()` 루프의 일부로 자연스럽게 통합된다.

```
process_waiting_human()     ← 매 tick 실행
  │
  items = issue_find_by_status("waiting_human")
  │
  for each item:
  │
  ▼
gh api repos/{repo}/issues/{N}/comments
  │
  ▼
<!-- autodev:waiting --> 메타 태그 이후
새 사람 댓글 있음?
  │         │
  NO        YES
  │         │
  skip      ▼
  (다음   context 보강:
   tick)    analysis_report
            + 사람 답변
               │
               ▼
          processing
               │
          run_claude(
            workflow.issue,
            enriched_context
          )
               │
               ▼
             done ──→ [Knowledge Extraction]
```

---

### PR Flow - 리뷰 → 개선 → 재리뷰 사이클

리뷰 결과를 JSON으로 받아 verdict에 따라 결정적으로 분기한다.
`request_changes` 시 자동으로 피드백을 반영하고, 재리뷰 후 approve되면 merge queue에 삽입된다.

```
process_pending()  (pr)
  │
  ├─ is_pr_reviewable? ──NO──→ done
  │   (open + no APPROVED)
  │
  ▼
reviewing (worker_id 할당)
  │
  │  workspace 준비 (clone + worktree, head_branch checkout)
  │  run_claude(workflow.pr, json)       ← /multi-review
  │
  ├─ 실패 ──→ failed
  │
  ▼
┌─────────────────────────────────────────┐
│ ReviewResult {                          │
│   verdict: "approve" | "request_changes"│
│   summary: String,                      │
│   comments: [{                          │
│     path: String,                       │
│     line: u32,                          │
│     body: String                        │
│   }]                                    │
│ }                                       │
└────────────────┬────────────────────────┘
                 │
        ┌────────┴────────┐
        │                 │
     approve        request_changes
        │                 │
        ▼                 ▼
  gh pr review       POST /pulls/{N}/reviews
  --approve            event: REQUEST_CHANGES
  -b "{summary}"       body: "{summary}"
        │              comments: [{path,line,body}]
        │                 │
        │                 ▼
        │            review_done
        │                 │
        │                 ▼
        │            improving
        │                 │  run_claude(
        │                 │    "/develop implement review feedback:"
        │                 │    + review_comment
        │                 │  )
        │                 │
        │                 ▼
        │            improved
        │                 │
        │                 ▼
        │            재리뷰 (run_claude /multi-review)
        │                 │
        │          ┌──────┴──────┐
        │          │             │
        │       approve    request_changes
        │          │             │
        │          ▼             ▼
        │     merge_ready   reviewing (반복)
        │          │
        ├──────────┘
        │
        ▼
  merge_queue INSERT(pending)
  status → approved
```

---

### Merge Flow

```
process_pending()  (merge)
  │
  ├─ is_pr_mergeable? ──NO──→ done
  │   (open + not merged)
  │
  ▼
merging (worker_id 할당)
  │  run_claude(/git-utils:merge-pr {N})
  │
  ├─ 성공 ──→ done ✓
  │
  ├─ conflict 감지
  │     ▼
  │  conflict
  │     │  run_claude(conflict resolution)
  │     │
  │     ├─ 해결 성공 ──→ done ✓
  │     └─ 해결 실패 ──→ failed
  │
  └─ 기타 에러 ──→ failed
```

---

### Workspace 관리 (git worktree)

각 태스크는 격리된 worktree에서 실행된다.
base clone을 공유하고, 태스크별 worktree를 생성/삭제한다.

```
~/.autodev/workspaces/
└── {sanitized-repo-name}/
    │
    ├── main/                    ← base clone (장기 유지)
    │   └── (전체 레포)            git clone --single-branch
    │                              git pull (scan 시 갱신)
    │
    ├── issue-42/                ← worktree (태스크 시작 시 생성)
    │   └── (분석 + 구현 작업)      git worktree add --detach
    │
    ├── pr-15/                   ← worktree (head_branch checkout)
    │   └── (리뷰 + 개선 작업)      git worktree add -b pr-15 origin/feature
    │
    └── merge-pr-12/             ← worktree (merge 시도)
        └── (merge + conflict)    git worktree add --detach

워크트리 생명주기:
  태스크 시작 → ensure_cloned() → create_worktree()
  태스크 완료 → remove_worktree() (done/failed 시 정리)

장점:
  • 태스크 간 완전 격리 (동시 issue-42 + pr-15 가능)
  • base clone 재사용 (네트워크 비용 최소화)
  • claude -p는 worktree cwd에서 실행
    → 레포의 .claude/, CLAUDE.md, 설치된 플러그인 자동 적용
    → 사람이 직접 레포 열어 작업하는 것과 100% 동일한 환경
```

---

### Session Runner (claude -p)

모든 LLM 작업은 `run_claude()` 함수를 통해 `claude -p`로 실행된다.

```
run_claude(cwd, prompt, output_format)
  │
  │  ┌──────────────────────────────────────────────┐
  │  │ 실행 명령:                                     │
  │  │                                               │
  │  │ claude -p "{prompt}" --output-format json     │
  │  │                                               │
  │  │ cwd = worktree 경로                            │
  │  │ env = GITHUB_TOKEN, ANTHROPIC_API_KEY 등       │
  │  │ timeout = stuck_threshold_secs                 │
  │  └──────────────────────────────────────────────┘
  │
  ▼
┌──────────────────────────────────────────────┐
│ SessionResult {                              │
│   stdout: String,      ← JSON 파싱 대상       │
│   stderr: String,      ← 디버깅용             │
│   exit_code: i32,      ← 0=성공, else=실패    │
│   duration_ms: u64                           │
│ }                                            │
└──────────────────────────────────────────────┘
  │
  ├─ exit_code == 0
  │    stdout → serde_json::from_str::<T>()
  │    → AnalysisResult | ReviewResult
  │
  └─ exit_code != 0
       → failed 상태 전이 + error_message = stderr
       → consumer_logs에 전체 기록

모든 실행은 consumer_logs에 기록:
  • command, stdout, stderr, exit_code
  • started_at, finished_at, duration_ms
  • queue_type, queue_item_id, worker_id
```

---

### Daemon Lifecycle

단일 인스턴스 보장, stuck 복구, 자동 재시도를 포함한 데몬 생명주기.

```
autodev start
  │
  ▼
┌─────────────────────────────────────────┐
│ PID 파일 체크 (~/.autodev/daemon.pid) │
│                                          │
│ 파일 존재 + 프로세스 살아있음 → 에러 종료   │
│ 파일 존재 + 프로세스 죽음 → stale PID 삭제  │
│ 파일 없음 → 정상 진행                      │
└────────────────┬────────────────────────┘
                 │
                 ▼
          PID 파일 생성
                 │
                 ▼
┌─────────────────────────────────────────┐
│ Stuck Item Recovery (시작 시 1회)         │
│                                          │
│ • analyzing/processing/reviewing/merging │
│   상태인데 stuck_threshold_secs 초과       │
│   → pending으로 리셋 (재처리 대상)          │
│                                          │
│ • waiting_human은 제외                    │
│   (사람 응답 대기는 의도적 대기)             │
│                                          │
│ • retry_count >= 3이면 → failed           │
│   (무한 재시도 방지)                        │
└────────────────┬────────────────────────┘
                 │
                 ▼
            Event Loop 진입
            (scan_all + process_all)
                 │
                 ▼
          SIGTERM 수신 시
                 │
                 ▼
┌─────────────────────────────────────────┐
│ Graceful Shutdown                        │
│ • 현재 진행 중인 consumer 완료 대기        │
│ • PID 파일 삭제                           │
│ • SQLite WAL checkpoint                  │
└─────────────────────────────────────────┘
```

---

### Dedup (ActiveItems)

Scanner가 같은 이슈/PR을 중복 삽입하지 않도록 in-memory 해시셋으로 추적한다.

```
┌──────────────────────────────────────────────────┐
│ ActiveItems (in-memory HashSet)                   │
│                                                   │
│ key = "{queue_type}:{repo_id}:{github_number}"    │
│ 예: "issue:abc-123:42", "pr:abc-123:15"           │
│                                                   │
│ 삽입 시점: scan에서 새 아이템 발견 시               │
│ 삭제 시점: done / failed 상태 전이 시              │
└──────────────────────────────────────────────────┘

scan 시 흐름:

gh api polling → 이슈 #42 발견
  │
  ├─ active.contains("issue", repo_id, 42)?
  │     │
  │     YES → SKIP (이미 큐에 있음)
  │     │
  │     NO  → DB에도 없는지 확인
  │            │
  │            ├─ DB에 있음 → active.insert() + SKIP
  │            │   (재시작 후 첫 scan에서 DB 동기화)
  │            │
  │            └─ DB에 없음 → active.insert() + queue INSERT
  │
  └─ 다음 아이템

done/failed 전이 시:
  active.remove("issue", repo_id, 42)
  → 이후 scan에서 같은 번호가 다시 등장하면 새 아이템으로 처리

O(1) lookup → scan 성능에 영향 없음
```

---

### Config 로딩 (Deep Merge)

Global 설정 + Per-repo 설정을 deep merge하여 최종 설정을 결정한다.

```
설정 파일 위치:
  Global:   ~/.develop-workflow.yaml
  Per-repo: <workspace>/.develop-workflow.yaml

로딩 흐름:

load_merged(global_path, repo_workspace_path)
  │
  ├─ global yaml 읽기
  │   consumer:
  │     scan_interval_secs: 300
  │     model: sonnet
  │     filter_labels: [autodev]
  │     ignore_authors: [dependabot, renovate]
  │
  ├─ repo yaml 읽기 (있으면)
  │   consumer:
  │     scan_interval_secs: 60    ← override
  │     model: opus               ← override
  │
  └─ deep merge (repo가 global을 override)
      consumer:
        scan_interval_secs: 60    ← repo 값
        model: opus               ← repo 값
        filter_labels: [autodev]  ← global 유지
        ignore_authors: [dependabot, renovate]  ← global 유지

scan_all()에서 레포마다 독립적으로 load_merged() 호출:

for repo in enabled_repos:
  cfg = load_merged(global, repo.workspace_path)
  │
  ├─ repo-a: scan_interval=60, model=opus
  ├─ repo-b: scan_interval=300, model=sonnet  (global 기본값)
  └─ repo-c: scan_interval=900, model=haiku
```

---

### Knowledge Extraction (Agent-Driven)

이슈 또는 PR이 완료(done)될 때, 에이전트가 CLI를 통해
완료된 작업 데이터를 능동적으로 탐색하며 인사이트를 도출한다.

#### 설계 원칙: Data-Only CLI + LLM 해석

```
CLI (Rust)  = 사실만 반환   "무엇이 일어났는가"
Agent (LLM) = 의미를 해석   "그래서 무슨 의미인가"
```

CLI는 SQLite에서 구조화된 데이터를 꺼내주는 역할만 한다.
"이 패턴이 무엇을 의미하는지", "어떤 개선이 필요한지"는
전적으로 에이전트가 판단한다.

```
❌ Rule-based (엣지케이스 누적)
   if error.contains("timeout")  → suggest "increase timeout"
   if file_edit_count > 3        → suggest "refactor"
   ... (끝없이 규칙 추가)

✅ Data-only + LLM 해석
   CLI: SELECT error, COUNT(*) GROUP BY error  → 통계만 반환
   Agent: "timeout이 5건 중 3건, 모두 external API 호출 시점
          → API 클라이언트에 retry/backoff 설정 필요"
```

#### 데이터 소스: 2-DB 아키텍처

autodev의 인사이트는 **두 개의 독립적인 데이터 소스**에서 나온다.
별도 CLI를 만들지 않고, 기존 suggest-workflow에 **세션 필터링 기능**을 추가하여 재사용한다.

```
┌──────────────────────────┐    ┌──────────────────────────────────┐
│ A. autodev.db         │    │ B. suggest-workflow index.db     │
│    (~/.autodev/)      │    │    (~/.claude/suggest-workflow-   │
│                          │    │     index/{project}/)            │
│ 태스크 메타데이터:          │    │                                  │
│ • issue_queue            │    │ 세션 실행 이력:                     │
│ • pr_queue               │    │ • sessions (+ first_prompt_      │
│ • merge_queue            │    │   snippet)                       │
│ • consumer_logs          │    │ • prompts                        │
│ • scan_cursors           │    │ • tool_uses (classified)         │
│                          │    │ • file_edits                     │
│ "무엇을 처리했는가"         │    │                                  │
│ (큐 상태, 에러, 소요 시간)  │    │ "어떻게 실행했는가"                │
│                          │    │ (도구 사용, 파일 수정, 프롬프트)     │
└──────────────────────────┘    └──────────────────────────────────┘
```

#### 세션 식별: `[autodev]` 마커 컨벤션

autodev consumer가 `claude -p` 실행 시, 첫 프롬프트에 마커를 삽입한다:

```
claude -p "[autodev] fix: resolve login timeout issue in auth module"
```

suggest-workflow는 인덱싱 시 `first_prompt_snippet` (첫 500자)을 저장한다.
이후 `--session-filter` 또는 `filtered-sessions` perspective로 autodev 세션만 조회 가능:

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

# 에이전트가 작성한 커스텀 쿼리
suggest-workflow query --sql-file /tmp/deep-dive.sql
```

#### Perspectives (세션 필터 지원 현황)

suggest-workflow의 기존 perspective 중 `--session-filter` 지원:

| Perspective | Session Filter | 설명 |
|-------------|:-:|------|
| `filtered-sessions` | - | 첫 프롬프트 패턴으로 세션 검색 (신규) |
| `tool-frequency` | `{SF}` | 도구 사용 빈도 |
| `repetition` | `{SF}` | 이상치 탐지 (z-score²) |
| `prompts` | `{SF}` | 프롬프트 키워드 검색 |
| `sessions` | `{SF}` | 세션 목록 및 요약 |
| `transitions` | - | 도구 전이 확률 (derived) |
| `trends` | - | 주간 트렌드 (derived) |
| `hotfiles` | - | 파일 핫스팟 (derived) |
| `sequences` | - | 도구 시퀀스 (derived) |

> derived table perspective는 전체 데이터에서 사전 계산되므로 세션 필터 미지원.
> 필터가 필요한 경우 `--sql-file`로 core table에서 직접 쿼리.

autodev 전용 perspective (autodev.db 대상, 추후 추가):

| Perspective | 반환 데이터 | 용도 |
|-------------|-----------|------|
| `task-timeline` | 상태 전이 이벤트 시간순 | 병목 구간 파악 |
| `error-frequency` | 에러 메시지별 빈도 | 반복 실패 패턴 |
| `hitl-history` | HITL 질문/답변 쌍 | 맥락 부족 패턴 |
| `duration-stats` | 단계별 소요 시간 | 성능 병목 |
| `retry-history` | 재시도/실패 이력 | 안정성 문제 |

#### 에이전트 탐색 흐름

```
done 전이 시
  │
  ▼
knowledge-extractor agent 시작
  │
  │  ══ 1차: autodev.db (태스크 메타) ══
  │
  │  autodev queue 조회 → 상태 전이, 소요 시간
  │  autodev logs 조회  → 에러 메시지, exit code
  │
  │  ══ 2차: suggest-workflow (세션 실행 이력) ══
  │
  │  suggest-workflow query \
  │    --perspective filtered-sessions \
  │    --param prompt_pattern="[autodev]"
  │  → autodev 세션 목록
  │
  │  suggest-workflow query \
  │    --perspective tool-frequency \
  │    --session-filter "first_prompt_snippet LIKE '[autodev]%'"
  │  → [{tool: "Edit", frequency: 45}, {tool: "Bash:test", frequency: 38}, ...]
  │
  │  ══ 3차: drill-down (관심 영역 심화) ══
  │
  │  suggest-workflow query \
  │    --perspective repetition \
  │    --session-filter "first_prompt_snippet LIKE '[autodev]%'"
  │  → 이상치 세션 발견: "session-abc에서 Bash:test 28회 반복"
  │
  │  suggest-workflow query --sql-file /tmp/deep-dive.sql
  │  → 에이전트가 직접 SQL 작성하여 cross-table 분석
  │
  │  ══ 4차: 인사이트 종합 ══
  │
  │  에이전트가 탐색 결과를 종합하여 판단:
  │  "autodev 세션에서 Bash:test가 평균 대비 3배 호출.
  │   테스트 실패 → 수정 → 재실행 루프가 반복됨.
  │   src/api/client.rs 수정 시 항상 발생.
  │   → .claude/rules/api-testing.md 에 테스트 전략 가이드 추가 제안"
  │
  ▼
┌───────────────────────────────────────┐
│ KnowledgeSuggestion {                 │
│   suggestions: [{                     │
│     type: "rule",                     │
│     target_file: ".claude/rules/...", │
│     content: "...",                   │
│     reason: "..."                     │
│   }]                                  │
│ }                                     │
└──────────────┬────────────────────────┘
               │
               ▼
          PR 생성 or
          이슈 코멘트로 제안
```

#### 크로스 태스크 학습

단일 태스크가 아닌 축적된 전체 이력에서 패턴을 발견한다.
두 DB를 교차 조회하여 "무엇을 처리했는가" + "어떻게 실행했는가"를 결합:

```
                                 ┌────────────────────────────────┐
                                 │      suggest-workflow          │
                                 │      index.db (per-project)    │
   autodev.db                 │                                │
   (태스크 메타)                   │  sessions                     │
                                 │  ├ first_prompt_snippet        │
   issue_queue ──┐               │  │ "[autodev] fix:..."      │
   pr_queue    ──┤               │  ├ tool_uses (classified)      │
   merge_queue ──┤               │  ├ prompts                     │
   consumer_   ──┘               │  └ file_edits                  │
     logs                        │                                │
       │                         └───────────────┬────────────────┘
       │                                         │
       │              knowledge-extractor        │
       │              agent 교차 조회              │
       └────────────────┬────────────────────────┘
                        │
           ┌────────────┴─────────────┐
           │ 발견 가능한 패턴 예시:      │
           │                          │
           │ • 같은 모듈 반복 수정       │  ← file_edits + session_filter
           │ • 동일 유형 HITL 질문 반복  │  ← consumer_logs + prompts
           │ • 특정 에러 반복 발생       │  ← consumer_logs (exit_code)
           │ • 리뷰 지적사항 패턴        │  ← tool_uses (Bash:test 반복)
           │ • 테스트 실패 루프          │  ← repetition perspective
           └──────────────────────────┘
```

---

## End-to-End

```
┌──────────────────────────────────────────────────────────────────┐
│                         EVENT LOOP                                │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ SCAN: gh api polling (per-repo interval)                    │ │
│  │       filter_labels로 대상 이슈/PR 식별                       │ │
│  │       cursor 기반 증분 탐색                                   │ │
│  └──────────────────────┬──────────────────────────────────────┘ │
│                          │ 신규 아이템                              │
│                          ▼                                        │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐                      │
│  │ Issue Q  │  │  PR Q    │  │ Merge Q   │  autodev.db       │
│  │ (pending)│  │ (pending)│  │ (pending) │                      │
│  └────┬─────┘  └────┬─────┘  └─────┬─────┘                      │
│       │              │              │                             │
│  ┌────┴──────────────┴──────────────┴────────────────────────┐   │
│  │ PROCESS:                                                  │   │
│  │                                                           │   │
│  │  Issues:                                                  │   │
│  │    pending ──→ 분석(JSON)                                  │   │
│  │      ├─ high confidence ──→ 바로 구현                       │   │
│  │      │   claude -p "[autodev] fix: ..."                │   │
│  │      │                  ▲ 마커 삽입                         │   │
│  │      └─ low confidence  ──→ 이슈 댓글 + 대기                │   │
│  │                                                           │   │
│  │  waiting_human:                                           │   │
│  │    댓글 확인 → 답변 있으면 context 보강 후 구현               │   │
│  │                                                           │   │
│  │  PRs:                                                     │   │
│  │    pending ──→ 리뷰(JSON)                                  │   │
│  │      ├─ approve ──→ merge queue INSERT                     │   │
│  │      └─ request_changes ──→ inline 댓글                    │   │
│  │                                                           │   │
│  │  Merges:                                                  │   │
│  │    pending ──→ merge 실행                                  │   │
│  │      ├─ 성공 ──→ done                                      │   │
│  │      └─ conflict ──→ 자동 해결 시도                         │   │
│  └────────────────────────────┬──────────────────────────────┘   │
│                                │                                  │
│                           done 전이                               │
│                                │                                  │
│  ┌─────────────────────────────┴─────────────────────────────┐   │
│  │ KNOWLEDGE EXTRACTION:                                     │   │
│  │                                                           │   │
│  │  ┌─────────────────────────────────────────────────────┐  │   │
│  │  │ Claude Code 세션 (JSONL)                             │  │   │
│  │  │ "[autodev] fix: ..." → suggest-workflow 인덱싱     │  │   │
│  │  │ → first_prompt_snippet에 마커 저장                    │  │   │
│  │  └────────────────────────────┬────────────────────────┘  │   │
│  │                               │                           │   │
│  │  knowledge-extractor agent:   │                           │   │
│  │    1. autodev.db 조회      │ suggest-workflow query    │   │
│  │       (태스크 메타, 에러)      │ --session-filter          │   │
│  │    2. suggest-workflow 조회 ──┘ "[autodev]%"           │   │
│  │       (도구 패턴, 파일 수정)                               │   │
│  │    3. 교차 분석 → 인사이트 도출                             │   │
│  │    4. KnowledgeSuggestion → PR or 이슈 코멘트             │   │
│  └───────────────────────────────────────────────────────────┘   │
│                                │                                  │
│                           sleep(tick)                             │
└──────────────────────────────────────────────────────────────────┘
```

---

## Status Transitions

| Queue | Flow | Stuck Reset 대상 |
|-------|------|-----------------|
| Issue | `pending → analyzing → processing → done` | `analyzing`, `processing` |
| Issue | `pending → analyzing → waiting_human → processing → done` | `analyzing` only |
| Issue | `pending → analyzing → done` (wontfix) | - |
| PR | `pending → reviewing → approved / changes_requested` | `reviewing` |
| Merge | `pending → merging → done / conflict → done` | `merging`, `conflict` |

- `waiting_human`은 stuck reset 대상에서 제외 (사람 응답 대기는 의도적 대기)
- 별도 TTL 관리 권장 (예: 7일 후 자동 close)

---

## JSON Schemas

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

---

## Configuration

`/auto-setup` 위자드 또는 YAML 파일로 설정한다.

### 설정 파일

- **Global**: `~/.develop-workflow.yaml`
- **Per-repo**: `<workspace>/.develop-workflow.yaml` (레포별 override)

```yaml
consumer:
  scan_interval_secs: 300        # 스캔 주기 (1분/5분/15분/커스텀)
  scan_targets: [issues, pulls]  # 감시 대상
  issue_concurrency: 1           # 동시 처리 이슈 수
  pr_concurrency: 1              # 동시 처리 PR 수
  merge_concurrency: 1           # 동시 처리 merge 수
  model: sonnet                  # 사용 모델
  filter_labels: [autodev]    # 이 라벨이 있는 이슈/PR만 처리
  ignore_authors: [dependabot, renovate]
  stuck_threshold_secs: 1800     # stuck 판정 기준 (30분)
  confidence_threshold: 0.7      # 이 이상이면 자동 구현

workflow:
  issue: /develop-workflow:develop-auto
  pr: /develop-workflow:multi-review
```

---

## Setup

```bash
# 1. 모니터링할 레포 디렉토리에서 실행
cd my-project
/auto-setup

# 2. 위자드가 안내:
#    - 감시 대상 (Issues / PRs / 둘 다)
#    - 스캔 주기 (1분 / 5분 / 15분 / 커스텀)
#    - 필터 라벨, 무시 작성자
#    - 워크플로우 선택

# 3. 데몬 시작
autodev start

# 4. 상태 확인
autodev status
autodev dashboard
```
