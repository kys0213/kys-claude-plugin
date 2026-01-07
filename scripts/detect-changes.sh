#!/bin/bash
#
# 모노레포 변경 감지 스크립트
# 사용법: ./scripts/detect-changes.sh [base_ref]
#
# 출력: 영향받는 패키지 목록 (줄바꿈 구분)
#   plugins/review
#   plugins/external-llm
#   common
#

set -e

BASE_REF=${1:-main}
REPO_ROOT=$(git rev-parse --show-toplevel)

# 변경된 파일 목록
CHANGED_FILES=$(git diff --name-only "$BASE_REF"...HEAD 2>/dev/null || git diff --name-only "$BASE_REF")

if [ -z "$CHANGED_FILES" ]; then
  echo "No changes detected"
  exit 0
fi

AFFECTED=""

# 1. 직접 변경된 패키지 감지
for file in $CHANGED_FILES; do
  if [[ $file == plugins/* ]]; then
    pkg=$(echo "$file" | cut -d'/' -f1-2)
    AFFECTED="$AFFECTED $pkg"
  elif [[ $file == common/* ]]; then
    AFFECTED="$AFFECTED common"
  elif [[ $file == marketplace/* ]]; then
    pkg=$(echo "$file" | cut -d'/' -f1-2)
    AFFECTED="$AFFECTED $pkg"
  fi
done

# 2. common 변경 시 → 참조하는 패키지 찾기
for file in $CHANGED_FILES; do
  if [[ $file == common/* ]]; then
    # plugins에서 참조 찾기
    if [ -d "$REPO_ROOT/plugins" ]; then
      refs=$(grep -rl "$file" "$REPO_ROOT/plugins" 2>/dev/null || true)
      for ref in $refs; do
        rel_path=${ref#$REPO_ROOT/}
        pkg=$(echo "$rel_path" | cut -d'/' -f1-2)
        AFFECTED="$AFFECTED $pkg"
      done
    fi

    # marketplace에서 참조 찾기
    if [ -d "$REPO_ROOT/marketplace" ]; then
      refs=$(grep -rl "$file" "$REPO_ROOT/marketplace" 2>/dev/null || true)
      for ref in $refs; do
        rel_path=${ref#$REPO_ROOT/}
        pkg=$(echo "$rel_path" | cut -d'/' -f1-2)
        AFFECTED="$AFFECTED $pkg"
      done
    fi
  fi
done

# 중복 제거 후 출력
echo "$AFFECTED" | tr ' ' '\n' | grep -v '^$' | sort -u
