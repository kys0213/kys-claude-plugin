#!/bin/bash

# codex review 네이티브 CLI를 사용하여 코드 리뷰를 수행하는 래퍼 스크립트
# 사용법: ./call-codex-review.sh [scope] [target] [prompt]
#   scope: uncommitted (기본), staged, pr, branch
#   target: branch scope일 때 base 브랜치명 (없으면 빈 문자열)
#   prompt: 추가 리뷰 지시사항 (선택)

set -e
set -u
set -o pipefail

# 환경변수
PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-$PWD}"
OUTPUT_DIR="${CLAUDE_REVIEW_OUTPUT_DIR:-$PROJECT_ROOT/.review-output}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
OUTPUT_FILE="$OUTPUT_DIR/codex-review-$TIMESTAMP.txt"

# 파라미터
SCOPE="${1:-uncommitted}"
TARGET="${2:-}"
PROMPT="${3:-}"

# 출력 디렉토리 생성
mkdir -p "$OUTPUT_DIR"

# codex CLI 확인
if ! command -v codex &> /dev/null; then
    echo "Error: codex CLI가 설치되지 않았습니다." >&2
    echo "설치: npm install -g @openai/codex" >&2
    exit 1
fi

# scope별 codex review 플래그 결정
CODEX_FLAGS=""
case "$SCOPE" in
    uncommitted)
        CODEX_FLAGS="--uncommitted"
        ;;
    staged)
        # codex review에 --staged 플래그가 없으므로 --uncommitted으로 대체.
        # staged만 정확히 보려면 get-diff.sh staged → diff 파일 기반 리뷰를 권장.
        CODEX_FLAGS="--uncommitted"
        ;;
    pr)
        # PR의 base 브랜치 가져오기
        PR_BASE=$(cd "$PROJECT_ROOT" && gh pr view --json baseRefName -q '.baseRefName' 2>/dev/null || echo "main")
        CODEX_FLAGS="--base $PR_BASE"
        ;;
    branch)
        BASE="${TARGET:-main}"
        CODEX_FLAGS="--base $BASE"
        ;;
    *)
        echo "Error: 알 수 없는 scope '$SCOPE'" >&2
        exit 1
        ;;
esac

# 프롬프트 구성
REVIEW_PROMPT="코드 변경사항을 리뷰해주세요."
if [ -n "$PROMPT" ]; then
    REVIEW_PROMPT="$PROMPT"
fi

# codex review 실행
echo "OpenAI Codex review 실행 중 (scope: $SCOPE)..." >&2
cd "$PROJECT_ROOT"
# shellcheck disable=SC2086
codex review $CODEX_FLAGS --prompt "$REVIEW_PROMPT" > "$OUTPUT_FILE" 2>&1

echo "codex review 완료" >&2

# 경로만 stdout으로 반환
echo "$OUTPUT_FILE"
