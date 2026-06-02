---
description: "스펙 기반 구현 갭을 탐지하여 autopilot ledger task로 등록합니다 (GitHub issue 생성 없음)"
argument-hint: ""
allowed-tools: ["Bash", "Glob", "Read", "Agent", "AskUserQuestion"]
---

# Gap Watch (/atelier:gap-watch)

스펙 문서와 구현 코드 사이의 갭을 분석하고, 발견된 갭을 **autopilot ledger task**(`gap-backlog` epic)로 등록합니다. GitHub issue 는 생성하지 않습니다.

> 파이프라인 제어와 단계별 프로토콜은 `autopilot-pipeline` skill 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 사용법

```bash
/atelier:gap-watch
```

> 반복 실행은 `/atelier:autopilot`이 `CronCreate`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 1: 전처리 (공통 + gap-watch 게이트)

`autopilot-pipeline` `references/pipeline-control.md` 의 3단계를 수행합니다. 단, gap-watch 는 idle check 앞에 분석 이력 게이트가 있습니다 (`references/gap-watch.md` §"전처리 특이사항"):
1. Base 브랜치 동기화 (`branch-sync` 스킬)
2. `gap-watch.state` 파일이 없으면 idle check 를 건너뛰고 Step 2 로 진행, 있으면 Pipeline Idle Check (capacity 검사 불필요)
3. Idle Count + Adaptive Throttling (loop 이름 `gap-watch`)

설정에서 `spec_paths`, `label_prefix`, `idle_shutdown.max_idle`(기본 5), `notification` 을 읽습니다.

### Step 2: spec↔code 갭 분석 → ledger 등록

`autopilot-pipeline` `references/gap-watch.md` 절차를 수행합니다:
- 설정 로딩 → 스펙 파일 수집(Glob + 필터링) → 갭 분석(gap-detector) → Ledger Epic 부트스트랩(필수 blocker) → Ledger Task 등록(정방향) → 역방향 갭 분석 + HITL(AskUserQuestion) → idle/active 마킹 + 결과 보고 + 세션 통계

> 역방향 분석(Step 5.5)은 gap-detector 호출 시 `reverse: true` 전달 시에만 활성화됩니다.

## 주의사항

- 토큰 최적화: MainAgent 는 스펙/코드 파일을 직접 읽지 않음. 경로만 수집하고 gap-detector 에 위임
- 스펙 변경이 없어도 코드 변경으로 갭이 해소되었을 수 있으므로 매번 전체 분석
- 동일 fingerprint 의 기존 ledger task 는 자동 흡수되므로 별도 중복 검사 불필요
- **GitHub issue 는 생성하지 않습니다** — 운영자는 `autopilot epic status gap-backlog` / `autopilot task list --epic gap-backlog`로 확인
- reverse-gap-ignore.json 의 internal 항목은 다음 cycle 부터 자동 제외

상세 프로토콜·필터 규칙·HITL 흐름·결과 형식은 `autopilot-pipeline` skill 의 references 참조.
