---
description: "autopilot PR을 CI/리뷰 상태 확인 후 머지합니다"
argument-hint: "[--branch=<branch>]"
allowed-tools: ["Bash", "Read", "Agent"]
---

# Merge PRs

autopilot이 생성한 PR들을 분석하여 문제가 없으면 머지하고, 문제가 있으면 해결을 시도합니다.

## 사용법

```bash
/github-autopilot:merge-prs                           # 전체 스캔 (cron 모드)
/github-autopilot:merge-prs --branch=feature/issue-42  # 타겟 PR (hybrid 모드)
```

> 반복 실행은 `/github-autopilot:autopilot`이 CronCreate 또는 Monitor로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 작업 프로세스

### Step 0: 인자 파싱

`$ARGUMENTS`에서 옵션을 추출합니다:
- `--branch=<branch>`: 특정 브랜치의 PR만 대상으로 처리 (hybrid 모드에서 이벤트로 전달)

`--branch`가 있으면 Step 2에서 해당 브랜치의 PR만 조회합니다:
```bash
gh pr list --head "{branch}" --label "{label_prefix}auto" --state open --json ... --limit 1
```

### Step 1: Base 브랜치 동기화

**branch-sync** 스킬의 절차를 수행합니다.

### Step 1.5: Pipeline Idle Check

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — merge-prs cycle 중단" 알림 발송 후 종료합니다.
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다.
- **exit 1 (active)**: Step 2부터 정상 진행.

### Step 1.7: Idle Count Check

이전 Step의 결과가 "대상 없음"(idle)이면, 연속 idle 횟수를 기록합니다.

```bash
autopilot check mark merge-prs --status idle
```

설정에서 `idle_shutdown.max_idle` 값을 읽습니다 (기본값: 5).

연속 idle 횟수가 `max_idle` 이상이면:
1. `autopilot cron self-delete --name "merge-prs"` 로 cron을 자동 해제합니다.
2. "연속 {N}회 idle — cron 자동 해제" 메시지를 출력하고 종료합니다.

실제 작업을 수행하면 idle count를 리셋합니다:
```bash
autopilot check mark merge-prs --status active
```

### Step 2: PR 목록 조회

설정에서 label_prefix를 확인합니다 (기본값: `autopilot:`).

```bash
gh pr list --label "{label_prefix}auto" --state open --json number,title,mergeable,statusCheckRollup,reviewDecision,headRefName,baseRefName --limit 20
```

PR이 없으면 `autopilot check mark merge-prs --status idle` 후 "머지 대상 PR 없음" 출력 후 종료.

### Step 3: PR 분류

각 PR을 상태별로 분류합니다:

| 조건 | 분류 |
|------|------|
| mergeable=MERGEABLE + CI 통과 + 리뷰 승인/없음 | **all-green** |
| mergeable=CONFLICTING | **conflict** |
| CI 실패 | **ci-pending** (ci-fix 루프에서 처리) |
| reviewDecision=CHANGES_REQUESTED | **review-requested** |

> **참고**: CI 실패 PR은 ci-fix 루프에서 tick 단위로 수정합니다. merge-prs는 CI 실패를 직접 수정하지 않습니다.

### Step 4: All-green PR 즉시 머지

머지를 시작하기 전에 idle count를 리셋합니다: `autopilot check mark merge-prs --status active`

**중요**: `gh pr merge` 의 종료 코드를 반드시 검사해야 합니다. 동시 머지 경합 (`Base branch was modified` / `Pull Request is not mergeable`) 으로 인해 머지가 실패한 경우 ledger close, `Closes #N` 이슈 close, worktree 정리는 **실행되지 않아야** 하며, 해당 PR은 Step 5 (문제 PR 분류) 로 라우팅합니다.

