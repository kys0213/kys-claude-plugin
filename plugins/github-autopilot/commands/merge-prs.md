---
description: "autopilot PR을 CI/리뷰 상태 확인 후 머지합니다"
argument-hint: ""
allowed-tools: ["Bash", "Read", "Agent"]
---

# Merge PRs

autopilot이 생성한 PR들을 분석하여 문제가 없으면 머지하고, 문제가 있으면 해결을 시도합니다.

## 사용법

```bash
/github-autopilot:merge-prs
```

> 반복 실행은 `/github-autopilot:autopilot`이 `CronCreate`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 작업 프로세스

### Step 1: Base 브랜치 동기화

**branch-sync** 스킬의 절차를 따릅니다:
1. `github-autopilot.local.md`에서 `work_branch` / `branch_strategy` 읽기
2. base 브랜치 결정 (work_branch > branch_strategy)
3. `git fetch origin` → `git checkout {base_branch}` → `git pull --rebase`

### Step 1.5: Pipeline Idle Check

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — merge-prs cycle 중단" 알림 발송 후 종료합니다.
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다.
- **exit 1 (active)**: Step 2부터 정상 진행.

### Step 2: PR 목록 조회

설정에서 label_prefix를 확인합니다 (기본값: `autopilot:`).

```bash
gh pr list --label "{label_prefix}auto" --state open --json number,title,mergeable,statusCheckRollup,reviewDecision,headRefName,baseRefName --limit 20
```

PR이 없으면 "머지 대상 PR 없음" 출력 후 종료.

### Step 3: PR 분류

각 PR을 상태별로 분류합니다:

| 조건 | 분류 |
|------|------|
| mergeable=MERGEABLE + CI 통과 + 리뷰 승인/없음 | **all-green** |
| mergeable=CONFLICTING | **conflict** |
| CI 실패 | **ci-pending** (ci-fix 루프에서 처리) |
| reviewDecision=CHANGES_REQUESTED | **review-requested** |

> **참고**: CI 실패 PR은 ci-fix 루프에서 tick 단위로 수정합니다. merge-prs는 CI 실패를 직접 수정하지 않습니다.

### Step 4: All-green PR 즉시 머지

```bash
gh pr merge ${PR_NUMBER} --squash --delete-branch
```

머지 성공 후 **관련 이슈 자동 close**:

PR body에서 `Closes #N` 패턴을 추출하여 이슈를 명시적으로 닫습니다. `--squash` 머지 시 GitHub의 auto-close가 동작하지 않을 수 있으므로 fallback으로 직접 실행합니다:

```bash
# PR body에서 모든 Closes #N 이슈 번호를 추출 (macOS/Linux 호환)
ISSUE_NUMBERS=$(gh pr view ${PR_NUMBER} --json body --jq '.body' | grep -oE 'Closes #[0-9]+' | grep -oE '[0-9]+')

# 각 이슈에 대해: 아직 open이면 close
for ISSUE_NUMBER in $ISSUE_NUMBERS; do
  STATE=$(gh issue view "$ISSUE_NUMBER" --json state --jq '.state' 2>/dev/null || echo "")
  if [ "$STATE" = "OPEN" ]; then
    gh issue close "$ISSUE_NUMBER" --comment "Closed by PR #${PR_NUMBER} merge (autopilot)" 2>/dev/null || true
  fi
done
```

머지 실패 시 문제 PR로 재분류.

### Step 5: 문제 PR 해결 (Agent Team)

문제가 있는 PR 각각에 대해 pr-merger 에이전트를 호출합니다:

**PR 수가 3개 이하**: 순차 호출 (background=false)
**PR 수가 4개 이상**: 병렬 호출 (background=true)

각 에이전트에게 전달:
- pr_number
- pr_title
- problems: 감지된 문제 목록 (conflict, review_changes_requested)

> CI 실패는 ci-fix 루프에서 처리하므로 pr-merger에 전달하지 않습니다.

**중요**: 사람이 명시적으로 `CHANGES_REQUESTED` 리뷰를 남긴 PR은 자동 머지하지 않습니다. pr-merger가 코멘트에 응답만 하고, 머지는 사람의 재리뷰 후 다음 cycle에서 처리합니다.

### Step 6: 결과 보고

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
