#!/usr/bin/env bash
# create-issue.sh — 통합 이슈 생성 스크립트
#
# fingerprint 중복 검사 → 라벨 자동 결정 → 이슈 생성을 단일 인터페이스로 통합합니다.
#
# Usage:
#   create-issue.sh --type <type> --title <title> --body <body> \
#                    --fingerprint <fp> --label-prefix <prefix> [--dry-run]
#
# Arguments:
#   --type          이슈 타입: gap | ci-failure | qa | test
#   --title         이슈 제목
#   --body          이슈 본문 (fingerprint 주석은 자동 삽입됨)
#   --fingerprint   중복 검사용 fingerprint
#   --label-prefix  라벨 접두사 (기본값: "autopilot:")
#   --dry-run       실제 생성 없이 미리보기만 출력
#
# Exit codes:
#   0 — 이슈 생성 성공 (또는 dry-run 완료)
#   1 — 중복 이슈 존재 (skip)
#   2 — 사용법 오류
#
# Output (성공 시):
#   JSON: {"created": true, "number": 42, "title": "...", "fingerprint": "..."}
# Output (중복 시):
#   JSON: {"created": false, "duplicate": true, "existing_issue": 42, "fingerprint": "..."}
# Output (dry-run 시):
#   JSON: {"dry_run": true, "title": "...", "labels": [...], "fingerprint": "..."}

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- 인자 파싱 ---
TYPE=""
TITLE=""
BODY=""
FINGERPRINT=""
LABEL_PREFIX="autopilot:"
DRY_RUN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --type)        TYPE="$2"; shift 2 ;;
    --title)       TITLE="$2"; shift 2 ;;
    --body)        BODY="$2"; shift 2 ;;
    --fingerprint) FINGERPRINT="$2"; shift 2 ;;
    --label-prefix) LABEL_PREFIX="$2"; shift 2 ;;
    --dry-run)     DRY_RUN=true; shift ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Usage: create-issue.sh --type <type> --title <title> --body <body> --fingerprint <fp> [--label-prefix <prefix>] [--dry-run]" >&2
      exit 2
      ;;
  esac
done

# --- 필수 인자 검증 ---
if [[ -z "$TYPE" ]] || [[ -z "$TITLE" ]] || [[ -z "$BODY" ]] || [[ -z "$FINGERPRINT" ]]; then
  echo "Error: --type, --title, --body, --fingerprint are required" >&2
  exit 2
fi

command -v gh &>/dev/null || { echo '{"error": "gh CLI not found"}' >&2; exit 2; }
command -v jq &>/dev/null || { echo '{"error": "jq not found"}' >&2; exit 2; }

# --- 타입별 라벨 결정 (issue-label 스킬 규칙 기반) ---
LABELS=()
case "$TYPE" in
  gap)
    LABELS=("${LABEL_PREFIX}ready")
    ;;
  ci-failure)
    LABELS=("${LABEL_PREFIX}ci-failure" "${LABEL_PREFIX}ready")
    ;;
  qa)
    LABELS=("${LABEL_PREFIX}ready")
    ;;
  test)
    LABELS=("${LABEL_PREFIX}ready")
    ;;
  *)
    echo "Error: unknown type '$TYPE'. Must be one of: gap, ci-failure, qa, test" >&2
    exit 2
    ;;
esac

# --- fingerprint 중복 검사 ---
DEDUP_RESULT=$("$SCRIPT_DIR/check-duplicate.sh" "$FINGERPRINT" 2>/dev/null) || DEDUP_EXIT=$?
DEDUP_EXIT=${DEDUP_EXIT:-0}

if [[ "$DEDUP_EXIT" -eq 1 ]]; then
  EXISTING_NUMBER=$(echo "$DEDUP_RESULT" | jq -r '.issue_number')
  jq -n --arg fp "$FINGERPRINT" --argjson num "$EXISTING_NUMBER" \
    '{created: false, duplicate: true, existing_issue: $num, fingerprint: $fp}'
  exit 1
elif [[ "$DEDUP_EXIT" -eq 2 ]]; then
  echo "$DEDUP_RESULT" >&2
  exit 2
fi

# --- body에 fingerprint 주석 삽입 ---
FULL_BODY="${BODY}

---
<!-- fingerprint: ${FINGERPRINT} -->"

# --- dry-run ---
if [[ "$DRY_RUN" = true ]]; then
  LABELS_JSON=$(printf '%s\n' "${LABELS[@]}" | jq -R . | jq -s .)
  jq -n --arg title "$TITLE" --argjson labels "$LABELS_JSON" --arg fp "$FINGERPRINT" \
    '{dry_run: true, title: $title, labels: $labels, fingerprint: $fp}'
  exit 0
fi

# --- 이슈 생성 ---
LABEL_ARGS=()
for label in "${LABELS[@]}"; do
  LABEL_ARGS+=(--label "$label")
done

RESULT=$(gh issue create \
  --title "$TITLE" \
  "${LABEL_ARGS[@]}" \
  --body "$FULL_BODY" 2>/dev/null)

# gh issue create 는 이슈 URL을 출력함 — 번호 추출
ISSUE_NUMBER=$(echo "$RESULT" | grep -oE '[0-9]+$' || echo "0")

jq -n --arg title "$TITLE" --arg fp "$FINGERPRINT" --argjson num "$ISSUE_NUMBER" \
  '{created: true, number: $num, title: $title, fingerprint: $fp}'
