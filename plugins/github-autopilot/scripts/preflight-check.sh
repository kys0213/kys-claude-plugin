#!/usr/bin/env bash
# preflight-check.sh — autopilot 시작 전 환경 검증 (결정적 검사)
#
# Usage:
#   preflight-check.sh [config_file]
#
# Arguments:
#   config_file — github-autopilot.local.md 경로 (default: github-autopilot.local.md)
#
# Output:
#   JSON with check results per category
#
# Exit codes:
#   0 — all PASS (WARN 허용)
#   1 — FAIL 항목 있음

set -uo pipefail

CONFIG_FILE="${1:-github-autopilot.local.md}"
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

RESULTS=()
HAS_FAIL=false

json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  printf '%s' "$s"
}

add_result() {
  local check="$1" status="$2" detail
  detail="$(json_escape "$3")"
  RESULTS+=("{\"check\":\"${check}\",\"status\":\"${status}\",\"detail\":\"${detail}\"}")
  [[ "$status" == "FAIL" ]] && HAS_FAIL=true
}

# ===================================================================
# A. Convention Verification
# ===================================================================

# A-1. CLAUDE.md 존재 및 파일트리
if [[ -f "${REPO_ROOT}/CLAUDE.md" ]]; then
  claude_content=$(cat "${REPO_ROOT}/CLAUDE.md")

  has_tree=false
  if echo "$claude_content" | grep -qE '(├|└|directory|structure|파일.?구조|file.?tree)'; then
    has_tree=true
  fi

  has_build=false
  if echo "$claude_content" | grep -qE '(cargo|npm|go |make|pytest|jest|gradle|mvn)'; then
    has_build=true
  fi

  has_convention=false
  if echo "$claude_content" | grep -qE '(stack|convention|기술|원칙|principle|컨벤션)'; then
    has_convention=true
  fi

  missing=()
  [[ "$has_tree" == "false" ]] && missing+=("file tree")
  [[ "$has_build" == "false" ]] && missing+=("build/test commands")
  [[ "$has_convention" == "false" ]] && missing+=("tech stack/conventions")

  if [[ ${#missing[@]} -eq 0 ]]; then
    add_result "CLAUDE.md" "PASS" "file tree, build commands, conventions 포함"
  elif [[ ${#missing[@]} -lt 3 ]]; then
    add_result "CLAUDE.md" "WARN" "누락: ${missing[*]}"
  else
    add_result "CLAUDE.md" "FAIL" "CLAUDE.md에 필수 항목 없음: ${missing[*]}"
  fi
else
  add_result "CLAUDE.md" "FAIL" "CLAUDE.md 파일 없음"
fi

# A-2. CLAUDE.md 파일트리 기반 Rules 커버리지 검증
if [[ "${has_tree:-false}" == "true" ]]; then
  # 파일트리에서 주요 디렉토리 추출 (├── dir/ 또는 └── dir/ 패턴)
  tree_dirs=$(echo "$claude_content" | grep -oE '(├──|└──)\s+\S+' | sed 's/[├└──│ ]//g' | sed 's|/$||' | sort -u)

  # .claude/rules/ 에서 paths frontmatter 수집
  rules_coverage=()
  if [[ -d "${REPO_ROOT}/.claude/rules" ]]; then
    for rule_file in "${REPO_ROOT}/.claude/rules/"*.md; do
      [[ ! -f "$rule_file" ]] && continue
      # paths frontmatter에서 경로 패턴 추출
      rule_paths=$(sed -n '/^paths:/,/^[^ -]/p' "$rule_file" | grep -oE '"[^"]*"' | tr -d '"' | head -20)
      if [[ -n "$rule_paths" ]]; then
        while IFS= read -r p; do
          rules_coverage+=("$p")
        done <<< "$rule_paths"
      fi
    done
  fi

  covered=()
  uncovered=()
  while IFS= read -r dir; do
    [[ -z "$dir" ]] && continue
    matched=false
    for pattern in "${rules_coverage[@]+"${rules_coverage[@]}"}"; do
      # pattern "**" covers everything; otherwise check if pattern contains dir name
      if [[ "$pattern" == "**" ]] || [[ "$pattern" == *"$dir"* ]] || [[ "$dir" == "$pattern" ]]; then
        matched=true
        break
      fi
    done
    if [[ "$matched" == "true" ]]; then
      covered+=("$dir")
    else
      uncovered+=("$dir")
    fi
  done <<< "$tree_dirs"

  total=$((${#covered[@]} + ${#uncovered[@]}))
  if [[ $total -eq 0 ]]; then
    add_result "Rules coverage" "WARN" "파일트리에서 디렉토리를 추출할 수 없음"
  elif [[ ${#uncovered[@]} -eq 0 ]]; then
    add_result "Rules coverage" "PASS" "${#covered[@]}/${total} 디렉토리 커버됨"
  elif [[ ${#uncovered[@]} -le 2 ]]; then
    add_result "Rules coverage" "WARN" "미커버: ${uncovered[*]} (${#covered[@]}/${total})"
  else
    add_result "Rules coverage" "FAIL" "미커버: ${uncovered[*]} (${#covered[@]}/${total})"
  fi
else
  add_result "Rules coverage" "WARN" "CLAUDE.md에 파일트리 없음 — 커버리지 검증 불가"
fi

# ===================================================================
# B. Automation Environment Verification
# ===================================================================

# B-1. GitHub 인증
if gh auth status &>/dev/null; then
  add_result "gh auth" "PASS" "authenticated"
else
  add_result "gh auth" "FAIL" "gh auth login 필요"
fi

# B-2. Guard PR Base Hook
if [[ -f "${REPO_ROOT}/.claude/settings.local.json" ]] && grep -q "guard-pr-base" "${REPO_ROOT}/.claude/settings.local.json" 2>/dev/null; then
  add_result "Hooks" "PASS" "guard-pr-base registered"
else
  add_result "Hooks" "WARN" "guard-pr-base hook 미등록"
fi

# B-3. Quality Gate Command
qg_cmd=""
if [[ -f "$CONFIG_FILE" ]]; then
  qg_cmd=$(grep -E '^quality_gate_command:' "$CONFIG_FILE" | sed 's/^quality_gate_command:\s*//' | sed 's/^"//;s/"$//' | sed "s/^'//;s/'$//" | xargs)
fi

if [[ -z "$qg_cmd" ]]; then
  add_result "Quality Gate" "PASS" "auto-detect"
else
  first_token="${qg_cmd%% *}"
  if command -v "$first_token" &>/dev/null; then
    add_result "Quality Gate" "PASS" "${qg_cmd}"
  else
    add_result "Quality Gate" "FAIL" "command not found: ${first_token}"
  fi
fi

# B-4. Git Remote
if git remote get-url origin &>/dev/null; then
  origin_url=$(git remote get-url origin)
  add_result "Git Remote" "PASS" "${origin_url}"
else
  add_result "Git Remote" "FAIL" "origin remote 없음"
fi

# ===================================================================
# C. Spec Existence Check
# ===================================================================

spec_paths=()
if [[ -f "$CONFIG_FILE" ]]; then
  while IFS= read -r line; do
    path=$(echo "$line" | sed 's/^[[:space:]]*-[[:space:]]*//' | sed 's/^"//;s/"$//' | sed "s/^'//;s/'$//")
    [[ -n "$path" ]] && spec_paths+=("$path")
  done < <(sed -n '/^spec_paths:/,/^[^ -]/p' "$CONFIG_FILE" | grep -E '^\s+-' | head -10)
fi

if [[ ${#spec_paths[@]} -eq 0 ]]; then
  add_result "Spec files" "WARN" "spec_paths 미설정"
else
  total_specs=0
  for sp in "${spec_paths[@]}"; do
    count=$(find "${REPO_ROOT}/${sp}" -name "*.md" -type f 2>/dev/null | wc -l | xargs)
    total_specs=$((total_specs + count))
  done

  if [[ $total_specs -gt 0 ]]; then
    add_result "Spec files" "PASS" "${total_specs}개 스펙 파일 발견 (${spec_paths[*]})"
  else
    add_result "Spec files" "FAIL" "spec_paths에 .md 파일 없음 (${spec_paths[*]})"
  fi
fi

# ===================================================================
# Output
# ===================================================================

# JSON output
echo "["
for i in "${!RESULTS[@]}"; do
  if [[ $i -lt $((${#RESULTS[@]} - 1)) ]]; then
    echo "  ${RESULTS[$i]},"
  else
    echo "  ${RESULTS[$i]}"
  fi
done
echo "]"

# exit code
if [[ "$HAS_FAIL" == "true" ]]; then
  exit 1
else
  exit 0
fi
