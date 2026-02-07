#!/bin/bash
# Smart commit with automatic Jira ticket detection

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Usage message
usage() {
  cat <<EOF
Usage: $0 <type> <description> [scope] [body]

Smart commit with automatic Jira ticket detection.
- For Jira branches (e.g., WAD-0212): Uses format [TICKET] type: description
- For regular branches: Uses format type(scope): description

Arguments:
  type         Commit type (feat, fix, docs, style, refactor, test, chore, perf)
  description  Short description (imperative mood)
  scope        Optional scope for regular branches
  body         Optional detailed description (multiple lines supported)

Examples:
  # On Jira branch (WAD-0212):
  $0 feat "implement user authentication"
  # Result: [WAD-0212] feat: implement user authentication

  # On feature branch (feature/user-auth):
  $0 feat "implement user authentication" "auth"
  # Result: feat(auth): implement user authentication

  # With detailed body:
  $0 feat "implement authentication" "auth" "- Add JWT tokens\n- Add bcrypt hashing"

Commit types:
  feat      New feature
  fix       Bug fix
  docs      Documentation changes
  style     Code formatting (no logic change)
  refactor  Code refactoring
  test      Adding/updating tests
  chore     Maintenance tasks
  perf      Performance improvements
EOF
  exit 1
}

# Check arguments
if [ $# -lt 2 ]; then
  usage
fi

TYPE="$1"
DESCRIPTION="$2"
SCOPE="${3:-}"
BODY="${4:-}"

# Validate type
VALID_TYPES="feat fix docs style refactor test chore perf"
if ! echo "$VALID_TYPES" | grep -w "$TYPE" > /dev/null; then
  echo "Error: Invalid type '$TYPE'" >&2
  echo "Valid types: $VALID_TYPES" >&2
  exit 1
fi

# Detect Jira ticket
if JIRA_TICKET=$("$SCRIPT_DIR/detect-jira-ticket.sh" 2>/dev/null); then
  # Jira branch format: [TICKET] type: description
  echo "Detected Jira ticket: $JIRA_TICKET"
  COMMIT_SUBJECT="[$JIRA_TICKET] $TYPE: $DESCRIPTION"
else
  # Regular branch format: type(scope): description
  if [ -n "$SCOPE" ]; then
    COMMIT_SUBJECT="$TYPE($SCOPE): $DESCRIPTION"
  else
    COMMIT_SUBJECT="$TYPE: $DESCRIPTION"
  fi
fi

# Build commit message
COMMIT_MESSAGE="$COMMIT_SUBJECT"

if [ -n "$BODY" ]; then
  COMMIT_MESSAGE="$COMMIT_MESSAGE

$BODY"
fi

# Add Claude Code signature
COMMIT_MESSAGE="$COMMIT_MESSAGE

🤖 Generated with [Claude Code](https://claude.com/claude-code)
Co-Authored-By: Claude <noreply@anthropic.com>"

# Stage all changes
git add -u

# Create commit
git commit -m "$COMMIT_MESSAGE"

echo "✓ Committed with message:"
echo "  $COMMIT_SUBJECT"
