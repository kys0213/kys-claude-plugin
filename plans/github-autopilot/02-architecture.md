# 02. Architecture

> 01 의 13개 유스케이스를 만족시키는 모듈 경계와 의존성 방향, 트랜잭션 / 동시성 정책을 정의한다. 본 문서가 정한 인터페이스는 03 (상세 스펙) 에서 시그니처로 구체화된다.

## 1. 설계 원칙 적용

| 원칙 | 본 시스템에서의 의미 |
|------|------------------|
| **SRP** | autopilot 은 ledger + CLI 만 담당. 탐지 / 결정 / 구현은 외부 Agent |
| **OCP** | 새 EventKind / 새 TaskStatus 추가 시 도메인 enum 확장만으로 흡수 |
| **LSP** | 인메모리 TaskStore 와 SQLite TaskStore 가 동일 계약 (트랜잭션 의미 포함) 을 만족 |
| **ISP** | 큰 단일 trait 대신 `EpicRepo` / `TaskRepo` / `EventLog` / `SuppressionRepo` 로 분리. `TaskStore` 는 슈퍼트레이트 |
| **DIP** | CLI 진입점은 trait 에만 의존. 어댑터는 컴포지션 루트에서 주입 |

## 2. 레이어와 의존성 방향

autopilot 자체는 단일 프로세스 / 단일 책임이다. **Agent 는 외부** 요소로, autopilot 의 의존 그래프 밖에 있다.

```
                 ┌────────────────────────────────────┐
                 │  Agent (Claude Code 세션, autodev,  │
                 │   GitHub Actions, ...)              │
                 │  - monitor 호출 / 분해 / 구현 / PR │
                 │  - escalation 이슈 발행            │
                 └─────────────┬──────────────────────┘
                               │ CLI 호출 (autopilot ...)
                               ▼
+--------------------------------------------------------+
|                cmd / CLI 진입점                         |  <- 컴포지션 루트
|  (epic, task, suppress, events, worktree, labels, ...) |
+----------------------------+---------------------------+
                             |
+----------------------------v---------------------------+
|                       Ports (trait)                    |
|  TaskStore (= EpicRepo + TaskRepo + EventLog +         |
|              SuppressionRepo)         Clock            |
+----------------------------+---------------------------+
                             |
+----------------------------v---------------------------+
|                       Adapters                         |
|  InMemoryTaskStore   SqliteTaskStore   StdClock        |
+--------------------------------------------------------+
                             ^
                             |
+----------------------------+---------------------------+
|                   Domain (pure)                        |
|  Epic  Task  TaskId  TaskStatus  TaskSource            |
|  TaskFailureOutcome  Event  EventKind  TaskGraph       |
|  DomainError                                           |
+--------------------------------------------------------+
```

엄격한 규칙:

- **Domain 모듈은 어떤 외부 의존성도 갖지 않는다** — std + serde + chrono 만 허용
- **CLI cmd 는 trait 만 호출** — adapter 를 직접 생성하지 않음 (`main.rs` 가 wiring)
- **Adapter 는 다른 adapter 를 직접 호출하지 않는다**
- **autopilot 코드는 git/github/Claude Code/spec-kit 을 직접 호출하지 않는다** — 그 책임은 agent 의 영역

기존 헬퍼 명령 (`worktree`, `pipeline idle`, `watch run`, `issue`, `labels`, ...) 은 이 다이어그램의 **CLI 진입점** 에 함께 위치한다. 그들은 ledger 와 무관하게 자체 어댑터 (`GhOps`, `GitOps`, `FsOps`) 를 사용하며, agent 가 호출하는 도구 모음으로 분류된다.

## 3. 모듈 배치 (Rust)

