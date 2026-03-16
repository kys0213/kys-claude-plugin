# Spec-Code Gap Analysis (v4)

> Date: 2026-03-16
> 분석 범위: `spec/` 13개 flow + DESIGN.md vs `cli/src/` 83개 Rust 소스 파일

---

## Gap 목록

### Critical (자율 루프 동작에 필수)

#### C1. Built-in Cron 자동 등록 미구현

| | |
|---|---|
| **스펙** | [`spec/13-cron/flow.md`](13-cron/flow.md) — "레포 등록 시 per-repo cron (claw-evaluate, gap-detection, knowledge-extract)과 global cron (hitl-timeout, daily-report, log-cleanup)이 자동 등록" |
| **코드** | [`cli/src/cli/mod.rs:104`](../cli/src/cli/mod.rs#L104) — `repo_add()` 함수에 cron 등록 로직 없음 |
| **영향** | Claw headless 루프 미동작. CronEngine이 DB에서 읽지만 초기 seed가 없어 built-in job이 실행되지 않음 |

#### C2. Notifier가 Daemon에 미연결

| | |
|---|---|
| **스펙** | [`spec/05-hitl-notification/flow.md`](05-hitl-notification/flow.md) — "Daemon이 HITL 이벤트 발생 시 NotificationDispatcher를 통해 모든 Notifier에 알림 전송" |
| **코드 (구현체)** | [`cli/src/service/daemon/notifiers/dispatcher.rs`](../cli/src/service/daemon/notifiers/dispatcher.rs) — `NotificationDispatcher` ✅ 구현 완료 |
| **코드 (미연결)** | [`cli/src/service/daemon/mod.rs:109-118`](../cli/src/service/daemon/mod.rs#L109) — `Daemon` 구조체에 `NotificationDispatcher` 필드 없음 |
| **코드 (미연결)** | [`cli/src/service/daemon/mod.rs:298-432`](../cli/src/service/daemon/mod.rs#L298) — `start()` 함수에서 Notifier 생성·등록 없음 |
| **코드 (미연결)** | [`cli/src/service/daemon/mod.rs:160-181`](../cli/src/service/daemon/mod.rs#L160) — `run()` 이벤트 루프에서 task completion 시 알림 전송 로직 없음 |
| **영향** | HITL 이벤트 생성 시 알림이 전송되지 않음. `GitHubCommentNotifier`, `WebhookNotifier` 구현체는 존재하나 사용되지 않음 |

#### C3. HITL Timeout CLI 미구현

| | |
|---|---|
| **스펙** | [`spec/05-hitl-notification/flow.md`](05-hitl-notification/flow.md) — "타임아웃 초과 HITL을 만료 상태로 변경" |
| **스펙** | [`spec/13-cron/flow.md`](13-cron/flow.md) — `hitl-timeout.sh`가 `autodev hitl timeout` 호출 |
| **스펙** | [`spec/12-cli-reference/flow.md`](12-cli-reference/flow.md) — `autodev hitl timeout` 명세 |
| **코드** | [`cli/src/main.rs:198-227`](../cli/src/main.rs#L198) — `HitlAction` enum에 `List`, `Show`, `Respond`만 존재. `Timeout` variant 없음 |
| **영향** | 미응답 HITL이 영구 대기 상태로 남음. `hitl-timeout.sh` cron이 호출할 CLI 명령이 없음 |

#### C4. Notifications 설정 미구현

| | |
|---|---|
| **스펙** | [`spec/05-hitl-notification/flow.md`](05-hitl-notification/flow.md) — "notifications.channels[] 설정으로 알림 채널 등록" |
| **코드** | [`cli/src/core/config/models.rs:14-19`](../cli/src/core/config/models.rs#L14) — `WorkflowConfig`에 `notifications` 섹션 없음 (sources, daemon, workflows, claw만 존재) |
| **영향** | 알림 채널(GitHub comment, webhook 등)을 설정 파일에서 구성할 수 없음 |

---

### High (Spec 모드 핵심 기능)

#### H1. Spec 완료 판정 자동화 없음

| | |
|---|---|
| **스펙** | [`spec/08-spec-completion/flow.md`](08-spec-completion/flow.md) — "완료 조건 = linked issues 전부 done + gap 없음 + acceptance criteria 통과 → HITL 최종 확인" |
| **코드** | [`cli/src/cli/spec.rs`](../cli/src/cli/spec.rs) — 스펙 상태 전이 로직 없음. DB 필드(`status`) 수동 변경만 가능 |
| **영향** | 수동으로만 스펙을 완료 처리할 수 있음. 자동 완료 판정 루프가 동작하지 않음 |

#### H2. 실패 에스컬레이션 로직 없음

| | |
|---|---|
| **스펙** | [`spec/09-failure-recovery/flow.md`](09-failure-recovery/flow.md) — "1단계(자동 재시도) → 2단계(코멘트) → 3단계(HITL) → 4단계(skip) → 5단계(/update-spec 제안)" |
| **코드** | [`cli/src/service/daemon/mod.rs:163-180`](../cli/src/service/daemon/mod.rs#L163) — task completion 시 로그 기록만 수행. `tracker.release()` + `manager.apply()` + 로그 삽입 |
| **영향** | Task 실패 시 에스컬레이션 없이 종료. 반복 실패 카운팅, 자동 HITL 생성, skip 판단 모두 미구현 |

#### H3. Force Trigger 미구현

| | |
|---|---|
| **스펙** | [`spec/DESIGN.md`](DESIGN.md) — "스펙 등록, Task 실패, HITL 응답 수신 시 claw-evaluate cron을 즉시 트리거" |
| **코드** | [`cli/src/cli/spec.rs:9-31`](../cli/src/cli/spec.rs#L9) — `spec_add()`: DB 삽입만 수행, cron trigger 호출 없음 |
| **코드** | [`cli/src/cli/hitl.rs:105-133`](../cli/src/cli/hitl.rs#L105) — `respond()`: DB 응답 기록만 수행, cron trigger 호출 없음 |
| **영향** | 스펙 등록 후 cron 주기(기본 60초)를 대기해야 함. 즉시 평가가 불가 |

#### H4. Spec 관리 서브커맨드 미구현

| | |
|---|---|
| **스펙** | [`spec/04-spec-priority/flow.md`](04-spec-priority/flow.md) — `autodev spec prioritize <id1> <id2> ...` |
| **스펙** | [`spec/03-spec-registration/flow.md`](03-spec-registration/flow.md), [`spec/12-cli-reference/flow.md`](12-cli-reference/flow.md) — `autodev spec evaluate <id>` |
| **스펙** | [`spec/12-cli-reference/flow.md`](12-cli-reference/flow.md) — `autodev spec status <id>`, `autodev spec decisions` |
| **코드** | [`cli/src/main.rs:362-442`](../cli/src/main.rs#L362) — `SpecAction` enum에 `Add`, `List`, `Show`, `Update`, `Pause`, `Resume`, `Link`, `Unlink`만 존재 |
| **영향** | Claw의 스펙 관리 도구 부족. 우선순위 지정, 즉시 평가, 진행도 상세, 결정 이력 조회 불가 |

#### H5. `claw edit` 미구현

| | |
|---|---|
| **스펙** | [`spec/10-claw-workspace/flow.md`](10-claw-workspace/flow.md) — "Claw 규칙 편집 (`autodev claw edit <rule>`)" |
| **코드** | [`cli/src/main.rs:147-160`](../cli/src/main.rs#L147) — `ClawAction` enum에 `Init`, `Rules`만 존재. `Edit` variant 없음 |
| **영향** | CLI에서 Claw 규칙을 편집할 수 없음 |

---

### Medium (UX/모니터링)

#### M1. TUI Dashboard가 v3 수준

| | |
|---|---|
| **스펙** | [`spec/06-kanban-board/flow.md`](06-kanban-board/flow.md) — "전체/레포별 Tab 전환, HITL 대기 목록, Claw 판단 이력, Acceptance Criteria 체크리스트, 키보드 단축키 (h/s/d/Enter)" |
| **코드** | [`cli/src/tui/views.rs:47-53`](../cli/src/tui/views.rs#L47) — `AppState`가 `log_lines`, `status_message` 중심의 v3 로그 뷰어 구조 |
| **코드** | [`cli/src/tui/views.rs:154`](../cli/src/tui/views.rs#L154) — `render()` 함수가 header/body(로그)/footer 3단 구성 |
| **영향** | v4 Spec의 칸반 + HITL + Claw 상태 통합 대시보드와 큰 차이 |

#### M2. BoardState에 hitl_items, claw_status 없음

| | |
|---|---|
| **스펙** | [`spec/06-kanban-board/flow.md`](06-kanban-board/flow.md) — `BoardState { repos, hitl_items: Vec<HitlItem>, claw_status: ClawStatus }` |
| **코드** | [`cli/src/core/board.rs:10-12`](../cli/src/core/board.rs#L10) — `BoardState { repos: Vec<RepoBoardState> }` — `hitl_items`, `claw_status` 필드 없음 |
| **영향** | 보드에서 cross-repo HITL 현황, Claw 마지막/다음 판단 시각 확인 불가 |

#### M3. Cron 환경변수 주입 불완전

| | |
|---|---|
| **스펙** | [`spec/13-cron/flow.md`](13-cron/flow.md) — Per-repo 환경변수: `AUTODEV_REPO_NAME`, `AUTODEV_REPO_ROOT`, `AUTODEV_REPO_URL`, `AUTODEV_REPO_DEFAULT_BRANCH`, `AUTODEV_WORKSPACE` / Global: `AUTODEV_HOME`, `AUTODEV_DB`, `AUTODEV_CLAW_WORKSPACE` |
| **코드** | [`cli/src/service/daemon/cron/runner.rs:72-102`](../cli/src/service/daemon/cron/runner.rs#L72) — `build_env_vars()` 구현: `AUTODEV_HOME`, `AUTODEV_DB`, `AUTODEV_JOB_NAME`, `AUTODEV_JOB_ID`, `AUTODEV_REPO_NAME`, `AUTODEV_REPO_URL`, `AUTODEV_REPO_ID`만 주입 |
| **미포함** | `AUTODEV_REPO_ROOT`, `AUTODEV_REPO_DEFAULT_BRANCH`, `AUTODEV_WORKSPACE`, `AUTODEV_CLAW_WORKSPACE` |
| **영향** | Per-repo 스크립트에서 레포 로컬 경로, 기본 브랜치, 워크스페이스 경로를 사용할 수 없음 |

#### M4. ClawConfig에 schedule/gap interval 없음

| | |
|---|---|
| **스펙** | [`spec/DESIGN.md`](DESIGN.md) — `claw.schedule_interval_secs` (claw-evaluate cron 기본 주기), `claw.gap_detection_interval_secs` (gap-detection cron 기본 주기) |
| **코드** | [`cli/src/core/config/models.rs:107-110`](../cli/src/core/config/models.rs#L107) — `ClawConfig { enabled: bool, recovery_interval_secs: u64 }` — 두 필드 미존재 |
| **영향** | Cron 주기 설정이 config에서 분리됨. Built-in cron 자동 등록(C1) 구현 시 이 설정에서 주기를 가져와야 함 |

---

### Low (향후 확장)

#### L1. Convention Phase 2 (자율 개선) 미구현

| | |
|---|---|
| **스펙** | [`spec/11-convention-bootstrap/flow.md`](11-convention-bootstrap/flow.md) — "Phase 2: 피드백 패턴 감지 → 규칙/스킬 업데이트 제안 → HITL 승인 → 적용" |
| **코드** | `convention detect/bootstrap` CLI는 구현됨 (Phase 1). Phase 2 자율 개선 루프는 미구현 |
| **영향** | 규칙 자동 정제 불가 |

#### L2. `repo show` vs `repo config` 역할 불명확

| | |
|---|---|
| **스펙** | [`spec/01-repo-registration/flow.md`](01-repo-registration/flow.md) — `autodev repo show <name> --json` 명세 |
| **코드** | [`cli/src/main.rs`](../cli/src/main.rs) — `RepoAction`에 `Config`만 존재, `Show` 없음 |
| **영향** | UX 혼동 가능. show(읽기 전용 상세)와 config(설정 관리)의 역할 차이 불명확 |

---

## 구현 완료 항목

### CLI 서브커맨드

| Spec 명세 | 코드 위치 | 비고 |
|-----------|----------|------|
| `autodev start/stop/restart` | `main.rs` — `Commands::Start/Stop/Restart` | ✅ |
| `autodev status` | `main.rs` — `Commands::Status` | ✅ |
| `autodev dashboard` | `main.rs` — `Commands::Dashboard` | ✅ |
| `autodev repo add/list/config/remove/update` | `main.rs` — `RepoAction::*` | ✅ |
| `autodev spec add/list/show/update/pause/resume` | `main.rs:362-442` — `SpecAction::*` | ✅ |
| `autodev spec link/unlink` | `main.rs` — `SpecAction::Link/Unlink` | ✅ |
| `autodev hitl list/show/respond` | `main.rs:198-227` — `HitlAction::*` | ✅ |
| `autodev cron list/add/update/pause/resume/remove/trigger` | `main.rs:229+` — `CronAction::*` | ✅ |
| `autodev decisions list/show` | `main.rs` — `DecisionsAction::*` | ✅ |
| `autodev claw init/rules` | `main.rs:147-160` — `ClawAction::*` | ✅ |
| `autodev board [--repo] [--json]` | `main.rs` — `Commands::Board` | ✅ |
| `autodev agent [-p] [--repo]` | `main.rs` — `Commands::Agent` | ✅ |
| `autodev logs` | `main.rs` — `Commands::Logs` | ✅ |
| `autodev usage` | `main.rs` — `Commands::Usage` | ✅ |
| `autodev config show` | `main.rs` — `Commands::Config` | ✅ |
| `autodev convention detect/bootstrap` | `main.rs` — `Commands::Convention` | ✅ |
| `autodev queue list/advance/skip` | `main.rs` — `QueueAction::*` | ✅ |

### Config 모델

| 설정 | 코드 위치 | 비고 |
|------|----------|------|
| `ClawConfig { enabled, recovery_interval_secs }` | `config/models.rs:107-110` | ✅ |
| `DaemonConfig` (tick, log, concurrent 등) | `config/models.rs` | ✅ |
| `GitHubSourceConfig` (scan, concurrency, model 등) | `config/models.rs` | ✅ |

### Core traits

| Trait | 코드 위치 | 비고 |
|-------|----------|------|
| `Notifier` | `core/notifier.rs` | ✅ |
| `Collector` | `core/collector.rs` | ✅ |
| `BoardRenderer` | `core/board.rs:4` | ✅ |

### Daemon 컴포넌트

| 컴포넌트 | 코드 위치 | 비고 |
|----------|----------|------|
| `NotificationDispatcher` | `service/daemon/notifiers/dispatcher.rs` | ✅ (미연결) |
| `GitHubCommentNotifier` | `service/daemon/notifiers/github_comment.rs` | ✅ + 테스트 |
| `WebhookNotifier` | `service/daemon/notifiers/webhook.rs` | ✅ + 테스트 |
| `CronEngine` | `service/daemon/cron/engine.rs` | ✅ + 테스트 |

### DB 스키마

| 테이블 | 코드 위치 | 비고 |
|--------|----------|------|
| 10개 테이블 (repositories, specs, hitl_events 등) | `infra/db/schema.rs` | ✅ |

---

## 구현 우선순위 제안

```
Phase A: 자율 루프 활성화 (C1~C4)
  1. Built-in cron 자동 등록 (repo add 시) ← C1, M4
  2. Notifier → Daemon 연결 + notifications config ← C2, C4
  3. hitl timeout CLI 명령 ← C3
  4. force trigger (spec add → claw-evaluate 즉시 실행) ← H3

Phase B: Spec 모드 핵심 (H1~H5)
  5. spec prioritize / evaluate / status / decisions 서브커맨드 ← H4
  6. claw edit 서브커맨드 ← H5
  7. 실패 에스컬레이션 로직 ← H2
  8. Spec 완료 판정 자동화 ← H1

Phase C: Dashboard 고도화 (M1~M2)
  9. BoardState 확장 (hitl_items, claw_status) ← M2
  10. TUI → v4 칸반 + HITL + Claw 상태 통합 ← M1

Phase D: 설정 정리 (M3~M4)
  11. Cron 환경변수 주입 보완 ← M3
  12. ClawConfig 확장 (schedule_interval_secs, gap_detection_interval_secs) ← M4
```
