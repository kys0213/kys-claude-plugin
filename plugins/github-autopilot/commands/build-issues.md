---
description: "autopilot 이슈를 분석하고 draft 브랜치에서 구현 후 PR을 생성합니다"
argument-hint: ""
allowed-tools: ["Bash", "Read", "Agent"]
---

# Build Issues

autopilot 라벨이 붙은 GitHub 이슈를 가져와 의존성을 분석하고, draft 브랜치에서 구현한 뒤 feature 브랜치로 승격하여 PR을 생성합니다.

## 사용법

```bash
/github-autopilot:build-issues
```

> 반복 실행은 `/github-autopilot:autopilot`이 `CronCreate`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 1: Base 브랜치 동기화

**branch-sync** 스킬의 절차를 수행합니다.

### Step 2: Pipeline Idle Check

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — build-issues cycle 중단" 알림 발송 후 종료합니다.
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다.
- **exit 1 (active)**: Step 3부터 정상 진행

### Step 3: Skip 이슈 알림

설정에서 `notification` 값을 확인합니다 (비어있으면 이 Step을 건너뜁니다).

autopilot 분석 코멘트가 있지만 `:ready` 라벨이 없는 이슈를 조회합니다:

```bash
gh issue list \
  --state open \
  --json number,title,labels,comments \
  --limit 50
```

필터 조건:
- `{label_prefix}` 로 시작하는 라벨이 **없음**
- 코멘트에 "Autopilot 분석 결과"가 **포함됨** (이전에 skip 판정을 받은 이슈)
- 이미 알림을 보낸 이슈는 제외 (코멘트에 `<!-- notified -->` 가 포함된 이슈)

해당 이슈가 있으면, `notification` 설정에 적힌 자연어 지시에 따라 알림을 보냅니다. 환경에 구성된 도구(MCP, Skill 등)를 활용합니다.

알림 내용:
- 대상 이슈 번호와 제목
- "이슈를 수정한 후 `/analyze-issue {번호}`를 실행해주세요"

알림 후 해당 이슈에 마커 코멘트를 남깁니다:

```bash
gh issue comment ${ISSUE_NUMBER} --body "<!-- notified -->"
```

### Step 4: Ready 이슈 조회

설정에서 label_prefix를 확인합니다 (기본값: `autopilot:`).

```bash
gh issue list \
  --label "{label_prefix}ready" \
  --state open \
  --json number,title,body,labels,comments \
  --limit 20
```

이미 `{label_prefix}wip` 라벨이 붙은 이슈는 제외합니다 (진행 중인 작업).

이슈가 없으면 "구현 대상 이슈 없음" 출력 후 종료.

### Step 4.5: 코멘트 기반 재작업 감지

Step 4에서 제외된 이슈(ready 라벨 없음) 중, 코멘트에 재작업 요청이 포함된 이슈를 탐색합니다.

```bash
gh issue list \
  --state open \
  --json number,title,body,labels,comments \
  --limit 50
```

필터 조건:
- `{label_prefix}ready` 라벨이 **없음**
- `{label_prefix}wip` 라벨이 **없음**
- 코멘트에 재작업 키워드 포함: `재구현 필요`, `재작업`, `rework`, `다시 구현`, `re-implement`, `라우트가.*제거됨`
- 해당 키워드가 포함된 코멘트 이후에 `<!-- autopilot:rework-resolved -->` 마커가 **없음** (이미 처리된 건 제외)

해당 이슈가 발견되면:
1. `{label_prefix}ready` 라벨을 재부여합니다:
   ```bash
   gh issue edit ${ISSUE_NUMBER} --add-label "{label_prefix}ready"
   ```
2. 마커 코멘트를 남깁니다:
   ```bash
   gh issue comment ${ISSUE_NUMBER} --body "Autopilot: 코멘트에서 재작업 요청 감지 — ready 라벨 재부여

   <!-- autopilot:rework-detected -->"
   ```
3. Step 4 결과 목록에 해당 이슈를 추가합니다.

### Step 5: 의존성 분석 (Agent)

issue-dependency-analyzer 에이전트를 호출합니다 (background=false):

전달 정보:
- 이슈 목록 (number, title, body)

결과: 배치 목록 (병렬 실행 가능한 이슈 그룹)

