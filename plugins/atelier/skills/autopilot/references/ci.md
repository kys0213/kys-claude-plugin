# CI 실패 분석 / 수정 (ci-watch · ci-fix)

CI 실패를 다루는 두 흐름. 전처리(base 동기화·idle/capacity·throttling)는 `pipeline-control.md`, 병렬 dispatch 메커니즘은 `orchestrator` skill 에 위임한다.

- **ci-watch**: default/autopilot 브랜치의 CI 실패를 감지 → 분석 → GitHub issue + ledger task 등록 (writer).
- **ci-fix**: autopilot 이 만든 PR(`{label_prefix}auto`)의 CI 실패를 tick 단위로 수정 → push.

두 흐름은 역할이 분리된다: ci-watch 는 새 실패를 이슈화, ci-fix 는 PR 의 CI 실패를 직접 수정. ci-fix 와 merge-prs 의 분리 — ci-fix 는 CI 수정만, merge-prs 는 conflict/review 만.

---

## A. ci-watch — CI 실패 분석 → 이슈 생성

> 인자: `--run-id=<id>` (특정 run 만 분석, Step 2 `gh run list` 건너뛰고 직접 분석, Step 3 중복 확인은 수행), `--branch=<branch>` (실패 브랜치 컨텍스트). hybrid 모드에서 이벤트로 전달.

전처리(`pipeline-control.md`)는 capacity 검사 불필요 — `--max-parallel` 없이 idle/active 2값으로 호출, loop 이름 `ci-watch`.

### Step 2: CI 실패 목록 조회

```bash
gh run list --status failure --limit 10 --json databaseId,name,headBranch,conclusion,createdAt,event
```

### Step 2.5: CI Failure 이슈 자동 정리

이슈 생성 **전에** 기존 CI failure 이슈 중 관련 PR이 이미 머지된 것을 정리한다:

```bash
autopilot issue close-resolved --label-prefix "{label_prefix}"
```

### Step 2.7: 오래된/머지된 PR 필터링

Step 2 결과에서 아래 조건의 실패를 **제외**한다. 설정(`github-autopilot.local.md`):

```yaml
ci_watch:
  max_age: "24h"                # non-default branch 실패 최대 수집 기간 (기본: 24h)
  default_branch_max_age: "7d"  # default branch 실패 최대 수집 기간 (기본: 7d)
  branch_filter: "autopilot"    # "autopilot" | "all" (기본: autopilot)
```

**2.7.1 브랜치 필터 (branch_filter)** — `headBranch`가 다음 중 하나가 아니면 **skip**:
- default branch (main, master, develop 등 `gh repo view --json defaultBranchRef`로 확인)
- autopilot 브랜치: `feature/issue-*` 또는 `draft/issue-*` 패턴 매칭
- 설정의 `branch_filter`가 `"all"`: 모든 브랜치 허용

`branch_filter`가 `"autopilot"`(기본값)이면 위 조건에 해당하지 않는 브랜치(일반 feature 브랜치 등)는 skip.

**2.7.2 머지/종료된 PR의 실패 제거** — `event`가 `pull_request`인 run 은 해당 `headBranch`의 PR 상태를 확인:

```bash
gh pr list --head "${HEAD_BRANCH}" --state merged --json number --limit 1
gh pr list --head "${HEAD_BRANCH}" --state closed --json number --limit 1
```

- PR이 MERGED 또는 CLOSED → **skip** (이미 종료된 PR의 과거 실패)
- PR이 OPEN 또는 PR 없음 → 계속 진행

**2.7.3 시간 기반 필터 (max_age)** — `createdAt`이 아래 기간 초과 시 **skip**:

| 브랜치 종류 | 기본 max_age | 설정 키 |
|---|---|---|
| default branch | `7d` | `ci_watch.default_branch_max_age` |
| non-default branch | `24h` | `ci_watch.max_age` |

default branch 실패는 만성 CI 실패를 놓치지 않도록 7일 이내, non-default 는 오래된 feature 브랜치 실패 방지를 위해 24시간 이내만 수집. 필터링 후 남은 실패만 Step 3로 진행.

### Step 3: 중복 이슈 필터링

설정의 label_prefix 확인(기본 `autopilot:`). 각 실패를 **fingerprint 기반**으로 중복 확인 (`references/issue-label.md` 참조):

```bash
# fingerprint 형식: ci:{workflow}:{branch}:{failure_type}
FINGERPRINT="ci:validate.yml:main:test-failure"
autopilot issue check-dup --fingerprint "$FINGERPRINT"   # exit 1이면 skip
```

중복이 아닌 실패만 Step 4로. 남은 실패가 0건이면 `autopilot check mark ci-watch --status idle` 후 종료.

### Step 4: 실패 분석 (Agent Team)

분석 전 idle count 리셋: `autopilot check mark ci-watch --status active`.

