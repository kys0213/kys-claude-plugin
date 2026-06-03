# 테스트 커버리지 보강 (qa-boost)

최근 변경사항을 QA 관점에서 분석해 누락된 테스트를 결정적 ledger 의 `qa-backlog` epic 에 task 로 기록한다. work-ledger reader 가 task 를 claim 해 build-issues 파이프라인 없이 직접 PR 을 발행한다. 전처리(base 동기화·idle/throttling)는 `pipeline-control.md`.

> 인자: `[commit_hash]` — `/^[0-9a-f]{7,40}$/` 매칭 시 해당 커밋 이후 변경 분석, 비어있으면 최근 20커밋 기준.

전처리(`pipeline-control.md`)는 capacity 검사 불필요 — `--max-parallel` 생략, loop 이름 `qa-boost`. idle 시 알림: "autopilot 파이프라인 완료 — qa-boost cycle 중단".

### Step 3: 변경사항 수집

**commit_hash 가 있는 경우**:

```bash
git diff ${commit_hash}..HEAD --name-only
git log ${commit_hash}..HEAD --oneline --format="%H %s"
```

**commit_hash 가 없는 경우**:

```bash
git log --oneline -20 --format="%H %s"
git diff --name-only HEAD~20..HEAD 2>/dev/null || git diff --name-only $(git rev-list --max-parents=0 HEAD)..HEAD
```

변경 파일에서 테스트 파일(`*_test.*`, `*_spec.*`, `test_*`, `tests/`, `__tests__/`, `e2e/`, `bench/`, `benches/`)을 제외한 소스 파일만 추출한다.

### Step 4: 테스트 매핑 분석

각 소스 파일에 대해 대응하는 테스트 파일이 있는지 Glob 으로 확인한다:

```
src/auth/mod.rs      → tests/auth_test.rs ✅ (존재)
src/auth/refresh.rs  → tests/refresh_test.rs ❌ (없음)
src/api/handler.rs   → tests/api_test.rs ⚠️ (있지만 handler 관련 테스트 없음)
```

테스트가 충분한 파일은 제외하고, 보강이 필요한 파일을 그룹화한다. 보강 대상이 없으면 `autopilot check mark qa-boost --status idle` 후 "테스트 보강 대상 없음" 출력 후 Step 7 로 이동.

### Step 5: Ledger Epic 부트스트랩

ledger 쓰기 직전, `qa-backlog` epic 을 한 번만 보장(idempotent):

```bash
EPIC_NAME="qa-backlog"
EPIC_SPEC="spec/qa-backlog.md"
if ! autopilot epic create --name "$EPIC_NAME" --spec "$EPIC_SPEC" --idempotent; then
  echo "ERROR: qa-backlog epic 부트스트랩 실패 — 이번 cycle 중단" >&2
  exit 1
fi
```

> ledger 는 qa-boost 의 유일한 출력 경로이므로 epic 부트스트랩 실패 시 cycle 중단하고 다음 tick 에서 재시도 (gap-watch 와 동일한 ledger-only 패턴).

### Step 6: Ledger Task 기록

ledger 쓰기 전 idle count 리셋: `autopilot check mark qa-boost --status active`.

테스트 보강이 필요한 각 항목에 대해 ledger task 를 기록한다. 동일 fingerprint 는 `task add`가 idempotent 하게 흡수하므로 별도 dedup 단계 불필요.

```bash
FINGERPRINT="qa:${SOURCE_FILE}:${TEST_TYPE}"
TASK_ID=$(printf '%s' "$FINGERPRINT" | shasum -a 256 | cut -c1-12)
TASK_TITLE="test(scope): add missing tests for ${SOURCE_FILE}"

autopilot task add "$TASK_ID" \
  --epic "$EPIC_NAME" \
  --title "$TASK_TITLE" \
  --fingerprint "$FINGERPRINT" \
  --source qa-boost \
  --body "$(cat <<'EOF'
## 테스트 보강 대상

- **소스 파일**: [변경된 소스 파일 경로]
- **변경 내용**: [변경된 함수/메서드 요약]

## 누락된 테스트

| 테스트 타입 | 대상 | 설명 |
|------------|------|------|
| unit | [함수명] | [정상 경로 / 에러 경로 / 경계값 등] |
| e2e | [워크플로우] | [엔드투엔드 시나리오] |

## 기존 테스트 현황

- 기존 테스트 파일: [경로 또는 없음]
- 커버리지 갭: [구체적으로 빠진 부분]

## 구현 가이드

- 프로젝트 테스트 컨벤션 따름
- 기존 테스트 수정 금지 (추가만)
EOF
)" \
  || echo "WARN: ledger task add 실패 (id 충돌 또는 환경 오류) — 다음 항목 진행"
```

> CLI 동작:
> - 신규 task id + 신규 fingerprint: `inserted task <id>` (exit 0)
> - 신규 task id + 기존 fingerprint: `duplicate of task <existing-id>` (exit 0, no-op)
> - 기존 task id (재실행): `task '<id>' already exists` (exit 1, no-op) — `|| echo WARN ...` 가 흡수
>
> task 가 기록되면 work-ledger reader 가 cron tick 에 claim 해 `issue-implementer` → `branch-promoter`로 PR 을 직접 발행한다 (build-issues 파이프라인 우회).

### Step 7: 결과 보고 + 세션 통계

```
## QA Boost 결과

### 분석
- 기준 커밋: abc1234 (또는 HEAD~20)
- 분석한 변경 파일: 15개
- 테스트 보강 필요: 5개

### 기록된 ledger task
| 소스 파일 | 테스트 타입 | task id | 상태 |
|-----------|-----------|---------|------|
| src/auth/refresh.rs | unit | a1b2c3d4e5f6 | inserted |
| src/api/handler.rs | unit, e2e | f7e8d9c0b1a2 | duplicate of <existing-id> |
```

> task id 는 fingerprint 의 sha256 첫 12-hex-char. 동일 fingerprint 재실행 시 duplicate 로 흡수되며 무해하다.

**7b. 세션 누적 통계** — 매 cycle 종료 시 세션 통계 업데이트:

- `PROCESSED` = ledger 에 기록된 QA task 수 (insert + duplicate 합 — Step 6 진입 항목 수)
- `SUCCESS` = 신규로 inserted 된 task 수 (`duplicate of ...` 흡수 제외)
- `FAILED` = `task add`가 비-zero exit (id 충돌 등)으로 WARN 된 항목 수
- `FALSE_POSITIVE` = `0` (qa-boost 는 false-positive 분류 단계 없음)

```bash
autopilot stats update --command qa-boost \
  --processed ${PROCESSED} --success ${SUCCESS} --failed ${FAILED} --false-positive ${FALSE_POSITIVE}
autopilot stats show --command qa-boost
```

> `processed=0`이면 `idle_cycles`, `processed>0`이면 `agent_calls` 자동 누적. 통계는 `/tmp/autopilot-{repo}/state/session-stats.json`, 세션 시작 시 `autopilot stats init`으로 초기화.

### 주의사항

- 테스트를 직접 구현하지 않음 — ledger task 로 기록해 work-ledger reader 가 처리.
- GitHub issue 를 생성하지 않음 (ledger-only writer). CI 실패처럼 팀 가시성이 필요한 경로는 ci-watch 가 dual-write 로 유지.
- 동일 fingerprint 는 `task add`가 결정적으로 흡수하므로 별도 중복 검사 불필요 (CLAUDE.md "책임 경계: CLI = 결정적 도구").
- ledger 쓰기 자체는 `|| echo WARN ...` 패턴으로 격리해 한 항목 실패가 나머지 진행을 막지 않음.
