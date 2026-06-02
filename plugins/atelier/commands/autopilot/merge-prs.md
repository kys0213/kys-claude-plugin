---
description: "autopilot PR을 CI/리뷰 상태 확인 후 머지합니다"
argument-hint: "[--branch=<branch>]"
allowed-tools: ["Bash", "Read", "Agent"]
---

# Merge PRs (/atelier:merge-prs)

autopilot 이 생성한 PR 들을 분석하여 문제가 없으면 머지하고, 문제가 있으면 해결을 시도합니다.

> 파이프라인 제어와 단계별 프로토콜은 `autopilot-pipeline` skill 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 사용법

```bash
/atelier:merge-prs                            # 전체 스캔 (cron 모드)
/atelier:merge-prs --branch=feature/issue-42  # 타겟 PR (hybrid 모드)
```

> 반복 실행은 `/atelier:autopilot`이 CronCreate 또는 Monitor로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 작업 프로세스

### Step 0: 인자 파싱

`$ARGUMENTS`에서 옵션을 추출합니다:
- `--branch=<branch>`: 특정 브랜치의 PR만 대상으로 처리 (있으면 Step 2 에서 해당 브랜치만 조회)

### Step 1: 전처리 (공통)

`autopilot-pipeline` `references/pipeline-control.md` 의 3단계를 수행합니다:
1. Base 브랜치 동기화 (`branch-sync` 스킬)
2. Pipeline Idle Check — capacity 검사 불필요(`--max-parallel` 생략, idle/active 2값)
3. Idle Count + Adaptive Throttling (loop 이름 `merge-prs`)

설정에서 `label_prefix`, `idle_shutdown.max_idle`(기본 5), `notification` 을 읽습니다.

### Step 2: PR 머지 파이프라인

`autopilot-pipeline` `references/merge.md` 절차를 수행합니다:
- PR 목록 조회 → PR 분류(all-green / conflict / ci-pending / review-requested) → all-green 즉시 머지(#643 종료코드 가드 + ledger close-the-loop + Closes #N + worktree cleanup) → 문제 PR 해결(Agent Team) → 결과 보고 + 세션 통계

> 머지·worktree·병렬 dispatch 메커니즘은 `orchestrator` skill 에 위임합니다 (merge.md 가 "무엇을 위임할지"와 머지 성공/실패 후속 가드만 정의).

## 주의사항

- 사람의 CHANGES_REQUESTED 리뷰가 있는 PR 은 자동 머지 금지
- squash merge 사용 (draft 의 지저분한 커밋 히스토리 정리)
- 머지 후 feature 브랜치 자동 삭제
- CI 실패 PR 은 ci-fix 루프에서 처리 (merge-prs 는 직접 수정하지 않음)

상세 프로토콜·#643 가드·ledger close-the-loop·결과 형식은 `autopilot-pipeline` skill 의 references 참조.