### Step 6: WIP 라벨 추가

현재 배치의 이슈들에 wip 라벨을 추가합니다 (중복 작업 방지):

```bash
gh issue edit ${ISSUE_NUMBER} --add-label "{label_prefix}wip"
```

### Step 7: 구현 (Agent Team)

첫 번째 배치(의존성 없는 이슈들)부터 순서대로 처리합니다.

각 배치 내 이슈들은 병렬로 구현합니다:

각 이슈에 대해 issue-implementer 에이전트를 호출합니다:
- `isolation: "worktree"` 로 독립 작업 공간 확보
- `run_in_background: true` 로 병렬 실행

전달 정보 (**모든 항목 필수 — 생략 금지**):
- issue_number
- issue_title
- issue_body (요구사항, 영향 범위, 구현 가이드)
- **issue_comments**: Step 4에서 가져온 comments 배열 **전체**를 반드시 포함한다. 분석 코멘트, 재작업 요청, 추가 컨텍스트가 포함되어 있으므로 절대 생략하지 않는다
- draft_branch: `draft/issue-{number}`
- base_branch: Step 1에서 결정한 base 브랜치
- quality_gate_command: 설정에서 읽은 값 (비어있으면 자동 감지)

### Step 8: 결과 수집

모든 에이전트의 결과를 수집합니다.

성공한 이슈:
- quality gate 통과
- draft 브랜치에 커밋 완료

실패한 이슈:
- wip 라벨 제거

### Step 8.5: 에스컬레이션 체크

실패한 이슈 각각에 대해 연속 실패 횟수를 확인합니다.

```bash
# 이슈 코멘트에서 실패 마커 조회
gh issue view ${ISSUE_NUMBER} --json comments --jq '.comments[].body' | grep -o '<!-- autopilot:failure:[0-9]* -->' | tail -1
```

마커에서 현재 실패 횟수 N을 추출합니다 (마커 없으면 N=0).

**N+1 >= `max_consecutive_failures` (기본: 3)**: 에스컬레이션
- 구조화된 에스컬레이션 리포트를 이슈 코멘트로 게시:
  ```markdown
  ## Autopilot Escalation Report

  **Consecutive failures**: {N+1}/{max_consecutive_failures}
  **Failure category**: {failure_category}

  ### Failure History
  | Attempt | Category | Summary |
  |---------|----------|---------|
  | 1 | lint_failure | cargo clippy warnings |
  | 2 | test_failure | assertion failed |
  | ... | ... | ... |

  ### Recommended Action
  - 이 이슈는 autopilot이 자동 해결하기 어려운 문제입니다
  - 사람의 검토가 필요합니다

  <!-- autopilot:escalated -->
  ```
- `{label_prefix}ready` 라벨 제거 (재시도 중단)
- `notification` 설정이 있으면 에스컬레이션 알림 발송

**N+1 < threshold**: 재시도 예약
- 실패 코멘트에 마커 포함:
  ```markdown
  Autopilot 구현 실패 (attempt {N+1}/{max_consecutive_failures})

  **Category**: {failure_category}
  **Reason**: {reason}
  **Details**: {details}

  다음 cycle에서 재시도합니다.

  <!-- autopilot:failure:{N+1} -->
  ```
- `{label_prefix}ready` 라벨 유지 (다음 cycle 재시도)

### Step 9: 승격 (Agent Team)

성공한 각 이슈에 대해 branch-promoter 에이전트를 호출합니다:

전달 정보:
- draft_branch: `draft/issue-{number}`
- issue_number
- issue_title
- base_branch: 설정에서 결정 (work_branch > branch_strategy)
- label_prefix
- pr_type: "auto"

**성공한 이슈 수가 3개 이하**: 순차 호출
**4개 이상**: 병렬 호출 (background=true)

### Step 10: 라벨 정리

- 승격 성공: `{label_prefix}wip` 제거, `{label_prefix}ready` 제거
- 승격 실패: `{label_prefix}wip` 제거 (다음 cycle에서 재시도)

### Step 11: 결과 보고

```
## Build Issues 결과

### Skip 이슈 알림
- 대기 중: #38 (알림 전송됨 → Slack DM)

### 구현 대상
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
