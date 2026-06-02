---
description: ready 라벨이 붙은 이슈를 의존성 분석 후 병렬 구현하고 PR 로 승격합니다
argument-hint: ""
allowed-tools: ["Bash", "Read", "Agent"]
---

# Build Issues (/build-issues)

`{label_prefix}ready` 이슈를 의존성 분석 → 병렬 구현 → PR 승격까지 처리합니다.

> 파이프라인 제어와 단계별 프로토콜은 `autopilot-pipeline` skill 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 사용법

```bash
/build-issues
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 작업 프로세스

### Step 1: 전처리 (공통)

`autopilot-pipeline` `references/pipeline-control.md` 의 3단계를 수행합니다:
1. Base 브랜치 동기화 (`branch-sync` 스킬)
2. Pipeline Idle / **Capacity** Check — `max_parallel_agents` 를 `--max-parallel` 로 필수 전달 (at-capacity 시 즉시 종료)
3. Idle Count + Adaptive Throttling (loop 이름 `build-issues`)

설정에서 `max_parallel_agents`(기본 3), `label_prefix`, `notification`, `max_consecutive_failures`(기본 3) 를 읽습니다.

### Step 2: 이슈 구현 파이프라인

`autopilot-pipeline` `references/build-pipeline.md` 절차를 수행합니다:
- Skip 이슈 알림 → 대상 이슈 수집(+재작업 감지) → 의존성 분석 + 유사도 검사 → WIP 라벨 + gap 사전 검증 → 구현(Agent Team) → 결과 수집 + 에스컬레이션 → 승격(Agent Team) → 라벨 정리 → 결과 보고 + 세션 통계

> 병렬 dispatch·worktree 격리·머지 메커니즘은 `orchestrator` skill 에 위임합니다 (build-pipeline.md 가 "무엇을 전달할지"만 정의).

## 주의사항

- 한 cycle 에서 첫 배치만 처리 (순차 의존 후속 배치는 다음 cycle)
- wip 라벨로 중복 작업 방지, draft 브랜치는 로컬 only
- MainAgent 는 이슈 조회·라벨 관리만, 구현은 Agent 위임

상세 프로토콜·에스컬레이션·결과 형식은 `autopilot-pipeline` skill 의 references 참조.
