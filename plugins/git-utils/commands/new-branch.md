---
name: new-branch
description: Create a new git branch from a base branch with latest changes
argument-hint: "[baseBranch]"
allowed-tools:
  - Bash
  - AskUserQuestion
  - Read
---

# New Branch Command

Create a new git branch based on the specified base branch (or default branch if not specified), ensuring the base branch is up-to-date before branching.

## Execution Steps

1. **Determine base branch**:
   - If `$ARGUMENTS` contains a branch name, use it as the base branch
   - Otherwise, run the script to find the default branch:
     ```bash
     bash ${CLAUDE_PLUGIN_ROOT}/scripts/find-default-branch.sh
     ```

2. **Update base branch**:
   - Fetch and checkout the base branch
   - Pull latest changes to ensure it's up-to-date:
     ```bash
     git fetch origin
     git checkout <baseBranch>
     git pull origin <baseBranch>
     ```

3. **Suggest branch name**:
   - Based on the current conversation context, suggest an appropriate branch name
   - Use naming conventions like:
     - `feature/<description>` for new features
     - `fix/<description>` for bug fixes
     - `refactor/<description>` for refactoring
     - `docs/<description>` for documentation
   - Use `AskUserQuestion` tool to propose the branch name and let the user confirm or modify:
     - Provide 2-3 suggested names based on context
     - Allow user to input custom name

4. **Create and checkout new branch**:
   - Create the new branch from the updated base branch:
     ```bash
     git checkout -b <newBranchName>
     ```

5. **Confirm completion**:
   - Report the created branch name and base branch
   - Do NOT push to remote

## Example Usage

- `/new-branch` - Create branch from default branch (main/master)
- `/new-branch develop` - Create branch from develop branch

## Notes

- Always ensure working directory is clean before switching branches
- If there are uncommitted changes, warn the user before proceeding