새 실패 각각에 대해 ci-failure-analyzer 에이전트**를** 디스패치한다. **실패 3개 이하**: 순차(background=false). **실패 4개 이상**: 병렬(background=true). 병렬 dispatch 메커니즘은 `orchestrator` skill 에 위임.

각 에이전트에 전달: run_id, run_name(워크플로우 이름), head_branch(실패한 브랜치).

### Step 5: Issue 생성 + Ledger 동기 기록

**5a. Ledger Epic 부트스트랩** — 이슈 생성 루프 진입 직전, `ci-backlog` epic 을 한 번만 보장(idempotent):

```bash
EPIC_NAME="ci-backlog"
EPIC_SPEC="specs/ci-backlog.md"
out=$(autopilot epic create --name "$EPIC_NAME" --spec "$EPIC_SPEC" 2>&1) || true
case "$out" in
  *"created"*|*"already exists"*)
    # 정상: 새로 생성 또는 이미 존재 (epic create는 이미 존재 시 exit 1)
    ;;
  *)
    echo "WARN: ci-backlog epic 부트스트랩 실패 — ledger 쓰기는 skip됩니다: $out"
    EPIC_NAME=""
    ;;
esac
```

> ledger 는 GitHub issue 와 독립적인 부가 기록. 부트스트랩 실패 시 `EPIC_NAME=""`로 두어 이번 cycle 의 ledger 쓰기를 모두 skip (ci-watch cycle 자체는 계속).

**5b. Issue 생성** — 분석 결과로 autopilot CLI 로 이슈 생성:

```bash
FINGERPRINT="ci:${WORKFLOW_NAME}:${BRANCH}:${FAILURE_TYPE}"

autopilot issue create \
  --title "fix: CI failure in ${WORKFLOW_NAME} on ${BRANCH}" \
  --label "{label_prefix}ci-failure" \
  --label "{label_prefix}ready" \
  --fingerprint "$FINGERPRINT" \
  --body "$(cat <<'EOF'
## CI 실패 분석

- **Run**: ${RUN_ID}
- **Workflow**: ${WORKFLOW_NAME}
- **Branch**: ${BRANCH}
- **실패 유형**: ${FAILURE_TYPE}

## 원인 분석

${ANALYSIS_SUMMARY}

## 영향 파일

${AFFECTED_FILES}

## 수정 제안

${SUGGESTED_FIX}
EOF
)"
```

> fingerprint HTML 주석은 CLI 가 body 하단에 자동 삽입한다.

**5c. Ledger 동기 기록 (observer)** — GitHub issue 생성 성공 시(중복 skip 제외)에만, `EPIC_NAME`이 비어있지 않으면 동일 fingerprint 로 ledger task 기록. ledger 실패는 WARN 으로 로그하고 ci-watch cycle 을 절대 막지 않는다:

```bash
if [ -n "${EPIC_NAME:-}" ]; then
  # task id는 fingerprint의 sha256 앞 12자리(hex). 동일 fingerprint → 동일 id (idempotent).
  TASK_ID=$(printf '%s' "$FINGERPRINT" | shasum -a 256 | cut -c1-12)
  autopilot task add "$TASK_ID" \
    --epic "$EPIC_NAME" \
    --title "fix: CI failure in ${WORKFLOW_NAME} on ${BRANCH}" \
    --fingerprint "$FINGERPRINT" \
    --source ci-watch \
    || echo "WARN: ledger task add 실패 (issue는 정상 생성됨) — 계속 진행"
fi
```

> CLI 동작: 신규 fingerprint → `inserted task <id>` (exit 0) / 이미 등록 → `duplicate of task <id>` (exit 0, no-op) / epic 미존재·환경 오류 → 비-zero exit, WARN 후 무시 (issue 는 이미 생성). fingerprint 형식·결정성 요건은 `ci-failure-analyzer`의 *Fingerprint 계약* 섹션 정의 (Step 3 중복 확인과 동일 값).

### Step 6: 결과 보고 + 세션 통계

생성된 이슈 목록과 분석 요약을 출력. 매 cycle 종료 시 세션 통계 업데이트:

- `PROCESSED` = Step 2 + 2.7 필터링 후 분석 진입한 CI 실패 수 (ci-failure-analyzer 호출 항목)
- `SUCCESS` = Step 5b 에서 GitHub issue 생성 성공 항목 수 (ledger task 는 best-effort observer, 별도 카운트 불필요)
- `FAILED` = issue 생성 시도가 비-zero exit 으로 실패한 항목 수
- `FALSE_POSITIVE` = Step 3 중복 또는 Step 2.7 머지/오래된 PR skip 항목 수

```bash
autopilot stats update --command ci-watch \
  --processed ${PROCESSED} --success ${SUCCESS} --failed ${FAILED} --false-positive ${FALSE_POSITIVE}
autopilot stats show --command ci-watch
```

