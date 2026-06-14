# Rebase 충돌 해결 정책

`git` skill 이 rebase 충돌을 해결할 때의 **판단·정책**. mechanical 한 git 사용법(`checkout --ours/--theirs`, `git add`, `--continue/--abort/--skip`, 마커 편집)은 모델이 직접 수행한다 — 여기에는 모델이 틀리기 쉬운 gotcha 와 진행 정책만 둔다. 여러 변경의 통합 순서 조정은 `orchestrator` skill 의 `references/merge-coordinator.md` 가 canonical 이며, 이 문서는 **단일 rebase 의 충돌 해결 전략**의 단일 출처다 (pr-merger·merge-coordinator 가 위임).

## ⚠️ rebase 의 ours/theirs 는 merge 와 반대다 (가장 자주 틀림)

```
<<<<<<< HEAD (ours)        ← Upstream/Base 브랜치 (예: origin/main)
=======
>>>>>>> commit (theirs)    ← 내 커밋 (현재 적용 중인 feature 커밋)
```

- `--ours` = **Upstream/base** 변경 (rebase 중 HEAD 가 base 를 가리킴)
- `--theirs` = **내 커밋** 변경

이유: rebase 는 내 커밋들을 base 위에 하나씩 다시 얹으므로 `HEAD = base`. merge 와 정반대다.

## 진행 정책

- **파일별 분할정복**: 충돌 파일을 하나씩, 각 파일마다 사용자에게 Ours / Theirs / Manual 을 확인하고 해결한다. 한 번에 일괄 처리하지 않는다.
- **양쪽 다 필요하면 Manual**: 서로 다른 import 추가처럼 둘 다 살려야 하면 직접 병합한다.
- **--continue 가드**: `git diff --name-only --diff-filter=U` 에 남은 충돌이 있으면 `--continue` 하지 않는다.
- **이미 push 한 브랜치**: 히스토리가 바뀌므로 rebase 후 `--force-with-lease` (절대 `--force` 아님).
- **충돌이 과도하면**: `--abort` 후 merge 로 전환하거나 더 작은 단위로 분할 rebase 를 고려한다.
