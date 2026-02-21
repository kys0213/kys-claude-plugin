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

### 상태 관리: GitHub 라벨 (SSOT) + 인메모리

```
GitHub (SSOT, 영속)              daemon (인메모리, 휘발)
┌──────────────────────┐        ┌──────────────────────────────┐
│ Labels:              │        │ ActiveItems: HashMap          │
│  autodev:wip   (3)   │  ←──→  │  issue:org/repo:42 → Analyzing│
│  autodev:done  (28)  │        │  pr:org/repo:10    → Reviewing│
│  autodev:skip  (5)   │        └──────────────────────────────┘
│  (없음) = 미처리      │
└──────────────────────┘
```

- **SQLite 없음** — GitHub 라벨이 영속 상태, HashMap이 런타임 추적
- 데몬 크래시 → 재시작 시 recovery()가 orphan `autodev:wip` 정리 → 자동 복구

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
loop (매 tick)
  │
  ├─ 1. recovery()   — orphan wip 라벨 정리
  ├─ 2. scan()       — 라벨 없는 이슈/PR 발견 → wip + active 등록
  ├─ 3. process()    — Phase별 작업 실행 → done/skip/재시도
  └─ 4. sleep(interval)
```

---

## Flows

### Issue: 분석 → 구현 → PR

```
scan 발견 → autodev:wip → 분석(claude -p)
  ├─ implement  → 구현(claude -p) → PR 생성 → autodev:done
  ├─ clarify    → 댓글 + autodev:skip
  └─ wontfix    → 댓글 + autodev:skip
  실패 시 → 라벨 제거 → 다음 tick 재시도
```

### PR: 리뷰 → 개선 → 재리뷰

```
scan 발견 → autodev:wip → 리뷰(/multi-review)
  ├─ approve → autodev:done (리뷰 완료)
  └─ request_changes → 인라인 댓글
       → 자동 개선(claude -p /develop implement feedback)
       → 재리뷰(/multi-review)
       → approve 될 때까지 반복 → autodev:done
  실패 시 → 라벨 제거 → 다음 tick 재시도
```

### Merge: 별도 큐

```
merge scan: approved + 라벨 없는 PR 발견 (사람/autodev approve 모두)
  → autodev:wip → 머지(/merge-pr)
  ├─ success → autodev:done
  ├─ conflict → 자동 해결 시도 → 재머지
  └─ failure → 라벨 제거 → 재시도
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
# 1. 모니터링할 레포 디렉토리에서 실행
cd my-project
/auto-setup

# 2. 데몬 시작
autodev start

# 3. 상태 확인
autodev status
autodev dashboard
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

daemon:
  tick_interval_secs: 10
  log_dir: ~/.autodev/logs           # 일자별 롤링 (daemon.YYYY-MM-DD.log)
  log_retention_days: 30
  daily_report_hour: 6               # 매일 06:00에 일일 리포트
```

---

상세 설계는 [DESIGN.md](./DESIGN.md) 참조.
