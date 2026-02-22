# Autonomous Plugin

기존 플러그인 생태계(`develop-workflow`, `git-utils`, `external-llm`)를 이벤트 드리븐 루프로 자동 실행하는 오케스트레이션 레이어.

```
autodev (오케스트레이터)
  ├── develop-workflow  → /develop, /multi-review
  ├── git-utils         → /merge-pr, /commit-and-pr
  └── external-llm      → /invoke-codex, /invoke-gemini
```

---

## Installation

### Pre-built Binary

릴리즈 페이지에서 플랫폼에 맞는 바이너리를 다운로드합니다.

```bash
# macOS (Apple Silicon)
curl -L https://github.com/kys0213/kys-claude-plugin/releases/latest/download/autodev-darwin-aarch64.tar.gz | tar xz
sudo mv autodev /usr/local/bin/

# Linux (x86_64)
curl -L https://github.com/kys0213/kys-claude-plugin/releases/latest/download/autodev-linux-x86_64.tar.gz | tar xz
sudo mv autodev /usr/local/bin/
```

### Build from Source

```bash
cd plugins/autodev/cli
cargo build --release
# 바이너리: target/release/autodev
```

### Requirements

- **GitHub CLI** (`gh`): GitHub API 호출에 필요 — `gh auth login`으로 인증
- **Claude CLI** (`claude`): 분석/구현/리뷰 실행에 필요 — PATH에 등록되어야 함
- **Git**: worktree 생성에 필요

---

## Quick Start

```bash
# 1. 레포 등록
autodev repo add https://github.com/org/my-repo

# 2. 데몬 시작
autodev start

# 3. 상태 확인
autodev status

# 4. TUI 대시보드
autodev dashboard
```

---

## Architecture

### 3-Tier 상태 관리

```
GitHub Labels (SSOT, 영속)         SQLite (영속 관리)
┌──────────────────────┐          ┌──────────────────────────┐
│  autodev:done  (28)  │          │ repositories  — 레포 등록  │
│  autodev:skip  (5)   │          │ scan_cursors  — API 최적화 │
│  autodev:wip   (3)   │          │ consumer_logs — 감사 로그  │
│  (없음) = 미처리      │          └──────────────────────────┘
└──────────────────────┘
            │
     In-Memory StateQueue (휘발)
     ┌──────────────────────────────────┐
     │ issues[Pending]  → [Analyzing]   │
     │ prs[Reviewing]   → [Improving]   │
     │ merges[Merging]  → [Conflict]    │
     │ index: HashMap<WorkId, State>    │
     └──────────────────────────────────┘
```

- **GitHub 라벨 = SSOT** — 작업 완료 상태의 유일한 영속 마커
- **SQLite** — 레포 관리 + scan 커서(최적화) + 실행 로그(감사). 작업 큐는 저장하지 않음
- **In-Memory StateQueue** — 상태별 큐로 이벤트 드리븐 처리. 재시작 시 bounded reconciliation으로 자동 복구

### 코드 구조

```
plugins/autodev/cli/src/
├── main.rs              # CLI 진입점 (clap subcommands)
├── daemon/              # 데몬 루프 + PID + recovery
├── scanner/             # GitHub 이벤트 감지 (cursor 기반 incremental)
├── pipeline/            # 흐름 오케스트레이션 (issue, pr, merge)
├── components/          # 비즈니스 로직 (workspace, analyzer, reviewer, merger, notifier)
├── infrastructure/      # 외부 시스템 추상화 (gh, git, claude — trait + mock + real)
├── queue/               # SQLite 스키마 + repository 패턴
├── tui/                 # ratatui 기반 TUI 대시보드
└── config/              # YAML 설정 로드/merge
```

### 라벨 상태 전이

```
(없음) ──scan──→ autodev:wip ──success──→ autodev:done
                     │
                     ├──skip────→ autodev:skip
                     ├──failure──→ (없음)  ← 재시도
                     └──crash────→ recovery() → (없음)  ← 재시도
```

---

## Daemon Loop

