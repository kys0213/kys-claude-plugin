---
name: autopilot-pipeline
description: 자율 개발 루프(autopilot)의 공통 파이프라인 제어와 단계별 프로토콜. base 동기화·idle/capacity 판정·adaptive throttling·ledger 운영의 공통 규칙과, build-issues·ci-watch·ci-fix·merge-prs·gap-watch·qa-boost·work-ledger·stale-task-review 의 단계별 절차를 담습니다. autopilot/* 커맨드가 이 skill 의 references 를 로드합니다.
---

# autopilot-pipeline

자율 개발 루프(autopilot)를 구성하는 모든 커맨드의 **공통 도메인 지식**입니다. 개별 커맨드는 진입점(인자 파싱 + 설정 로딩 + 오케스트레이션)만 담고, 파이프라인 제어 규칙과 단계별 프로토콜은 이 skill 의 `references/` 에서 progressive disclosure 로 로드합니다.

> **결정적 상태 전이는 CLI 로 위임**: task add/claim/complete, epic status, pipeline idle, check mark 등은 `atelier autopilot` CLI 호출로 처리합니다 (판단은 skill/커맨드, 변환은 CLI — CLAUDE.md 책임 경계).

## 공통 파이프라인 제어 (모든 autopilot 커맨드 공통)

거의 모든 autopilot 커맨드는 본 작업 전에 동일한 전처리 3단계를 수행합니다. 상세는 `references/pipeline-control.md`:

1. **Base 브랜치 동기화** — `branch-sync` 스킬 절차
2. **Pipeline Idle / Capacity Check** — `autopilot pipeline idle` exit code 로 idle/active/at-capacity 판정
3. **Idle Count + Adaptive Throttling** — `autopilot check mark --status idle|active` 로 idle 횟수 추적, cron 간격 동적 조정/해제

## 머지·병렬·worktree 는 orchestrator 에 위임 (단일 소유)

autopilot 은 worktree·병렬 dispatch·머지 조정을 **자체 서술하지 않습니다**. "무엇을 위임할지"만 두고, "어떻게 병렬화/머지할지"는 `orchestrator` skill 이 단일 소유합니다 (05 §4.5). Agent Team / worktree / 머지가 필요한 단계는 orchestrator 의 `references/delegation-patterns.md`·`merge-coordinator.md`·`worktree-lifecycle.md` 를 로드합니다.

## references 로드 가이드

| reference | 언제 로드 | 출처 커맨드 |
|---|---|---|
| `references/pipeline-control.md` | 모든 autopilot 커맨드 전처리 (idle/capacity/throttling) | 공통 |
| `references/build-pipeline.md` | 이슈 구현 파이프라인 (capacity·의존성·재작업·에스컬레이션) | build-issues |
| `references/ci.md` | CI 실패 분석/수정 | ci-watch, ci-fix |
| `references/merge.md` | PR 분류·머지·문제 해결 | merge-prs |
| `references/gap-watch.md` | spec↔code 갭 감시 + ledger 등록 + 역방향 갭 | gap-watch |
| `references/qa-boost.md` | 변경 기반 테스트 커버리지 보강 | qa-boost |
| `references/ledger.md` | epic 선택 전략·task claim·디스패치·stale 회수 | work-ledger, stale-task-review |

## 공통 원칙

- **설정 파일** `github-autopilot.local.md` 에서 `max_parallel_agents`·`label_prefix`·`idle_shutdown.max_idle`·`notification` 등을 읽습니다. 경로/스키마는 기존과 동일.
- **idle 시 알림**: `notification` 설정이 있으면 파이프라인 완료 시 자연어 지시대로 알림 발송.
- **at-capacity 시 즉시 종료**: 비용 발생 없이 다음 cron tick 에 재시도.
