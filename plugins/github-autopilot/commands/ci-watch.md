---
description: "CI 실패를 모니터링하고 분석하여 GitHub issue를 생성합니다"
argument-hint: "[--run-id=<id> --branch=<branch>]"
allowed-tools: ["Bash", "Read", "Agent"]
---

# CI Watch

GitHub Actions의 CI 실패를 감시하고, 실패 원인을 분석하여 GitHub issue로 등록합니다.

## 사용법

```bash
/github-autopilot:ci-watch                                       # 전체 스캔 (cron 모드)
/github-autopilot:ci-watch --run-id=12345 --branch=main          # 타겟 분석 (hybrid 모드)
```

> 반복 실행은 `/github-autopilot:autopilot`이 CronCreate 또는 Monitor로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 0: 인자 파싱

`$ARGUMENTS`에서 옵션을 추출합니다:
- `--run-id=<id>`: 특정 CI run만 분석 (hybrid 모드에서 이벤트로 전달)
- `--branch=<branch>`: 실패가 발생한 브랜치 (이벤트 컨텍스트)

`--run-id`가 있으면 Step 2의 `gh run list` 쿼리를 건너뛰고 해당 run만 직접 분석합니다 (Step 3의 중복 확인은 수행).

### Step 1: Base 브랜치 동기화

**branch-sync** 스킬의 절차를 수행합니다.

### Step 1.5: Pipeline Idle Check

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — ci-watch cycle 중단" 알림 발송 후 종료합니다.
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다.
- **exit 1 (active)**: Step 2부터 정상 진행.

### Step 2: CI 실패 목록 조회

```bash
gh run list --status failure --limit 10 --json databaseId,name,headBranch,conclusion,createdAt,event
```

### Step 2.5: CI Failure 이슈 자동 정리

이슈를 생성하기 **전에**, 기존 CI failure 이슈 중 관련 PR이 이미 머지된 것을 정리합니다:

```bash
autopilot issue close-resolved --label-prefix "{label_prefix}"
```

### Step 2.7: 오래된/머지된 PR 필터링

Step 2의 결과에서 다음 조건에 해당하는 실패를 **제외**합니다.

설정 예시 (`github-autopilot.local.md`):
```yaml
ci_watch:
  max_age: "24h"             # non-default branch 실패 최대 수집 기간 (기본: 24h)
  default_branch_max_age: "7d"  # default branch 실패 최대 수집 기간 (기본: 7d)
  branch_filter: "autopilot"   # "autopilot" | "all" (기본: autopilot)
```

#### 2.7.1 브랜치 필터 (branch_filter)

`headBranch`가 다음 중 하나에 해당하지 않으면 **skip**:

- **default branch** (main, master, develop 등 `gh repo view --json defaultBranchRef`로 확인)
- **autopilot 브랜치**: `feature/issue-*` 또는 `draft/issue-*` 패턴에 매칭
- **설정의 `branch_filter`가 `"all"`**: 모든 브랜치를 허용

`branch_filter`가 `"autopilot"` (기본값)이면 위 조건에 해당하지 않는 브랜치(일반 feature 브랜치 등)는 skip합니다.

#### 2.7.2 머지/종료된 PR의 실패 제거

`event`가 `pull_request`인 run에 대해 해당 `headBranch`의 PR 상태를 확인합니다:
```bash
gh pr list --head "${HEAD_BRANCH}" --state merged --json number --limit 1
gh pr list --head "${HEAD_BRANCH}" --state closed --json number --limit 1
```
- PR이 MERGED 또는 CLOSED → **skip** (이미 종료된 PR의 과거 실패)
- PR이 OPEN 또는 PR 없음 → 계속 진행

#### 2.7.3 시간 기반 필터 (max_age)

`createdAt`이 현재 시각 기준으로 다음 기간을 초과하면 **skip**:

| 브랜치 종류 | 기본 max_age | 설정 키 |
|---|---|---|
| default branch | `7d` | `ci_watch.default_branch_max_age` |
| non-default branch | `24h` | `ci_watch.max_age` |

- default branch의 실패는 `default_branch_max_age` (기본 7일) 이내만 수집 — 만성 CI 실패를 놓치지 않기 위함
- non-default branch의 실패는 `max_age` (기본 24시간) 이내만 수집 — 오래된 feature 브랜치 실패 방지

필터링 후 남은 실패만 Step 3로 진행합니다.

### Step 3: 중복 이슈 필터링

설정에서 label_prefix를 확인합니다 (기본값: `autopilot:`).

각 실패에 대해 **fingerprint 기반**으로 중복을 확인합니다 (issue-label 스킬 참조):

```bash
# fingerprint 형식: ci:{workflow}:{branch}:{failure_type}
FINGERPRINT="ci:validate.yml:main:test-failure"

# 중복 확인 — exit 1이면 skip
autopilot issue check-dup --fingerprint "$FINGERPRINT"
```

중복이 아닌 실패만 Step 4로 진행합니다.

### Step 4: 실패 분석 (Agent Team)

새로운 실패 각각에 대해 ci-failure-analyzer 에이전트를 호출합니다.

**실패가 3개 이하**: 순차 호출 (background=false)
**실패가 4개 이상**: 병렬 호출 (background=true)

각 에이전트에게 전달:
- run_id
- run_name (워크플로우 이름)
- head_branch (실패한 브랜치)

### Step 5: Issue 생성

분석 결과를 바탕으로 autopilot CLI로 이슈를 생성합니다:

```bash
autopilot issue create \
  --title "fix: CI failure in ${WORKFLOW_NAME} on ${BRANCH}" \
  --label "{label_prefix}ci-failure" \
  --label "{label_prefix}ready" \
  --fingerprint "ci:${WORKFLOW_NAME}:${BRANCH}:${FAILURE_TYPE}" \
  --body "$(cat <<'EOF'
## CI 실패 분석

- **Run**: ${RUN_ID}
- **Workflow**: ${WORKFLOW_NAME}
- **Branch**: ${BRANCH}
- **실패 유형**: ${FAILURE_TYPE}

## 원인 분석

${ANALYSIS_SUMMARY}

## 영향 파일

${AFFECTED_FILES}

## 수정 제안

${SUGGESTED_FIX}
EOF
)"
```

> **참고**: fingerprint HTML 주석은 CLI가 body 하단에 자동 삽입합니다.

### Step 6: 결과 보고

생성된 이슈 목록과 분석 요약을 사용자에게 출력합니다.

## 주의사항

- issue-label 스킬의 라벨 필수 규칙과 fingerprint 규칙을 반드시 따른다
- 토큰 최적화: MainAgent는 CI 로그를 직접 읽지 않음. 모든 로그 분석은 ci-failure-analyzer 에이전트가 수행
- flaky test와 실제 실패를 구분하여 라벨링
