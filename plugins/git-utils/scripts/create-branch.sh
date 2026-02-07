#!/bin/bash
# Create a new branch from the default branch or specified base branch

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Usage message
usage() {
  cat <<EOF
Usage: $0 <branch-name> [base-branch]

Create a new branch from the default branch (main/master) or specified base branch.

Arguments:
  branch-name   Name of the new branch to create
  base-branch   (Optional) Base branch to create from. Defaults to main/master.

Examples:
  $0 feature/user-auth               # Create from default branch (main/master)
  $0 feature/user-auth develop       # Create from develop branch
  $0 WAD-0212                        # Create Jira ticket branch from default
  $0 fix/hotfix release/1.0          # Create fix branch from release/1.0

Branch naming conventions:
  - Jira tickets: WAD-0212, PROJ-123
  - Features: feature/descriptive-name
  - Fixes: fix/descriptive-name
  - Docs: docs/descriptive-name
  - Refactor: refactor/descriptive-name
  - Performance: perf/descriptive-name
  - Tests: test/descriptive-name
EOF
  exit 1
}

# Check arguments
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  usage
fi

BRANCH_NAME="$1"
BASE_BRANCH="${2:-}"

# Detect base branch (use default if not specified)
if [ -z "$BASE_BRANCH" ]; then
  BASE_BRANCH=$("$SCRIPT_DIR/detect-default-branch.sh")
fi

# Check for uncommitted changes
CHANGES=$(git status --porcelain)
if [ -n "$CHANGES" ]; then
  echo "Error: Uncommitted changes detected."
  echo "Please commit or stash your changes before creating a new branch."
  echo ""
  echo "Changed files:"
  git status --short
  exit 1
fi

echo "Base branch: $BASE_BRANCH"
echo "Creating branch: $BRANCH_NAME"

# Fetch and verify base branch exists
git fetch origin --prune 2>/dev/null || true

# Check if base branch exists locally or remotely
LOCAL_EXISTS=$(git show-ref --verify --quiet "refs/heads/$BASE_BRANCH" && echo "yes" || echo "no")
REMOTE_EXISTS=$(git show-ref --verify --quiet "refs/remotes/origin/$BASE_BRANCH" && echo "yes" || echo "no")

if [ "$LOCAL_EXISTS" = "no" ] && [ "$REMOTE_EXISTS" = "no" ]; then
  echo "Error: Base branch '$BASE_BRANCH' does not exist locally or remotely."
  exit 1
fi

# Checkout and update base branch
if [ "$LOCAL_EXISTS" = "yes" ]; then
  git checkout "$BASE_BRANCH"
  git pull origin "$BASE_BRANCH" 2>/dev/null || true
else
  # Create local tracking branch from remote
  git checkout -b "$BASE_BRANCH" --track "origin/$BASE_BRANCH"
fi

# Create new branch
git checkout -b "$BRANCH_NAME"

echo "✓ Branch '$BRANCH_NAME' created successfully from '$BASE_BRANCH'"
