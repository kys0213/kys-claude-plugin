# 02. Architecture

> 01 의 13개 유스케이스를 만족시키는 모듈 경계와 의존성 방향, 트랜잭션 / 동시성 정책을 정의한다. 본 문서가 정한 인터페이스는 03 (상세 스펙) 에서 시그니처로 구체화된다.

## 1. 설계 원칙 적용

| 원칙 | 본 시스템에서의 의미 |
|------|------------------|
| **SRP** | 도메인 / 포트 / 어댑터 / 오케스트레이션 / CLI 레이어를 분리. 한 모듈은 하나의 변경 이유만 갖는다 |
| **OCP** | 새 watch 종류, 새 알림 채널, 새 spec 분해기 추가 시 기존 코드 수정 없이 어댑터만 추가 |
| **LSP** | 인메모리 TaskStore 와 SQLite TaskStore 가 동일 계약 (트랜잭션 의미 포함) 을 만족 |
| **ISP** | 큰 단일 trait 대신 `EpicRepo`, `TaskRepo`, `EventLog` 등 역할별 분리. `TaskStore` 는 이들의 슈퍼트레이트 형태 |
| **DIP** | 오케스트레이션은 포트(trait)에만 의존. CLI 진입점이 컴포지션 루트로서 어댑터를 wiring |

## 2. 레이어와 의존성 방향

```
+--------------------------------------------------------+
|                     cmd / CLI 진입점                    |  <- 컴포지션 루트
|         (epic_start, epic_resume, watch, build, ...)   |
+----------------------------+---------------------------+
                             |
+----------------------------v---------------------------+
|                    Orchestration                       |
|  EpicManager  BuildLoop  WatchDispatcher  Reconciler   |
|  MergeLoop    Escalator  EscalationWatcher             |
+----------------------------+---------------------------+
                             |
+----------------------------v---------------------------+
|                       Ports (trait)                    |
|  TaskStore  GitClient  GitHubClient  SpecDecomposer    |
|             Notifier   Clock         IdGenerator       |
+----------------------------+---------------------------+
                             |  (구현 의존성은 오직 한 방향)
+----------------------------v---------------------------+
|                       Adapters                         |
|  SqliteTaskStore  GitCli  GitHubMcpClient              |
|                   SpecKitDecomposer  StdClock          |
+--------------------------------------------------------+
                             ^
                             |
+----------------------------+---------------------------+
|                   Domain (pure)                        |
|  Epic  Task  TaskId  TaskStatus  TaskSource            |
|  TaskFailureOutcome  Event  TaskGraph (deps)           |
+--------------------------------------------------------+
```

엄격한 규칙:

- **Domain 모듈은 어떤 외부 의존성도 갖지 않는다** — std + serde 만 허용
- **Orchestration 은 어댑터를 직접 import 하지 않는다** — 오직 `ports` 의 trait 만
- **Adapter 는 다른 adapter 를 직접 호출하지 않는다** — 필요하면 orchestration 으로 끌어올린다
- **CLI cmd 는 orchestration 을 호출하지 직접 store/git 를 호출하지 않는다** — 단순 출력 변환과 wiring 만

## 3. 모듈 배치 (Rust)

기존 `plugins/github-autopilot/cli/` 단일 crate 를 유지한다. 워크스페이스로 분할하지 않는 이유: 현재 외부 재사용 요구가 없고, 단일 crate 안에서 mod 경계만으로도 위 규칙을 강제할 수 있다.

