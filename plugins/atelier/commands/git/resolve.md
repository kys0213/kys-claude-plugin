---
description: Rebase conflict 발생 시 파일별로 충돌을 하나씩 리뷰하며 해결
argument-hint: "[--continue|--abort|--skip]"
allowed-tools:
  - Bash
  - Read
  - Edit
  - AskUserQuestion
---

# Git Resolve - Rebase Conflict Resolution

Rebase 중 발생한 충돌을 파일별로 하나씩 리뷰하며 분할정복 방식으로 해결합니다.

> 파일별 해결 전략·conflict marker 의미·실전 시나리오는 `git` skill 의 `references/conflict-resolution.md` 에 있습니다. 이 커맨드는 진입점(인자 처리 + rebase 상태 확인)만 담습니다.

## Context

- Rebase in progress: !`test -d "$(git rev-parse --git-dir)/rebase-merge" && echo "yes" || echo "no"`
- Current branch: !`git branch --show-current 2>/dev/null || cat "$(git rev-parse --git-dir)/rebase-merge/head-name" 2>/dev/null | sed 's|refs/heads/||'`
- Conflicted files: !`git diff --name-only --diff-filter=U 2>/dev/null`
- Current commit being rebased: !`cat "$(git rev-parse --git-dir)/rebase-merge/message" 2>/dev/null | head -1`

## Usage

- `/atelier:resolve` - 충돌 상태 확인 및 대화형 해결 시작
- `/atelier:resolve --continue` - 현재 충돌 해결 완료 후 rebase 계속
- `/atelier:resolve --abort` - rebase 전체 취소 (원래 상태로 복원)
- `/atelier:resolve --skip` - 현재 커밋 건너뛰고 다음 커밋으로

## Execution

### Step 0: 인자 처리

```bash
case "$1" in
  --continue)
    UNRESOLVED=$(git diff --name-only --diff-filter=U)
    if [ -n "$UNRESOLVED" ]; then
      echo "⚠️ 아직 해결되지 않은 충돌이 있습니다:"
      echo "$UNRESOLVED" | while read file; do echo "  - $file"; done
      echo ""
      echo "충돌을 먼저 해결한 후 다시 시도하세요."
      exit 1
    fi
    git rebase --continue
    exit $?
    ;;
  --abort)
    git rebase --abort
    echo "✓ Rebase aborted. Returned to original state."
    exit 0
    ;;
  --skip)
    git rebase --skip
    exit $?
    ;;
esac
```

### Step 1: Rebase 상태 확인

```bash
GIT_DIR=$(git rev-parse --git-dir)
REBASE_DIR="$GIT_DIR/rebase-merge"

if [ ! -d "$REBASE_DIR" ]; then
  echo "ℹ️ No rebase in progress."
  echo ""
  echo "To start a rebase:"
  echo "  git fetch origin"
  echo '  git rebase origin/main  # (or your default branch)'
  exit 0
fi
```

### Step 2: 충돌 파일 목록 확인

```bash
CONFLICTED_FILES=$(git diff --name-only --diff-filter=U)

if [ -z "$CONFLICTED_FILES" ]; then
  echo "✓ No conflicts remaining in current commit."
  echo ""
  echo "Run '/atelier:resolve --continue' to proceed with rebase."
  exit 0
fi

CURRENT=$(cat "$REBASE_DIR/msgnum" 2>/dev/null)
TOTAL=$(cat "$REBASE_DIR/end" 2>/dev/null)
COMMIT_MSG=$(cat "$REBASE_DIR/message" 2>/dev/null | head -1)

echo "📍 Rebase Progress: $CURRENT / $TOTAL"
echo "📝 Current commit: $COMMIT_MSG"
echo ""
echo "⚠️ Conflicted files:"
echo "$CONFLICTED_FILES" | while read file; do echo "  - $file"; done
```

### Step 3: 파일별 충돌 해결

`git` skill 의 `references/conflict-resolution.md` 의 **파일별 분할정복 절차**(파일 선택 → 충돌 분석 → 전략 선택(Ours/Theirs/Manual/Show diff) → 전략별 처리)를 수행합니다. 남은 충돌이 없을 때까지 반복.

### Step 4: Rebase 완료

모든 충돌 해결 후 `/atelier:resolve --continue` 안내. 모든 커밋이 끝나면:

```bash
echo "🎉 Rebase completed successfully!"
git log --oneline -5
```

## Quick Reference

| 명령어 | 설명 |
|--------|------|
| `/atelier:resolve` | 충돌 상태 확인 및 대화형 해결 |
| `/atelier:resolve --continue` | 현재 충돌 해결 완료, rebase 계속 |
| `/atelier:resolve --abort` | rebase 전체 취소 |
| `/atelier:resolve --skip` | 현재 커밋 건너뛰기 |

상세 해결 전략·conflict marker 의미·주의사항은 `git` skill 의 `references/conflict-resolution.md` 참조.
