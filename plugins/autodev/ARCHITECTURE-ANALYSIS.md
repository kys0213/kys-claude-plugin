# Autodev Architecture Analysis Report

> **Date**: 2026-02-22
> **Scope**: 설계(DESIGN.md) vs 구현(cli/src/) 전체 분석 + 리팩토링 방향 확정
> **이전 리포트 통합**: REVIEW-REPORT.md, GAP-ANALYSIS.md, REFACTORING-PLAN.md

---

## 1. Executive Summary

autodev 플러그인은 GitHub 이슈 분석/구현, PR 리뷰, 머지를 자동화하는 Rust 기반 데몬이다.
설계(DESIGN.md)는 **GitHub Labels SSOT + In-Memory StateQueue** 아키텍처를 정의했으나,
구현은 **SQLite 영속 큐 중심**으로 만들어졌다. 이 차이는 단순한 구현 누락이 아니라
**상태 관리 철학 자체가 다른 근본적 gap**이다.

본 리포트는 이 gap을 분석하고, 설계 방향으로의 리팩토링을 확정한다.
특히 **CLI 역할 재정의**를 통해 리팩토링 scope를 명확히 한다.

### 핵심 결론

| 항목 | 현재 (구현) | 목표 (설계) |
|------|------------|------------|
| 작업 큐 | SQLite 테이블 (issue_queue, pr_queue, merge_queue) | In-Memory StateQueue |
| 상태 SSOT | DB status 컬럼 | GitHub Labels (autodev:wip/done/skip) |
| 재시도 | CLI `queue retry` + DB retry_count | 라벨 제거 → 다음 scan에서 자동 재발견 |
| 복구 | DB stuck reset + auto retry | startup_reconcile (bounded 24h window) |
| CLI 역할 | 큐 제어 (retry, clear, list) | **읽기 전용 상태 조회** |
| 제어 인터페이스 | CLI 명령어 | **GitHub Labels (UI/API)** |

---

## 2. Current Implementation Analysis

### 2.1 아키텍처 (현재)

```
CLI (main.rs) ──→ SQLite ←── Daemon (main loop)
     │                           │
     │  queue retry              │  scan → insert
     │  queue list               │  process → update status
     │  repo add/remove          │  recovery → stuck reset
     │                           │
     └─── 별도 프로세스 ──────────┘
```

CLI와 Daemon이 **SQLite를 공유 매체**로 사용하여 통신한다.
CLI가 `queue retry`로 status를 'pending'으로 되돌리면, Daemon이 다음 tick에서 pick up한다.

### 2.2 SQLite 큐 (현재 schema)

```sql
-- 3개의 영속 큐 테이블
issue_queue   (status: pending/analyzing/ready/waiting_human/done/failed)
pr_queue      (status: pending/reviewing/review_done/failed)
merge_queue   (status: pending/merging/done/conflict/failed)

-- 보조 테이블
repositories   (레포 등록/관리)
scan_cursors   (incremental scan 최적화)
consumer_logs  (실행 감사 로그)
```

### 2.3 CLI 서브커맨드 (현재)

```
autodev start/stop/restart     # 데몬 제어
autodev status                 # 상태 조회 (DB에서 pending 카운트)
autodev repo add/list/config/remove  # 레포 관리
autodev queue list/retry/clear # 큐 제어 ← SQLite 직접 조작
autodev logs                   # 실행 로그 조회
autodev dashboard              # TUI 대시보드
```

### 2.4 상태 전이 (현재)

```
Issue: pending → analyzing → ready → processing → done/failed
       failed → (CLI retry) → pending (재시도)
       analyzing/processing → (stuck reset) → pending (timeout 복구)

PR:    pending → reviewing → review_done/failed
       (리뷰 1회로 종료, 피드백 루프 없음)

Merge: pending → merging → done/conflict/failed
       conflict → (해결 시도) → done/failed (재머지 사이클 없음)
```

