---
description: "CI 실패를 모니터링하고 분석하여 GitHub issue를 생성합니다"
argument-hint: "[--run-id=<id> --branch=<branch>]"
allowed-tools: ["Bash", "Read", "Agent"]
---

# CI Watch (/atelier:ci-watch)

GitHub Actions 의 CI 실패를 감시하고, 실패 원인을 분석하여 GitHub issue 로 등록합니다.

> 파이프라인 제어와 단계별 프로토콜은 `autopilot-pipeline` skill 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 사용법

```bash
/atelier:ci-watch                                # 전체 스캔 (cron 모드)
/atelier:ci-watch --run-id=12345 --branch=main   # 타겟 분석 (hybrid 모드)
```

> 반복 실행은 `/atelier:autopilot`이 CronCreate 또는 Monitor로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 0: 인자 파싱

`$ARGUMENTS`에서 옵션을 추출합니다:
- `--run-id=<id>`: 특정 CI run만 분석 (Step 2 `gh run list` 건너뛰고 직접 분석, 중복 확인은 수행)
- `--branch=<branch>`: 실패가 발생한 브랜치 (이벤트 컨텍스트)

### Step 1: 전처리 (공통)

`autopilot-pipeline` `references/pipeline-control.md` 의 3단계를 수행합니다:
1. Base 브랜치 동기화 (`branch-sync` 스킬)
2. Pipeline Idle Check — capacity 검사 불필요(`--max-parallel` 생략, idle/active 2값)
3. Idle Count + Adaptive Throttling (loop 이름 `ci-watch`)

설정에서 `label_prefix`, `idle_shutdown.max_idle`(기본 5), `notification`, `ci_watch.*`(max_age/default_branch_max_age/branch_filter) 를 읽습니다.

### Step 2: CI 실패 분석 → 이슈 생성

`autopilot-pipeline` `references/ci.md` §A(ci-watch) 절차를 수행합니다:
- CI 실패 목록 조회 → 이슈 자동 정리(close-resolved) → 오래된/머지된 PR 필터링 → 중복 이슈 필터링(fingerprint) → 실패 분석(Agent Team) → Issue 생성 + Ledger 동기 기록(observer) → 결과 보고 + 세션 통계

> 병렬 dispatch 메커니즘은 `orchestrator` skill 에 위임합니다 (ci.md 가 "무엇을 전달할지"만 정의).

## 주의사항

- issue-label 스킬의 라벨 필수 규칙·fingerprint 규칙을 따른다
- 토큰 최적화: MainAgent 는 CI 로그를 직접 읽지 않음. 로그 분석은 ci-failure-analyzer 가 수행
- flaky test 와 실제 실패를 구분하여 라벨링
- ledger 쓰기는 GitHub issue 흐름의 보조 observer — 실패가 issue 생성 결과를 무효화하지 않도록 격리

상세 프로토콜·필터 규칙·fingerprint 계약·결과 형식은 `autopilot-pipeline` skill 의 references 참조.