```
plugins/github-autopilot/cli/src/
├── main.rs                          # 컴포지션 루트
├── lib.rs                           # pub mod 선언
│
├── domain/                          # pure
│   ├── mod.rs
│   ├── epic.rs
│   ├── task.rs
│   ├── task_id.rs
│   ├── deps.rs
│   ├── event.rs
│   └── error.rs
│
├── ports/                           # trait only
│   ├── mod.rs
│   ├── task_store.rs                # TaskStore = EpicRepo + TaskRepo
│   │                                #             + EventLog + SuppressionRepo
│   └── clock.rs
│
├── store/
│   ├── mod.rs
│   ├── memory.rs                    # InMemoryTaskStore
│   ├── sqlite.rs                    # SqliteTaskStore
│   └── migrations/                  # V1, V2, ...
│
├── cmd/
│   ├── mod.rs
│   ├── epic.rs                      # ledger CLI (신규)
│   ├── task.rs                      # ledger CLI (확장)
│   ├── suppress.rs                  # ledger CLI (신규)
│   ├── events.rs                    # ledger CLI (신규)
│   │
│   ├── pipeline.rs                  # 기존 헬퍼 (idle 등 유지)
│   ├── watch/                       # 기존 헬퍼 (이벤트 emitter)
│   ├── worktree.rs                  # 기존 헬퍼
│   ├── issue.rs, issue_list.rs      # 기존 헬퍼
│   ├── labels.rs                    # 기존 헬퍼
│   ├── preflight.rs, simhash.rs     # 기존 헬퍼
│   ├── stats.rs, check/             # 기존 헬퍼
│   └── ...
│
├── git.rs, github.rs, gh.rs, fs.rs  # 기존 헬퍼 어댑터 (GitOps/GhOps/...)
└── ...
```

기존 `git.rs`, `github.rs`, `gh.rs`, `fs.rs` 와 `cmd/pipeline.rs`, `cmd/watch/`, `cmd/worktree.rs`, `cmd/issue*.rs`, `cmd/labels.rs`, `cmd/check/`, `cmd/preflight.rs`, `cmd/simhash.rs`, `cmd/stats.rs` 는 그대로 유지한다. **그들은 ledger 와 결합되지 않으며**, agent 가 자기 워크플로에서 자유롭게 호출한다.

## 4. 핵심 포트 (인터페이스 윤곽)

상세 시그니처는 03 에서 다루며, 본 절은 책임과 역할 경계만 명시한다.

### 4.1 TaskStore

DB 의 epic / task / dep / event / suppression 행에 대한 CRUD + 상태 전이.

- **책임**: 원자적 상태 전이, 결정적 ID 보존, 이벤트 기록, fingerprint 억제
- **비책임**: spec 분해, git 호출, 이슈 발행, 알림, 의사결정
- **분할 trait** (ISP):
  - `EpicRepo`: epic 행 + spec_path 매칭
  - `TaskRepo`: task 행 + deps + claim/release/complete/fail/escalate/force-status + reconcile 적용
  - `EventLog`: event 행 append/query
  - `SuppressionRepo`: fingerprint × reason → until 매핑
- 슈퍼트레이트 `TaskStore: EpicRepo + TaskRepo + EventLog + SuppressionRepo` 로 통합

### 4.2 Clock

테스트 격리용 시간 주입. autopilot 자체가 직접 의존하는 유일한 외부 포트.

- 구현체: `StdClock`, `FixedClock` (테스트)

### 4.3 외부 의존 도구 (autopilot 내부 포트가 아님)

다음은 **agent 의 책임 영역** 으로, autopilot 의 trait 시스템에 들어가지 않는다:

- spec 분해기 (spec-kit / 자체 markdown 파서 / LLM prompt — agent 가 채택)
- git/GitHub 클라이언트 (agent 가 사용. autopilot 의 기존 헬퍼 명령은 그 위에서 동작)
- 알림 (notification — agent 가 자기 알림 채널 사용)
- 분해 결과 캐시 / artifact 저장 — agent 가 책임

이 분리 덕분에 autopilot 은 단일 의존성 (SQLite + Clock) 만 가지며, 테스트 / 배포 / 마이그레이션이 단순하다.

## 5. 트랜잭션 경계

DB 의 일관성 보장은 `TaskStore` 의 메서드 단위에서 보장된다. 호출자(CLI)는 트랜잭션을 직접 열지 않는다.