---

## 3. Design vs Implementation Gaps

### 3.1 Fundamental Gaps

#### F-01: 작업 큐 — SQLite vs In-Memory

| | 설계 | 구현 |
|---|------|------|
| 큐 저장소 | `StateQueue<T>` (HashMap + VecDeque) | SQLite 테이블 3개 |
| 상태 전이 | `queue.transit(from, to)` | `UPDATE ... SET status = ?` |
| 중복 방지 | `index: HashMap<WorkId, State>` | `UNIQUE(repo_id, github_number)` + `active.rs` HashSet |
| 재시작 복구 | startup_reconcile (GitHub API 24h window) | DB에 이미 영속 (복구 불필요) |

**gap 원인**: 재시작 안전성을 DB 영속으로 해결했으나, 설계는 reconciliation으로 해결하도록 의도함.

#### F-02: 상태 SSOT — GitHub Labels vs DB status

| | 설계 | 구현 |
|---|------|------|
| SSOT | GitHub Labels | DB `status` 컬럼 |
| label_add | scan 시 `autodev:wip` 설정 | **미구현** (trait에 메서드 없음) |
| label_remove | done/skip/failure 시 전이 | recovery에서 wip 제거만 |
| 외부 가시성 | GitHub UI에서 상태 확인 가능 | DB 직접 조회 필요 |

### 3.2 Major Gaps

| ID | 설계 | 구현 | 영향 |
|----|------|------|------|
| M-01 | startup_reconcile (24h bounded) | stuck reset + auto retry (DB) | F-01에 종속 |
| M-02 | PR 리뷰→개선→재리뷰 사이클 | 1회 리뷰로 종료 (dead end) | 리뷰 품질 |
| M-03 | Merge conflict→재머지 사이클 | 해결 후 바로 done | 머지 안정성 |
| M-04 | Knowledge Extraction (per-task + daily) | 미구현 | 학습 피드백 |

### 3.3 구현이 설계보다 나은 부분

| 항목 | 설계 | 구현 | 평가 |
|------|------|------|------|
| Pre-flight check | 불필요 (scan에서 확인) | 매번 GitHub API 재확인 | 구현이 안전 (시차 고려) |
| Retry/stuck recovery | 미정의 | retry_count + stuck_threshold | 구현이 실용적 |
| Concurrency control | 미정의 | issue/pr/merge_concurrency 설정 | 구현이 실용적 |

---

## 4. CLI 역할 재정의 (핵심 결론)

### 4.1 문제: CLI-Daemon 간 통신

현재 CLI와 Daemon은 **SQLite를 공유 매체**로 사용한다.
In-Memory 큐로 전환하면 CLI에서 큐 데이터에 접근할 수 없다.

```
현재:  CLI ──→ SQLite ←── Daemon    (공유 DB로 통신)
목표:  CLI ──→ ??? ←── Daemon       (메모리 큐는 Daemon 프로세스 내부)
```

### 4.2 해법: CLI = 읽기 전용, 제어 = GitHub Labels

In-Memory 큐 전환 시, CLI에서 `queue retry/clear` 같은 **쓰기 명령이 불가능**해진다.
이를 해결하려면 IPC (Unix socket, gRPC 등)가 필요한데, 이는 복잡도를 크게 높인다.

그러나 **GitHub Labels SSOT** 설계에서는 이 문제가 자연스럽게 해소된다:

```
재시도:     GitHub에서 autodev:wip 제거 → 라벨 없음 → 다음 scan에서 자동 재발견
skip 해제:  GitHub에서 autodev:skip 제거 → 다음 scan에서 자동 재발견
강제 재처리: GitHub에서 autodev:done 제거 → 다음 scan에서 자동 재발견
```

**GitHub Labels 자체가 제어 인터페이스**이므로 CLI에 쓰기 명령이 필요 없다.

### 4.3 CLI 서브커맨드 (목표)

