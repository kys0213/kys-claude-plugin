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

# Pattern 1: prefix/TICKET-123 or prefix/TICKET-123/description
if [[ $BRANCH_NAME =~ ^[a-z]+[-/]([A-Za-z]+-[0-9]+) ]]; then
  TICKET="${BASH_REMATCH[1]}"
  echo "${TICKET^^}"
  exit 0
# Pattern 2: TICKET-123 anywhere in branch name
elif [[ $BRANCH_NAME =~ ([A-Z]+-[0-9]+) ]]; then
  TICKET="${BASH_REMATCH[1]}"
  echo "$TICKET"
  exit 0
# Pattern 3: lowercase ticket anywhere
elif [[ $BRANCH_NAME =~ ([a-z]+-[0-9]+) ]]; then
  TICKET="${BASH_REMATCH[1]}"
  echo "${TICKET^^}"
  exit 0
else
  exit 1
fi
