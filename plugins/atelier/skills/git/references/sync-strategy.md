# 브랜치 동기화 전략

`git` skill(또는 `atelier git sync` CLI)이 지정 브랜치(또는 기본 브랜치)로 전환 후 원격 최신 상태로 동기화하는 절차와 판단.

## 인자 파싱

`--force` 와 브랜치 이름은 **순서 무관**. 인자 순회로 `--force` → FORCE=true, 그 외 → TARGET_BRANCH:

```bash
FORCE=false; TARGET_BRANCH=""
for arg in "$@"; do
  case "$arg" in
    --force) FORCE=true ;;
    *) TARGET_BRANCH="$arg" ;;
  esac
done
CURRENT_BRANCH=$(git branch --show-current)
CHANGES=$(git status --porcelain)
# 브랜치 미지정 시 기본 브랜치 자동 감지
if [ -z "$TARGET_BRANCH" ]; then
  TARGET_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's|refs/remotes/origin/||' || echo 'main')
fi
```

## 변경사항 처리 (stash 정책)

```bash
# --force 없이 변경사항 있으면 중단
if [ -n "$CHANGES" ] && [ "$FORCE" != "true" ]; then
  echo "⚠️ Uncommitted changes detected. Use --force to stash and continue."
  exit 1
fi
# --force 면 stash (버리지 않음)
if [ -n "$CHANGES" ] && [ "$FORCE" = "true" ]; then
  git stash push -m "auto-stash before git-sync"
  STASHED=true
fi
```

## 브랜치 존재 여부 → 처리 매트릭스

```bash
LOCAL_EXISTS=$(git show-ref --verify --quiet "refs/heads/$TARGET_BRANCH" && echo yes || echo no)
git fetch origin --prune 2>/dev/null
REMOTE_EXISTS=$(git show-ref --verify --quiet "refs/remotes/origin/$TARGET_BRANCH" && echo yes || echo no)
```

| 로컬 | 원격 | 처리 |
|------|------|------|
| 있음 | - | `git checkout $TARGET_BRANCH && git pull origin $TARGET_BRANCH` |
| 없음 | 있음 | `git checkout -b $TARGET_BRANCH --track origin/$TARGET_BRANCH` (CREATED_TRACKING) |
| 없음 | 없음 | AskUserQuestion 으로 확인 (아래) |

**둘 다 없을 때** AskUserQuestion: "Branch '{TARGET_BRANCH}' 이 로컬·원격 모두 없습니다. 어떻게 처리할까요?" → Create new branch (`git checkout -b $TARGET_BRANCH`, CREATED_NEW) / Cancel (중단).

## 결과 보고

```bash
echo "✓ Synced to $TARGET_BRANCH"
echo "  Previous branch: $CURRENT_BRANCH"
[ "$STASHED" = true ] && echo "  ⚠️ Changes were stashed. Use 'git stash pop' to restore."
[ "$CREATED_TRACKING" = true ] && echo "  ℹ️ Created local branch tracking origin/$TARGET_BRANCH"
[ "$CREATED_NEW" = true ] && echo "  ℹ️ New branch created from $CURRENT_BRANCH"
```

## Output Examples

```
✓ Synced to main
  Previous branch: feat/wad-0212
```
```
✓ Synced to feat/shared
  Previous branch: main
  ℹ️ Created local branch tracking origin/feat/shared
```
```
✓ Synced to develop
  Previous branch: feat/wad-0212
  ⚠️ Changes were stashed. Use 'git stash pop' to restore.
```

## Notes

- 브랜치 미지정 시 기본 브랜치(main/master 등) 자동 감지
- `--force` 와 브랜치 이름은 순서 무관
- `--force` 는 stash 만 하고 변경사항을 버리지 않음 (`git stash list` 로 확인)
- 원격에만 있는 브랜치 지정 시 자동 tracking 브랜치 생성
- 존재하지 않는 브랜치 지정 시 AskUserQuestion 으로 새 브랜치 생성 여부 확인
