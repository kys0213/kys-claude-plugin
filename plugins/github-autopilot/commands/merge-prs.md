---
description: "autopilot PR을 CI/리뷰 상태 확인 후 머지합니다"
argument-hint: "[interval: 10m, 30m, ...]"
allowed-tools: ["Bash", "Read", "Agent", "CronCreate"]
---

# Merge PRs

autopilot이 생성한 PR들을 분석하여 문제가 없으면 머지하고, 문제가 있으면 해결을 시도합니다.

## 사용법

```bash
/github-autopilot:merge-prs          # 1회 실행
/github-autopilot:merge-prs 10m      # 10분마다 반복
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 작업 프로세스

### Step 1: 인자 파싱

`$ARGUMENTS`에서 interval을 추출합니다.
- `/^\d+[smh]$/` 패턴 매칭 → interval 모드
- 비어있으면 → 1회 실행 모드

### Step 2: 최신 상태 동기화

```bash
git fetch origin
```

### Step 2.5: Pipeline Idle Check

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/check-idle.sh "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — merge-prs cycle 중단" 알림 발송. CronCreate를 등록하지 않고 종료.
- **exit 1 (active)**: Step 3부터 정상 진행.

### Step 3: PR 목록 조회

설정에서 label_prefix를 확인합니다 (기본값: `autopilot:`).

```bash
gh pr list --label "{label_prefix}auto" --state open --json number,title,mergeable,statusCheckRollup,reviewDecision,headRefName,baseRefName --limit 20
```

PR이 없으면 "머지 대상 PR 없음" 출력 후 종료.

### Step 4: PR 분류

각 PR을 상태별로 분류합니다:

| 조건 | 분류 |
|------|------|
| mergeable=MERGEABLE + CI 통과 + 리뷰 승인/없음 | **all-green** |
| mergeable=CONFLICTING | **conflict** |
| CI 실패 | **ci-failure** |
| reviewDecision=CHANGES_REQUESTED | **review-requested** |

### Step 5: All-green PR 즉시 머지

```bash
gh pr merge ${PR_NUMBER} --squash --delete-branch
```

머지 성공 시 기록, 실패 시 문제 PR로 재분류.

### Step 6: 문제 PR 해결 (Agent Team)

문제가 있는 PR 각각에 대해 pr-merger 에이전트를 호출합니다:

**PR 수가 3개 이하**: 순차 호출 (background=false)
**PR 수가 4개 이상**: 병렬 호출 (background=true)

각 에이전트에게 전달:
- pr_number
- pr_title
- problems: 감지된 문제 목록 (conflict, ci_failure, review_changes_requested)

**중요**: 사람이 명시적으로 `CHANGES_REQUESTED` 리뷰를 남긴 PR은 자동 머지하지 않습니다. pr-merger가 코멘트에 응답만 하고, 머지는 사람의 재리뷰 후 다음 cycle에서 처리합니다.

### Step 7: CronCreate (interval 모드)

interval이 지정된 경우에만 실행합니다:

CronCreate를 호출하여 `/github-autopilot:merge-prs`를 지정된 interval로 등록합니다.

### Step 8: 결과 보고

```
## Merge 결과
- 머지 완료: #50, #52 (2건)
- 해결 시도: #51 (conflict → resolved → merged)
- 보류: #53 (human review requested)
- 실패: #54 (conflict resolution failed)
```

## 주의사항

- 사람의 CHANGES_REQUESTED 리뷰가 있는 PR은 자동 머지 금지
- squash merge 사용 (draft에서의 지저분한 커밋 히스토리 정리)
- 머지 후 feature 브랜치 자동 삭제
