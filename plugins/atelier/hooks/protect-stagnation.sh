#!/usr/bin/env bash
# protect-stagnation.sh — PreToolUse hook (Bash matcher)
# `atelier autopilot task claim` 명령 직전에 `atelier autopilot check stagnation`으로
# 유사 task 그룹의 반복 실패를 감지하여 redirect 또는 escalate 합니다.
#
# 트리거: PreToolUse (Bash matcher)
# 동작:
#   - claim 패턴이 아니면 → exit 0 (overhead 거의 없음)
#   - stagnation OK (exit 0) → exit 0
#   - stagnation detected (exit 4) → exit 2 + stderr redirect prompt
#   - stagnation escalate (exit 5) → exit 2 + auto-escalate + 사람 개입 안내
#   - 기타 (atelier/jq 부재 등) → exit 0 (best-effort 통과)

set -e

input=$(cat)

# jq 부재 시 best-effort 통과 (가드가 머지 자체를 막지 않게)
if ! command -v jq >/dev/null 2>&1; then
  exit 0
fi

cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // empty' 2>/dev/null || true)

if [ -z "$cmd" ]; then
  exit 0
fi

# 빠른 prefilter: autopilot task claim 패턴이 아니면 통과
if ! printf '%s' "$cmd" | grep -qE '(^|[[:space:]]|;|&&|\|\|)autopilot[[:space:]]+task[[:space:]]+claim([[:space:]]|$)'; then
  exit 0
fi

# task ID 추출 — `autopilot task claim <id>` 또는 `--task <id>` / `--task=<id>`
task_id=$(printf '%s' "$cmd" | sed -nE 's/.*autopilot[[:space:]]+task[[:space:]]+claim[[:space:]]+([a-f0-9]+)\b.*/\1/p')
if [ -z "$task_id" ]; then
  task_id=$(printf '%s' "$cmd" | sed -nE 's/.*--task[[:space:]]*[= ][[:space:]]*([a-f0-9]+)\b.*/\1/p')
fi

if [ -z "$task_id" ]; then
  # task ID 못 찾으면 통과 (CLI 자체 validation 으로 차단)
  exit 0
fi

# atelier 부재 시 best-effort 통과
if ! command -v atelier >/dev/null 2>&1; then
  exit 0
fi

# stagnation 체크
set +e
result=$(atelier autopilot check stagnation --task "$task_id" 2>/dev/null)
exit_code=$?
set -e

case "$exit_code" in
  0)
    # OK — 정상 진행
    exit 0
    ;;
  4)
    # Stagnation detected — redirect prompt 노출
    echo "[STAGNATION DETECTED] task ${task_id}" >&2
    printf '%s' "$result" | jq -r '
      "This task'"'"'s territory is exhausted — \(.similar_tasks | length) similar tasks have failed before:",
      (.pattern.shared_paths | if length > 0 then "  - same paths: \(. | join(", "))" else empty end),
      (.pattern.common_failure_categories | if length > 0 then "  - same failure category: \(. | join(", "))" else empty end),
      "",
      "DO NOT proceed with the same approach. Try one of:",
      "  1. Different file area entirely",
      (if .recommended_persona then "  2. Persona shift: \"\(.recommended_persona)\" — challenge the underlying assumption" else "  2. Different approach to the same problem" end),
      "",
      (if .recommended_persona then "Recommended persona: \(.recommended_persona)" else empty end)
    ' 2>/dev/null >&2 || echo "[INFO] failed to format stagnation prompt — see raw output below" >&2
    exit 2
    ;;
  5)
    # Escalate — 자동 escalate 호출 + 사람 개입 prompt
    echo "[STAGNATION ESCALATED] task ${task_id}" >&2
    echo "Stagnation 임계 (N>=5) 도달. 자동 escalate 됩니다." >&2
    atelier autopilot task escalate "$task_id" --reason "stagnation: auto-escalated by hook (N>=5)" 2>/dev/null \
      || echo "[WARN] auto-escalate 호출 실패 — 수동으로 'atelier autopilot task escalate $task_id' 실행 필요" >&2
    echo "이 영역은 자동 retry 한도를 넘었습니다. 사람의 검토가 필요합니다." >&2
    exit 2
    ;;
  *)
    # 기타 (validation error 등) → 통과 (best-effort)
    exit 0
    ;;
esac
