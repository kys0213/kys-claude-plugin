# Remaining Work (v4)

> **Date**: 2026-03-17
> **기준**: DESIGN.md spec vs 실제 코드 대조 결과, 미구현 항목만 정리

---

## 완료 확인된 항목 (구현 완료)

| 항목 | 코드 위치 |
|------|----------|
| DB 테이블 5개 (specs, spec_issues, hitl_events, hitl_responses, claw_decisions) | `infra/db/schema.rs` |
| Repository traits (Spec, Hitl, ClawDecision, Cron, Queue) | `core/repository.rs` |
| CLI 서브커맨드 (spec, hitl, queue, claw, cron, decisions, convention) | `cli/*.rs` |
| Collector trait + GitHubTaskSource | `core/collector.rs`, `daemon/collectors/github.rs` |
| Notifier trait + GitHub/Webhook 구현체 + Dispatcher | `core/notifier.rs`, `daemon/notifiers/` |
| CronEngine (tick + job 관리) | `daemon/cron/engine.rs` |
| Cron 자동 등록 (레포 등록 시 per-repo cron seed) | `cli/mod.rs:142` → `cron::seed_per_repo_crons()` |
| Claw workspace (init, rules) | `cli/claw.rs` |
| Plugin commands (/add-spec, /update-spec) | `commands/*.md` |
| autodev agent (headless + interactive) | `main.rs` |
| BoardRenderer trait + TUI | `core/board.rs`, `tui/` |
| Convention engine (TechStack 감지) | `cli/convention.rs` |
| scan_done_merged() (merged PR → Extract 큐잉) | `tasks/helpers/git_ops.rs:470` |

---

## 남은 작업

### 1. Daemon drain 제거 — Collector를 수집 전용으로 축소

**현재**: `GitHubTaskSource.poll()`이 `drain_queue_items()`까지 호출하여 Collector가 수집+드레인을 모두 수행
**목표**: Collector는 수집만 (`Pending`으로 저장), 드레인은 Claw의 `queue advance`로 전이

- `daemon/collectors/github.rs:261` — `drain_queue_items()` 제거
- `poll()` 에서 drain 호출 제거 (`github.rs:431`)
- Daemon tick에서 Ready 상태만 실행하는 단순 executor로 축소
- 관련 spec: DESIGN.md "큐 상태 머신" 섹션

### 2. `hitl timeout` 서브커맨드

**현재**: `HitlAction`에 List/Show/Respond만 존재
**목표**: `autodev hitl timeout` — 타임아웃 초과 HITL 이벤트 만료 처리

- `main.rs` — `HitlAction` enum에 `Timeout` variant 추가
- `cli/hitl.rs` — timeout 로직 구현 (hitl_events에서 created_at + timeout 초과 항목 → status 변경)
- 관련 spec: `05-hitl-notification/flow.md`, DESIGN.md global cron "hitl-timeout"

### 3. `claw edit` 서브커맨드

**현재**: `ClawAction`에 Init/Rules만 존재
**목표**: `autodev claw edit` — claw-workspace의 rules/skills를 편집하는 인터페이스

- `main.rs` — `ClawAction` enum에 `Edit` variant 추가
- `cli/claw.rs` — edit 로직 구현
- 관련 spec: `10-claw-workspace/flow.md`

### 4. Force trigger (이벤트 기반 즉시 실행)

**현재**: claw-evaluate는 cron 주기에만 실행
**목표**: 특정 이벤트 발생 시 claw-evaluate 즉시 실행

트리거 시점:
- 스펙 등록 (`autodev spec add`)
- Task 실패 완료
- 스펙 연관 Task 완료
- HITL 응답 수신

구현 방식: `autodev cron trigger claw-evaluate` CLI가 이미 존재하므로, 위 이벤트 핸들러에서 호출 추가

- `cli/spec.rs` — spec add 후 trigger
- `cli/hitl.rs` — respond 후 trigger
- Task 완료 핸들러 — failure/done 시 trigger
- 관련 spec: DESIGN.md "force 트리거" 섹션

### 5. Notifier를 Daemon에 연결

**현재**: NotificationDispatcher + GitHubComment/Webhook 구현체 존재하지만 daemon loop에서 미사용
**목표**: Task 완료/실패, HITL 생성 등 이벤트 발생 시 Notifier 호출

- `daemon/mod.rs` — Dispatcher 초기화 + 이벤트 발생 지점에 notify() 호출 추가
- 관련 spec: `05-hitl-notification/flow.md`

### 6. Spec completion 판정

**현재**: TUI에서 진행률 표시만 (linked issues 수 / done 수)
**목표**: 완료 조건 자동 검증 → HITL 최종 확인 요청

완료 조건:
1. 모든 linked issues가 done (PR merged)
2. Gap detection 결과 없음
3. Acceptance criteria 통과 (spec에 기록된 test_commands 실행)

- spec 상태 머신: `active → completing → done`
- `completing` 진입 시 HITL 이벤트 생성 (최종 확인)
- 관련 spec: `08-spec-completion/flow.md`

### 7. Failure escalation 5단계

**현재**: review 실패 시 2단계만 (retry loop → skip)
**목표**: 모든 Task 유형에 5단계 에스컬레이션

```
Level 1: retry        — 같은 Task 재실행 (현재 구현됨)
Level 2: comment      — GitHub 이슈에 실패 원인 코멘트
Level 3: HITL         — 사람에게 판단 요청
Level 4: skip         — 자동 skip (현재 구현됨)
Level 5: /update-spec — 스펙 수정 제안
```

- 공통 escalation 로직을 trait 또는 helper로 추출
- 각 Task (Analyze, Implement, Review, Improve)의 after_invoke에 적용
- 관련 spec: `09-failure-recovery/flow.md`