| 메서드 | 트랜잭션 범위 |
|--------|------------|
| `insert_epic_with_tasks` | epic + 모든 task + 모든 dep + 진입점 ready 승격을 한 tx |
| `claim_next_task` | 후보 SELECT + UPDATE WHERE status='ready' + claimed event INSERT 한 tx (changes()==1 검증) |
| `complete_task_and_unblock` | task done UPDATE + 이 task 에 의존하던 blocked 들의 deps 재평가 + ready 승격 + completed event 한 tx |
| `mark_task_failed` | attempts 증가 + (재시도 가능이면) ready 복귀 / (max 도달이면) escalated 전이 + 의존 task 들 blocked + event 한 tx. 반환값으로 outcome 알림 |
| `escalate_task` | escalated 전이 + escalated_issue 기록 + event 한 tx |
| `release_claim` | wip → ready + attempts -1 한 tx |
| `force_status` | 임의 상태 → target + force_status event 한 tx (자식 unblock 안 함) |
| `apply_reconciliation` | reconcile 결과 (변경 행들) 을 한 tx 로 반영. orphan 처리 포함 |
| `append_event` (단일) | 자동 commit |

원자성 검증 규칙: 모든 상태 전이 UPDATE 는 `WHERE` 에 기대 상태 (e.g. `status='ready'`) 를 명시하고 changes() 를 검사한다. 0 이면 동시 변경자가 있었던 것이므로 호출자에게 명시적 결과로 알린다 (예: `claim_next_task` 가 None 반환).

## 6. 동시성 정책

### 6.1 단일 머신 — 다중 CLI 호출

배포 모델은 **머신당 `.autopilot/state.db` 1개**. autopilot 은 **장수 데몬을 안 만든다**. 매 호출이 별도 프로세스 — cron tick 의 `autopilot task claim` 과 운영자의 `autopilot task force-status` 가 동시 실행될 수 있다.

### 6.2 SQLite 모드

```
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;       -- 5s
PRAGMA foreign_keys = ON;
```

- WAL: 다중 reader / 단일 writer 동시 동작
- BUSY 5s: 짧은 충돌 자연 해소
- FK: deps / events 의 무결성

이 셋만으로 다중 프로세스 호출이 안전하다. 데이터 손상은 없다.

### 6.3 Race window 와 정책

상태 전이 메서드의 SELECT-then-UPDATE 가 별 트랜잭션이면 race 시 audit payload 에 약간의 부정확이 가능 (예: `force_status` 의 `from` 이 직전 값과 어긋남). 이 정도는 **운영 모델에서 수용** — autopilot 은 spec 의 단일 작가 가정 하에 잘 동작하며, multi-process 도 데이터 무결성 위반 없이 동작.

`claim_next_task` 의 원자성 (단일 UPDATE WHERE) 만은 엄격히 보장한다 — task 가 두 caller 에 동시 할당되면 git push reject 로 자연 차단되긴 하나, DB 차원에서도 한 명만 winner 가 되도록 한다 (UC-11).

### 6.4 git push 차원의 동시성

UC-11 (두 머신 / 두 프로세스가 같은 task 를 동시에 claim 후 push) 은 git remote 가 자연 차단 (non-fast-forward reject). agent 는 push 결과를 검사하고 reject 시 `autopilot task release` 호출.

## 7. 에러 처리 정책

### 7.1 에러 분류

| 분류 | 예시 | 처리 |
|------|------|------|
| **Domain error** | invalid status transition, dep cycle, inconsistency | 호출자 (agent) 에게 `Result::Err`. CLI 가 사용자용 메시지 + non-zero exit code 로 변환 |
| **Storage error** | DB 락, 디스크 full, schema mismatch | BUSY 재시도 후 실패 시 에러 종료. agent 가 retry 결정 |
| **CLI usage error** | 잘못된 인자, 누락 옵션 | clap 이 처리. exit 1 |

autopilot 자체는 네트워크 / git / GitHub 에러를 다루지 않는다 — agent 의 영역.

### 7.2 에러 타입

`thiserror` 기반 enum 을 도메인 / 포트별로 분리:

- `domain::DomainError` (DepCycle, IllegalTransition, EpicAlreadyExists, UnknownDepTarget, DuplicateTaskId, Inconsistency)
- `ports::TaskStoreError` (Busy, Backend, SchemaMismatch, NotFound, Domain)

CLI 진입점에서 사용자용 메시지로 변환.

## 8. 설정 / 컴포지션 루트

`autopilot.toml` (기존 위치 유지) 의 ledger 관련 항목은 단순:

