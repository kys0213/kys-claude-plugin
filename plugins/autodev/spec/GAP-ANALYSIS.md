# Spec-Code Gap Analysis (v4)

> Date: 2026-03-16
> 분석 범위: `spec/` 13개 flow + DESIGN.md vs `cli/src/` 83개 Rust 소스 파일

---

## 요약

| 카테고리 | 구현 완료 | 부분 구현 | 미구현 |
|---------|----------|----------|-------|
| CLI 서브커맨드 | 26 | 2 | 5 |
| DB 스키마/테이블 | 10 | 0 | 0 |
| Core traits | 3 | 1 | 0 |
| Daemon 통합 | 4 | 2 | 2 |
| Config 모델 | 3 | 1 | 2 |
| Plugin Commands | 0 | 0 | 2 |

---

## 1. CLI 서브커맨드 Gap

### 구현 완료

| Spec 명세 | CLI 코드 | 비고 |
|-----------|----------|------|
| `autodev start/stop/restart` | `Commands::Start/Stop/Restart` | ✅ |
| `autodev status` | `Commands::Status` | ✅ |
| `autodev dashboard` | `Commands::Dashboard` | ✅ |
| `autodev repo add/list/config/remove/update` | `RepoAction::*` | ✅ |
| `autodev spec add/list/show/update/pause/resume` | `SpecAction::*` | ✅ |
| `autodev spec link/unlink` | `SpecAction::Link/Unlink` | ✅ (spec 추가) |
| `autodev hitl list/show/respond` | `HitlAction::*` | ✅ |
| `autodev cron list/add/update/pause/resume/remove/trigger` | `CronAction::*` | ✅ |
| `autodev decisions list/show` | `DecisionsAction::*` | ✅ |
| `autodev claw init/rules` | `ClawAction::*` | ✅ |
| `autodev board [--repo] [--json]` | `Commands::Board` | ✅ |
| `autodev agent [-p] [--repo]` | `Commands::Agent` | ✅ |
| `autodev logs` | `Commands::Logs` | ✅ |
| `autodev usage` | `Commands::Usage` | ✅ |
| `autodev config show` | `Commands::Config` | ✅ |
| `autodev convention detect/bootstrap` | `Commands::Convention` | ✅ |
| `autodev queue list/advance/skip` | `QueueAction::*` | ✅ |

### 미구현 서브커맨드

| Spec 명세 | Flow | 상태 | 설명 |
|-----------|------|------|------|
| `autodev hitl timeout` | Flow 5, 13 | ❌ 미구현 | HITL 타임아웃 초과 이벤트를 만료 처리하는 CLI 명령. `hitl-timeout.sh` cron이 호출해야 함 |
| `autodev spec prioritize <id1> <id2> ...` | Flow 4 | ❌ 미구현 | 스펙 간 우선순위를 수동 지정. Claw가 판단할 때 참조 |
| `autodev spec evaluate <id>` | Flow 3, 12 | ❌ 미구현 | 특정 스펙의 claw-evaluate cron을 즉시 트리거 |
| `autodev spec status <id>` | Flow 12 | ❌ 미구현 | 스펙 진행도 상세 (linked issues 상태 + acceptance criteria 검증 현황). `spec show`는 있으나 `spec status`는 별도 |
| `autodev spec decisions` | Flow 12 | ❌ 미구현 | 스펙별 Claw 결정 이력 조회 (`--json [-n 20]`). `decisions list`는 있으나 spec 서브커맨드 내 `decisions`는 미구현 |

### 부분 구현 서브커맨드

| Spec 명세 | 상태 | 설명 |
|-----------|------|------|
| `autodev repo show <name>` | ⚠️ 부분 | Spec에 `repo show --json`이 명세되어 있으나 코드에는 `repo config`만 존재. show와 config의 역할 차이 불명확 |
| `autodev claw edit <rule>` | ⚠️ 부분 | Flow 10에 명세되었으나 `ClawAction`에 `Edit` variant 없음 |

---

## 2. Config 모델 Gap

### 구현된 설정

| 설정 | 파일 | 비고 |
|------|------|------|
| `ClawConfig { enabled, recovery_interval_secs }` | `config/models.rs` | ✅ |
| `DaemonConfig` (tick, log, concurrent 등) | `config/models.rs` | ✅ |
| `GitHubSourceConfig` (scan, concurrency, model 등) | `config/models.rs` | ✅ |

### 미구현 설정

