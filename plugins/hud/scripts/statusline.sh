#!/bin/sh
input=$(cat)

# --- Parse input ---
cwd=$(echo "$input" | jq -r '.workspace.current_dir // .cwd // ""')
model=$(echo "$input" | jq -r '.model.display_name // ""')
used=$(echo "$input" | jq -r '.context_window.used_percentage // empty')

# --- Colors ---
RESET='\033[0m'
CYAN='\033[36m'
GREEN='\033[32m'
YELLOW='\033[33m'
RED='\033[31m'
MAGENTA='\033[35m'
DIM='\033[2m'

# --- Git info ---
repo=""
repo_url=""
branch=""
toplevel=""
if git -C "$cwd" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  toplevel=$(git -C "$cwd" -c core.hooksPath=/dev/null rev-parse --show-toplevel 2>/dev/null)
  repo=$(basename "$toplevel" 2>/dev/null)
  branch=$(git -C "$cwd" -c core.hooksPath=/dev/null symbolic-ref --short HEAD 2>/dev/null \
           || git -C "$cwd" -c core.hooksPath=/dev/null rev-parse --short HEAD 2>/dev/null)

  # GitHub remote URL
  remote=$(git -C "$cwd" -c core.hooksPath=/dev/null remote get-url origin 2>/dev/null)
  case "$remote" in
    https://github.com/*)
      repo_url=$(echo "$remote" | sed 's/\.git$//')
      ;;
    git@github.com:*)
      repo_url="https://github.com/$(echo "$remote" | sed 's|git@github.com:||;s|\.git$||')"
      ;;
  esac
fi

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

  # Build bar: 10 segments
  filled=$((pct / 10))
  partial=$((pct % 10))
  empty=$((10 - filled - (partial > 0 ? 1 : 0)))

  bar=""
  i=0; while [ $i -lt "$filled" ]; do bar="${bar}█"; i=$((i + 1)); done
  if [ "$partial" -gt 0 ] && [ "$filled" -lt 10 ]; then
    if [ "$partial" -ge 5 ]; then bar="${bar}▓"; else bar="${bar}░"; fi
  fi
  i=0; while [ $i -lt "$empty" ]; do bar="${bar}░"; i=$((i + 1)); done

  ctx_section="${bar_color}${bar}${RESET} ${DIM}${pct}%${RESET}"
fi

# --- Assemble ---
line=""

# Project directory name (clickable → opens VS Code)
if [ -n "$toplevel" ]; then
  dir_name=$(basename "$toplevel")
  line="${CYAN}\033]8;;vscode://file${toplevel}\007 ${dir_name}\033]8;;\007${RESET}"
fi

# Repo:Branch  [repo:branch]
if [ -n "$repo" ]; then
  # Repo part (clickable → opens GitHub)
  if [ -n "$repo_url" ]; then
    repo_part="\033]8;;${repo_url}\007${repo}\033]8;;\007"
  else
    repo_part="${repo}"
  fi

  # Combine [repo:branch]
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
