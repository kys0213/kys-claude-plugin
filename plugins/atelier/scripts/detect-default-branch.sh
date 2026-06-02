#!/bin/bash
# Detect the default branch of the current repository

set -e

# Method 1: Get the default branch from remote HEAD (cached)
DEFAULT_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@')

# Method 2: Fetch from remote and set origin/HEAD automatically
# This ensures we get the actual default branch configured on the remote
if [ -z "$DEFAULT_BRANCH" ]; then
  git remote set-head origin --auto 2>/dev/null || true
  DEFAULT_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@')
fi

# Method 3: Last resort fallback - check for common default branch names
# Only used if remote doesn't provide default branch info
if [ -z "$DEFAULT_BRANCH" ]; then
  if git show-ref --verify --quiet refs/remotes/origin/main 2>/dev/null; then
    DEFAULT_BRANCH="main"
  elif git show-ref --verify --quiet refs/remotes/origin/develop 2>/dev/null; then
    DEFAULT_BRANCH="develop"
  elif git show-ref --verify --quiet refs/remotes/origin/master 2>/dev/null; then
    DEFAULT_BRANCH="master"
  fi
fi

if [ -z "$DEFAULT_BRANCH" ]; then
  echo "Error: Could not detect default branch. Make sure you have a remote configured." >&2
  exit 1
fi

echo "$DEFAULT_BRANCH"
