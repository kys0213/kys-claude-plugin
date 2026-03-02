#!/bin/bash

# scope별 diff를 임시 파일에 저장하고 경로만 반환하는 유틸 스크립트
# 사용법: ./get-diff.sh [scope] [target]
#   scope: uncommitted (기본), staged, pr, branch
#   target: branch scope일 때 base 브랜치명

set -e
set -u
set -o pipefail

# 환경변수
PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-$PWD}"
OUTPUT_DIR="${CLAUDE_REVIEW_OUTPUT_DIR:-$PROJECT_ROOT/.review-output}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
OUTPUT_FILE="$OUTPUT_DIR/diff-$TIMESTAMP.txt"

# 파라미터
SCOPE="${1:-uncommitted}"
TARGET="${2:-}"

# 출력 디렉토리 생성
mkdir -p "$OUTPUT_DIR"

# scope별 diff 생성
case "$SCOPE" in
    uncommitted)
        echo "uncommitted 변경사항 수집 중..." >&2
        DIFF_CONTENT=$(cd "$PROJECT_ROOT" && git diff HEAD 2>/dev/null || true)
        # untracked 파일 요약 추가
        UNTRACKED=$(cd "$PROJECT_ROOT" && git ls-files --others --exclude-standard 2>/dev/null || true)
        if [ -n "$UNTRACKED" ]; then
            DIFF_CONTENT+="

# Untracked files:
$UNTRACKED"
        fi
        ;;
    staged)
        echo "staged 변경사항 수집 중..." >&2
        DIFF_CONTENT=$(cd "$PROJECT_ROOT" && git diff --cached 2>/dev/null || true)
        ;;
    pr)
        echo "PR diff 수집 중..." >&2
        DIFF_CONTENT=$(cd "$PROJECT_ROOT" && gh pr diff 2>/dev/null || true)
        ;;
    branch)
        BASE="${TARGET:-main}"
        echo "branch diff 수집 중 (base: $BASE)..." >&2
        DIFF_CONTENT=$(cd "$PROJECT_ROOT" && git diff "$BASE"...HEAD 2>/dev/null || true)
        ;;
    *)
        echo "Error: 알 수 없는 scope '$SCOPE'" >&2
        echo "사용 가능한 scope: uncommitted, staged, pr, branch" >&2
        exit 1
        ;;
esac

# diff가 비어있으면 에러
if [ -z "$DIFF_CONTENT" ]; then
    echo "Error: '$SCOPE' scope에 변경사항이 없습니다." >&2
    exit 1
fi

# 파일에 저장
echo "$DIFF_CONTENT" > "$OUTPUT_FILE"
echo "diff 저장 완료: $(wc -l < "$OUTPUT_FILE") lines" >&2

# 경로만 stdout으로 반환
echo "$OUTPUT_FILE"
