#!/bin/bash
# Smart commit with automatic Jira ticket detection

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Usage message
usage() {
  cat <<EOF
Usage: $0 <type> <description> [scope] [body] [--skip-add]

Smart commit with automatic Jira ticket detection.
- For Jira branches (e.g., WAD-0212): Uses format [TICKET] type: description
- For regular branches: Uses format type(scope): description

Arguments:
  type         Commit type (feat, fix, docs, style, refactor, test, chore, perf)
  description  Short description (imperative mood)
  scope        Optional scope for regular branches
  body         Optional detailed description (multiple lines supported)
  --skip-add   Skip automatic 'git add -u' (use when files are already staged)

Examples:
  # On Jira branch (WAD-0212):
  $0 feat "implement user authentication"
  # Result: [WAD-0212] feat: implement user authentication

  # On feature branch (feature/user-auth):
  $0 feat "implement user authentication" "auth"
  # Result: feat(auth): implement user authentication

  # With detailed body:
  $0 feat "implement authentication" "auth" "- Add JWT tokens\n- Add bcrypt hashing"

  # With pre-staged files:
  $0 feat "implement authentication" "auth" "" --skip-add

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

SKIP_ADD=false
TYPE=""
DESCRIPTION=""
SCOPE=""
BODY=""

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --skip-add)
      SKIP_ADD=true
      shift
      ;;
    *)
      if [ -z "$TYPE" ]; then
        TYPE="$1"
      elif [ -z "$DESCRIPTION" ]; then
        DESCRIPTION="$1"
      elif [ -z "$SCOPE" ]; then
        SCOPE="$1"
      elif [ -z "$BODY" ]; then
        BODY="$1"
      fi
      shift
      ;;
  esac
done

# Validate required arguments
if [ -z "$TYPE" ] || [ -z "$DESCRIPTION" ]; then
  usage
fi

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

# Stage all changes (unless --skip-add flag is passed)
if [ "$SKIP_ADD" = "false" ]; then
  git add -u
fi

# Create commit
git commit -m "$COMMIT_MESSAGE"

echo "✓ Committed with message:"
echo "  $COMMIT_SUBJECT"
