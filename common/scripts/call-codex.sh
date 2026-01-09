#!/bin/bash

# OpenAI Codex CLI를 사용하여 프롬프트를 실행하는 유틸 스크립트
# 자연어 프롬프트에서 파일 경로를 추출하고 파일 내용을 읽어 LLM에 전달
# 사용법: ./call-codex.sh "[자연어 프롬프트]"

set -e
set -u
set -o pipefail

# 환경변수
PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-$PWD}"
OUTPUT_DIR="${CLAUDE_REVIEW_OUTPUT_DIR:-$PROJECT_ROOT/.review-output}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
OUTPUT_FILE="$OUTPUT_DIR/codex-$TIMESTAMP.txt"

# 파라미터
USER_PROMPT="${1:?Error: Prompt required. Usage: $0 \"[prompt]\"}"

# 출력 디렉토리 생성
mkdir -p "$OUTPUT_DIR"

# 파일 경로 추출 ("대상 파일:" 이후의 "- file.md" 패턴만)
FILE_PATHS=$(echo "$USER_PROMPT" | awk '/^대상 파일:|^Target [Ff]iles?:/{flag=1;next}/^[^ \t-]/{flag=0}flag && /^[-*] /{sub(/^[-*] /,"");print}')

# 파일 내용 읽기
FILES_CONTENT=""
if [ -n "$FILE_PATHS" ]; then
    echo "파일 읽는 중..." >&2
    while IFS= read -r file_path; do
        FULL_PATH="$PROJECT_ROOT/$file_path"

        if [ -f "$FULL_PATH" ]; then
            echo "  - $file_path" >&2
            FILES_CONTENT+="

---
File: $file_path
---

$(cat "$FULL_PATH")
"
        else
            echo "  ⚠ Warning: $file_path not found, skipping" >&2
        fi
    done <<< "$FILE_PATHS"
fi

# 전체 프롬프트 구성
FULL_PROMPT="$USER_PROMPT"

if [ -n "$FILES_CONTENT" ]; then
    FULL_PROMPT+="

# Files Content

아래는 대상 파일들의 실제 내용입니다:
$FILES_CONTENT
"
fi

# Codex CLI 호출
echo "OpenAI Codex 호출 중..." >&2
echo "$FULL_PROMPT" | codex exec --skip-git-repo-check - > "$OUTPUT_FILE" 2>&1

# 파일 경로 반환 (stdout)
echo "$OUTPUT_FILE"