```
startup:
  0. startup_reconcile()  — bounded recovery (cursor - 24h)
                            라벨 기반 필터 → memory queue 복구

loop (매 tick):
  1. recovery()    — orphan wip 라벨 정리
  2. scan()        — cursor 기반 incremental scan → 신규만 queue.push(Pending)
  3. consume()     — queue에서 pop → 이벤트 드리븐 처리 (pre-flight API 불필요)
  4. sleep(tick_interval)
```

---

## Flows

### Issue: 분석 → 구현 → PR

```
scan 발견 → wip + queue[Pending]
  → 분석(claude -p) → queue[Analyzing]
  ├─ implement  → queue[Ready] → 구현(claude -p) → PR 생성 → autodev:done
  ├─ clarify    → 댓글 + autodev:skip + queue.remove()
  └─ wontfix    → 댓글 + autodev:skip + queue.remove()
  실패 시 → 라벨 제거 + queue.remove() → 다음 scan에서 재발견
```

### PR: 리뷰 → 개선 → 재리뷰

```
scan 발견 → wip + queue[Pending]
  → 리뷰(/multi-review) → queue[Reviewing]
  ├─ approve → autodev:done + queue.remove()
  └─ request_changes → 인라인 댓글
       → queue[Improving] → 자동 개선(claude -p)
       → queue[Improved] → 재리뷰
       → approve 될 때까지 반복 → autodev:done
  실패 시 → 라벨 제거 + queue.remove() → 재시도
```

### Merge: 별도 큐

```
merge scan: approved + 라벨 없는 PR 발견 (사람/autodev approve 모두)
  → wip + queue[Pending] → 머지(/merge-pr) → queue[Merging]
  ├─ success  → autodev:done + queue.remove()
  ├─ conflict → queue[Conflict] → 자동 해결 시도 → 재머지
  └─ failure  → 라벨 제거 + queue.remove() → 재시도
```

### Knowledge Extraction

```
Per-task (done 전이 시):
  해당 세션 1건 분석 → 즉시 피드백 (이슈 코멘트)

Daily (매일 daily_report_hour):
  전일 daemon.YYYY-MM-DD.log 전체 + suggest-workflow 교차 분석
  → 일일 리포트 (GitHub 이슈) + 크로스 태스크 패턴 발견
  → KnowledgeSuggestion → 규칙 제안 PR (autodev:skip 라벨)
```

**Knowledge PR 생성**: DailyReport의 suggestions를 Git trait로 직접 파일 쓰기하여 PR 생성.
Claude 세션 불필요 — `Suggestion.target_file`과 `content`를 그대로 사용한다.
PR에 `autodev:skip` 라벨을 부착하여 스캐너가 자동 처리하지 않도록 한다 (사람이 리뷰 후 수동 merge).

**`[autodev]` 세션 마커**: 모든 `claude -p` 호출의 프롬프트 앞에 `[autodev] {action}: {context}` 마커 삽입.
suggest-workflow가 autodev 세션을 식별하는 데 사용된다.

---

## CLI Commands

```bash
# 데몬 제어
autodev start              # 데몬 시작 (포그라운드, 단일 인스턴스)
autodev stop               # 데몬 중지 (PID → SIGTERM)
autodev restart            # 데몬 재시작

# 레포 관리
autodev repo add <url>     # 레포 등록 (URL에서 name 자동 추출)
autodev repo list          # 등록된 레포 목록 (enabled/disabled 표시)
autodev repo config <name> # 레포별 설정 확인 (글로벌 + 워크스페이스 merge 결과)
autodev repo remove <name> # 레포 제거

# 상태 조회
autodev status             # 데몬 상태 + 큐 깊이 요약 + 레포별 통계
autodev dashboard          # TUI 대시보드

# 큐 관리
autodev queue list <repo>  # 큐 상태 확인
autodev queue retry <id>   # 실패 항목 재시도
autodev queue clear <repo> # 큐 비우기 (done/failed 항목 삭제)

# 실행 로그
autodev logs               # 최근 실행 로그 (기본 20건)
autodev logs <repo> -n 50  # 레포별 로그, 건수 지정
```

