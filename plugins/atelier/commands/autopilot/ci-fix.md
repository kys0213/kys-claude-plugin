---
description: "autopilot PR의 CI 실패를 tick 단위로 분석/수정합니다"
argument-hint: "[--branch=<branch>]"
allowed-tools: ["Bash", "Read", "Agent"]
---

# CI Fix (/atelier:ci-fix)

autopilot 이 생성한 PR 의 CI 실패를 감지하고, tick 단위로 수정을 시도합니다. 한 호출에서 수정 → push 까지만 수행하고, CI 결과 확인은 다음 tick 에서 수행합니다.

> 파이프라인 제어와 단계별 프로토콜은 `autopilot-pipeline` skill 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 사용법

```bash
/atelier:ci-fix                            # 전체 스캔 (cron 모드)
/atelier:ci-fix --branch=feature/issue-42  # 타겟 브랜치 (hybrid 모드)
```

> 반복 실행은 `/atelier:autopilot`이 CronCreate 또는 Monitor로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 작업 프로세스

### Step 0: 인자 파싱

`$ARGUMENTS`에서 옵션을 추출합니다:
- `--branch=<branch>`: 특정 브랜치의 PR만 대상으로 처리 (있으면 Step 2 에서 해당 브랜치만 조회)

### Step 1: 전처리 (공통)

`autopilot-pipeline` `references/pipeline-control.md` 의 3단계를 수행합니다:
1. Base 브랜치 동기화 (`branch-sync` 스킬)
2. Pipeline Idle Check — capacity 검사 불필요(`--max-parallel` 생략, idle/active 2값)
3. Idle Count + Adaptive Throttling (loop 이름 `ci-fix`)

설정에서 `label_prefix`, `idle_shutdown.max_idle`(기본 5), `notification`, `max_ci_fix_retries`(기본 3), `quality_gate_command` 를 읽습니다.

### Step 2: CI 실패 PR 수정 파이프라인

`autopilot-pipeline` `references/ci.md` §B(ci-fix) 절차를 수행합니다:
- CI 실패 PR 조회(statusCheckRollup=FAILURE) → 재시도 횟수 확인(마커 + 에스컬레이션) → CI 수정(Agent Team) → 결과 수집(재시도 마커) → 결과 보고

> 병렬 dispatch 메커니즘은 `orchestrator` skill 에 위임합니다 (ci.md 가 "무엇을 전달할지"만 정의).

## 주의사항

- **cron 모드**: 1 tick = 1 수정 시도. CI 실행 완료를 기다리지 않음
- **hybrid 모드**: fix push 후 one-shot Monitor 로 CI 완료를 감시하여 즉시 반응
- CI 가 아직 실행 중인 PR 은 skip (statusCheckRollup 에 PENDING 있으면)
- merge-prs 루프와 역할 분리: ci-fix 는 CI 수정만, merge-prs 는 conflict/review 만
- 토큰 최적화: MainAgent 는 PR 목록 조회·마커 관리만, CI 분석/수정은 모두 Agent 에 위임

상세 프로토콜·에스컬레이션·결과 형식은 `autopilot-pipeline` skill 의 references 참조.