```bash
if gh pr merge ${PR_NUMBER} --squash --delete-branch; then
  # === 머지 성공 경로 ===
  # 아래 후속 작업은 모두 머지가 실제로 성공했을 때만 실행합니다.

  # ---------------------------------------------------------------------
  # (1) Ledger Close-the-Loop (best-effort)
  #     PR을 소유한 ledger task 가 있으면 Wip → Done 으로 전환합니다.
  #     fast-path 는 pr-merger 를 거치지 않으므로 동일한 close-the-loop 로직을 인라인 수행.
  #     이 단계의 어떤 실패도 머지 결과 자체에는 영향을 주지 않습니다.
  # ---------------------------------------------------------------------
  # set -e 가 켜져 있어도 모든 ledger 실패를 흡수하기 위해 호출 전체를 if/then 으로 감쌉니다.
  # - `if cmd; then` 컨텍스트에서는 cmd 실패가 set -e 를 트리거하지 않습니다.
  # - jq 가 부재하거나 JSON 이 깨져도 `|| true` 로 빈 문자열로 흘려보냅니다.
  if FIND_OUT=$(autopilot task find-by-pr "${PR_NUMBER}" --json 2>/dev/null); then
    TASK_ID=$(printf '%s' "${FIND_OUT}" | jq -r '.id // empty' 2>/dev/null || true)
    if [ -n "${TASK_ID}" ]; then
      if autopilot task complete "${TASK_ID}" --pr "${PR_NUMBER}"; then
        echo "[INFO] ledger: completed task ${TASK_ID} for PR #${PR_NUMBER}"
      else
        echo "[WARN] ledger: task complete failed for ${TASK_ID} (PR #${PR_NUMBER}) — continuing" >&2
      fi
    else
      echo "[WARN] ledger: could not parse task id from find-by-pr output for PR #${PR_NUMBER} — skipping" >&2
    fi
  else
    # find-by-pr exit 1 = pre-ledger PR (소유한 task 없음). 정상 케이스이므로 INFO.
    # 그 외 exit 코드(DB 오류, 바이너리 부재 등)도 동일하게 흡수합니다 — best-effort.
    echo "[INFO] ledger: no task owns PR #${PR_NUMBER} (or ledger unavailable) — skipping complete" >&2
  fi

  # ---------------------------------------------------------------------
  # (2) 관련 이슈 자동 close
  #     PR body 의 모든 `Closes #N` 패턴을 추출하여 명시적으로 close 합니다.
  #     `--squash` 머지 시 GitHub auto-close 가 동작하지 않을 수 있으므로 fallback.
  #     **머지 실패 경로에서는 절대 실행되지 않아야 합니다** (이슈 오close 방지 — #643).
  # ---------------------------------------------------------------------
  # PR body에서 모든 Closes #N 이슈 번호를 추출 (macOS/Linux 호환)
  ISSUE_NUMBERS=$(gh pr view ${PR_NUMBER} --json body --jq '.body' | grep -oE 'Closes #[0-9]+' | grep -oE '[0-9]+')

  for ISSUE_NUMBER in $ISSUE_NUMBERS; do
    STATE=$(gh issue view "$ISSUE_NUMBER" --json state --jq '.state' 2>/dev/null || echo "")
    if [ "$STATE" = "OPEN" ]; then
      # 실패해도 머지/ledger 결과를 되돌릴 수 없으므로 best-effort 이지만,
      # 침묵 swallow 대신 stderr 로 WARN 을 남겨 사람이 추적할 수 있게 합니다.
      if ! gh issue close "$ISSUE_NUMBER" --comment "Closed by PR #${PR_NUMBER} merge (autopilot)" 2>/dev/null; then
        echo "[WARN] gh issue close failed for #${ISSUE_NUMBER} (PR #${PR_NUMBER}) — manual close may be required" >&2
      fi
    fi
  done

  # ---------------------------------------------------------------------
  # (3) 로컬 worktree 자동 정리
  #     머지된 브랜치에 연결된 worktree 가 남아있으면 `git branch -D` 가 실패하고
  #     `cannot delete branch used by worktree` 경고가 반복됩니다.
  # ---------------------------------------------------------------------
  HEAD_BRANCH=$(gh pr view ${PR_NUMBER} --json headRefName --jq '.headRefName')
  autopilot worktree cleanup --branch "${HEAD_BRANCH}"

else
  # === 머지 실패 경로 ===
  # `gh pr merge` 가 비-zero 로 종료한 경우 (예: Base branch was modified, not mergeable,
  # required reviews missing 등). 이 PR 은 Step 5 (문제 PR 분류) 로 핸드오프합니다.
  #
  # **중요**: ledger close / Closes #N / worktree cleanup 은 실행하지 않습니다.
  # 과거(#643)에는 이 경로에서도 후속 작업이 무조건 실행되어 머지에 실패한 PR 의
  # 연결 이슈가 잘못 close 되는 버그가 있었습니다.
  echo "[ERROR] merge failed for PR #${PR_NUMBER}: routing to Step 5 problem-PR classification" >&2
  # 호출자(merge-prs 오케스트레이터) 는 이 PR 을 Step 5 의 pr-merger 에이전트로 전달합니다.
fi
```

**Ledger close-the-loop 동작 요약 (best-effort):**

| 케이스 | 동작 |
|--------|------|
| `find-by-pr` exit 1 (no task owns PR) | INFO 로그 후 skip — 머지는 성공 보고 |
| `find-by-pr` exit ≠ 0/1 (DB 오류 등) | INFO 로그 후 skip — 머지는 성공 보고 |
| `autopilot` 바이너리 부재 | `if` 가 비-zero 로 평가되어 자동 skip |
| `jq` 부재 또는 JSON 파싱 실패 | `\|\| true` 로 빈 id 처리 → WARN 로그 후 skip |
| `task complete` 실패 (이미 done, not found 등) | WARN 로그 후 skip |

> 이 단계의 어떤 실패도 fast-path 머지 결과를 변경하지 않습니다. 동일한 로직이 `agents/pr-merger.md` Step 4 에도 존재합니다 (문제 PR 경로용).

**머지 종료 코드 가드 (#643):** 위 `if gh pr merge ...; then ... else ... fi` 구조는 머지가 실패했을 때 `Closes #N` 이슈가 잘못 close 되는 것을 방지합니다. 실패 시 PR 은 Step 5 의 `pr-merger` 에이전트에게 위임되어 conflict / changes-requested 분류 후 처리됩니다.

### Step 5: 문제 PR 해결 (Agent Team)

문제가 있는 PR 각각에 대해 pr-merger 에이전트를 호출합니다:

**PR 수가 3개 이하**: 순차 호출 (background=false)
**PR 수가 4개 이상**: 병렬 호출 (background=true)

각 에이전트에게 전달:
- pr_number
- pr_title
- problems: 감지된 문제 목록 (conflict, review_changes_requested)

> CI 실패는 ci-fix 루프에서 처리하므로 pr-merger에 전달하지 않습니다.

**중요**: 사람이 명시적으로 `CHANGES_REQUESTED` 리뷰를 남긴 PR은 자동 머지하지 않습니다. pr-merger가 코멘트에 응답만 하고, 머지는 사람의 재리뷰 후 다음 cycle에서 처리합니다.

### Step 6: 결과 보고

```
## Merge 결과
- 머지 완료: #50, #52 (2건)
- 해결 시도: #51 (conflict → resolved → merged)
- 보류: #53 (human review requested)
- 실패: #54 (conflict resolution failed)
```

## 주의사항

- 사람의 CHANGES_REQUESTED 리뷰가 있는 PR은 자동 머지 금지
- squash merge 사용 (draft에서의 지저분한 커밋 히스토리 정리)
- 머지 후 feature 브랜치 자동 삭제
