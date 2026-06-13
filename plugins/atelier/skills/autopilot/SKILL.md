---
name: autopilot
description: 자율 개발 루프의 파이프라인 제어와 단계별 프로토콜. base 동기화·idle/capacity 판정·adaptive throttling·ledger 운영의 공통 규칙과, 이슈 구현(build)·CI 감시/수정·PR 머지·spec 갭 감시·QA 보강·ledger 운영·정체 방어의 절차를 담습니다. autopilot 진입점이 CronCreate/Monitor 로 이 skill 의 references 를 내부 디스패치합니다.
version: 1.0.0
user-invocable: false
---

# autopilot

자율 개발 루프(autopilot)의 **공통 도메인 지식**입니다. autopilot 진입점이 `autopilot watch` CLI daemon 을 Monitor 로 띄우고, daemon 이 emit 하는 이벤트(CI_FAILURE, TASK_READY 등)나 CronCreate 스케줄에 따라 **이 skill 의 references 를 내부 디스패치**합니다. 개별 루프는 별도 슬래시가 아니라 autopilot 한 번의 진입으로 CLI 와 함께 동작합니다.

> 파이프라인 제어 규칙과 단계별 프로토콜은 `references/` 에서 progressive disclosure 로 로드합니다.

> **결정적 상태 전이는 CLI 로 위임**: task add/claim/complete, epic status, pipeline idle, check mark 등은 `atelier autopilot` CLI 호출로 처리합니다 (판단은 skill/커맨드, 변환은 CLI — CLAUDE.md 책임 경계).

## 공통 파이프라인 제어 (모든 autopilot 커맨드 공통)

거의 모든 autopilot 커맨드는 본 작업 전에 동일한 전처리 3단계를 수행합니다. 상세는 `references/pipeline-control.md`:

1. **Base 브랜치 동기화** — `references/branch-sync.md` 절차
2. **Pipeline Idle / Capacity Check** — `autopilot pipeline idle` exit code 로 idle/active/at-capacity 판정
3. **Idle Count + Adaptive Throttling** — `autopilot check mark --status idle|active` 로 idle 횟수 추적, cron 간격 동적 조정/해제

## 머지·병렬·worktree 는 orchestrator 에 위임 (단일 소유)

autopilot 은 worktree·병렬 dispatch·머지 조정을 **자체 서술하지 않습니다**. "무엇을 위임할지"만 두고, "어떻게 병렬화/머지할지"는 `orchestrator` skill 이 단일 소유합니다 (05 §4.5). Agent Team / worktree / 머지가 필요한 단계는 orchestrator 의 `references/delegation-patterns.md`·`merge-coordinator.md`·`worktree-lifecycle.md` 를 로드합니다.

## references 로드 가이드

| reference | 언제 로드 | 출처 커맨드 |
|---|---|---|
| `references/startup.md` | autopilot 진입 시 시작 절차 전체 (preflight·품질 게이트·초기 스캔·Monitor/Cron 등록·스냅샷) | autopilot (진입점) |
| `references/pipeline-control.md` | 모든 autopilot 커맨드 전처리 (idle/capacity/throttling) | 공통 |
| `references/branch-sync.md` | base 브랜치 결정 + checkout/pull 동기화 | 공통 (전처리 Step 1) |
| `references/draft-branch.md` | draft 브랜치 라이프사이클·승격 규칙·설정 스키마 | build-issues, qa-boost, 승격 |
| `references/build-pipeline.md` | 이슈 구현 파이프라인 (capacity·의존성·재작업·에스컬레이션) | build-issues |
| `references/ci.md` | CI 실패 분석/수정 | ci-watch, ci-fix |
| `references/merge.md` | PR 분류·머지·문제 해결 | merge-prs |
| `references/gap-watch.md` | spec↔code 갭 감시 + ledger 등록 + 역방향 갭 | gap-watch |
| `references/qa-boost.md` | 변경 기반 테스트 커버리지 보강 | qa-boost |
| `references/ledger.md` | epic 선택 전략·task claim·디스패치·stale 회수 | work-ledger, stale-task-review |
| `references/stagnation-redirect.md` | task 단위 정체 방어 (simhash/Jaccard → persona 재설정). PreToolUse hook(`protect-stagnation.sh`) 이 `autopilot check stagnation` exit 4/5 일 때 발동 | (hook 자동 트리거) |

> **두 가지 정체 방어 (상호 보완)**: `stagnation-redirect.md` 는 **task 단위**(worker 가 같은 영역 반복) 방어, `orchestrator` skill 의 `references/autonomous-driving.md` 는 **루프 단위**(분해→위임→머지 self-drive 의 no_progress/예산) 방어를 담당합니다. 자율 모드 운용 시 두 층위가 함께 작동합니다.

## 공통 원칙

- **설정 파일** `github-autopilot.local.md` 에서 `max_parallel_agents`·`label_prefix`·`idle_shutdown.max_idle`·`notification` 등을 읽습니다. 경로/스키마는 기존과 동일.
- **idle 시 알림**: `notification` 설정이 있으면 파이프라인 완료 시 자연어 지시대로 알림 발송.
- **at-capacity 시 즉시 종료**: 비용 발생 없이 다음 cron tick 에 재시도.
