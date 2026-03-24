#!/usr/bin/env bash
# check-duplicate.sh — fingerprint 기반 이슈 중복 검사
#
# Usage:
#   check-duplicate.sh <fingerprint>
#
# Exit codes:
#   0 — 중복 없음 (생성 가능)
#   1 — 중복 존재 (skip 해야 함)
#   2 — 사용법 오류
#
# Output (중복 시):
#   JSON: {"duplicate": true, "issue_number": 42, "issue_title": "..."}
# Output (중복 아닐 때):
#   JSON: {"duplicate": false}

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: check-duplicate.sh <fingerprint>" >&2
  exit 2
fi

FINGERPRINT="$1"

# gh issue list --search 로 body에서 fingerprint 검색
RESULT=$(gh issue list --state open --search "\"${FINGERPRINT}\" in:body" --json number,title --limit 1 2>/dev/null || echo "[]")

COUNT=$(echo "$RESULT" | jq 'length')

if [[ "$COUNT" -gt 0 ]]; then
  NUMBER=$(echo "$RESULT" | jq -r '.[0].number')
  TITLE=$(echo "$RESULT" | jq -r '.[0].title')
  echo "{\"duplicate\": true, \"issue_number\": ${NUMBER}, \"issue_title\": \"${TITLE}\"}"
  exit 1
else
  echo "{\"duplicate\": false}"
  exit 0
fi