```toml
[storage]
db_path = ".autopilot/state.db"

[epic]
max_attempts = 3                        # task fail 시 escalate 임계
hitl_label   = "autopilot:hitl-needed"  # 정보용; agent 가 사용
```

기존 헬퍼 (gh / git / watch / pipeline ...) 의 설정은 별도 섹션이며 ledger 와 독립.

`main.rs` 가 컴포지션 루트로서 다음을 수행:

1. 설정 로드 + 검증
2. SQLite 연결 + 마이그레이션 적용 (V1 → V2 → ...)
3. `StdClock` 인스턴스화
4. clap 서브커맨드 디스패치 (각 cmd 핸들러에 store + clock 주입)

테스트는 4번에서 `InMemoryTaskStore` + `FixedClock` 주입.

## 9. 마이그레이션 정책

스키마 변경은 `migrations/` 폴더의 numbered SQL 을 순서대로 적용:

```
src/store/migrations/
├── V1__initial.sql
├── V2__lookup_indexes.sql
└── ...
```

`meta(key='schema_version')` 행으로 적용 상태 추적. 시작 시 누락된 버전 적용. 다운그레이드는 비지원 (사용자에게 DB 삭제 + reconcile 안내).

## 10. 인터페이스 ↔ UC 매핑

각 UC 의 핵심은 "agent 가 호출하는 CLI 시퀀스" 와 "autopilot 이 트리거하는 trait 메서드" 로 표현된다.

| UC | Agent CLI 호출 시퀀스 | Autopilot trait 메서드 |
|----|--------------------|----------------------|
| 1 (epic-start) | `epic create` → `task add` × N (또는 batch) → `git push` (헬퍼) | `EpicRepo::upsert_epic`, `TaskRepo::insert_epic_with_tasks` |
| 2 (task 자동 구현) | `task claim` → 구현 → `task complete --pr N` (또는 fail / release) | `TaskRepo::claim_next_task`, `complete_task_and_unblock`, `mark_task_failed`, `release_claim` |
| 3 (deps) | (UC-2 와 동일) | `claim_next_task` 의 deps 필터, `complete_task_and_unblock` 의 unblock 로직 |
| 4-5 (resume / 복구) | monitor 호출 → git scan → `epic reconcile --plan ...` | `TaskRepo::apply_reconciliation` |
| 6 (watch 매칭) | monitor 호출 → `epic find-by-spec-path` → `task add` | `EpicRepo::find_active_by_spec_path`, `TaskRepo::upsert_watch_task` |
| 7 (watch 미매칭) | `suppress check` → 신규면 GitHub 이슈 발행 → `suppress add` | `SuppressionRepo::is_suppressed`, `suppress` |
| 8 (반복 실패) | `task fail` → outcome=Escalated 면 GitHub 이슈 + `task escalate` | `TaskRepo::mark_task_failed`, `escalate_task` |
| 9 (escalation 해소) | issue close 감지 → `epic reconcile` 또는 `suppress add reason='rejected_by_human'` | `apply_reconciliation`, `SuppressionRepo::suppress` |
| 10 (epic 완료) | `epic status` 가 all-done → `epic complete` + 알림 | `EpicRepo::set_epic_status` |
| 11 (동시 push 차단) | push reject 감지 → `task release` | `TaskRepo::release_claim` (attempts -1) |
| 12 (epic 중단) | `epic abandon` | `EpicRepo::set_epic_status` |
| 13 (마이그레이션) | 사람 이슈 본문 → spec 매칭 → `task add --source=human` + 라벨 정리 | `TaskRepo::upsert_watch_task` (source=Human) |

## 11. 테스트 가능성

각 어댑터는 LSP 를 만족하는 인메모리 / fake 구현을 가진다:

- `InMemoryTaskStore`: HashMap/BTreeMap 기반, 동일 트랜잭션 의미 (단일 mutex)
- `FixedClock`: 테스트 시 시간 고정

orchestration 어댑터 (FakeGit, FakeGitHub, FakeDecomposer, FakeNotifier) 는 **본 spec 의 책임이 아니다** — agent 측에서 자기 워크플로에 맞춰 격리한다.

블랙박스 테스트는 `TaskStore` 의 conformance suite 로 양 어댑터에 동일 검증을 적용 (04 §4).
