# Rebase 충돌 해결 판단

`git` skill 이 rebase 충돌을 파일별로 분할정복하며 해결할 때의 판단 프로토콜. 충돌 조정(여러 변경을 어떤 순서로 통합할지)이 필요하면 `orchestrator` skill 의 `references/merge-coordinator.md` 가 canonical 이며, 이 문서는 **단일 rebase 의 파일별 해결 전략**만 다룬다.

## 인자 처리 (--continue / --abort / --skip)

사용자가 rebase 진행 제어를 요청하면 먼저 인자를 처리한다.

```bash
case "$1" in
  --continue)
    # 미해결 충돌 가드: 남은 충돌이 있으면 rebase --continue 전에 거부 (exit 1)
    UNRESOLVED=$(git diff --name-only --diff-filter=U)
    if [ -n "$UNRESOLVED" ]; then
      echo "⚠️ 아직 해결되지 않은 충돌이 있습니다:"
      echo "$UNRESOLVED" | while read f; do echo "  - $f"; done
      echo "충돌을 먼저 해결한 후 다시 시도하세요."
      exit 1
    fi
    git rebase --continue; exit $? ;;
  --abort)
    git rebase --abort
    echo "✓ Rebase aborted. Returned to original state."; exit 0 ;;
  --skip)
    git rebase --skip; exit $? ;;
esac
```

인자가 없으면 아래 대화형 해결로 진입한다. (rebase 미진행 시 안내 후 종료, 충돌 0건이면 `--continue` 안내.)

## 파일별 분할정복 절차

충돌 파일 목록에서 첫 번째 파일부터 하나씩 처리한다.

1. **파일 선택** — AskUserQuestion 으로 충돌 파일 목록에서 처리할 파일 선택 (또는 "모두 건너뛰기" → 수동 해결 후 `--continue`).
2. **충돌 분석** — Read 로 파일을 읽고 충돌 마커(`<<<<<<<`, `=======`, `>>>>>>>`) 영역 식별. `git diff --color=always "$FILE"` 로 표시.
3. **해결 전략 선택** — AskUserQuestion (Ours / Theirs / Manual / Show diff).
4. **전략별 처리** (아래).
5. 현재 파일 해결 후 남은 충돌이 있으면 1번으로 반복. 모두 해결되면 `--continue` 안내.

## 전략별 처리

**Ours (Upstream/Base)** — rebase 대상(base) 브랜치 변경 유지:
```bash
git checkout --ours "$FILE" && git add "$FILE"
```

**Theirs (내 커밋)** — 현재 적용 중인 내 커밋 변경으로 대체:
```bash
git checkout --theirs "$FILE" && git add "$FILE"
```

**Manual (수동 병합)** — Read 로 전체 표시 → 충돌 섹션 설명 → Edit 로 함께 해결 → `git add "$FILE"`.

**Show diff** — 양쪽 버전 비교 후 전략 선택으로 복귀:
```bash
echo "=== Ours: Upstream/Base version (rebase 대상) ==="
git show :2:"$FILE" 2>/dev/null || echo "(file doesn't exist in ours)"
echo "=== Theirs: My commit version (적용 중인 내 커밋) ==="
git show :3:"$FILE" 2>/dev/null || echo "(file doesn't exist in theirs)"
```

## Conflict Marker 의미 (rebase 는 merge 와 반대!)

```
<<<<<<< HEAD (ours)
Upstream/Base 브랜치의 코드 (예: origin/main)
=======
내 커밋의 코드 (현재 적용 중인 feature 브랜치 커밋)
>>>>>>> commit-hash (theirs)
```

**⚠️ rebase 에서 ours/theirs 는 merge 와 반대다:**
- `--ours`: Upstream(base) 브랜치 변경 (HEAD 가 가리키는 rebase 대상)
- `--theirs`: 내 커밋 변경 (현재 적용 중인 feature 브랜치 커밋)

이유: rebase 중에는 HEAD 가 upstream 을 가리키고 내 커밋들이 하나씩 그 위에 적용되기 때문.

## 실전 시나리오

**Feature 브랜치를 main 에 rebase**: `git fetch origin` → `git checkout feature/my-work` → `git rebase origin/main` → 충돌 시 이 절차로 파일별 해결 → 해결 후 `--continue` 인자 처리.

**양쪽 변경 모두 필요** (예: 서로 다른 import 추가): Manual 선택 후 양쪽을 병합:
```typescript
import { ServiceA } from './serviceA';
import { ServiceB } from './serviceB';   // ours
import { ServiceC } from './serviceC';   // theirs
```

**충돌이 너무 복잡할 때**: `--skip` (현재 커밋 건너뛰기) 또는 `--abort` (전체 취소 후 merge 고려 / 더 작은 단위로 분할).

## 주의사항

- **rebase 전 브랜치 백업 고려**: `git branch backup/my-work`
- **이미 push 한 브랜치 rebase 주의**: 히스토리 변경 → 공유 브랜치는 rebase 후 `--force-with-lease` 필요
- **충돌이 너무 많으면**: `--abort` 후 merge 고려, 또는 더 작은 단위로 rebase
