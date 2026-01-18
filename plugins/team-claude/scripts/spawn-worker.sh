#!/bin/bash

# spawn-worker.sh
# Creates a new worktree and spawns a Worker Claude instance
#
# Usage: spawn-worker.sh <feature-name> [base-branch] [task-spec-file]
#
# Arguments:
#   feature-name:   Name of the feature (e.g., "auth", "payment")
#   base-branch:    Base branch to create from (default: main)
#   task-spec-file: Path to task specification file (optional)

set -e

# Configuration
PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:-$(dirname "$(dirname "$0")")}"
PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-$(pwd)}"
WORKTREES_DIR="${PROJECT_ROOT}/../worktrees"
SERVER_URL="${TEAM_CLAUDE_SERVER_URL:-http://localhost:3847}"

# Parse arguments
FEATURE_NAME="$1"
BASE_BRANCH="${2:-main}"
TASK_SPEC_FILE="$3"

if [[ -z "$FEATURE_NAME" ]]; then
    echo "Usage: spawn-worker.sh <feature-name> [base-branch] [task-spec-file]"
    exit 1
fi

# Normalize feature name
FEATURE_NAME="${FEATURE_NAME//[^a-zA-Z0-9-]/-}"
WORKTREE_NAME="feature-${FEATURE_NAME}"
WORKTREE_PATH="${WORKTREES_DIR}/${WORKTREE_NAME}"
BRANCH_NAME="feature/${FEATURE_NAME}"

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║            Spawning Worker Claude                            ║"
echo "╠══════════════════════════════════════════════════════════════╣"
echo "║  Feature: ${FEATURE_NAME:0:52}$(printf '%*s' $((52 - ${#FEATURE_NAME})) '')║"
echo "║  Branch:  ${BRANCH_NAME:0:52}$(printf '%*s' $((52 - ${#BRANCH_NAME})) '')║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Create worktrees directory if needed
mkdir -p "$WORKTREES_DIR"

# Check if worktree already exists
if [[ -d "$WORKTREE_PATH" ]]; then
    echo "[spawn] Worktree already exists: $WORKTREE_PATH"
    echo "[spawn] Use 'git worktree remove' to clean up first if needed"
    exit 1
fi

# Fetch latest from remote
echo "[spawn] Fetching latest from origin/${BASE_BRANCH}..."
git fetch origin "$BASE_BRANCH" 2>/dev/null || {
    echo "[spawn] Warning: Could not fetch from origin, using local branch"
}

# Create worktree with new branch
echo "[spawn] Creating worktree at: $WORKTREE_PATH"
git worktree add "$WORKTREE_PATH" -b "$BRANCH_NAME" "origin/${BASE_BRANCH}" 2>/dev/null || \
git worktree add "$WORKTREE_PATH" -b "$BRANCH_NAME" "$BASE_BRANCH"

# Setup .claude directory in worktree
CLAUDE_DIR="${WORKTREE_PATH}/.claude"
mkdir -p "$CLAUDE_DIR"

# Copy worker CLAUDE.md template
echo "[spawn] Setting up worker configuration..."
if [[ -f "${PLUGIN_ROOT}/templates/worker-claude.md" ]]; then
    cp "${PLUGIN_ROOT}/templates/worker-claude.md" "${CLAUDE_DIR}/CLAUDE.md"
else
    # Create default CLAUDE.md
    cat > "${CLAUDE_DIR}/CLAUDE.md" << 'CLAUDE_MD'
# Worker Claude Configuration

You are a Worker Claude instance working on a specific feature in an isolated git worktree.

## Your Role
- Focus on implementing the assigned task
- Follow the specifications provided
- Report blockers if you encounter issues you cannot resolve
- Commit your work frequently with clear messages

## Important Files
- `.claude/task-spec.md` - Your task specification (if provided)
- `.claude/feedback.md` - Feedback from Main Claude (check before starting)
- `.claude/blockers.md` - Write here if you're blocked on something

## Guidelines
1. Read the task specification carefully before starting
2. Check for any feedback from Main Claude
3. Implement the feature as specified
4. Write tests for your implementation
5. Commit changes with descriptive messages
6. Keep your changes focused on the assigned task

## Communication
- Your work will be reviewed by Main Claude when you finish
- If you need clarification, create `.claude/blockers.md` with your questions
- Your Stop hook will automatically report completion to the coordination server
CLAUDE_MD
fi

# Append task specification if provided
if [[ -n "$TASK_SPEC_FILE" ]] && [[ -f "$TASK_SPEC_FILE" ]]; then
    echo "[spawn] Adding task specification..."
    cp "$TASK_SPEC_FILE" "${CLAUDE_DIR}/task-spec.md"

    # Also append to CLAUDE.md
    echo "" >> "${CLAUDE_DIR}/CLAUDE.md"
    echo "---" >> "${CLAUDE_DIR}/CLAUDE.md"
    echo "" >> "${CLAUDE_DIR}/CLAUDE.md"
    echo "## Task Specification" >> "${CLAUDE_DIR}/CLAUDE.md"
    echo "" >> "${CLAUDE_DIR}/CLAUDE.md"
    cat "$TASK_SPEC_FILE" >> "${CLAUDE_DIR}/CLAUDE.md"
fi

# Copy hooks configuration
echo "[spawn] Setting up hooks..."
if [[ -f "${PLUGIN_ROOT}/hooks/hooks.json" ]]; then
    cp "${PLUGIN_ROOT}/hooks/hooks.json" "${CLAUDE_DIR}/settings.local.json"
fi

# Register worker with coordination server
echo "[spawn] Registering worker with coordination server..."
REGISTER_PAYLOAD=$(cat <<EOF
{
    "worktree": "$WORKTREE_NAME",
    "feature": "$FEATURE_NAME",
    "branch": "$BRANCH_NAME"
}
EOF
)

curl -s -X POST "${SERVER_URL}/workers" \
    -H "Content-Type: application/json" \
    -d "$REGISTER_PAYLOAD" \
    --max-time 5 2>/dev/null || {
    echo "[spawn] Warning: Could not register with coordination server"
    echo "[spawn] Server may not be running. Worker will self-register on completion."
}

echo ""
echo "[spawn] Worker worktree created successfully!"
echo ""
echo "To start the Worker Claude, run:"
echo ""
echo "  cd $WORKTREE_PATH && claude"
echo ""
echo "Or open a new terminal and run Claude Code in that directory."
echo ""

# Try to open a new terminal (platform-specific)
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    echo "[spawn] Opening new terminal window..."
    osascript -e "tell app \"Terminal\" to do script \"cd '$WORKTREE_PATH' && claude\"" 2>/dev/null || {
        echo "[spawn] Could not open Terminal automatically"
    }
elif command -v gnome-terminal &>/dev/null; then
    # Linux with GNOME
    gnome-terminal -- bash -c "cd '$WORKTREE_PATH' && claude; bash" 2>/dev/null || {
        echo "[spawn] Could not open gnome-terminal automatically"
    }
elif command -v xterm &>/dev/null; then
    # Linux with xterm
    xterm -e "cd '$WORKTREE_PATH' && claude" 2>/dev/null &
fi

exit 0
