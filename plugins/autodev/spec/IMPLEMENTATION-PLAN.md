# Implementation Plan (v4 Gap Resolution)

> Date: 2026-03-16
> 기반: [GAP-ANALYSIS.md](GAP-ANALYSIS.md)

### 공통 제약사항

- **DB migration**: 컬럼 추가 시 `infra/db/schema.rs`의 `migrate_v2` 패턴(`ALTER TABLE ... ADD COLUMN` + duplicate-column 에러 무시)을 따라 `migrate_v3()` 함수 작성
- **Config 역직렬화 호환**: 신규 config struct는 반드시 `#[derive(Default)]` + `#[serde(default)]` 적용. 기존 `.autodev.yaml` 파싱이 깨지지 않아야 함
- **알림 dispatch 실패**: 로그 기록 후 이벤트 루프 계속 진행. Daemon 블로킹 금지

---

## Phase A: 자율 루프 활성화

> 목표: Daemon → CronEngine → Claw headless 루프가 실제로 동작하도록 만든다.

### A-1. Built-in Cron 자동 등록 (C1 + M4)

**목적**: `autodev repo add` 시 per-repo cron 3개 + 최초 1회 global cron 3개를 DB에 seed한다.

**변경 파일**:
- `cli/src/core/config/models.rs:107-110` — `ClawConfig` 확장
- `cli/src/cli/mod.rs:104` — `repo_add()` 함수
- `cli/src/service/daemon/mod.rs:298` — `start()` 함수 (global cron seed)
- `cli/src/infra/db/repository.rs` — cron seed 전용 DB 메서드 추가

**구현 내용**:

1. **ClawConfig 필드 추가** (`config/models.rs`)
   ```rust
   pub struct ClawConfig {
       pub enabled: bool,
       pub recovery_interval_secs: u64,
       pub schedule_interval_secs: u64,        // 신규: default 60
       pub gap_detection_interval_secs: u64,    // 신규: default 3600
   }
   ```

2. **Built-in cron 스크립트 템플릿** — Rust 코드에서 `~/.autodev/crons/` 하위에 기본 스크립트 파일을 생성하는 유틸리티

3. **`repo_add()` 확장**: DB 등록 후 per-repo cron 3개 (`claw-evaluate`, `gap-detection`, `knowledge-extract`)를 `cron_add(builtin=1)`로 삽입

4. **`daemon::start()` 확장**: 시작 시 global cron (`hitl-timeout`, `daily-report`, `log-cleanup`)이 DB에 없으면 seed

**의존성**: M4 (ClawConfig 확장)를 함께 처리

**테스트**:
- `repo_add()` 호출 후 `cron_list()`에 per-repo 3개 cron 존재 확인
- `daemon::start()` 후 global 3개 cron 존재 확인
- 이미 존재하는 cron은 중복 생성하지 않음 (멱등성)

---

### A-2. Notifier → Daemon 연결 (C2 + C4)

**목적**: HITL 이벤트 발생 시 `NotificationDispatcher`를 통해 알림이 전송되도록 한다.

**변경 파일**:
- `cli/src/core/config/models.rs:14-19` — `WorkflowConfig`에 `notifications` 섹션 추가
- `cli/src/service/daemon/mod.rs:109-118` — `Daemon` 구조체에 dispatcher 필드 추가
- `cli/src/service/daemon/mod.rs:121-140` — `Daemon::new()`에 dispatcher 주입
- `cli/src/service/daemon/mod.rs:298-432` — `start()`에서 config 기반 Notifier 생성·등록
- `cli/src/service/daemon/mod.rs:160-181` — `run()` 이벤트 루프에서 알림 전송

**구현 내용**:

1. **Notifications config 모델** (`config/models.rs`)
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize, Default)]
   #[serde(default)]
   pub struct NotificationsConfig {
       pub channels: Vec<ChannelConfig>,
   }

   /// tagged enum으로 채널별 config를 컴파일타임 검증
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(tag = "type")]
   pub enum ChannelConfig {
       #[serde(rename = "github-comment")]
       GitHubComment { mention: Option<String> },
       #[serde(rename = "webhook")]
       Webhook { url: String },
   }
   ```
   `WorkflowConfig`에 `pub notifications: NotificationsConfig` 추가 (`#[serde(default)]`)

2. **Daemon 구조체 확장**
   ```rust
   pub struct Daemon {
       // ...기존 필드
       dispatcher: NotificationDispatcher,  // 신규
   }
   ```