```
# 데몬 제어 (유지)
autodev start              # 데몬 시작
autodev stop               # 데몬 중지
autodev restart             # 데몬 재시작

# 레포 관리 (유지 — SQLite repositories 테이블)
autodev repo add <url>      # 레포 등록
autodev repo list           # 등록된 레포 목록
autodev repo config <name>  # 레포별 설정 확인
autodev repo remove <name>  # 레포 제거

# 상태 조회 (GitHub API 기반으로 전환)
autodev status              # 데몬 상태 + 레포별 라벨 통계

# TUI (데몬 내부 상태 + GitHub API)
autodev dashboard           # TUI 대시보드

# 제거
# autodev queue list        ← GitHub UI로 대체
# autodev queue retry       ← GitHub 라벨 제거로 대체
# autodev queue clear       ← 불필요
# autodev logs              ← dashboard에 통합 또는 로그 파일 직접 확인
```

### 4.4 `autodev status` 출력 (목표)

```
$ autodev status

Daemon:  running (pid 1234, uptime 2h 15m)

Repositories:
  kys0213/my-project (enabled)
    Issues:  autodev:wip 1 | autodev:done 12 | autodev:skip 3 | pending 2
    PRs:     autodev:wip 0 | autodev:done 8  | autodev:skip 1 | pending 0
```

`status`는 GitHub API를 호출하여 라벨별 카운트를 보여준다.
"pending"은 `라벨 없는 open 이슈/PR` 수로 계산한다.

---

## 5. Code Review Issues (우선순위별)

### 5.1 Critical (리팩토링 시 자동 해소)

| ID | 내용 | 파일 | 리팩토링 영향 |
|----|------|------|-------------|
| C-01 | repo_remove 트랜잭션 없음 | queue/repository.rs | 큐 테이블 제거 시 자동 해소 |
| C-04 | issue_insert 반환값 오류 (upsert) | queue/repository.rs | 큐 테이블 제거 시 자동 해소 |

### 5.2 Critical (리팩토링과 별개로 수정 필요)

| ID | 내용 | 파일 | 수정 방향 |
|----|------|------|----------|
| C-02 | git/real.rs path panic (.unwrap()) | infrastructure/git/real.rs | `.to_string_lossy()` 또는 `anyhow::bail!` |
| C-03 | Schema migration race condition | queue/schema.rs | `BEGIN EXCLUSIVE` 트랜잭션 |

### 5.3 High (리팩토링 시 함께 처리)

| ID | 내용 | 파일 | 방향 |
|----|------|------|------|
| H-01 | TUI handle_skip이 Repository 우회 | tui/mod.rs | 큐 제거 시 skip = 라벨 조작으로 전환 |
| H-02 | SQL string interpolation | queue/repository.rs, tui/mod.rs | 큐 테이블 제거 시 해소 |
| H-04 | PR 리뷰 결과 GitHub 미게시 | pipeline/pr.rs | 피드백 루프 구현 시 해결 |
| H-06 | Verdict를 String으로 관리 | claude/output.rs, pipeline/issue.rs | enum 전환 |
| H-07 | DESIGN.md와 구현 불일치 | daemon/mod.rs | 리팩토링 자체가 해결 |

### 5.4 High (리팩토링과 별개)

| ID | 내용 | 파일 | 수정 방향 |
|----|------|------|----------|
| H-03 | `#![allow(dead_code)]` 전역 억제 | main.rs, lib.rs | 제거 후 필요한 곳만 적용 |
| H-05 | worktree_remove 실패 무시 | infrastructure/git/real.rs | exit code 확인 추가 |

### 5.5 Medium/Low

리팩토링 이후 별도 작업으로 처리. 상세 내용은 REVIEW-REPORT.md 참조.

---

## 6. Refactoring Plan (확정)

### 6.1 아키텍처 (목표)

