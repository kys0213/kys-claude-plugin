---
name: git-sync
description: Switch to default branch and sync with latest remote changes
allowed-tools:
  - Bash
  - AskUserQuestion
  - Read
---

# Sync Command

Switch to the default branch (main/master) and pull the latest changes from remote.

## Execution Steps

1. **Check for uncommitted changes**:
   - Run `git status --porcelain` to check for uncommitted changes
   - If there are changes, use `AskUserQuestion` to ask the user:
     - **Stash changes**: Stash current changes, sync, then restore
     - **Abort**: Cancel the sync operation
     - **Continue anyway**: Proceed without handling changes (may fail if conflicts)

2. **Find default branch**:
   - Run the script to determine the default branch:
     ```bash
     bash ${CLAUDE_PLUGIN_ROOT}/scripts/find-default-branch.sh
     ```

3. **Switch to default branch**:
   ```bash
   git checkout <defaultBranch>
   ```

4. **Pull latest changes**:
   ```bash
   git pull origin <defaultBranch>
   ```

5. **Restore stashed changes** (if stashed in step 1):
   ```bash
   git stash pop
   ```

6. **Report completion**:
   - Show current branch and sync status
   - If stash was applied, mention it

## Example Usage

- `/git-sync` - Switch to default branch and pull latest

## Notes

- This command is useful before creating new branches to ensure you're starting from the latest code
- Works well in combination with `/new-git-branch` command
