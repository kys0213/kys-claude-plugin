# Autonomous Plugin

기존 플러그인 생태계(`develop-workflow`, `git-utils`, `external-llm`)를 이벤트 드리븐 루프로 자동 실행하는 오케스트레이션 레이어.

```
autodev (오케스트레이터)
  ├── develop-workflow  → /develop, /multi-review
  ├── git-utils         → /merge-pr, /commit-and-pr
  └── external-llm      → /invoke-codex, /invoke-gemini
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

Daily (매일 06:00):
  전일 daemon.YYYY-MM-DD.log 전체 + suggest-workflow 교차 분석
  → 일일 리포트 (GitHub 이슈) + 크로스 태스크 패턴 발견
  → KnowledgeSuggestion → 규칙 제안 PR
```

---

## Setup

```bash
# 1. 레포 등록
autodev repo add https://github.com/org/my-repo

# 2. 데몬 시작
autodev start

# 3. 상태 확인
autodev status
autodev dashboard

# 레포 관리
autodev repo list              # 등록된 레포 목록
autodev repo config org/repo   # 레포별 설정 확인
autodev repo remove org/repo   # 레포 제거
```

---

## Configuration

```yaml
# ~/.autodev/config.yaml
repos:
  - name: org/my-repo
    url: https://github.com/org/my-repo
    enabled: true
    scan_interval_secs: 300
    scan_targets: [issues, pulls]
    filter_labels: []
    ignore_authors: [dependabot, renovate]
    model: sonnet
    confidence_threshold: 0.7
    auto_merge: true               # approved PR 자동 머지
    merge_require_ci: true         # CI checks 통과 필수

daemon:
  tick_interval_secs: 10
  reconcile_window_hours: 24       # 재시작 시 복구 윈도우
  log_dir: ~/.autodev/logs         # 일자별 롤링 (daemon.YYYY-MM-DD.log)
  log_retention_days: 30
  daily_report_hour: 6             # 매일 06:00에 일일 리포트
```

---

상세 설계는 [DESIGN.md](./DESIGN.md) 참조.
