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

### Step 2: 설정 로딩

설정 파일(`github-autopilot.local.md`)에서 다음 값을 읽습니다:

- `max_parallel_agents`: 동시에 실행할 최대 에이전트 수 (기본값: 3)

### Step 3: Pipeline Idle Check

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — build-issues cycle 중단" 알림 발송 후 종료합니다.
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다.
- **exit 1 (active)**: Step 4부터 정상 진행

### Step 3.5: Idle Count Check

이전 Step의 결과가 "대상 없음"(idle)이면, 연속 idle 횟수를 기록합니다.

```bash
autopilot check mark build-issues --status idle
```

설정에서 `idle_shutdown.max_idle` 값을 읽습니다 (기본값: 5).

연속 idle 횟수가 `max_idle` 이상이면:
1. `autopilot cron self-delete --name "build-issues"` 로 cron을 자동 해제합니다.
2. "연속 {N}회 idle — cron 자동 해제" 메시지를 출력하고 종료합니다.

실제 작업을 수행하면 idle count를 리셋합니다:
```bash
autopilot check mark build-issues --status active
```

### Step 4: Skip 이슈 알림

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

### Step 5: Ready 이슈 조회

설정에서 label_prefix를 확인합니다 (기본값: `autopilot:`).

```bash
gh issue list \
  --label "{label_prefix}ready" \
  --state open \
  --json number,title,body,labels,comments \
  --limit 20
```

이미 `{label_prefix}wip` 라벨이 붙은 이슈는 제외합니다 (진행 중인 작업).

이슈가 없으면 `autopilot check mark build-issues --status idle` 후 "구현 대상 이슈 없음" 출력 후 종료.

### Step 5.5: 코멘트 기반 재작업 감지

Step 5에서 제외된 이슈(ready 라벨 없음) 중, 코멘트에 재작업 요청이 포함된 이슈를 탐색합니다.

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
3. Step 5 결과 목록에 해당 이슈를 추가합니다.

### Step 6: 의존성 분석 (Agent)

issue-dependency-analyzer 에이전트를 호출합니다 (background=false):

전달 정보:
- 이슈 목록 (number, title, body)

결과: 배치 목록 (병렬 실행 가능한 이슈 그룹)

### Step 6.5: 이슈 간 유사도 검사

첫 번째 배치(병렬 실행 대상) 내 이슈들의 텍스트 유사도를 측정합니다:

```bash
echo '${BATCH_ISSUES_JSON}' | autopilot issue detect-overlap --threshold 15
```

- 입력: 배치 내 이슈들의 `[{"number", "title", "body"}]` JSON
- 출력: `review_required` 배열에 유사도가 높은 이슈 쌍 + 양쪽 본문 포함

`review_required`가 비어있으면 Step 7로 진행합니다.

`review_required`에 항목이 있으면:
1. 해당 이슈 쌍의 **제목, 본문, 유사도 distance**를 issue-dependency-analyzer에 추가 context로 전달합니다.
2. "이 이슈들이 유사한 내용을 다루고 있습니다. 같은 파일을 수정할 가능성이 있으므로 병렬/순차 실행 여부를 판단해주세요."
3. dependency-analyzer가 순차 실행이 필요하다고 판단하면 배치를 재구성합니다.

### Step 7: WIP 라벨 추가

현재 배치의 이슈들에 wip 라벨을 추가합니다 (중복 작업 방지):

```bash
gh issue edit ${ISSUE_NUMBER} --add-label "{label_prefix}wip"
```

### Step 8: 구현 (Agent Team)

구현을 시작하기 전에 idle count를 리셋합니다: `autopilot check mark build-issues --status active`

첫 번째 배치(의존성 없는 이슈들)부터 순서대로 처리합니다.

각 배치 내 이슈들을 `max_parallel_agents` 단위로 분할하여 순차 그룹으로 실행합니다:

- 배치 내 이슈가 `max_parallel_agents`보다 많으면:
  1. 이슈를 `max_parallel_agents` 크기의 서브그룹으로 분할
  2. 서브그룹 1 병렬 실행 → 완료 대기
  3. **Rate limit 체크**: 결과에 rate limit(429) 에러가 있으면 → 60초 대기 후 다음 서브그룹 진행
  4. 서브그룹 2 병렬 실행 → 완료 대기
  5. ...반복
- 예: `max_parallel_agents=3`, 배치 이슈 7개
  → 그룹1: #1,#2,#3 (병렬) → 그룹2: #4,#5,#6 (병렬) → 그룹3: #7 (단독)

각 이슈에 대해 issue-implementer 에이전트를 호출합니다:
- `isolation: "worktree"` 로 독립 작업 공간 확보
- `run_in_background: true` 로 병렬 실행

각 이슈의 comments를 필터링합니다:

```bash
echo '${COMMENTS_JSON}' | autopilot issue filter-comments
```

출력의 `comments`가 필터링된 코멘트, `failure_analysis`가 실패 패턴 분석 결과입니다.

전달 정보 (**모든 항목 필수 — 생략 금지**):
- issue_number
- issue_title
- issue_body (요구사항, 영향 범위, 구현 가이드)
- **issue_comments**: `filter-comments` 출력의 `comments` 배열. 불필요한 내부 마커와 중복 실패 코멘트가 제거되어 있다
- **recommended_persona**: `filter-comments` 출력의 `failure_analysis.recommended_persona` (null이면 생략). 같은 카테고리의 실패가 반복되면 persona가 추천된다
- draft_branch: `draft/issue-{number}`
- base_branch: Step 1에서 결정한 base 브랜치
- quality_gate_command: 설정에서 읽은 값 (비어있으면 자동 감지)

### Step 9: 결과 수집

모든 에이전트의 결과를 수집합니다.

성공한 이슈:
- quality gate 통과
- draft 브랜치에 커밋 완료

실패한 이슈:
- wip 라벨 제거

### Step 9.5: 에스컬레이션 체크

실패한 이슈 각각에 대해 연속 실패 횟수를 확인합니다.

**Rate limit(429) 실패는 코드 문제가 아니므로 특별 처리합니다:**
- 실패 마커(failure count) 증가 없이 wip 라벨만 제거
- 다음 cycle에서 자동 재시도
- 에스컬레이션 대상에서 제외

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

### Step 10: 승격 (Agent Team)

성공한 각 이슈에 대해 branch-promoter 에이전트를 호출합니다:

전달 정보:
- draft_branch: `draft/issue-{number}`
- issue_number
- issue_title
- base_branch: 설정에서 결정 (work_branch > branch_strategy)
- label_prefix
- pr_type: "auto"

성공한 이슈들을 Step 8과 동일한 `max_parallel_agents` 서브그룹 방식으로 실행합니다.
- 이슈 수가 `max_parallel_agents` 이하: 순차 호출 (background=false)
- 이슈 수가 `max_parallel_agents` 초과: 서브그룹 분할 후 병렬 실행

### Step 11: 라벨 정리

- 승격 성공: `{label_prefix}wip` 제거, `{label_prefix}ready` 제거
- 승격 실패: `{label_prefix}wip` 제거 (다음 cycle에서 재시도)

### Step 12: 결과 보고

```
## Build Issues 결과

### Skip 이슈 알림
- 대기 중: #38 (알림 전송됨 → Slack DM)

### 구현 대상
- 대상 이슈: 5개
- 배치: 3개 (batch 1: #42, #44 | batch 2: #43 | batch 3: #45)

### 설정
- max_parallel_agents: 3

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
