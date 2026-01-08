#!/bin/bash
# Find the default branch from remote (origin)
# Returns: branch name (e.g., main, master)

REMOTE="${1:-origin}"

# Try to get default branch from remote HEAD
DEFAULT_BRANCH=$(git remote show "$REMOTE" 2>/dev/null | grep "HEAD branch" | awk '{print $NF}')

if [ -n "$DEFAULT_BRANCH" ]; then
    echo "$DEFAULT_BRANCH"
    exit 0
fi

# Fallback: check common branch names in remote
for branch in main master develop; do
    if git ls-remote --exit-code --heads "$REMOTE" "$branch" &>/dev/null; then
        echo "$branch"
        exit 0
    fi
done

# Last fallback: check local branches
for branch in main master develop; do
    if git show-ref --verify --quiet "refs/heads/$branch"; then
        echo "$branch"
        exit 0
    fi
done

echo "main"
exit 0