> `processed=0`이면 `idle_cycles`, `processed>0`이면 `agent_calls` 자동 누적. 통계는 `/tmp/autopilot-{repo}/state/session-stats.json`, 세션 시작 시 `autopilot stats init`으로 초기화.

### ci-watch 원칙

- `references/issue-label.md` 의 라벨 필수 규칙·fingerprint 규칙을 따른다.
- 토큰 최적화: MainAgent 는 CI 로그를 직접 읽지 않음. 모든 로그 분석은 ci-failure-analyzer 가 수행.
- flaky test 와 실제 실패를 구분해 라벨링.
- ledger 쓰기는 보조 observer — 실패가 issue 생성 결과를 무효화하지 않도록 `|| echo WARN ...` 로 격리 (Step 5c).

---

## B. ci-fix — PR CI 실패 tick 단위 수정

> 인자: `--branch=<branch>` (특정 브랜치 PR 만 대상, Step 2 에서 해당 브랜치만 조회). 한 호출에서 수정 → push 까지만, CI 결과 확인은 다음 tick.

전처리(`pipeline-control.md`)는 capacity 검사 불필요 — `--max-parallel` 없이 idle/active 2값으로 호출, loop 이름 `ci-fix`.

### Step 2: CI 실패 PR 조회

설정의 label_prefix 확인(기본 `autopilot:`).

```bash
gh pr list --label "{label_prefix}auto" --state open --json number,title,headRefName,baseRefName,statusCheckRollup --limit 20
```

statusCheckRollup 에서 FAILURE 상태인 PR 만 필터링. CI 실패 PR 이 없으면 `autopilot check mark ci-fix --status idle` 후 "CI 실패 PR 없음" 출력 후 종료.

### Step 3: 재시도 횟수 확인

각 CI 실패 PR 코멘트에서 재시도 마커 확인:

```bash
gh pr view ${PR_NUMBER} --json comments --jq '.comments[].body' | grep -o '<!-- autopilot:ci-fix:[0-9]* -->' | tail -1
```

마커에서 현재 재시도 횟수 N 추출(마커 없으면 N=0). 설정의 `max_ci_fix_retries` 확인(기본 3).

**N >= max_ci_fix_retries**: 에스컬레이션 — PR에 코멘트 게시 후 skip, 다음 PR 진행. `notification` 설정이 있으면 알림 발송.

```markdown
## CI Fix Escalation

**Retries exhausted**: {N}/{max_ci_fix_retries}
CI 실패를 자동으로 해결하지 못했습니다. 사람의 검토가 필요합니다.

<!-- autopilot:ci-fix:escalated -->
```

**N < max_ci_fix_retries**: Step 4로 진행.

### Step 4: CI 수정 (Agent Team)

수정 전 idle count 리셋: `autopilot check mark ci-fix --status active`.

수정 대상 PR 각각에 대해 ci-fixer 에이전트**를** 디스패치한다. **PR 3개 이하**: 순차(background=false). **PR 4개 이상**: 병렬(background=true). 병렬 dispatch 메커니즘은 `orchestrator` skill 에 위임.

각 에이전트에 전달: pr_number, pr_title, head_branch, base_branch, retry_count: N, quality_gate_command(설정값).

### Step 5: 결과 수집

**fix_pushed** (수정 push 완료) — PR에 재시도 마커 코멘트 게시:

```markdown
CI fix attempt {N+1}/{max_ci_fix_retries}

**Failure type**: {failure_type}
**Fix**: {fix_summary}
**Files**: {files_modified}

다음 tick에서 CI 결과를 확인합니다.

<!-- autopilot:ci-fix:{N+1} -->
```

**fix_failed** (수정 실패) — PR에 실패 코멘트 게시(재시도 마커 포함), 다음 tick 에서 재시도.

### Step 6: 결과 보고

```
## CI Fix 결과

### 대상 PR
- CI 실패 PR: 3개

### 수정
- fix pushed: #50 (lint fix), #52 (test fix)
- fix failed: #51 (complex logic - needs human review)

### 에스컬레이션
- #53 (3/3 retries exhausted → escalated)
```

### ci-fix 원칙

- **cron 모드**: 1 tick = 1 수정 시도. CI 실행 완료를 기다리지 않음.
- **hybrid 모드**: fix push 후 one-shot Monitor 로 CI 완료를 감시해 즉시 반응.
- CI 가 아직 실행 중인 PR 은 skip (statusCheckRollup 에 PENDING 있으면).
- merge-prs 루프와 역할 분리: ci-fix 는 CI 수정만, merge-prs 는 conflict/review 만.
- 토큰 최적화: MainAgent 는 PR 목록 조회·마커 관리만, CI 분석/수정은 모두 Agent 에 위임.