| Spec 명세 | Flow | 상태 |
|-----------|------|------|
| `claw.schedule_interval_secs` | DESIGN.md | ❌ `ClawConfig`에 해당 필드 없음. Spec에서 claw-evaluate cron의 기본 주기로 사용 |
| `claw.gap_detection_interval_secs` | DESIGN.md | ❌ `ClawConfig`에 해당 필드 없음. gap-detection cron의 기본 주기로 사용 |
| `notifications.channels[]` 설정 | Flow 5 | ❌ `WorkflowConfig`에 notifications 섹션 없음. Notifier 구현체는 존재하나 설정에서 채널 등록 불가 |
| `hitl.timeout_hours` / `hitl.timeout_action` | Flow 5 | ❌ HITL 타임아웃 관련 설정 전무 |

---

## 3. Daemon 통합 Gap

### Notifier 미연결

**Spec**: Daemon이 HITL 이벤트 발생 시 `NotificationDispatcher`를 통해 모든 Notifier에 알림 전송.

**코드 현황**:
- `Notifier` trait: ✅ 구현 (`core/notifier.rs`)
- `NotificationDispatcher`: ✅ 구현 (`service/daemon/notifiers/dispatcher.rs`)
- `GitHubCommentNotifier`: ✅ 구현 + 테스트
- `WebhookNotifier`: ✅ 구현 + 테스트

**Gap**: `Daemon` 구조체에 `NotificationDispatcher`가 주입되지 않음. `daemon::start()`에서 Notifier가 생성·등록되지 않음. **HITL 이벤트 생성 시 알림이 전송되지 않는다.**

### Built-in Cron 자동 등록 미구현

**Spec** (Flow 13): 레포 등록 시 per-repo cron (claw-evaluate, gap-detection, knowledge-extract)과 global cron (hitl-timeout, daily-report, log-cleanup)이 자동 등록.

**코드 현황**: `repo_add()`에서 cron 등록 로직 없음. CronEngine이 DB에서 읽지만, 초기 seed 로직이 없음.

### Cron 환경변수 주입 불완전

**Spec** (Flow 13, DESIGN.md): Per-repo job 실행 시 `AUTODEV_REPO_URL`, `AUTODEV_REPO_DEFAULT_BRANCH`, `AUTODEV_WORKSPACE` 등 주입.

**코드 현황** (`cron/runner.rs`를 확인 필요): `ScriptRunner::build_env_vars`에서 `RepoInfo { name, url, enabled }`만 전달. `default_branch`, `workspace path` 등은 미포함.

### Force Trigger 미구현

**Spec** (DESIGN.md): 스펙 등록, Task 실패, HITL 응답 수신 시 `claw-evaluate` cron을 즉시 트리거.

**코드 현황**: `spec_add`, `hitl respond` 등에서 cron trigger 호출 로직 없음.

---

## 4. Board/Dashboard Gap

### TUI Dashboard vs Spec

**Spec** (Flow 6):
- 전체 보기 + 레포별 보기 (Tab 전환)
- HITL 대기 목록 표시
- Claw 마지막 판단 시각 / 다음 판단까지 카운트다운
- Acceptance Criteria 체크리스트
- Claw 판단 이력 (최근 N건)
- 키보드 단축키: `h` (HITL), `s` (스펙 상세), `d` (판단 이력), `Enter` (이슈 상세)

**코드 현황**:
- TUI는 로그 테일링 기반 대시보드 (v3 구조)
- 칸반 보드는 `autodev board` CLI 명령으로 텍스트 출력
- HITL 표시: 없음 (spec 수준 hitl_count만 텍스트 보드에 표시)
- Claw 상태/판단 이력: 없음
- Acceptance Criteria: 없음
- 전체/레포별 Tab 전환: 없음

**Gap**: TUI Dashboard는 v3 로그 뷰어 수준이며, v4 Spec의 칸반 + HITL + Claw 상태 통합 대시보드와 큰 차이가 있음.

### BoardState 구조 차이

**Spec**:
```rust
pub struct BoardState {
    pub repos: Vec<RepoBoardState>,
    pub hitl_items: Vec<HitlItem>,       // cross-repo HITL
    pub claw_status: ClawStatus,         // 마지막/다음 판단
}
```

**코드** (`core/board.rs`):
```rust
pub struct BoardState {
    pub repos: Vec<RepoBoardState>,
    // hitl_items: 없음
    // claw_status: 없음
}
```

---

## 5. Plugin Commands Gap

### /add-spec, /update-spec 미구현

**Spec** (Flow 3, 7, 12): 레포 Claude 세션에서 실행하는 SSOT plugin command. 대화형 검증 + 보완 + autodev CLI 호출.

**코드 현황**: `plugins/autodev/commands/` 디렉토리에 slash command 파일이 존재하지만, `/add-spec`과 `/update-spec`의 구현 수준을 확인 필요.