```
plugins/github-autopilot/cli/src/
├── main.rs
├── lib.rs                           # pub mod 선언만
│
├── domain/                          # pure, no I/O
│   ├── mod.rs
│   ├── epic.rs                      # Epic, EpicStatus
│   ├── task.rs                      # Task, TaskId, TaskStatus, TaskSource
│   ├── task_id.rs                   # 결정적 ID 함수
│   ├── deps.rs                      # TaskGraph, cycle detection
│   └── event.rs                     # Event, EventKind
│
├── ports/                           # trait only
│   ├── mod.rs
│   ├── task_store.rs                # TaskStore (= EpicRepo + TaskRepo + EventLog)
│   ├── git.rs                       # GitClient
│   ├── github.rs                    # GitHubClient (이슈/PR)
│   ├── decompose.rs                 # SpecDecomposer
│   ├── notifier.rs                  # Notifier
│   └── clock.rs                     # Clock
│
├── store/
│   └── sqlite.rs                    # SqliteTaskStore impl TaskStore
│
├── git.rs                           # (기존) GitCli impl GitClient
├── github.rs                        # (기존) impl GitHubClient via gh
├── gh.rs                            # (기존) gh CLI helper
│
├── decompose/
│   └── speckit.rs                   # SpecKitDecomposer
│
├── orchestration/
│   ├── mod.rs
│   ├── epic_manager.rs              # start / resume / stop / status
│   ├── build_loop.rs                # claim → IM 호출 → push 결과 처리
│   ├── watch_dispatcher.rs          # watch 결과 → epic 매칭 / append / escalate
│   ├── merge_loop.rs                # PR 머지 + task done 전이 + epic 완료 판정
│   ├── reconciler.rs                # git remote ↔ DB
│   ├── escalator.rs                 # escalation 이슈 + suppression
│   └── escalation_watcher.rs        # escalated task 의 issue close 폴링 → reconcile
│
└── cmd/                             # CLI 핸들러 (clap subcommand 매핑)
    ├── mod.rs
    ├── epic.rs                      # start / resume / stop / status
    ├── watch/                       # (기존, 일부 변경)
    ├── pipeline.rs                  # (기존, build-tasks/merge-prs 진입)
    └── ...
```

기존 파일과의 관계:

- `git.rs`, `github.rs`, `gh.rs` 는 그대로 두되, 새 `ports` 의 trait 을 구현하도록 시그니처를 정리한다 (어댑터화)
- `cmd/issue.rs`, `cmd/issue_list.rs`, `cmd/labels.rs` 는 epic 기반 전환 후 일부 사용. 04 watch-integration 에서 다룸
- `cmd/pipeline.rs` 의 build-issues 흐름은 build-tasks 흐름으로 대체 (UC-2 매핑)

## 4. 핵심 포트 (인터페이스 윤곽)

상세 시그니처는 03 에서 다루며, 본 절은 책임과 역할 경계만 명시한다.

### 4.1 TaskStore

DB 의 epic / task / dep / event 행에 대한 CRUD + 상태 전이.

- **책임**: 원자적 상태 전이, 결정적 ID 보존, 이벤트 기록
- **비책임**: spec 분해, git 호출, 이슈 발행
- **분할 trait** (ISP):
  - `EpicRepo`: epic 행
  - `TaskRepo`: task 행 + deps + claim/done/fail
  - `EventLog`: event 행 append/query
- 슈퍼트레이트 `TaskStore: EpicRepo + TaskRepo + EventLog` 로 통합

### 4.2 GitClient

git 명령어의 추상.

- **책임**: branch 생성/삭제/push/fetch, ls-remote 스캔, working tree 상태 조회
- **비책임**: GitHub API (PR/이슈)
- 구현체: `GitCli` (subprocess), 테스트용 `InMemoryGit`

### 4.3 GitHubClient

GitHub API 의 추상.

- **책임**: PR / 이슈 / 라벨 / 코멘트
- **비책임**: git 자체 동작
- 구현체: `GitHubMcpClient` (MCP 도구 경유 / 또는 기존 `gh` CLI), 테스트용 `FakeGitHub`

### 4.4 SpecDecomposer

spec 마크다운을 task 후보 목록으로 분해.

- **책임**: spec 파일 읽기 + 결정적 task_id 부여 + 의존성 추출
- **비책임**: DB insert (호출자가 함)
- 구현체: `SpecKitDecomposer` (기존 spec-kit 플러그인 호출), 테스트용 `FakeDecomposer`

### 4.5 Notifier

epic 완료 / escalation 등 사용자 알림.

- **책임**: 메시지 송신
- **비책임**: 메시지 내용 결정 (호출자가 정함)
- 구현체: `NotificationConfigured` (기존 notification 설정 사용), `FakeNotifier`

### 4.6 Clock / IdGenerator

테스트 격리용 시간 / 외부 ID (escalation 시 fingerprint suppression key 등) 주입.