```
┌─────────────────────────────────────────────────────────────┐
│                    GitHub Labels (SSOT)                       │
│  autodev:wip  — 처리중                                       │
│  autodev:done — 완료 (영속 마커)                              │
│  autodev:skip — 건너뜀 (영속 마커)                            │
│  (없음)       — 미처리 → scan 대상                            │
│                                                              │
│  제어: GitHub UI/API에서 라벨 추가/제거                        │
│  재시도: 라벨 제거 → 다음 scan에서 자동 재발견                  │
└──────────────────────┬───────────────────────────────────────┘
                       │ gh api
┌──────────────────────▼───────────────────────────────────────┐
│                 SQLite (영속 관리)                             │
│  repositories  — 레포 등록/활성화                              │
│  scan_cursors  — incremental scan 최적화 (보조)               │
│  consumer_logs — 실행 감사 로그                                │
│                                                              │
│  ※ 작업 큐 테이블 없음 (issue_queue, pr_queue, merge_queue 제거)│
└──────────────────────┬───────────────────────────────────────┘
                       │
┌──────────────────────▼───────────────────────────────────────┐
│              In-Memory StateQueue (휘발)                      │
│  issues:  StateQueue<IssueItem>                               │
│  prs:     StateQueue<PrItem>                                  │
│  merges:  StateQueue<MergeItem>                               │
│  index:   HashMap<WorkId, State>  ← O(1) dedup               │
│                                                              │
│  재시작 시: startup_reconcile (bounded 24h) → 자동 복구        │
└──────────────────────────────────────────────────────────────┘

CLI (별도 프로세스):
  autodev status  → GitHub API 호출 → 라벨별 카운트 표시
  autodev repo *  → SQLite repositories 직접 조작
  autodev start/stop → PID 파일 기반 데몬 제어
```

### 6.2 라벨 상태 전이

```
(없음) ──scan──→ autodev:wip ──success──→ autodev:done
                     │
                     ├──skip────→ autodev:skip
                     │
                     ├──failure──→ (없음)   ← 자동 재시도
                     │
                     └──crash────→ autodev:wip (orphan)
                                     │
                                  recovery()
                                     │
                                     ▼
                                   (없음)   ← 자동 재시도
```

### 6.3 CLI 재시도 = 불필요

| 시나리오 | 현재 (CLI) | 목표 (Label) |
|---------|-----------|-------------|
| 실패 재시도 | `autodev queue retry <id>` | 자동 (wip 제거 → scan 재발견) |
| skip 재처리 | 불가 | GitHub에서 `autodev:skip` 제거 |
| 강제 재처리 | 불가 | GitHub에서 `autodev:done` 제거 |

**GitHub 라벨이 곧 제어 인터페이스**이므로 CLI 쓰기 명령이 불필요하다.

### 6.4 Phase별 구현 계획

```
Phase 1: StateQueue + TaskQueues 구현          [신규 2 + 수정 2]
  ├── queue/state_queue.rs  (StateQueue<T> 자료구조)
  ├── queue/task_queues.rs  (TaskQueues + dedup index)
  ├── queue/models.rs       (인메모리 모델로 교체)
  └── queue/mod.rs          (모듈 export)

Phase 2: Gh trait에 label_add 추가             [수정 3]
  ├── infrastructure/gh/mod.rs   (trait 메서드 추가)
  ├── infrastructure/gh/real.rs  (실구현)
  └── infrastructure/gh/mock.rs  (테스트용)

Phase 3: Scanner → StateQueue 전환             [수정 3]
  ├── scanner/mod.rs     (시그니처: ActiveItems → TaskQueues)
  ├── scanner/issues.rs  (DB insert → queues.push + label_add)
  └── scanner/pulls.rs   (동일 패턴)

Phase 4: Pipeline → StateQueue + 라벨 관리      [수정 4]
  ├── pipeline/mod.rs    (TaskQueues 주입)
  ├── pipeline/issue.rs  (DB query → queues.pop + 라벨 전이)
  ├── pipeline/pr.rs     (피드백 루프 추가)
  └── pipeline/merge.rs  (conflict→재머지 사이클)

Phase 5: Daemon 루프 + startup_reconcile        [수정 2]
  ├── daemon/mod.rs      (startup_reconcile + TaskQueues 소유)
  └── daemon/recovery.rs (ActiveItems → TaskQueues)

Phase 6: SQLite 큐 테이블 제거                  [수정 2]
  ├── queue/schema.rs      (issue/pr/merge_queue DDL 제거)
  └── queue/repository.rs  (Queue trait 제거, Repo/Cursor/Log만 유지)

Phase 7: CLI 정리                              [수정 2]
  ├── client/mod.rs  (queue 명령 제거, status를 GitHub API 기반으로)
  └── main.rs        (queue 서브커맨드 제거)

Phase 8: 테스트                                [신규 2 + 수정 다수]
  ├── queue/state_queue.rs   (unit tests)
  ├── queue/task_queues.rs   (unit tests)
  └── 기존 pipeline 테스트 수정 (Mock 패턴으로)
```