3. **`start()` 함수**: `NotificationsConfig.channels`를 순회하여 `GitHubCommentNotifier` / `WebhookNotifier` 인스턴스 생성 → `NotificationDispatcher`에 등록 → `Daemon`에 주입

4. **`run()` 이벤트 루프**: task completion 시 HITL 이벤트가 생성되었는지 확인 → 있으면 `dispatcher.dispatch()` 호출

**테스트**:
- 인메모리 `MockNotifier`로 HITL 이벤트 발생 시 `dispatch()` 호출 검증
- config에 webhook 채널이 있으면 `WebhookNotifier` 생성 확인
- config에 채널이 없으면 `GitHubCommentNotifier`만 기본 등록

---

### A-3. HITL Timeout CLI (C3)

**목적**: `autodev hitl timeout` 명령을 추가하여 미응답 HITL을 만료 처리한다.

**변경 파일**:
- `cli/src/main.rs:198-227` — `HitlAction` enum에 `Timeout` variant 추가
- `cli/src/main.rs` — `Commands::Hitl` match arm에 `Timeout` 핸들러 추가
- `cli/src/cli/hitl.rs` — `timeout()` 함수 구현
- `cli/src/infra/db/repository.rs` — `hitl_expire_timed_out()` DB 메서드 추가
- `cli/src/core/config/models.rs` — HITL timeout 설정 추가

**구현 내용**:

1. **HITL timeout 설정** (`config/models.rs`)
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct HitlConfig {
       pub timeout_hours: u64,   // default 24
   }
   ```
   `WorkflowConfig`에 `pub hitl: HitlConfig` 추가 (`#[serde(default)]`)

   > `timeout_action` (Remind/Skip/PauseSpec)은 향후 확장. 초기 구현은 단순 만료(`Expired`) 전이만 수행.

2. **CLI variant** (`main.rs`)
   ```rust
   enum HitlAction {
       // ...기존
       Timeout {
           #[arg(long)]
           hours: Option<u64>,  // 설정 오버라이드
       },
   }
   ```

3. **`timeout()` 함수** (`cli/hitl.rs`):
   - `created_at`이 `timeout_hours` 이전인 `Pending` 상태 HITL 조회
   - 상태를 `Expired`로 전이
   - 만료 건수 출력

4. **DB 메서드** (`repository.rs`):
   ```rust
   fn hitl_find_timed_out(&self, before: &str) -> Result<Vec<HitlEvent>>
   fn hitl_set_expired(&self, id: &str) -> Result<()>
   ```

**테스트**:
- 24시간 이전 `Pending` HITL → `timeout()` 호출 → `Expired` 상태 전이 확인
- `Responded` 상태 HITL은 무시 확인

---

### A-4. Force Trigger (H3)

**목적**: 스펙 등록, HITL 응답 시 `claw-evaluate` cron을 즉시 실행한다.

**변경 파일**:
- `cli/src/cli/spec.rs:9-31` — `spec_add()` 함수
- `cli/src/cli/hitl.rs:105-133` — `respond()` 함수
- `cli/src/infra/db/repository.rs` — `cron_trigger()` DB 메서드 추가

**구현 내용**:

1. **`cron_trigger()` DB 메서드**: `last_run_at`을 `NULL`로 설정하여 다음 `cron_find_due()`에서 즉시 선택되도록 함 (기존 코드가 `None`을 "미실행 → 즉시 due"로 처리)
   ```rust
   fn cron_trigger(&self, name: &str, repo_id: Option<&str>) -> Result<()> {
       // last_run_at = NULL 로 설정 → cron_find_due()가 즉시 선택
   }
   ```

2. **`spec_add()` 확장**: DB 삽입 후 `cron_trigger("claw-evaluate", Some(&repo_id))` 호출

3. **`respond()` 확장**: 응답 기록 후 해당 HITL의 repo_id로 `cron_trigger("claw-evaluate", ...)` 호출

**테스트**:
- `spec_add()` 후 `claw-evaluate` cron의 `last_run_at`이 리셋되었는지 확인
- `hitl respond` 후 동일 확인

---

## Phase B: Spec 모드 핵심

> 목표: Claw가 스펙 라이프사이클을 관리하는 데 필요한 CLI 도구와 자동화 로직을 구현한다.

### B-1. Spec 관리 서브커맨드 (H4)