- 구현체: `StdClock`, `FixedClock` (테스트), `Sha256IdGen` 등

## 5. 트랜잭션 경계

DB 의 일관성 보장은 `TaskStore` 의 메서드 단위에서 보장된다. 호출자(orchestration)는 트랜잭션을 직접 열지 않는다.

| 메서드 | 트랜잭션 범위 |
|--------|------------|
| `insert_epic_with_tasks` | epic + 모든 task + 모든 dep + 진입점 ready 승격을 한 tx |
| `claim_next_task` | 후보 SELECT + UPDATE WHERE status='ready' + claimed event INSERT 한 tx (changes()==1 검증) |
| `complete_task_and_unblock` | task done UPDATE + 이 task 에 의존하던 blocked 들의 deps 재평가 + ready 승격 + completed event 한 tx |
| `mark_task_failed` | attempts 증가 + (재시도 가능이면) ready 복귀 / (max 도달이면) escalated 전이 + event 한 tx. 반환값으로 outcome 알림 |
| `escalate_task` | escalated 전이 + escalated_issue 기록 + 의존 task 들 blocked 전이 + event 한 tx |
| `apply_reconciliation` | reconcile 결과 (변경 행들) 을 한 tx 로 반영. orphan 처리 포함 |
| `append_event` (단일) | 자동 commit |

원자성 검증 규칙: 모든 상태 전이 UPDATE 는 `WHERE` 에 기대 상태 (e.g. `status='ready'`) 를 명시하고 changes() 를 검사한다. 0 이면 동시 변경자가 있었던 것이므로 호출자에게 명시적 결과로 알린다 (예: `claim_next_task` 가 None 반환).

## 6. 동시성 정책

### 6.1 단일 writer 가정

DB 는 메인 autopilot 프로세스 (ML) 만 쓴다. 같은 머신의 다른 프로세스가 DB 를 직접 만지지 않으며, 자식 IM 도 DB 미접근. 다른 머신은 `epic-resume` 으로 reconcile 후 자기 DB 를 가짐.

이 가정 하에 동시 접근자는 메인 프로세스 안의 sub-loop 들 (`gap-watch`, `build-tasks`, `merge-prs`, `qa-boost`, `ci-watch`, `epic_manager`) 뿐이다.

### 6.2 SQLite 모드

```
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;       -- 5s
PRAGMA foreign_keys = ON;
```

- WAL: 다중 reader / 단일 writer 동시 동작
- BUSY 5s: sub-loop 간 짧은 충돌 자연 해소
- FK: deps / events 의 무결성

### 6.3 sub-loop 직렬화

Rust 차원에서는 단일 `Arc<dyn TaskStore>` 를 공유한다. SqliteTaskStore 내부에 `Mutex<Connection>` (단일 connection) 또는 `r2d2::Pool` (multi-conn + WAL) 중 후자를 선택한다. 사유: 읽기 다중 + 쓰기 BUSY 대기로 단순.

쓰기 메서드는 짧게 유지하여 BUSY 충돌을 최소화한다 (특히 reconcile 같은 큰 작업은 staged commit 으로 분할 — 03 에서 다룸).

### 6.4 git push 차원의 동시성

UC-11 의 두 머신 케이스는 git remote 에서 자연 차단된다 (non-fast-forward reject). orchestration 의 BuildLoop 는 IM 의 push 결과를 항상 검사하고 reject 시 task 를 ready 로 되돌린다.

## 7. 에러 처리 정책

### 7.1 에러 분류

| 분류 | 예시 | 처리 |
|------|------|------|
| **Domain error** | invalid status transition, dep cycle | 호출자에게 `Result::Err`. 발생 시 작업 중단 |
| **Storage error** | DB 락, 디스크 full | 재시도 (BUSY) → 그래도 실패 시 sub-loop 종료 + 알림 |
| **Network error** | git fetch 실패, GitHub API 타임아웃 | exponential backoff 재시도 (4회). 그래도 실패 시 다음 cycle 까지 대기 |
| **Configuration error** | 설정 누락 | 시작 시 검증, 즉시 에러 종료 |
| **Inconsistency** | DB 와 git remote 가 모순 (예: DB 는 done 인데 PR 미존재) | reconcile 으로 자동 해소 / 해소 안 되면 escalation |

