---
description: "CI 실패를 모니터링하고 분석하여 GitHub issue를 생성합니다"
argument-hint: ""
allowed-tools: ["Bash", "Read", "Agent"]
---

# CI Watch

GitHub Actions의 CI 실패를 감시하고, 실패 원인을 분석하여 GitHub issue로 등록합니다.

## 사용법

```bash
/github-autopilot:ci-watch
```

> 반복 실행은 `/github-autopilot:autopilot`이 `CronCreate`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

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

Step 2의 결과에서 다음 조건에 해당하는 실패를 **제외**합니다:

1. **머지/종료된 PR의 실패 제거**: `event`가 `pull_request`인 run에 대해 해당 `headBranch`의 PR 상태를 확인합니다:
   ```bash
   gh pr list --head "${HEAD_BRANCH}" --state merged --json number --limit 1
   gh pr list --head "${HEAD_BRANCH}" --state closed --json number --limit 1
   ```
   - PR이 MERGED 또는 CLOSED → **skip** (이미 종료된 PR의 과거 실패)
   - PR이 OPEN 또는 PR 없음 → 계속 진행

2. **7일 이상 된 실패 제거**: `createdAt`이 현재 시각 기준 7일 이전이면 **skip** (단, default branch의 실패는 예외 — 만성 CI 실패를 놓치지 않기 위함)

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