**변경 파일**:
- `cli/src/main.rs:362-442` — `SpecAction` enum 확장
- `cli/src/cli/spec.rs` — 핸들러 함수 구현
- `cli/src/infra/db/repository.rs` — 필요 DB 메서드 추가

**구현 내용**:

1. **`spec prioritize <id1> <id2> ...`**
   - `specs` 테이블에 `priority` 컬럼 추가 (INTEGER, default 0) — `migrate_v3()`에서 `ALTER TABLE` 처리
   - 인자 순서대로 priority = 1, 2, 3... 설정
   - Claw가 `spec list --json`에서 priority 기준 정렬된 결과를 참조

2. **`spec evaluate <id>`**
   - 해당 스펙의 repo_id를 찾아 `cron_trigger("claw-evaluate", repo_id)` 호출
   - A-4 (Force Trigger)의 재사용

3. **`spec status <id>`**
   - 스펙 상세 + linked issues 상태 + acceptance criteria 검증 현황을 종합 출력
   - DB 조인: `specs` + `spec_issues` + `queue_items` + `hitl_events`

4. **`spec decisions <spec-id>`**
   - 기존 `decisions list`의 spec 필터 버전 (별도 DB 메서드가 아닌 `decisions_list()`에 `spec_id` 파라미터 추가)
   - `--json`, `-n <count>` 옵션 지원

**테스트**:
- `prioritize` 후 `spec list --json`에서 priority 순 정렬 확인
- `evaluate` 후 cron trigger 확인
- `status` 출력에 linked issues 상태, HITL 현황 포함 확인

---

### B-2. Claw Edit (H5)

**변경 파일**:
- `cli/src/main.rs:147-160` — `ClawAction` enum에 `Edit` 추가
- `cli/src/cli/claw.rs` — `claw_edit()` 함수 구현

**구현 내용**:

```rust
enum ClawAction {
    Init { repo: Option<String> },
    Rules { repo: Option<String> },
    Edit {                                // 신규
        rule: String,                     // 규칙 이름 (예: branch-naming)
        #[arg(long)]
        repo: Option<String>,             // 레포별 오버라이드
    },
}
```

- `$EDITOR` 또는 `vi`로 `~/.autodev/claw-workspace/rules/{rule}.md` 열기
- `--repo` 지정 시 레포별 오버라이드 경로 사용

**테스트**:
- 규칙 파일 경로 결정 로직 단위 테스트 (global vs per-repo)

---

### B-3. 실패 에스컬레이션 로직 (H2)

**변경 파일**:
- `cli/src/core/models.rs` — `QueueItem`에 `failure_count` 필드 추가
- `cli/src/infra/db/schema.rs` — `queue_items` 테이블에 `failure_count` 컬럼 (`migrate_v3()`에서 `ALTER TABLE` 처리)
- `cli/src/service/daemon/mod.rs:163-180` — task completion 이벤트 루프 확장
- `cli/src/service/daemon/` — `escalation.rs` 신규 모듈

**구현 내용**:

1. **에스컬레이션 판정 함수** (`service/daemon/escalation.rs`)
   ```rust
   /// 순수 함수: 실패 횟수 → 에스컬레이션 액션 결정
   pub fn decide_escalation(failure_count: u32) -> EscalationAction {
       match failure_count {
           1..=2 => EscalationAction::Retry,
           3     => EscalationAction::Comment,
           4..=5 => EscalationAction::Hitl,
           6     => EscalationAction::Skip,
           _     => EscalationAction::UpdateSpec,
       }
   }
   ```
   별도 struct 없이 순수 함수로 구현. Daemon이 이미 소유한 dispatcher로 액션을 실행.

2. **Daemon 이벤트 루프 확장**: task completion 시 `result.status`가 실패이면 `db.increment_failure_count()` → `decide_escalation()` → 액션 실행

3. **Level별 액션**:
   - Level 1 (Retry): `pending_tasks`에 재삽입
   - Level 2 (Comment): `gh issue comment` 호출
   - Level 3 (HITL): `hitl_events` 테이블에 삽입 + dispatcher로 알림
   - Level 4 (Skip): `autodev:skip` 라벨 추가
   - Level 5 (UpdateSpec): HITL 이벤트에 `/update-spec 제안` 옵션 포함

**테스트**:
- 인메모리 DB + MockNotifier로 실패 횟수별 에스컬레이션 단계 전이 검증
- 연속 실패 카운트가 정확히 증가하는지 확인

