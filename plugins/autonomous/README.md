# Autonomous Plugin

기존 플러그인 생태계(`develop-workflow`, `git-utils`, `external-llm`)를 polling 기반 이벤트 루프로 자동 실행하는 오케스트레이션 레이어.

```
autonomous (오케스트레이터)
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
  │  filter_labels: ["autonomous"]
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
│ • filter_labels 매칭      │  ← "autonomous" 등 특정 라벨
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
          │   │ <!-- autonomous:waiting     │
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
<!-- autonomous:waiting --> 메타 태그 이후
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

### PR Flow - JSON 리뷰

리뷰 결과를 JSON으로 받아 verdict에 따라 결정적으로 분기한다.
approve 시 자동으로 merge queue에 삽입된다.

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
  │  run_claude(workflow.pr, json)
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
        ▼                 ▼
  merge_queue        changes_requested
  INSERT(pending)    (status 저장)
        │
        ▼
    approved
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

#### Insight CLI

```bash
# 완료 태스크 데이터를 SQLite에 인덱싱
autonomous insight index

# perspective 기반 쿼리 (사실만 반환)
autonomous insight query --perspective <name> [--param key=value]

# 에이전트가 작성한 커스텀 쿼리 (SELECT only)
autonomous insight query --sql-file /tmp/custom.sql
```

#### Perspectives (데이터 조회 관점)

| Perspective | 반환 데이터 | 용도 |
|-------------|-----------|------|
| `task-timeline` | 상태 전이 이벤트 시간순 | 병목 구간 파악 |
| `error-frequency` | 에러 메시지별 빈도 | 반복 실패 패턴 |
| `hitl-history` | HITL 질문/답변 쌍 | 맥락 부족 패턴 |
| `file-hotspots` | 파일별 수정 빈도 | 변경 집중 영역 |
| `duration-stats` | 단계별 소요 시간 | 성능 병목 |
| `cross-task` | 유사 태스크 비교 | 반복 패턴 발견 |
| `retry-history` | 재시도/실패 이력 | 안정성 문제 |

각 perspective는 파라미터화된 SQL 쿼리일 뿐, 분석 로직을 포함하지 않는다:

```rust
// perspectives.rs — 데이터 조회만, 해석 로직 없음
PerspectiveInfo {
    name: "error-frequency",
    params: vec![
        ParamDef { name: "repo", param_type: ParamType::Text, .. },
        ParamDef { name: "since", param_type: ParamType::Text, .. },
        ParamDef { name: "top", param_type: ParamType::Integer, default: "10", .. },
    ],
    sql: "SELECT error_message, COUNT(*) AS count, \
          GROUP_CONCAT(DISTINCT queue_type) AS queue_types \
          FROM consumer_logs \
          WHERE repo_id = :repo AND exit_code != 0 \
            AND started_at >= :since \
          GROUP BY error_message \
          ORDER BY count DESC \
          LIMIT :top",
}
```

#### 에이전트 탐색 흐름

```
done 전이 시
  │
  ▼
knowledge-extractor agent 시작
  │
  │  ── 1차 탐색 (전체 윤곽) ──
  │
  │  $CLI insight query --perspective task-timeline --param item_id=xxx
  │  → "analyzing 2분 → waiting_human 4시간 → processing 8분"
  │
  │  $CLI insight query --perspective error-frequency --param repo=org/repo
  │  → [{error: "timeout", count: 3}, {error: "parse error", count: 1}]
  │
  │  ── 2차 탐색 (drill-down) ──
  │
  │  (1차에서 timeout 3건 발견 → 관심 영역)
  │  $CLI insight query --perspective cross-task --param error=timeout
  │  → "issue-38, issue-42, issue-45 모두 external API 모듈에서 발생"
  │
  │  $CLI insight query --perspective file-hotspots --param repo=org/repo
  │  → [{file: "src/api/client.rs", edits: 12}, ...]
  │
  │  ── 3차 탐색 (커스텀 쿼리) ──
  │
  │  (에이전트가 직접 SQL 작성)
  │  $CLI insight query --sql-file /tmp/deep-dive.sql
  │  → "HITL 질문의 70%가 API 스펙 관련"
  │
  │  ── 인사이트 종합 ──
  │
  │  에이전트가 탐색 결과를 종합하여 판단:
  │  "API client 모듈에 timeout retry 규칙이 없고,
  │   HITL 질문도 대부분 API 스펙 관련
  │   → .claude/rules/api-client.md 에 가이드라인 추가 제안"
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

