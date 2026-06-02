---
name: branch-sync
description: autopilot 공통 브랜치 동기화 절차. 모든 커맨드의 Step 1에서 참조하여 설정 기반 base 브랜치로 checkout + pull을 수행
version: 1.0.0
---

# Branch Sync

autopilot의 모든 커맨드가 작업 시작 전 수행하는 공통 동기화 절차.

## 절차

### 0. Stale Worktree 정리

```bash
autopilot worktree cleanup-stale
```

이전 cycle에서 정리되지 못한 `draft/*` worktree를 정리한다.
uncommitted changes는 partial commit으로 브랜치에 보존한 뒤 worktree만 제거한다.
draft 브랜치 자체는 삭제하지 않으므로, 다음 cycle에서 이전 작업을 이어받을 수 있다.

### 1. 설정 로딩

`github-autopilot.local.md` frontmatter에서 `work_branch`와 `branch_strategy`를 읽는다.

### 2. Base 브랜치 결정

| 우선순위 | 조건 | base 브랜치 |
|---------|------|------------|
| 1 | `work_branch`가 설정됨 | `work_branch` 값 (예: `"alpha"`) |
| 2 | `branch_strategy: "draft-develop-main"` | `develop` |
| 3 | `branch_strategy: "draft-main"` 또는 기본값 | `main` |

### 3. Fetch + Checkout + Pull

```bash
git fetch origin
git checkout {base_branch}
git pull --rebase origin {base_branch} 2>/dev/null || true
```

## 이유

autopilot은 주기적으로 실행되므로, 이전 실행 이후 다른 agent나 사람이 변경한 내용을 반영하지 않으면 충돌이나 중복 작업이 발생한다. 또한 현재 체크아웃된 브랜치가 base 브랜치와 다를 수 있으므로, 명시적으로 base 브랜치로 전환해야 한다.