### 7.2 에러 타입

`thiserror` 기반 enum 을 도메인 / 포트별로 분리:

- `domain::DomainError`
- `ports::TaskStoreError`, `GitError`, `GitHubError`, `DecomposeError`
- orchestration 에서는 `OrchestrationError` 로 wrap

CLI 진입점에서 사용자용 메시지로 변환.

## 8. 설정 / 컴포지션 루트

`AutopilotConfig` 가 모든 외부 입력을 한 곳에 모은다 (파일 + 환경변수 + CLI 플래그 우선순위 적용).

```
[storage]
db_path = ".autopilot/state.db"

[epic]
branch_prefix = "epic/"
max_attempts = 3

[hitl]
label = "autopilot:hitl-needed"
escalation_suppression_window_hours = 24

[concurrency]
max_parallel_agents = 4
```

`main.rs` 가 컴포지션 루트로서 다음을 수행:

1. 설정 로드 + 검증
2. SQLite 연결 + 마이그레이션 적용
3. 어댑터들 인스턴스화 (SqliteTaskStore, GitCli, GitHubMcpClient, ...)
4. orchestration 컴포넌트 인스턴스화 (포트 주입)
5. clap 서브커맨드 디스패치

테스트는 4번에서 fake 어댑터를 주입한다.

## 9. 마이그레이션 정책

스키마 변경은 `migrations/` 폴더의 numbered SQL 을 순서대로 적용:

```
src/store/migrations/
├── V1__initial.sql
├── V2__...
└── ...
```

`meta(key='schema_version')` 행으로 적용 상태 추적. 시작 시 누락된 버전 적용. 다운그레이드는 비지원 (사용자에게 DB 삭제 + resume 안내).

## 10. 인터페이스 ↔ UC 매핑

본 문서의 포트가 01 의 시나리오를 어떻게 충족하는지:

| UC | 사용 포트 | 핵심 호출 경로 |
|----|----------|--------------|
| 1 | TaskStore + GitClient + SpecDecomposer | EpicManager::start → decompose → insert_epic_with_tasks → git push epic branch |
| 2 | TaskStore + GitClient + GitHubClient | BuildLoop::tick → claim_next_task → IM 호출 → push 결과 → branch_promoter (PR) → MergeLoop::tick → complete_task_and_unblock |
| 3 | TaskStore | claim 시 deps 필터, complete 시 unblock_dependents |
| 4-5 | TaskStore + GitClient + SpecDecomposer + GitHubClient | Reconciler::reconcile_epic → fetch + ls-remote + PR 스캔 → apply_reconciliation |
| 6 | TaskStore | WatchDispatcher::dispatch → epic 매칭 → upsert task |
| 7-9 | GitHubClient + TaskStore | Escalator::escalate → 이슈 발행 + escalated_issue 기록. EscalationWatcher → 사람이 close 한 이슈 자동 인식 → reconcile |
| 8 | TaskStore | mark_task_failed → outcome 분기 |
| 10 | TaskStore + Notifier | MergeLoop::tick → all_tasks_done 판정 → notifier.send. escalated 잔류 시 EpicCompleted 미발송 (사람이 force-status 또는 코드 push 로 해소해야 함) |
| 11 | GitClient | BuildLoop 의 push reject 처리 (DB 변경 없음) |
| 12 | TaskStore | EpicManager::stop → epic.status=abandoned |
| 13 | TaskStore + GitHubClient | MigrateCommand → import_issue → upsert task + 이슈 라벨 정리 |

## 11. 테스트 가능성

각 어댑터는 LSP 를 만족하는 인메모리 / fake 구현을 가진다:

- `InMemoryTaskStore`: HashMap 기반, 동일 트랜잭션 의미 (단일 mutex)
- `FakeGitClient`: 가상 ref 그래프 + push reject 시뮬레이션
- `FakeGitHubClient`: 이슈/PR 가상 저장소
- `FakeDecomposer`: 사전 정의된 task 목록 반환
- `FakeNotifier`: 송신 메시지 캡처

블랙박스 테스트는 orchestration 컴포넌트 단위로 fake 들을 wiring 하여 04 의 시나리오를 검증한다.
