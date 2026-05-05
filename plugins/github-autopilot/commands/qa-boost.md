---
description: "최근 변경사항의 테스트 커버리지를 분석하고 누락된 테스트를 ledger task로 기록합니다"
argument-hint: "[commit_hash]"
allowed-tools: ["Bash", "Glob", "Read", "Grep"]
---

# QA Boost

최근 변경사항을 QA 관점에서 분석하고, 누락된 테스트를 결정적 ledger의 `qa-backlog` epic에 task로 기록합니다. work-ledger reader가 task를 claim하여 build-issues 파이프라인 없이 직접 PR을 발행합니다.

## 사용법

```bash
/github-autopilot:qa-boost                    # 최근 20커밋 기준, 1회 실행
/github-autopilot:qa-boost abc1234            # 특정 커밋 이후 변경 분석
```

> 반복 실행은 `/github-autopilot:autopilot`이 `CronCreate`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`
- 최근 커밋: !`git log --oneline -5`

## 작업 프로세스

### Step 1: 인자 파싱

`$ARGUMENTS`에서 commit_hash를 추출합니다.
- `/^[0-9a-f]{7,40}$/` 패턴 매칭 → commit_hash
- 비어있으면 → 최근 20커밋 기준

### Step 2: Base 브랜치 동기화

**branch-sync** 스킬의 절차를 수행합니다.

### Step 2.5: Pipeline Idle Check

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — qa-boost cycle 중단" 알림 발송 후 종료합니다.
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다.
- **exit 1 (active)**: Step 3부터 정상 진행.

### Step 2.7: Idle Count Check

이전 Step의 결과가 "대상 없음"(idle)이면, 연속 idle 횟수를 기록합니다.

```bash
autopilot check mark qa-boost --status idle
```

설정에서 `idle_shutdown.max_idle` 값을 읽습니다 (기본값: 5).

연속 idle 횟수가 `max_idle` 이상이면:
1. `autopilot cron self-delete --name "qa-boost"` 로 cron을 자동 해제합니다.
2. "연속 {N}회 idle — cron 자동 해제" 메시지를 출력하고 종료합니다.

실제 작업을 수행하면 idle count를 리셋합니다:
```bash
autopilot check mark qa-boost --status active
```

### Step 3: 변경사항 수집

#### commit_hash가 있는 경우

```bash
git diff ${commit_hash}..HEAD --name-only
git log ${commit_hash}..HEAD --oneline --format="%H %s"
```

#### commit_hash가 없는 경우

```bash
git log --oneline -20 --format="%H %s"
git diff --name-only HEAD~20..HEAD 2>/dev/null || git diff --name-only $(git rev-list --max-parents=0 HEAD)..HEAD
```

변경 파일에서 테스트 파일(`*_test.*`, `*_spec.*`, `test_*`, `tests/`, `__tests__/`, `e2e/`, `bench/`, `benches/`)을 제외한 소스 파일만 추출합니다.

### Step 4: 테스트 매핑 분석

각 소스 파일에 대해 대응하는 테스트 파일이 있는지 Glob으로 확인합니다:

```
src/auth/mod.rs      → tests/auth_test.rs ✅ (존재)
src/auth/refresh.rs  → tests/refresh_test.rs ❌ (없음)
src/api/handler.rs   → tests/api_test.rs ⚠️ (있지만 handler 관련 테스트 없음)
```

테스트가 충분한 파일은 제외하고, 보강이 필요한 파일을 그룹화합니다.

보강 대상이 없으면 `autopilot check mark qa-boost --status idle` 후 "테스트 보강 대상 없음" 출력 후 Step 7로 이동.

### Step 5: Ledger Epic 부트스트랩

ledger 쓰기 직전에, 결정적 ledger의 `qa-backlog` epic이 존재하도록 한 번만 보장합니다 (idempotent).

```bash
EPIC_NAME="qa-backlog"
EPIC_SPEC="spec/qa-backlog.md"
if ! autopilot epic create --name "$EPIC_NAME" --spec "$EPIC_SPEC" --idempotent; then
  echo "ERROR: qa-backlog epic 부트스트랩 실패 — 이번 cycle 중단" >&2
  exit 1
fi
```

> ledger는 qa-boost의 유일한 출력 경로이므로 epic 부트스트랩 실패 시 cycle을 중단하고 다음 tick에서 재시도합니다 (gap-watch와 동일한 ledger-only 패턴).

### Step 6: Ledger Task 기록

ledger 쓰기를 시작하기 전에 idle count를 리셋합니다: `autopilot check mark qa-boost --status active`

테스트 보강이 필요한 각 항목에 대해 ledger task를 기록합니다. 동일 fingerprint는 `task add`가 idempotent하게 흡수하므로 별도 dedup 단계가 필요 없습니다.

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
> - 기존 task id (재실행): `task '<id>' already exists` (exit 1, no-op) — `|| echo WARN ...` 가 흡수합니다
>
> task가 기록되면 `/github-autopilot:work-ledger` reader가 cron tick에 claim하여 `issue-implementer` → `branch-promoter`로 PR을 직접 발행합니다 (build-issues 파이프라인 우회).

### Step 7: 결과 보고

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

> task id는 fingerprint의 sha256 첫 12-hex-char입니다. 동일 fingerprint 재실행 시 duplicate로 흡수되며 무해합니다.

## 주의사항

- 테스트를 직접 구현하지 않음 — ledger task로 기록하여 work-ledger reader가 처리
- GitHub issue를 생성하지 않음 (ledger-only writer). CI 실패처럼 팀 가시성이 필요한 경로는 `ci-watch`가 dual-write로 유지합니다.
- 동일 fingerprint는 `task add`가 결정적으로 흡수하므로 별도 중복 검사가 필요 없음 (CLAUDE.md "책임 경계: CLI = 결정적 도구")
- ledger 쓰기 자체는 `|| echo WARN ...` 패턴으로 격리하여 한 항목 실패가 나머지 진행을 막지 않음