> 이 항목은 Plugin command (markdown 기반) 영역이므로 Rust CLI gap이 아닌 별도 영역.

---

## 6. Spec 모드 핵심 플로우 Gap

### Claw의 큐 조작 흐름

**Spec**: Claw(headless)가 `autodev queue list --json` → 판단 → `autodev queue advance/skip` 호출.

**코드 현황**: `queue advance/skip` CLI 명령은 구현됨. 그러나 Claw가 이를 호출하는 headless 플로우 (claw-evaluate cron)는 스크립트 레벨에서만 존재하며, built-in cron 자동 등록이 없어 실질적으로 동작하지 않음.

### Spec 완료 판정 (Flow 8)

**Spec**: 완료 조건 = linked issues 전부 done + gap 없음 + acceptance criteria 통과 → HITL 최종 확인.

**코드 현황**: 스펙 상태 전이 로직이 DB 필드(`status`)에만 존재하며, 자동 완료 판정 로직은 미구현.

### 실패 복구 에스컬레이션 (Flow 9)

**Spec**: 1단계(자동 재시도) → 2단계(코멘트) → 3단계(HITL) → 4단계(skip) → 5단계(/update-spec 제안).

**코드 현황**: Task 실패 시 로그 기록만 수행. 에스컬레이션 단계 로직, 반복 실패 카운팅, 자동 HITL 생성은 미구현.

### Convention 자율 개선 (Flow 11 Phase 2)

**Spec**: 피드백 패턴 감지 → 규칙/스킬 업데이트 제안 → HITL 승인 → 적용.

**코드 현황**: `convention detect/bootstrap` CLI는 구현됨 (Phase 1). Phase 2 자율 개선 루프는 미구현.

---

## 7. Gap 심각도 분류

### Critical (자율 루프 동작에 필수)

| # | Gap | 영향 |
|---|-----|------|
| C1 | Built-in cron 자동 등록 미구현 | Claw headless 루프 미동작 |
| C2 | Notifier가 Daemon에 미연결 | HITL 알림 미전송 |
| C3 | HITL timeout CLI 미구현 | 미응답 HITL 영구 대기 |
| C4 | notifications 설정 미구현 | 알림 채널 구성 불가 |

### High (Spec 모드 핵심 기능)

| # | Gap | 영향 |
|---|-----|------|
| H1 | Spec 완료 판정 자동화 없음 | 수동으로만 스펙 완료 가능 |
| H2 | 실패 에스컬레이션 로직 없음 | 실패 시 무한 대기 또는 수동 개입 필요 |
| H3 | Force trigger 미구현 | 스펙 등록 후 즉시 평가 불가 (cron 주기 대기) |
| H4 | `spec prioritize/evaluate/status/decisions` 미구현 | Claw의 스펙 관리 도구 부족 |
| H5 | `claw edit` 미구현 | 규칙 편집 불가 |

### Medium (UX/모니터링)

| # | Gap | 영향 |
|---|-----|------|
| M1 | TUI Dashboard가 v3 수준 | v4 칸반/HITL/Claw 상태 미표시 |
| M2 | BoardState에 hitl_items, claw_status 없음 | 보드에서 cross-repo HITL 확인 불가 |
| M3 | Cron 환경변수 주입 불완전 | Per-repo 스크립트에서 일부 환경변수 사용 불가 |
| M4 | ClawConfig에 schedule/gap interval 없음 | Cron 주기 설정이 config와 분리됨 |

### Low (향후 확장)

| # | Gap | 영향 |
|---|-----|------|
| L1 | Convention Phase 2 (자율 개선) 미구현 | 규칙 자동 정제 불가 |
| L2 | `repo show` vs `repo config` 역할 불명확 | UX 혼동 가능 |

---

## 8. 구현 우선순위 제안

```
Phase A: 자율 루프 활성화 (C1~C4)
  1. built-in cron 자동 등록 (repo add 시)
  2. Notifier → Daemon 연결 + notifications config
  3. hitl timeout CLI 명령
  4. force trigger (spec add → claw-evaluate 즉시 실행)

Phase B: Spec 모드 핵심 (H1~H5)
  5. spec prioritize / evaluate / status / decisions 서브커맨드
  6. claw edit 서브커맨드
  7. 실패 에스컬레이션 로직
  8. Spec 완료 판정 자동화

Phase C: Dashboard 고도화 (M1~M2)
  9. BoardState 확장 (hitl_items, claw_status)
  10. TUI → v4 칸반 + HITL + Claw 상태 통합

Phase D: 설정 정리 (M3~M4)
  11. ClawConfig 확장 (schedule_interval_secs, gap_detection_interval_secs)
  12. Cron 환경변수 주입 보완
```
