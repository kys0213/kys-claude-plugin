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
    # 해결되지 않은 충돌이 있는지 확인
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

# 현재 rebase 진행 상황
CURRENT=$(cat "$REBASE_DIR/msgnum" 2>/dev/null)
TOTAL=$(cat "$REBASE_DIR/end" 2>/dev/null)
COMMIT_MSG=$(cat "$REBASE_DIR/message" 2>/dev/null | head -1)

echo "📍 Rebase Progress: $CURRENT / $TOTAL"
echo "📝 Current commit: $COMMIT_MSG"
echo ""
echo "⚠️ Conflicted files:"
echo "$CONFLICTED_FILES" | while read file; do
  echo "  - $file"
done
```

### Step 3: 파일별 충돌 해결 (분할정복)

충돌 파일 목록에서 첫 번째 파일부터 하나씩 처리합니다.

**3-1. 파일 선택**

```
question: "어떤 파일의 충돌을 먼저 해결할까요?"
header: "File"
options:
  - (충돌 파일 목록에서 동적 생성)
  - label: "모두 건너뛰기"
    description: "현재 커밋의 모든 충돌을 수동으로 해결 후 --continue"
multiSelect: false
```

**3-2. 선택된 파일의 충돌 분석**

Read 도구를 사용하여 충돌 파일 내용을 읽고, 충돌 마커(`<<<<<<<`, `=======`, `>>>>>>>`)가 있는 부분을 식별합니다.

```bash
# 충돌 영역 표시
git diff --color=always "$SELECTED_FILE"
```

**3-3. 해결 전략 선택**

```
question: "{SELECTED_FILE} 파일의 충돌을 어떻게 해결할까요?"
header: "Strategy"
options:
  - label: "Ours (Upstream/Base)"
    description: "Upstream(base) 브랜치의 변경사항 유지 (rebase 대상)"
  - label: "Theirs (내 커밋)"
    description: "현재 적용 중인 내 커밋의 변경사항으로 대체"
  - label: "Manual (수동 병합)"
    description: "양쪽 변경사항을 검토하고 수동으로 병합"
  - label: "Show diff"
    description: "충돌 내용을 자세히 보여주기"
multiSelect: false
```

**3-4. 전략별 처리**

**Ours 선택 시:**
```bash
git checkout --ours "$SELECTED_FILE"
git add "$SELECTED_FILE"
echo "✓ $SELECTED_FILE: Kept upstream (base) version"
```

**Theirs 선택 시:**
```bash
git checkout --theirs "$SELECTED_FILE"
git add "$SELECTED_FILE"
echo "✓ $SELECTED_FILE: Applied my commit's changes"
```

**Manual 선택 시:**
1. Read 도구로 파일 전체 내용 표시
2. 충돌 마커가 있는 섹션 설명
3. Edit 도구로 사용자와 함께 충돌 해결
4. 해결 완료 후:
   ```bash
   git add "$SELECTED_FILE"
   echo "✓ $SELECTED_FILE: Manually resolved"
   ```

**Show diff 선택 시:**
```bash
# 양쪽 버전 비교
echo "=== Ours: Upstream/Base version (rebase 대상) ==="
git show :2:"$SELECTED_FILE" 2>/dev/null || echo "(file doesn't exist in ours)"

echo ""
echo "=== Theirs: My commit version (적용 중인 내 커밋) ==="
git show :3:"$SELECTED_FILE" 2>/dev/null || echo "(file doesn't exist in theirs)"
```
그 후 다시 3-3 해결 전략 선택으로 돌아갑니다.

### Step 4: 다음 충돌 파일 처리

현재 파일 해결 후, 남은 충돌 파일이 있으면 Step 3으로 돌아갑니다.

```bash
REMAINING=$(git diff --name-only --diff-filter=U)

if [ -z "$REMAINING" ]; then
  echo ""
  echo "✅ All conflicts in current commit resolved!"
  echo ""
  echo "Next steps:"
  echo "  /atelier:resolve --continue  # Continue rebase"
  echo "  /atelier:resolve --abort     # Cancel entire rebase"
else
  echo ""
  echo "⚠️ Remaining conflicts:"
  echo "$REMAINING" | while read file; do
    echo "  - $file"
  done
fi
```

### Step 5: Rebase 완료

모든 커밋의 충돌이 해결되면:

```bash
echo "🎉 Rebase completed successfully!"
echo ""
git log --oneline -5
```

## Conflict Markers 이해하기

충돌 파일에서 볼 수 있는 마커:

```
<<<<<<< HEAD (ours)
Upstream/Base 브랜치의 코드 (예: origin/main)
=======
내 커밋의 코드 (현재 적용 중인 feature 브랜치 커밋)
>>>>>>> commit-hash (theirs)
```

**⚠️ 중요**: Rebase에서 "ours"와 "theirs"는 merge와 **반대**입니다!
- `--ours`: Upstream(base) 브랜치의 변경사항 (HEAD가 가리키는 rebase 대상)
- `--theirs`: 내 커밋의 변경사항 (현재 적용 중인 feature 브랜치 커밋)

**왜 반대인가?**: Rebase 중에는 HEAD가 upstream 브랜치를 가리키고, 내 커밋들이 하나씩 적용되기 때문입니다.

## 실전 시나리오

### 시나리오 1: Feature 브랜치를 main에 rebase

```bash
# 1. main 최신화
git fetch origin
git checkout main
git pull

# 2. feature 브랜치로 이동 후 rebase
git checkout feature/my-work
git rebase origin/main

# 3. 충돌 발생 시
/atelier:resolve
# → 파일별로 충돌 해결

# 4. 해결 완료 후
/atelier:resolve --continue
```

### 시나리오 2: 복잡한 충돌 - 양쪽 변경 모두 필요

**충돌 발생 상태:**
```typescript
<<<<<<< HEAD
import { ServiceA } from './serviceA';
import { ServiceB } from './serviceB';
=======
import { ServiceA } from './serviceA';
import { ServiceC } from './serviceC';
>>>>>>> feat/add-service-c
```

**해결: 양쪽 변경 모두 병합 (Manual 선택)**
```typescript
import { ServiceA } from './serviceA';
import { ServiceB } from './serviceB';
import { ServiceC } from './serviceC';
```

### 시나리오 3: 충돌이 너무 복잡할 때

```bash
# 현재 커밋 건너뛰기
/atelier:resolve --skip

# 또는 전체 rebase 취소
/atelier:resolve --abort
```

## 주의사항

1. **Rebase 전 항상 브랜치 백업 고려**
   ```bash
   git branch backup/my-work
   ```

2. **이미 push한 브랜치는 rebase 주의**
   - Rebase는 커밋 히스토리를 변경
   - 공유된 브랜치 rebase 후에는 `--force-with-lease` 필요

3. **충돌이 너무 많으면**
   - `--abort` 후 merge 고려
   - 또는 더 작은 단위로 나누어 rebase

## Quick Reference

| 명령어 | 설명 |
|--------|------|
| `/atelier:resolve` | 충돌 상태 확인 및 대화형 해결 |
| `/atelier:resolve --continue` | 현재 충돌 해결 완료, rebase 계속 |
| `/atelier:resolve --abort` | rebase 전체 취소 |
| `/atelier:resolve --skip` | 현재 커밋 건너뛰기 |

| Git 상태 명령어 | 설명 |
|----------------|------|
| `git status` | 충돌 파일 목록 확인 |
| `git diff` | 충돌 내용 확인 |
| `git log --oneline` | 커밋 히스토리 확인 |
