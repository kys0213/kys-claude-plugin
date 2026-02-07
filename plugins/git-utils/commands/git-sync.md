---
name: git-sync
description: 지정한 브랜치(또는 기본 브랜치)로 전환 후 최신 상태로 동기화
argument-hint: "[branch] [--force]"
allowed-tools:
  - Bash
  - AskUserQuestion
---

# Git Sync

지정한 브랜치(또는 기본 브랜치)로 전환하고 원격 저장소의 최신 상태로 동기화합니다.

## Context

- Current branch: !`git branch --show-current`
- Has uncommitted changes: !`git status --short`

## Usage

- `/git-sync` - 기본 브랜치로 동기화
- `/git-sync develop` - develop 브랜치로 동기화
- `/git-sync --force` - stash 후 기본 브랜치로 동기화
- `/git-sync develop --force` - stash 후 develop으로 동기화
- `/git-sync --force develop` - 위와 동일 (순서 무관)

## Execution

### Step 1: 인자 파싱 및 현재 상태 확인

```bash
# 인자 파싱 (--force와 브랜치 이름 순서 무관)
FORCE=false
TARGET_BRANCH=""

for arg in "$@"; do
  case "$arg" in
    --force)
      FORCE=true
      ;;
    *)
      TARGET_BRANCH="$arg"
      ;;
  esac
done

# 현재 상태 확인
CURRENT_BRANCH=$(git branch --show-current)
CHANGES=$(git status --porcelain)

# 브랜치 미지정 시 기본 브랜치 사용
if [ -z "$TARGET_BRANCH" ]; then
  TARGET_BRANCH=$(${CLAUDE_PLUGIN_ROOT}/scripts/detect-default-branch.sh)
fi
```

### Step 2: 변경사항 처리

```bash
# --force 플래그 없이 변경사항이 있으면 중단
if [ -n "$CHANGES" ] && [ "$FORCE" != "true" ]; then
  echo "⚠️ Uncommitted changes detected. Use /git-sync --force to stash and continue."
  exit 1
fi

# --force 플래그로 stash
if [ -n "$CHANGES" ] && [ "$FORCE" = "true" ]; then
  git stash push -m "auto-stash before git-sync"
  STASHED=true
fi
```

### Step 3: 브랜치 존재 여부 확인

```bash
# 로컬 브랜치 존재 여부 확인
LOCAL_EXISTS=$(git show-ref --verify --quiet "refs/heads/$TARGET_BRANCH" && echo "yes" || echo "no")

# 원격 브랜치 존재 여부 확인
git fetch origin --prune 2>/dev/null
REMOTE_EXISTS=$(git show-ref --verify --quiet "refs/remotes/origin/$TARGET_BRANCH" && echo "yes" || echo "no")
```

**브랜치 상태에 따른 처리:**

| 로컬 | 원격 | 처리 |
|------|------|------|
| 있음 | - | `git checkout $TARGET_BRANCH` |
| 없음 | 있음 | `git checkout -b $TARGET_BRANCH --track origin/$TARGET_BRANCH` |
| 없음 | 없음 | AskUserQuestion으로 사용자 확인 (Step 3-1) |

### Step 3-1: 브랜치 미존재 시 사용자 확인

브랜치가 로컬과 원격 모두에 존재하지 않으면 **AskUserQuestion**을 사용하여 사용자에게 확인합니다:

```
question: "Branch '{TARGET_BRANCH}'이(가) 로컬과 원격 모두에 존재하지 않습니다. 어떻게 처리할까요?"
header: "Branch"
options:
  - label: "Create new branch"
    description: "현재 HEAD에서 '{TARGET_BRANCH}' 브랜치 새로 생성"
  - label: "Cancel"
    description: "작업 중단"
multiSelect: false
```

- **Create new branch 선택 시**: `git checkout -b $TARGET_BRANCH`
- **Cancel 선택 시**: 작업 중단

### Step 4: 브랜치 이동 및 pull

```bash
# 로컬에 존재하는 경우
if [ "$LOCAL_EXISTS" = "yes" ]; then
  git checkout "$TARGET_BRANCH"
  git pull origin "$TARGET_BRANCH"

# 원격에만 존재하는 경우
elif [ "$REMOTE_EXISTS" = "yes" ]; then
  git checkout -b "$TARGET_BRANCH" --track "origin/$TARGET_BRANCH"
  CREATED_TRACKING=true

# 새로 생성하는 경우 (AskUserQuestion에서 Create 선택)
else
  git checkout -b "$TARGET_BRANCH"
  CREATED_NEW=true
fi
```

### Step 5: 결과 보고

```bash
echo "✓ Synced to $TARGET_BRANCH"
echo "  Previous branch: $CURRENT_BRANCH"

if [ "$STASHED" = "true" ]; then
  echo "  ⚠️ Changes were stashed. Use 'git stash pop' to restore."
fi

if [ "$CREATED_TRACKING" = "true" ]; then
  echo "  ℹ️ Created local branch tracking origin/$TARGET_BRANCH"
fi

if [ "$CREATED_NEW" = "true" ]; then
  echo "  ℹ️ New branch created from $CURRENT_BRANCH"
fi
```

## Output Examples

### 기본 브랜치로 이동

```
✓ Synced to main
  Previous branch: feat/wad-0212
```

### 특정 브랜치로 이동

```
✓ Synced to develop
  Previous branch: main
```

### 원격 브랜치 tracking 설정 (로컬에 없던 브랜치)

```
✓ Synced to feat/shared
  Previous branch: main
  ℹ️ Created local branch tracking origin/feat/shared
```

### stash 사용 시 (--force)

```
✓ Synced to develop
  Previous branch: feat/wad-0212
  ⚠️ Changes were stashed. Use 'git stash pop' to restore.
```

### 새 브랜치 생성 시 (AskUserQuestion에서 Create 선택)

```
✓ Synced to feat/new-feature
  Previous branch: main
  ℹ️ New branch created from main
```

## Notes

- 브랜치 미지정 시 기본 브랜치(main/master)로 이동 (`detect-default-branch.sh`로 자동 감지)
- `--force`와 브랜치 이름은 순서 무관하게 사용 가능
- `--force`는 stash만 하고 변경사항을 버리지 않음
- stash된 변경사항은 `git stash list`로 확인 가능
- 원격에만 있는 브랜치 지정 시 자동으로 tracking 브랜치 생성
- 존재하지 않는 브랜치 지정 시 AskUserQuestion으로 새 브랜치 생성 여부 확인
