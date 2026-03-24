---
description: "CI 실패를 모니터링하고 분석하여 GitHub issue를 생성합니다"
argument-hint: "[interval: 20m, 1h, ...]"
allowed-tools: ["Bash", "Read", "Agent", "CronCreate"]
---

# CI Watch

GitHub Actions의 CI 실패를 감시하고, 실패 원인을 분석하여 GitHub issue로 등록합니다.

## 사용법

```bash
/github-autopilot:ci-watch          # 1회 실행
/github-autopilot:ci-watch 20m      # 20분마다 반복
/github-autopilot:ci-watch 1h       # 1시간마다 반복
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

### Step 3: CI 실패 목록 조회

```bash
gh run list --status failure --limit 10 --json databaseId,name,headBranch,conclusion,createdAt
```

### Step 4: 중복 이슈 필터링

설정에서 label_prefix를 확인합니다 (기본값: `autopilot:`).

각 실패에 대해 **fingerprint 기반**으로 중복을 확인합니다 (issue-label 스킬 참조):

```bash
# fingerprint 형식: ci:{workflow}:{branch}:{failure_type}
FINGERPRINT="ci:validate.yml:main:test-failure"

# 중복 확인 — exit 1이면 skip
bash ${CLAUDE_PLUGIN_ROOT}/scripts/check-duplicate.sh "$FINGERPRINT"
```

중복이 아닌 실패만 Step 5로 진행합니다.

### Step 5: 실패 분석 (Agent Team)

새로운 실패 각각에 대해 ci-failure-analyzer 에이전트를 호출합니다.

**실패가 3개 이하**: 순차 호출 (background=false)
**실패가 4개 이상**: 병렬 호출 (background=true)

각 에이전트에게 전달:
- run_id
- run_name (워크플로우 이름)
- head_branch (실패한 브랜치)

### Step 6: Issue 생성

분석 결과를 바탕으로 GitHub issue를 생성합니다:

```bash
gh issue create \
  --title "fix: CI failure in ${WORKFLOW_NAME} on ${BRANCH}" \
  --label "{label_prefix}ci-failure" \
  --label "{label_prefix}ready" \
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

---
<!-- fingerprint: ci:${WORKFLOW_NAME}:${BRANCH}:${FAILURE_TYPE} -->
EOF
)"
```

> **중요**: `<!-- fingerprint: ... -->` 주석을 body 맨 하단에 반드시 포함한다.

### Step 7: CronCreate (interval 모드)

interval이 지정된 경우에만 실행합니다:

CronCreate를 호출하여 `/github-autopilot:ci-watch` 를 지정된 interval로 등록합니다.
등록 시 interval 인자는 포함하지 않습니다 (재귀 등록 방지).

### Step 8: 결과 보고

생성된 이슈 목록과 분석 요약을 사용자에게 출력합니다.

## 주의사항

- issue-label 스킬의 라벨 필수 규칙과 fingerprint 규칙을 반드시 따른다
- 토큰 최적화: MainAgent는 CI 로그를 직접 읽지 않음. 모든 로그 분석은 ci-failure-analyzer 에이전트가 수행
- flaky test와 실제 실패를 구분하여 라벨링