---

### B-4. Spec 완료 판정 자동화 (H1)

**변경 파일**:
- `cli/src/service/daemon/` — `spec_completion.rs` 신규 모듈
- `cli/src/service/daemon/mod.rs` — tick 이벤트에 완료 판정 로직 추가

**구현 내용**:

1. **완료 판정 함수** (`service/daemon/spec_completion.rs`)
   ```rust
   /// 스펙 완료 조건을 검사하는 순수 함수
   pub fn check_spec_completion(db: &Database, spec_id: &str) -> CompletionStatus {
       // 1. linked issues 전부 Done인지 확인
       // 2. gap-detection 결과에 미해결 gap 없는지 확인
       // 3. acceptance_criteria 검증 (test_commands 실행 결과)
       // → 전부 만족하면 CompletionStatus::ReadyForConfirmation
   }
   ```

2. **Daemon tick 확장**: active spec에 대해 주기적으로 `check()` 실행 → `ReadyForConfirmation`이면 HITL 최종 확인 이벤트 생성

3. **HITL 확인 후**: 사용자가 승인하면 `spec_set_status(id, Completed)` 호출

**테스트**:
- linked issues 전부 done + gap 없음 → `ReadyForConfirmation` 반환 확인
- linked issues 중 하나가 미완료 → `NotReady` 반환 확인

---

## Phase C: Dashboard 고도화

> 목표: TUI Dashboard를 v4 Spec 수준의 칸반 + HITL + Claw 상태 통합 대시보드로 업그레이드한다.

### C-1. BoardState 확장 (M2)

**변경 파일**:
- `cli/src/core/board.rs:10-12` — `BoardState` 구조체 확장
- `cli/src/core/models.rs` — `ClawStatus` 구조체 신규
- `cli/src/tui/board.rs` — `BoardStateBuilder` 확장

**구현 내용**:

1. **BoardState 확장** (`core/board.rs`)
   ```rust
   pub struct BoardState {
       pub repos: Vec<RepoBoardState>,
       pub hitl_items: Vec<HitlBoardItem>,    // 신규: cross-repo HITL
       pub claw_status: ClawStatus,           // 신규: Claw 상태
   }
   ```

2. **ClawStatus 구조체** (`core/models.rs`)
   ```rust
   pub struct ClawStatus {
       pub last_evaluation_at: Option<String>,
       pub next_evaluation_in_secs: Option<u64>,
       pub total_decisions: u64,
   }
   ```

3. **BoardStateBuilder 확장** (`tui/board.rs`):
   - `hitl_events` 테이블에서 `Pending` 상태 HITL 조회
   - `cron_jobs`에서 `claw-evaluate`의 `last_run_at` + interval로 다음 실행 시간 계산
   - `claw_decisions` 테이블에서 총 결정 수 조회

**테스트**:
- BoardStateBuilder가 HITL items과 ClawStatus를 올바르게 포함하는지 확인

---

### C-2. TUI v4 칸반 통합 대시보드 (M1)

**변경 파일**:
- `cli/src/tui/views.rs:47-53` — `AppState` 확장
- `cli/src/tui/views.rs:154` — `render()` 함수 재구성
- `cli/src/tui/mod.rs` — 키보드 단축키 추가

**구현 내용**:

1. **AppState 확장**
   ```rust
   pub struct AppState {
       // ...기존 필드
       pub board_state: Option<BoardState>,    // 신규
       pub view_mode: ViewMode,                // 신규: Full / PerRepo
       pub selected_repo: Option<usize>,       // 신규
   }
   ```

2. **View 구성**:
   - **Tab 1 (Overview)**: 전체 레포 칸반 요약 + HITL 대기 목록 + Claw 상태
   - **Tab 2 (Per-repo)**: 선택된 레포의 상세 칸반 + Acceptance Criteria

3. **키보드 단축키**:
   - `Tab`: 전체 ↔ 레포별 전환
   - `h`: HITL 목록 포커스
   - `s`: 스펙 상세
   - `d`: Claw 판단 이력
   - `Enter`: 이슈 상세

4. **데이터 갱신**: 기존 로그 폴링과 함께 `BoardStateBuilder`를 주기적으로 호출하여 board_state 갱신

**테스트**:
- 키보드 이벤트에 따른 view_mode 전환 로직 단위 테스트
- render 함수가 board_state 유무에 따라 적절히 렌더링하는지 확인

---

