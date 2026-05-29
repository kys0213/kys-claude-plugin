#!/bin/bash
# Usage: unresolved-reviews.sh [PR_NUMBER]
# PR의 리뷰 쓰레드를 GraphQL로 조회하여 JSON 출력

set -euo pipefail

# GitHub 환경변수 로드 (GH_HOST 등)
[ -f ~/.git-workflow-env ] && source ~/.git-workflow-env

# PR 번호 결정 (인자 or 현재 브랜치 자동감지)
PR_NUMBER="${1:-$(gh pr view --json number --jq '.number' 2>/dev/null || true)}"
if [ -z "$PR_NUMBER" ]; then
  echo "Error: No PR found. Usage: unresolved-reviews.sh <PR_NUMBER>"
  exit 1
fi

# 리포지토리 정보
OWNER=$(gh repo view --json owner --jq '.owner.login')
REPO=$(gh repo view --json name --jq '.name')

# GraphQL로 리뷰 쓰레드 조회
gh api graphql -f query='
  query($owner:String!, $repo:String!, $pr:Int!) {
    repository(owner:$owner, name:$repo) {
      pullRequest(number:$pr) {
        title
        url
        reviewThreads(first:100) {
          nodes {
            isResolved
            isOutdated
            path
            line
            comments(first:10) {
              nodes {
                author { login }
                body
                createdAt
                url
              }
            }
          }
        }
      }
    }
  }
' -f owner="$OWNER" -f repo="$REPO" -F pr="$PR_NUMBER"
