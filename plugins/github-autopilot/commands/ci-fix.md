---
description: "autopilot PR의 CI 실패를 tick 단위로 분석/수정합니다"
argument-hint: ""
allowed-tools: ["Bash", "Read", "Agent"]
---

# CI Fix

autopilot이 생성한 PR의 CI 실패를 감지하고, tick 단위로 수정을 시도합니다. 한 번의 호출에서 수정 → push까지만 수행하고, CI 결과 확인은 다음 tick에서 수행합니다.

## 사용법

```bash
/github-autopilot:ci-fix
```

> 반복 실행은 `/github-autopilot:autopilot`이 `CronCreate`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 작업 프로세스

### Step 1: Base 브랜치 동기화

**branch-sync** 스킬의 절차를 수행합니다.

### Step 1.5: Pipeline Idle Check

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — ci-fix cycle 중단" 알림 발송 후 종료합니다.
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다.
- **exit 1 (active)**: Step 2부터 정상 진행.

### Step 2: CI 실패 PR 조회

설정에서 label_prefix를 확인합니다 (기본값: `autopilot:`).

```bash
gh pr list --label "{label_prefix}auto" --state open --json number,title,headRefName,baseRefName,statusCheckRollup --limit 20
```

statusCheckRollup에서 FAILURE 상태인 PR만 필터링합니다.

CI 실패 PR이 없으면 "CI 실패 PR 없음" 출력 후 종료.

### Step 3: 재시도 횟수 확인

각 CI 실패 PR의 코멘트에서 재시도 마커를 확인합니다:

```bash
gh pr view ${PR_NUMBER} --json comments --jq '.comments[].body' | grep -o '<!-- autopilot:ci-fix:[0-9]* -->' | tail -1
```

마커에서 현재 재시도 횟수 N을 추출합니다 (마커 없으면 N=0).

설정에서 `max_ci_fix_retries`를 확인합니다 (기본값: 3).

**N >= max_ci_fix_retries**: 에스컬레이션
- PR에 에스컬레이션 코멘트 게시:
  ```markdown
  ## CI Fix Escalation

  **Retries exhausted**: {N}/{max_ci_fix_retries}
  CI 실패를 자동으로 해결하지 못했습니다. 사람의 검토가 필요합니다.

  <!-- autopilot:ci-fix:escalated -->
  ```
- `notification` 설정이 있으면 알림 발송
- 해당 PR을 skip하고 다음 PR로 진행

**N < max_ci_fix_retries**: Step 4로 진행

### Step 4: CI 수정 (Agent Team)

수정 대상 PR 각각에 대해 ci-fixer 에이전트를 호출합니다:

**PR 수가 3개 이하**: 순차 호출 (background=false)
**PR 수가 4개 이상**: 병렬 호출 (background=true)

각 에이전트에게 전달:
- pr_number
- pr_title
- head_branch
- base_branch
- retry_count: N
- quality_gate_command: 설정에서 읽은 값

### Step 5: 결과 수집

각 에이전트 결과를 처리합니다:

**fix_pushed** (수정 push 완료):
- PR에 재시도 마커 코멘트 게시:
  ```markdown
  CI fix attempt {N+1}/{max_ci_fix_retries}

  **Failure type**: {failure_type}
  **Fix**: {fix_summary}
  **Files**: {files_modified}

  다음 tick에서 CI 결과를 확인합니다.

  <!-- autopilot:ci-fix:{N+1} -->
  ```

**fix_failed** (수정 실패):
- PR에 실패 코멘트 게시 (재시도 마커 포함)
- 다음 tick에서 다시 시도

### Step 6: 결과 보고

```
## CI Fix 결과

### 대상 PR
- CI 실패 PR: 3개

### 수정
- fix pushed: #50 (lint fix), #52 (test fix)
- fix failed: #51 (complex logic - needs human review)

### 에스컬레이션
- #53 (3/3 retries exhausted → escalated)
```

## 주의사항

- 1 tick = 1 수정 시도. CI 실행 완료를 기다리지 않음
- CI가 아직 실행 중인 PR은 skip (statusCheckRollup에 PENDING이 있으면)
- merge-prs 루프와의 역할 분리: ci-fix는 CI 수정만, merge-prs는 conflict/review만
- 토큰 최적화: MainAgent는 PR 목록 조회와 마커 관리만 수행, CI 분석/수정은 모두 Agent에 위임
