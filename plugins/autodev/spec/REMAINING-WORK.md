# Remaining Work (v4)

> **Date**: 2026-03-17
> **기준**: 13개 spec flow 문서 vs 실제 Rust 코드 대조 결과, 미구현 항목만 정리

---

## 완료 확인된 항목

| 항목 | 코드 위치 |
|------|----------|
| DB 테이블 5개 (specs, spec_issues, hitl_events, hitl_responses, claw_decisions) | `infra/db/schema.rs` |
| Repository traits (Spec, Hitl, ClawDecision, Cron, Queue) | `core/repository.rs` |
| CLI 서브커맨드 (spec, hitl, queue, claw, cron, decisions, convention) | `cli/*.rs` |
| Collector trait + GitHubTaskSource | `core/collector.rs`, `daemon/collectors/github.rs` |
| Notifier trait + GitHub/Webhook 구현체 + Dispatcher | `core/notifier.rs`, `daemon/notifiers/` |
| CronEngine (tick + job 관리) | `daemon/cron/engine.rs` |
| Cron 자동 등록 (레포 등록 시 per-repo cron seed) | `cli/mod.rs:142` |
| Claw workspace (init, rules, per-repo override) | `cli/claw.rs` |
| Plugin commands (/add-spec, /update-spec) | `commands/*.md` |
| autodev agent (headless + interactive) | `main.rs` |
| BoardRenderer trait + TUI + BoardState | `core/board.rs`, `tui/` |
| Convention engine (TechStack 감지 + bootstrap) | `cli/convention.rs` |
| scan_done_merged() (merged PR → Extract 큐잉) | `tasks/helpers/git_ops.rs:470` |
| Task pipeline (Analyze → Implement → Review → Improve → Extract) | `service/tasks/` |
| Repo CRUD + config deep merge | `cli/mod.rs` |
| HITL 생성 (review overflow) | `cli/queue.rs:92-116` |
| Spec CRUD + link/unlink + pause/resume | `cli/spec.rs` |
| Label 상수 + add-first 전환 패턴 (11곳) | `core/labels.rs`, `service/tasks/` |

---

## 남은 작업

### Critical — 자율 루프 동작에 필수

#### C1. Daemon drain 제거 (Collector 수집 전용화)

> 관련 spec: DESIGN.md "큐 상태 머신"

**현재**: `GitHubTaskSource.poll()`이 `drain_queue_items()`까지 호출 — Collector가 수집+드레인 겸임
**목표**: Collector는 수집만 (Pending 저장), Ready 전이는 Claw `queue advance`로

- `daemon/collectors/github.rs:261` — `drain_queue_items()` 제거
- `poll()` 호출 체인에서 drain 제거 (`github.rs:431`)
- Daemon tick: Ready 상태만 실행하는 단순 executor로 축소

#### C2. Notifier를 Daemon에 연결

> 관련 spec: `05-hitl-notification/flow.md`

**현재**: NotificationDispatcher + 구현체 존재하나 daemon loop에서 미사용
**목표**: Task 완료/실패, HITL 생성 시 Notifier 호출

- `daemon/mod.rs` — Dispatcher 초기화
- Task 완료/실패 이벤트 → `notify()` 호출 추가
- HITL 생성 시 → `notify()` 호출 추가

#### C3. Force trigger (이벤트 → claw-evaluate 즉시 실행)

> 관련 spec: DESIGN.md "force 트리거"

**현재**: claw-evaluate는 cron 주기에만 실행
**목표**: 특정 이벤트에서 즉시 실행

트리거 시점:
- 스펙 등록 (`spec add`)
- Task 실패 완료
- 스펙 연관 Task 완료
- HITL 응답 수신

구현: `autodev cron trigger claw-evaluate` CLI 이미 존재 → 이벤트 핸들러에서 호출

#### C4. Failure escalation 5단계

> 관련 spec: `09-failure-recovery/flow.md`

**현재**: 2단계만 구현 (retry → skip)
**목표**: 모든 Task 유형에 5단계

