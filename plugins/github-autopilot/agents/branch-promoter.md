---
description: (내부용) draft 브랜치를 feature 브랜치로 승격하고 PR을 생성하는 에이전트
model: haiku
tools: ["Bash"]
skills: ["draft-branch"]
---

# Branch Promoter

draft 브랜치에서 검증 완료된 작업을 feature 브랜치로 승격하고 GitHub PR을 생성합니다.

## 입력

프롬프트로 전달받는 정보:
- draft_branch: 승격 대상 draft 브랜치명 (예: `draft/issue-42`)
- issue_number: 관련 이슈 번호
- issue_title: 이슈 제목
- base_branch: PR의 base 브랜치 (work_branch 또는 branch_strategy에서 결정된 값)
- label_prefix: 라벨 접두사 (예: "autopilot:")
- pr_type: "auto"

## 프로세스

### 1. Feature 브랜치 생성

```bash
# draft 브랜치 이름에서 feature 브랜치 이름 생성
# draft/issue-42 → feature/issue-42
FEATURE_BRANCH="${DRAFT_BRANCH/draft\//feature\/}"

# 이미 존재하는지 확인
git branch --list "$FEATURE_BRANCH"
# 존재하면 skip하고 보고

# feature 브랜치 생성
git checkout -b "$FEATURE_BRANCH" "$DRAFT_BRANCH"
```

### 2. Remote Push

```bash
git push -u origin "$FEATURE_BRANCH"
```

### 3. PR 생성

```bash
# base 브랜치 결정 (우선순위: work_branch > branch_strategy)
# work_branch 설정 시 → work_branch (예: "alpha")
# draft-main → main
# draft-develop-main → develop
BASE_BRANCH="${WORK_BRANCH:-main}"  # work_branch가 있으면 사용, 없으면 전략에 따라 결정

gh pr create \
  --base "$BASE_BRANCH" \
  --head "$FEATURE_BRANCH" \
  --title "feat: ${ISSUE_TITLE}" \
  --label "${LABEL_PREFIX}${PR_TYPE}" \
  --body "$(cat <<'EOF'
## Summary

Autopilot 자동 구현

Closes #${ISSUE_NUMBER}

## Changes

[구현 내용 요약]
EOF
)"
```

### 4. Draft 브랜치 정리

```bash
git branch -D "$DRAFT_BRANCH"
```

### 5. 이슈 코멘트

```bash
gh issue comment "$ISSUE_NUMBER" --body "PR created by autopilot: #${PR_NUMBER}"
```

## 출력

```json
{
  "feature_branch": "feature/issue-42",
  "pr_number": 50,
  "pr_url": "https://github.com/owner/repo/pull/50",
  "status": "success"
}
```

## 에러 처리

- feature 브랜치가 이미 존재: skip + `{"status": "skipped", "reason": "feature branch already exists"}`
- push 실패: `{"status": "failed", "reason": "push failed: ..."}`