---

## TUI Dashboard

`autodev dashboard` 실행 시 ratatui 기반 터미널 대시보드 표시.

```
┌─────────────────────────────────────────────────────────┐
│  autodev v0.1.0          ● running    │  3 repos │ [?]  │
├──────────┬──────────────────────────────────────────────┤
│          │  [I] org/repo#42  analyzing   Bug fix        │
│  Repos   │  [P] org/repo#10  reviewing   Add feature    │
│  ● repo1 │  [M] org/repo#15  merging     Release v2     │
│  ● repo2 ├──────────────────────────────────────────────┤
│  ○ repo3 │  autodev:wip    3  ███░░░░░░░░░░░░           │
│          │  autodev:done  28  ████████████████           │
│          │  autodev:skip   0  ░░░░░░░░░░░░░░░           │
│          │  failed         1  █░░░░░░░░░░░░░░           │
│          ├──────────────────────────────────────────────┤
│          │  14:32 INFO starting daemon                  │
│          │  14:30 WARN retrying item                    │
│          │  14:28 ERROR scan failed: timeout            │
└──────────┴──────────────────────────────────────────────┘
 Tab:panel  j/k:navigate  r:retry  s:skip  R:refresh  q:quit
```

### Key Bindings

| Key | Action |
|-----|--------|
| `Tab` | 패널 전환 (Repos → ActiveItems → Labels → Logs) |
| `j` / `↓` | 목록 아래로 이동 |
| `k` / `↑` | 목록 위로 이동 |
| `r` | 실패 항목 재시도 (ActiveItems 패널에서) |
| `s` | 항목 건너뛰기 (ActiveItems 패널에서) |
| `R` | 로그 새로고침 |
| `?` | 도움말 토글 |
| `q` | 종료 |

---

## Configuration

```yaml
# ~/.autodev/config.yaml
repos:
  - name: org/my-repo
    url: https://github.com/org/my-repo
    enabled: true
    scan_interval_secs: 300       # scan 주기 (기본 5분)
    scan_targets: [issues, pulls]
    filter_labels: []              # 빈 배열 = 전체 대상
    ignore_authors: [dependabot, renovate]
    model: sonnet                  # claude -p 모델
    confidence_threshold: 0.7      # 분석 자동 구현 최소 신뢰도
    auto_merge: true               # approved PR 자동 머지
    merge_require_ci: true         # CI checks 통과 필수

daemon:
  tick_interval_secs: 10           # 메인 루프 주기 (초)
  reconcile_window_hours: 24       # 재시작 시 복구 윈도우 (시간)
  log_dir: ~/.autodev/logs         # 일자별 롤링 (daemon.YYYY-MM-DD.log)
  log_retention_days: 30           # 로그 보존 기간 (일)
  daily_report_hour: 6             # 매일 06:00에 일일 리포트
```

> **daemon 섹션**: daemon tick loop에서만 사용하는 설정.
> `daily_report_hour`, `reconcile_window_hours` 등은 `DaemonConfig` 구조체에 매핑된다.

### File Locations

```
~/.autodev/
├── config.yaml          # 글로벌 설정
├── autodev.db           # SQLite (repositories, scan_cursors, consumer_logs)
├── daemon.pid           # PID 파일 (단일 인스턴스 보장)
├── workspaces/          # 레포별 워크스페이스
│   └── {org}/{repo}/
│       ├── main/        # base clone (장기 유지)
│       └── issue-42/    # worktree (태스크별 격리)
└── logs/
    ├── daemon.2026-02-22.log   # 일자별 롤링
    └── ...
```

---

## Slash Commands

플러그인 설치 후 Claude Code에서 사용 가능한 슬래시 커맨드:

| Command | Description |
|---------|-------------|
| `/auto` | 데몬 시작/중지/상태 확인 |
| `/auto-setup` | 레포 등록 위자드 |
| `/auto-config` | 설정 변경 |
| `/auto-dashboard` | TUI 대시보드 열기 |

---

상세 설계는 [DESIGN.md](./DESIGN.md) 참조.
