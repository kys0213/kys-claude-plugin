#!/bin/sh
input=$(cat)

# --- Parse all fields in a single jq call ---
eval "$(printf '%s' "$input" | jq -r '
  @sh "cwd=\(.workspace.current_dir // .cwd // "")",
  @sh "model=\(.model.display_name // "")",
  @sh "used=\(.context_window.used_percentage // "")"
')"

# --- Colors ---
RESET='\033[0m'
CYAN='\033[36m'
GREEN='\033[32m'
YELLOW='\033[33m'
RED='\033[31m'
MAGENTA='\033[35m'
DIM='\033[2m'

# --- Git helper (reduces repeated flags) ---
_git() { git -C "$cwd" -c core.hooksPath=/dev/null "$@" 2>/dev/null; }

# --- Git info ---
toplevel=""
repo_url=""
branch=""
if _git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  toplevel=$(_git rev-parse --show-toplevel)
  branch=$(_git symbolic-ref --short HEAD || _git rev-parse --short HEAD)

  # GitHub remote URL (pure shell, no sed)
  remote=$(_git remote get-url origin)
  case "$remote" in
    https://github.com/*)
      repo_url="${remote%.git}"
      ;;
    git@github.com:*)
      tmp="${remote#git@github.com:}"
      repo_url="https://github.com/${tmp%.git}"
      ;;
  esac
fi

# Project directory name (parameter expansion, no basename subprocess)
dir_name="${toplevel##*/}"

# --- Branch color: main/master = yellow, others = green ---
branch_color="$GREEN"
case "$branch" in
  main|master) branch_color="$YELLOW" ;;
esac

# --- Context progress bar (10 blocks) ---
ctx_section=""
if [ -n "$used" ]; then
  pct=$(printf "%.0f" "$used" 2>/dev/null || echo "$used")

  # Dynamic color based on usage
  if [ "$pct" -ge 85 ] 2>/dev/null; then
    bar_color="$RED"
  elif [ "$pct" -ge 60 ] 2>/dev/null; then
    bar_color="$YELLOW"
  else
    bar_color="$GREEN"
  fi

  # Build bar: 10 segments (POSIX-compatible, no ternary)
  filled=$((pct / 10))
  partial=$((pct % 10))
  has_partial=0
  if [ "$partial" -gt 0 ] && [ "$filled" -lt 10 ]; then has_partial=1; fi
  empty=$((10 - filled - has_partial))

  bar=""
  i=0; while [ $i -lt "$filled" ]; do bar="${bar}█"; i=$((i + 1)); done
  if [ "$has_partial" -eq 1 ]; then
    if [ "$partial" -ge 5 ]; then bar="${bar}▓"; else bar="${bar}░"; fi
  fi
  i=0; while [ $i -lt "$empty" ]; do bar="${bar}░"; i=$((i + 1)); done

  ctx_section="${bar_color}${bar}${RESET} ${DIM}${pct}%${RESET}"
fi

# --- Assemble ---
line=""

# Project directory name (clickable → opens VS Code)
if [ -n "$toplevel" ]; then
  line="${CYAN}\033]8;;vscode://file${toplevel}\007 ${dir_name}\033]8;;\007${RESET}"
fi

# [repo:branch] (repo clickable → opens GitHub)
if [ -n "$toplevel" ]; then
  if [ -n "$repo_url" ]; then
    repo_part="\033]8;;${repo_url}\007${dir_name}\033]8;;\007"
  else
    repo_part="${dir_name}"
  fi

  if [ -n "$branch" ]; then
    line="${line}  ${DIM}[${RESET}${repo_part}${DIM}:${RESET}${branch_color}${branch}${RESET}${DIM}]${RESET}"
  else
    line="${line}  ${DIM}[${RESET}${repo_part}${DIM}]${RESET}"
  fi
fi

# Model
if [ -n "$model" ]; then
  line="${line}  ${MAGENTA} ${model}${RESET}"
fi

# Context bar
if [ -n "$ctx_section" ]; then
  line="${line}  ${ctx_section}"
fi

printf '%b' "$line"