```
Level 1: retry        — 같은 Task 재실행 (구현됨)
Level 2: comment      — GitHub 이슈에 실패 원인 코멘트 (부분 구현)
Level 3: HITL         — 사람에게 판단 요청
Level 4: skip         — 자동 skip (구현됨)
Level 5: /update-spec — 스펙 수정 제안
```

추가 필요:
- 연속 실패 카운터 (failure_count 추적)
- 공통 escalation helper 추출
- `DecisionType::Replan` 존재하나 실제 로직 없음

---

### High — Spec 모드 핵심 기능

#### H1. Spec completion 판정

> 관련 spec: `08-spec-completion/flow.md`

**현재**: TUI 진행률 표시만 (linked issues count)
**목표**: 완료 조건 자동 검증 → HITL 최종 확인

완료 조건:
1. 모든 linked issues done (PR merged)
2. Gap detection 결과 없음
3. test_commands 실행 통과

- spec 상태 머신: `active → completing → done`
- `completing` 진입 시 HITL 생성 (최종 확인)
- test_commands 실행 + 결과 해석

#### H2. Spec 필수 섹션 검증

> 관련 spec: `03-spec-registration/flow.md`

**현재**: `spec_add()`가 body를 그대로 저장 (검증 없음)
**목표**: 5개 필수 섹션 존재 확인

필수 섹션: 개요, 요구사항, 아키텍처, 테스트 환경, 수용 기준
- `/add-spec` command에서는 대화형으로 보완
- CLI `spec add`에서는 경고 출력 (강제 차단 X)

#### H3. `spec prioritize` 서브커맨드

> 관련 spec: `04-spec-priority/flow.md`

**현재**: spec list/show/pause/resume만 존재
**목표**: `autodev spec prioritize <id1> <id2>...` — 순서 지정

- 스펙 간 의존성 판단은 Claw (skills/prioritize)
- CLI는 순서만 저장

#### H4. `hitl timeout` 서브커맨드

> 관련 spec: `05-hitl-notification/flow.md`

**현재**: HitlAction에 List/Show/Respond만 존재
**목표**: `autodev hitl timeout` — 타임아웃 초과 HITL 만료 처리

- cron 템플릿 (`HITL_TIMEOUT_SH`) 존재하나 CLI 엔트리포인트 없음

#### H5. `claw edit` 서브커맨드

> 관련 spec: `10-claw-workspace/flow.md`

**현재**: ClawAction에 Init/Rules만 존재
**목표**: `autodev claw edit` — rules/skills 편집 인터페이스

---

### Medium — UX/자동화 품질

#### M1. Convention 자율 정제

> 관련 spec: `11-convention-bootstrap/flow.md`

**현재**: TechStack 감지 + bootstrap CLI 존재
**목표**: 피드백 패턴 감지 → 규칙 변경 제안 → HITL 승인 → 자동 업데이트

- HITL 메시지, PR 리뷰 패턴, 반복 피드백 수집
- 규칙 수정 PR 자동 생성

#### M2. Spec 충돌 감지

> 관련 spec: `04-spec-priority/flow.md`

**현재**: 미구현
**목표**: 동일 파일/모듈 수정하는 스펙 간 충돌 감지 → HITL 생성

#### M3. Worktree preservation on failure

> 관련 spec: `09-failure-recovery/flow.md`

**현재**: impl 실패 시 worktree 정리됨
**목표**: 실패 시 worktree 보존하여 수동 디버깅 가능

---

## 구현 순서 제안

```
Phase A (Critical → 자율 루프 가동):
  C1 drain 제거 → C2 Notifier 연결 → C3 Force trigger → C4 Escalation

Phase B (High → Spec 모드 완성):
  H1 Spec completion → H2 섹션 검증 → H3 prioritize → H4 hitl timeout → H5 claw edit

Phase C (Medium → 품질):
  M1 Convention 자율 정제 → M2 충돌 감지 → M3 Worktree preservation
```
