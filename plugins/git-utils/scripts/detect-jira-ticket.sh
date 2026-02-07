#!/bin/bash
# Detect Jira ticket from current branch name
# Returns the ticket number in uppercase (e.g., WAD-0212), otherwise exits with code 1
#
# Supported patterns:
#   - WAD-0212                (direct ticket)
#   - feat/WAD-0212           (prefix with uppercase ticket)
#   - feat/wad-0212           (prefix with lowercase ticket)
#   - fix/wad-2223            (any type with lowercase)

set -e

BRANCH_NAME=$(git branch --show-current)

# Pattern 1: prefix/TICKET or prefix/ticket (e.g., feat/WAD-0212, feat/wad-0212)
if [[ $BRANCH_NAME =~ ^[a-z]+/([A-Za-z]+-[0-9]+)$ ]]; then
  JIRA_TICKET="${BASH_REMATCH[1]}"
  # Convert to uppercase
  JIRA_TICKET=$(echo "$JIRA_TICKET" | tr '[:lower:]' '[:upper:]')
  echo "$JIRA_TICKET"
  exit 0
# Pattern 2: Direct TICKET (e.g., WAD-0212)
elif [[ $BRANCH_NAME =~ ^([A-Z]+-[0-9]+)$ ]]; then
  JIRA_TICKET="${BASH_REMATCH[1]}"
  echo "$JIRA_TICKET"
  exit 0
# Pattern 3: Direct lowercase ticket (e.g., wad-0212) - less common but supported
elif [[ $BRANCH_NAME =~ ^([a-z]+-[0-9]+)$ ]]; then
  JIRA_TICKET="${BASH_REMATCH[1]}"
  # Convert to uppercase
  JIRA_TICKET=$(echo "$JIRA_TICKET" | tr '[:lower:]' '[:upper:]')
  echo "$JIRA_TICKET"
  exit 0
else
  exit 1
fi