### 6.5 의존성 그래프

```
Phase 1 (StateQueue) ← 독립, TDD로 먼저 구현
    ↓
Phase 2 (label_add)  ← 독립, Phase 1과 병렬 가능
    ↓
Phase 3 (Scanner)    ← Phase 1 + 2 필요
    ↓
Phase 4 (Pipeline)   ← Phase 1 + 2 필요, Phase 3과 병렬 가능
    ↓
Phase 5 (Daemon)     ← Phase 3 + 4 필요
    ↓
Phase 6 (Cleanup)    ← Phase 5 이후
    ↓
Phase 7 (CLI)        ← Phase 6 이후
    ↓
Phase 8 (Tests)      ← 각 Phase마다 TDD로 병행
```

---

## 7. 삭제/유지 대상

### 삭제

| 대상 | 이유 |
|------|------|
| `active.rs` | TaskQueues.index로 대체 |
| `queue/schema.rs` 내 issue_queue, pr_queue, merge_queue DDL | 인메모리 전환 |
| `queue/repository.rs` 내 Issue/Pr/MergeQueueRepository trait | 인메모리 전환 |
| `queue/repository.rs` 내 QueueAdmin의 queue_retry/clear/reset_stuck | 라벨 기반 재시도로 대체 |
| `queue/models.rs` 내 DB 전용 모델 (IssueQueueItem 등) | 인메모리 모델로 교체 |
| `client/mod.rs` 내 queue_list/retry/clear | 라벨 기반 제어로 대체 |
| `main.rs` 내 queue 서브커맨드 | CLI 정리 |

### 유지 (변경 없음)

| 대상 | 이유 |
|------|------|
| `infrastructure/claude/` | 변경 불필요 |
| `infrastructure/git/` | 변경 불필요 (C-02, H-05는 별도 수정) |
| `components/workspace.rs` | 변경 불필요 |
| `components/verdict.rs` | 변경 불필요 |
| `components/reviewer.rs` | 변경 불필요 |
| `components/merger.rs` | 변경 불필요 |
| `config/` | reconcile_window_hours 추가만 |

### 유지 (수정)

| 대상 | 변경 내용 |
|------|----------|
| `infrastructure/gh/mod.rs` | label_add 메서드 추가 |
| `infrastructure/gh/real.rs` | label_add 구현 |
| `infrastructure/gh/mock.rs` | label_add mock |
| `scanner/` | DB → TaskQueues, 라벨 필터 추가 |
| `pipeline/` | DB → TaskQueues, 라벨 관리, PR 피드백 루프 |
| `daemon/` | startup_reconcile, TaskQueues 소유 |
| `client/mod.rs` | status를 GitHub API 기반으로, queue 명령 제거 |
| `tui/` | DB 직접 SQL → TaskQueues/GitHub API |