단일 태스크가 아닌 축적된 전체 이력에서 패턴을 발견한다:

```
                    ┌─────────────────────────┐
                    │ insight SQLite DB        │
                    │                          │
issue-38 done ─────→│ task_events             │
issue-42 done ─────→│ consumer_logs           │
pr-15 done    ─────→│ hitl_exchanges          │
issue-45 done ─────→│ file_changes            │
pr-18 done    ─────→│ review_comments         │
                    │                          │
                    └────────────┬─────────────┘
                                 │
                    에이전트가 perspectives로 조회
                                 │
                    ┌────────────┴─────────────┐
                    │ 발견 가능한 패턴 예시:      │
                    │                          │
                    │ • 같은 모듈 반복 수정       │
                    │ • 동일 유형 HITL 질문 반복  │
                    │ • 특정 에러 반복 발생       │
                    │ • 리뷰 지적사항 패턴        │
                    └──────────────────────────┘
```

---

## End-to-End

```
┌──────────────────────────────────────────────────────┐
│                    EVENT LOOP                         │
│                                                      │
│  ┌────────────────────────────────────────────────┐  │
│  │ SCAN: gh api polling (per-repo interval)       │  │
│  │       filter_labels로 대상 이슈/PR 식별          │  │
│  │       cursor 기반 증분 탐색                      │  │
│  └───────────────────┬────────────────────────────┘  │
│                      │ 신규 아이템                     │
│                      ▼                               │
│  ┌────────┐  ┌────────┐  ┌─────────┐                │
│  │ Issue  │  │  PR    │  │ Merge   │                │
│  │ Queue  │  │ Queue  │  │ Queue   │                │
│  └───┬────┘  └───┬────┘  └────┬────┘                │
│      │           │            │                      │
│  ┌───┴───────────┴────────────┴──────────────────┐   │
│  │ PROCESS:                                      │   │
│  │                                               │   │
│  │  Issues:                                      │   │
│  │    pending ──→ 분석(JSON)                      │   │
│  │      ├─ high confidence ──→ 바로 구현           │   │
│  │      └─ low confidence  ──→ 이슈 댓글 + 대기    │   │
│  │                                               │   │
│  │  waiting_human:                               │   │
│  │    댓글 확인 → 답변 있으면 context 보강 후 구현   │   │
│  │                                               │   │
│  │  PRs:                                         │   │
│  │    pending ──→ 리뷰(JSON)                      │   │
│  │      ├─ approve ──→ merge queue INSERT         │   │
│  │      └─ request_changes ──→ inline 댓글        │   │
│  │                                               │   │
│  │  Merges:                                      │   │
│  │    pending ──→ merge 실행                      │   │
│  │      ├─ 성공 ──→ done                          │   │
│  │      └─ conflict ──→ 자동 해결 시도             │   │
│  │                                               │   │
│  │  done 전이 시:                                 │   │
│  │    ──→ insight index (SQLite 축적)             │   │
│  │    ──→ knowledge-extractor agent              │   │
│  │       perspectives 탐색 → 인사이트 도출         │   │
│  └───────────────────────────────────────────────┘   │
│                      │                               │
│                 sleep(tick)                           │
└──────────────────────────────────────────────────────┘
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
  filter_labels: [autonomous]    # 이 라벨이 있는 이슈/PR만 처리
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
autonomous start

# 4. 상태 확인
autonomous status
autonomous dashboard
```
