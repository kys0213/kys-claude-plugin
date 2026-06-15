# Branch Sync

autopilot의 모든 커맨드가 작업 시작 전 수행하는 공통 동기화 절차. 설정 기반 base 브랜치 결정과 checkout + pull 을 단일 출처로 정의한다 (`pipeline-control.md` Step 1 이 로드).

## 절차

### 0. Stale Worktree 정리

```bash
autopilot worktree cleanup-stale
```

이전 cycle에서 정리되지 못한 `draft/*` worktree를 정리한다.
uncommitted changes는 partial commit으로 브랜치에 보존한 뒤 worktree만 제거한다.
draft 브랜치 자체는 삭제하지 않으므로, 다음 cycle에서 이전 작업을 이어받을 수 있다.

### 1. Base 브랜치 결정

`github-autopilot.local.md` 의 `work_branch` > `branch_strategy` 규칙으로 base 브랜치를 결정한다. 이 결정적 계산은 CLI 단일 출처가 담당하며, 결과는 모든 하위 에이전트에 `base_branch` 입력으로 전달된다(에이전트는 재계산하지 않는다):

```bash
# config 가 프로젝트 루트에 있으므로 루트에서 실행한다 (다른 cwd 면 --project-dir 로 지정).
base_branch=$(atelier autopilot base-branch)
```

규칙(참고): `work_branch` 설정 시 그 값 / `branch_strategy: "draft-develop-main"` → `develop` / `"draft-main"`·미설정 → `main`.

### 2. Fetch + Checkout + Pull

```bash
git fetch origin
git checkout {base_branch}
git pull --rebase origin {base_branch} 2>/dev/null || true
```

## 이유

autopilot은 주기적으로 실행되므로, 이전 실행 이후 다른 agent나 사람이 변경한 내용을 반영하지 않으면 충돌이나 중복 작업이 발생한다. 또한 현재 체크아웃된 브랜치가 base 브랜치와 다를 수 있으므로, 명시적으로 base 브랜치로 전환해야 한다.

