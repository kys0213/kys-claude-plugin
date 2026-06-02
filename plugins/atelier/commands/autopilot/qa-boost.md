---
description: "최근 변경사항의 테스트 커버리지를 분석하고 누락된 테스트를 ledger task로 기록합니다"
argument-hint: "[commit_hash]"
allowed-tools: ["Bash", "Glob", "Read", "Grep"]
---

# QA Boost (/atelier:qa-boost)

최근 변경사항을 QA 관점에서 분석하고, 누락된 테스트를 결정적 ledger 의 `qa-backlog` epic 에 task 로 기록합니다. work-ledger reader 가 task 를 claim 하여 build-issues 파이프라인 없이 직접 PR 을 발행합니다.

> 파이프라인 제어와 단계별 프로토콜은 `autopilot-pipeline` skill 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 사용법

```bash
/atelier:qa-boost                    # 최근 20커밋 기준, 1회 실행
/atelier:qa-boost abc1234            # 특정 커밋 이후 변경 분석
```

> 반복 실행은 `/atelier:autopilot`이 `CronCreate`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`
- 최근 커밋: !`git log --oneline -5`

## 작업 프로세스

### Step 1: 인자 파싱

`$ARGUMENTS`에서 commit_hash 를 추출합니다:
- `/^[0-9a-f]{7,40}$/` 패턴 매칭 → commit_hash
- 비어있으면 → 최근 20커밋 기준

### Step 2: 전처리 (공통)

`autopilot-pipeline` `references/pipeline-control.md` 의 3단계를 수행합니다:
1. Base 브랜치 동기화 (`branch-sync` 스킬)
2. Pipeline Idle Check — capacity 검사 불필요(`--max-parallel` 생략, idle/active 2값)
3. Idle Count + Adaptive Throttling (loop 이름 `qa-boost`)

설정에서 `label_prefix`, `idle_shutdown.max_idle`(기본 5), `notification` 을 읽습니다.

### Step 3: 테스트 커버리지 보강 → ledger 기록

`autopilot-pipeline` `references/qa-boost.md` 절차를 수행합니다:
- 변경사항 수집(git diff/log) → 테스트 매핑 분석(Glob) → Ledger Epic 부트스트랩(필수 blocker) → Ledger Task 기록(idempotent) → 결과 보고 + 세션 통계

## 주의사항

- 테스트를 직접 구현하지 않음 — ledger task 로 기록하여 work-ledger reader 가 처리
- GitHub issue 를 생성하지 않음 (ledger-only writer). 팀 가시성이 필요한 경로는 ci-watch 가 dual-write 로 유지
- 동일 fingerprint 는 `task add`가 결정적으로 흡수하므로 별도 중복 검사 불필요
- ledger 쓰기는 `|| echo WARN ...` 패턴으로 격리하여 한 항목 실패가 나머지 진행을 막지 않음

상세 프로토콜·task body 형식·결과 형식은 `autopilot-pipeline` skill 의 references 참조.
