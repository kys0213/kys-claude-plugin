#!/bin/bash
# Create a Pull Request with quality checks

set -e

# GitHub 환경변수 로드 (GH_HOST 등)
[ -f ~/.git-workflow-env ] && source ~/.git-workflow-env

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Usage message
usage() {
  cat <<EOF
Usage: $0 <title> [description]

Create a Pull Request with automatic Jira ticket detection.

Arguments:
  title        PR title (auto-formatted with Jira ticket if detected)
  description  Optional PR description

Examples:
  # On Jira branch (WAD-0212):
  $0 "Implement user authentication"
  # PR title: [WAD-0212] Implement user authentication

  # On feature branch:
  $0 "Add user authentication" "This PR adds JWT-based authentication"

Notes:
  - The script will automatically detect the default branch as base
  - The PR will be created via 'gh pr create'
EOF
  exit 1
}

# Check arguments
if [ $# -lt 1 ]; then
  usage
fi

TITLE="$1"
DESCRIPTION="${2:-}"

# Detect default branch
DEFAULT_BRANCH=$("$SCRIPT_DIR/detect-default-branch.sh")
echo "Default branch: $DEFAULT_BRANCH"

# Get current branch
CURRENT_BRANCH=$(git branch --show-current)
echo "Current branch: $CURRENT_BRANCH"

# Check if we're on default branch
if [ "$CURRENT_BRANCH" = "$DEFAULT_BRANCH" ]; then
  echo "Error: Cannot create PR from default branch ($DEFAULT_BRANCH)" >&2
  echo "Please checkout a feature branch first." >&2
  exit 1
fi

# Check GitHub CLI authentication
echo ""
echo "Checking GitHub CLI authentication..."
GH_AUTH_ARGS=""
if [ -n "${GH_HOST:-}" ]; then
  GH_AUTH_ARGS="--hostname $GH_HOST"
fi
if ! gh auth status $GH_AUTH_ARGS > /dev/null 2>&1; then
  echo ""
  echo "❌ GitHub CLI is not authenticated." >&2
  echo "" >&2
  echo "Please run one of the following commands to authenticate:" >&2
  echo "" >&2
  echo "  1. Login with web browser:" >&2
  echo "     gh auth login" >&2
  echo "" >&2
  echo "  2. Login with token:" >&2
  echo "     gh auth login --with-token < your-token.txt" >&2
  echo "" >&2
  echo "For more information, visit: https://cli.github.com/manual/gh_auth_login" >&2
  exit 1
fi
echo "✓ GitHub CLI authenticated"

# Format title with Jira ticket if detected
if JIRA_TICKET=$("$SCRIPT_DIR/detect-jira-ticket.sh" 2>/dev/null); then
  echo "Detected Jira ticket: $JIRA_TICKET"
  PR_TITLE="[$JIRA_TICKET] $TITLE"
else
  PR_TITLE="$TITLE"
fi

# Push to remote
echo ""
echo "Pushing to remote..."
git push origin "$CURRENT_BRANCH" 2>/dev/null || git push -u origin "$CURRENT_BRANCH"

# Create PR
echo ""
echo "Creating Pull Request..."
echo "Title: $PR_TITLE"
echo "Base: $DEFAULT_BRANCH"
echo ""

if [ -n "$DESCRIPTION" ]; then
  PR_URL=$(gh pr create --base "$DEFAULT_BRANCH" --title "$PR_TITLE" --body "$DESCRIPTION")
else
  PR_URL=$(gh pr create --base "$DEFAULT_BRANCH" --title "$PR_TITLE" --body "")
fi

echo ""
echo "=========================================="
echo "✓ Pull Request created successfully!"
echo "=========================================="
echo ""
echo "📋 PR URL:"
echo "   $PR_URL"
echo ""