---

## 8. 사이드이펙트 분석

| 영역 | 영향 | 대응 |
|------|------|------|
| 데몬 재시작 | 큐 데이터 휘발 | startup_reconcile (24h bounded window) |
| consumer_logs | 유지 (SQLite) | 변경 없음 |
| retry_count | DB 필드 제거 | scan에서 자연 재시도 (라벨 없으면 재발견) |
| stuck recovery | DB 기반 불가 | 불필요 (메모리 큐는 프로세스 종료 시 자동 정리) |
| CLI queue 명령 | 제거 | GitHub Labels로 대체 |
| TUI dashboard | DB SQL 우회 코드 제거 필요 | TaskQueues 상태 + GitHub API로 전환 |
| GitHub API 비용 | startup_reconcile 시 bounded scan | reconcile_window_hours 설정으로 조절 |

---

## 9. Pre-flight Check 처리

### 현재: 매 처리마다 GitHub API 호출

```rust
// pipeline/issue.rs — process_pending
let state = gh.api_get_field(repo_name, number, "state", gh_host).await;
if state.as_deref() == Some("closed") {
    // mark done, skip processing
}
```

### 설계: pre-flight 불필요

scan 시점에 open 상태를 확인했으므로 consumer에서 재확인 불필요.
단, scan과 consume 사이 시차에서 상태 변경 가능성이 있다.

### 결론: pre-flight 유지 (구현이 더 안전)

scan_interval이 5분이면, scan 후 최대 5분간 상태 변경이 발생할 수 있다.
그 사이에 이슈가 닫히거나 PR이 머지될 수 있으므로, **pre-flight check를 유지**한다.
단, 라벨 SSOT에서는 pre-flight 실패 시 `wip 라벨 제거 + 큐에서 remove`로 처리한다.

---

## 10. 리팩토링 리스크 및 완화

| 리스크 | 확률 | 영향 | 완화 |
|--------|------|------|------|
| 24h 이상 데몬 다운 시 데이터 유실 | 낮음 | 중간 | reconcile_window_hours 설정 확대 + 모니터링 |
| GitHub API rate limit | 중간 | 높음 | cursor 기반 incremental scan 유지 |
| 라벨 수동 조작 실수 | 낮음 | 낮음 | autodev: prefix로 일반 라벨과 구분 |
| TUI 리팩토링 복잡도 | 중간 | 중간 | TUI는 별도 phase로 분리 |

---

## 11. Summary

### 확정된 방향

1. **In-Memory StateQueue**: SQLite 큐 테이블 제거, 메모리 자료구조로 전환
2. **GitHub Labels SSOT**: scan 시 `autodev:wip` 설정, 완료 시 `autodev:done` 전이
3. **CLI = 읽기 전용**: `autodev status`로 GitHub API 기반 상태 조회, 큐 제어 명령 제거
4. **제어 = GitHub Labels**: 재시도/skip 해제/강제 재처리 모두 라벨 조작으로 수행
5. **startup_reconcile**: 재시작 시 bounded 24h window로 메모리 큐 자동 복구
6. **PR 피드백 루프**: 리뷰→개선→재리뷰 사이클 구현
7. **pre-flight 유지**: scan-consume 시차 안전성 확보

### 이전 리포트와의 관계

| 리포트 | 역할 | 상태 |
|--------|------|------|
| REVIEW-REPORT.md | 코드 품질 이슈 25건 식별 | 본 리포트 Section 5에 통합 |
| GAP-ANALYSIS.md | 설계-구현 gap 10건 식별 | 본 리포트 Section 3에 통합 |
| REFACTORING-PLAN.md | 8-phase 리팩토링 계획 | 본 리포트 Section 6에 **CLI 정리 phase 추가**하여 확정 |
| **ARCHITECTURE-ANALYSIS.md** | **최종 통합 분석 + CLI 역할 재정의** | **본 리포트 (신규)** |
