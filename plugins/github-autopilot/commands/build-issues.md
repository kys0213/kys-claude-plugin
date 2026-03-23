---
description: "autopilot 이슈를 분석하고 draft 브랜치에서 구현 후 PR을 생성합니다"
argument-hint: "[interval: 15m, 30m, ...]"
allowed-tools: ["Bash", "Read", "Agent", "CronCreate"]
---

# Build Issues

autopilot 라벨이 붙은 GitHub 이슈를 가져와 의존성을 분석하고, draft 브랜치에서 구현한 뒤 feature 브랜치로 승격하여 PR을 생성합니다.

## 사용법

```bash
/github-autopilot:build-issues          # 1회 실행
/github-autopilot:build-issues 15m      # 15분마다 반복
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 1: 인자 파싱

`$ARGUMENTS`에서 interval을 추출합니다.
- `/^\d+[smh]$/` 패턴 매칭 → interval 모드
- 비어있으면 → 1회 실행 모드

### Step 2: 최신 상태 동기화

```bash
git fetch origin
```

### Step 3: 이슈 목록 조회

설정에서 label_prefix를 확인합니다 (기본값: `autopilot:`).

```bash
gh issue list \
  --label "{label_prefix}ready" \
  --state open \
  --json number,title,body,labels \
  --limit 20
```

이미 `{label_prefix}wip` 라벨이 붙은 이슈는 제외합니다 (진행 중인 작업).

이슈가 없으면 "구현 대상 이슈 없음" 출력 후 종료.

### Step 4: 의존성 분석 (Agent)

issue-dependency-analyzer 에이전트를 호출합니다 (background=false):

전달 정보:
- 이슈 목록 (number, title, body)

결과: 배치 목록 (병렬 실행 가능한 이슈 그룹)

### Step 5: WIP 라벨 추가

현재 배치의 이슈들에 wip 라벨을 추가합니다 (중복 작업 방지):

```bash
gh issue edit ${ISSUE_NUMBER} --add-label "{label_prefix}wip"
```

### Step 6: 구현 (Agent Team)

첫 번째 배치(의존성 없는 이슈들)부터 순서대로 처리합니다.

각 배치 내 이슈들은 병렬로 구현합니다:

각 이슈에 대해 issue-implementer 에이전트를 호출합니다:
- `isolation: "worktree"` 로 독립 작업 공간 확보
- `run_in_background: true` 로 병렬 실행

전달 정보:
- issue_number
- issue_title
- issue_body (요구사항, 영향 범위, 구현 가이드)
- draft_branch: `draft/issue-{number}`

### Step 7: 결과 수집

모든 에이전트의 결과를 수집합니다.

성공한 이슈:
- quality gate 통과
- draft 브랜치에 커밋 완료

실패한 이슈:
- wip 라벨 제거, 이슈에 실패 코멘트 추가

### Step 8: 승격 (Agent Team)

성공한 각 이슈에 대해 branch-promoter 에이전트를 호출합니다:

전달 정보:
- draft_branch: `draft/issue-{number}`
- issue_number
- issue_title
- branch_strategy: 설정에서 로딩
- label_prefix
- pr_type: "auto"

**성공한 이슈 수가 3개 이하**: 순차 호출
**4개 이상**: 병렬 호출 (background=true)

### Step 9: 라벨 정리

- 승격 성공: `{label_prefix}wip` 제거, `{label_prefix}ready` 제거
- 승격 실패: `{label_prefix}wip` 제거 (다음 cycle에서 재시도)

### Step 10: CronCreate (interval 모드)

interval이 지정된 경우에만 실행합니다:

CronCreate를 호출하여 `/github-autopilot:build-issues`를 지정된 interval로 등록합니다.

### Step 11: 결과 보고

```
## Build Issues 결과

### 분석
- 대상 이슈: 5개
- 배치: 3개 (batch 1: #42, #44 | batch 2: #43 | batch 3: #45)

### 구현
- 성공: #42, #43, #44 (3건)
- 실패: #45 (test failures)

### PR 생성
- #42 → PR #50 (feature/issue-42)
- #43 → PR #51 (feature/issue-43)
- #44 → PR #52 (feature/issue-44)
```

## 주의사항

- 한 cycle에서 첫 번째 배치만 처리 (순차 의존성이 있는 후속 배치는 다음 cycle에서)
- wip 라벨로 중복 작업 방지
- 실패한 이슈는 코멘트로 실패 사유 기록
- draft 브랜치는 로컬 only (remote push 안함)
- 토큰 최적화: MainAgent는 이슈 목록 조회와 라벨 관리만 수행, 구현은 모두 Agent에 위임