## Phase D: 설정 정리

> 목표: Cron 환경변수와 ClawConfig를 스펙에 맞게 보완한다.

### D-1. Cron 환경변수 주입 보완 (M3)

**변경 파일**:
- `cli/src/service/daemon/cron/runner.rs:72-102` — `build_env_vars()` 확장
- `cli/src/core/models.rs:208-212` — `RepoInfo` 확장
- `cli/src/infra/db/repository.rs` — `repo_find_enabled()` 반환값에 추가 필드

**구현 내용**:

1. **RepoInfo 확장** — `default_branch`와 `local_path`는 DB 컬럼이 아닌 런타임 도출
   ```rust
   pub struct RepoInfo {
       pub name: String,
       pub url: String,
       pub enabled: bool,
       pub default_branch: Option<String>,   // 신규: git remote show 또는 config에서 도출
       pub local_path: Option<String>,       // 신규: workspace 규칙에서 도출 (~/.autodev/workspaces/{name})
   }
   ```
   > `repo_find_enabled()` 조회 시 추가 컬럼 없이 `name`으로부터 workspace 경로를 계산하고, `default_branch`는 config 또는 `git ls-remote --symref`로 해결

2. **`build_env_vars()` 확장**: 누락된 환경변수 추가
   ```rust
   // Per-repo 추가
   "AUTODEV_REPO_ROOT"            // repo local path
   "AUTODEV_REPO_DEFAULT_BRANCH"  // default branch
   "AUTODEV_WORKSPACE"            // ~/.autodev/workspaces/{org-repo}

   // Global 추가
   "AUTODEV_CLAW_WORKSPACE"       // ~/.autodev/claw-workspace
   ```

**테스트**:
- `build_env_vars()` 반환값에 모든 스펙 환경변수가 포함되는지 확인
- `repo_info`가 None일 때 per-repo 변수가 빠지는지 확인

---

## 의존성 그래프

```
A-1 (Built-in Cron) ─────────────────────────────┐
  └─ M4 (ClawConfig 확장) 함께 처리              │
                                                   │
A-2 (Notifier 연결) ──┐                           │
  └─ C4 (Config) 함께 │                           │
                       ├─ B-3 (에스컬레이션) 의존  │
A-3 (HITL Timeout) ────┘                           │
                                                   │
A-4 (Force Trigger) ──── B-1 (spec evaluate 재사용)│
                                                   │
B-1 (Spec 서브커맨드) ── B-4 (완료 판정에서 status 사용)
B-2 (Claw Edit) ─── 독립                          │
B-3 (에스컬레이션) ── A-2 의존                     │
B-4 (완료 판정) ──── B-1 의존                      │
                                                   │
C-1 (BoardState) ──── C-2 (TUI) 의존              │
C-2 (TUI) ──── C-1 의존                           │
                                                   │
D-1 (Env 보완) ── 독립                            │
```

## 구현 순서

```
Step 1:  A-1 (Built-in Cron + ClawConfig)      ← 자율 루프의 기반
Step 2:  A-2 ∥ A-3 (Notifier 연결 ∥ HITL Timeout)  ← 병렬 가능
Step 3:  A-4 (Force Trigger)                    ← 즉시 평가
Step 4:  B-1 ∥ B-2 ∥ D-1 (Spec 서브커맨드 ∥ Claw Edit ∥ Env 보완)  ← 병렬 가능
Step 5:  B-3 (에스컬레이션)                      ← A-2 완료 후
Step 6:  B-4 (완료 판정)                         ← B-1 완료 후
Step 7:  C-1 (BoardState 확장)                   ← 독립
Step 8:  C-2 (TUI 통합 대시보드)                  ← C-1 완료 후
```

**병렬 가능 조합**:
- A-2 + A-3 (서로 독립)
- B-1 + B-2 + D-1 (서로 독립, A-1 완료 후 어느 시점에서든 시작 가능)
- C-1은 Phase B와 병렬 가능

---

## Deferred (Low)

다음 항목은 현재 계획에 포함하지 않으며, Phase A~D 완료 후 별도 계획:

| Gap ID | 항목 | 사유 |
|--------|------|------|
| L1 | Convention Phase 2 (자율 개선) | Phase 1 (detect/bootstrap)이 안정화된 후 진행 |
| L2 | `repo show` vs `repo config` 역할 정리 | UX 혼동이 실제 발생하는지 사용 데이터 수집 후 결정 |
