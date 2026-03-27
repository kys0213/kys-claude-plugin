---
description: "설정된 테스트 스위트를 주기적으로 실행하고 실패 시 이슈를 생성합니다"
argument-hint: "<test_name> [interval]"
allowed-tools: ["Bash", "Read", "Agent", "CronCreate", "CronDelete", "CronList"]
---

# Test Watch

설정된 테스트 스위트(e2e, 성능 테스트 등)를 주기적으로 실행하고, 실패 시 GitHub issue를 생성하여 autopilot 파이프라인에 연결합니다.

## 사용법

```bash
/github-autopilot:test-watch                    # 설정된 모든 스위트 1회 실행 + CronCreate 등록
/github-autopilot:test-watch e2e                # e2e 스위트만 1회 실행
/github-autopilot:test-watch e2e 2h             # e2e 스위트를 2시간마다 반복
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -30 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 1: 인자 파싱

`$ARGUMENTS`에서 test_name과 interval을 추출합니다.
- `{test_name}` → 특정 스위트 1회 실행
- `{test_name} {interval}` → 특정 스위트 반복 실행
- 비어있으면 → 전체 스위트 1회 실행 + 각 스위트별 CronCreate 등록

### Step 2: 최신 상태 동기화

```bash
git fetch origin
```

### Step 2.5: Pipeline Idle Check

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/check-idle.sh "{label_prefix}"
```

- **exit 0 (idle)**: 기존 cron을 정리한 뒤 종료합니다.
  1. CronList로 현재 등록된 cron 목록을 조회
  2. `test-watch`가 포함된 cron job을 찾아 CronDelete로 삭제
  3. `notification` 설정이 있으면 "autopilot 파이프라인 완료 — test-watch cycle 중단" 알림 발송
  4. CronCreate를 등록하지 않고 종료
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다 (CronCreate는 등록하여 다음 cycle에서 재시도).
- **exit 1 (active)**: Step 3부터 정상 진행.

### Step 3: 테스트 스위트 로딩

설정에서 `test_watch` 배열을 읽습니다:

```yaml
test_watch:
  - name: "e2e"
    command: "npm run test:e2e"
    interval: "2h"
  - name: "performance"
    command: "cargo bench"
    interval: "6h"
    threshold: 10
```

- test_name이 지정되면 → 해당 스위트만 필터링
- test_name이 없으면 → 전체 스위트
- `test_watch`가 비어있으면 "테스트 스위트가 설정되지 않았습니다" 출력 후 종료

### Step 4: 테스트 실행 (Agent Team)

각 스위트에 대해 test-analyzer 에이전트를 호출합니다:

**스위트 수가 3개 이하**: 순차 호출 (background=false)
**스위트 수가 4개 이상**: 병렬 호출 (background=true)

각 에이전트에게 전달:
- test_name
- command
- threshold (기본: 0)
- label_prefix

### Step 5: 결과 수집 및 이슈 생성

각 에이전트 결과를 처리합니다:

**passed / within_threshold**: 기록만 남김

**failed**: 이슈 생성 프로세스 진행

1. fingerprint 중복 확인:
   ```bash
   bash ${CLAUDE_PLUGIN_ROOT}/scripts/check-duplicate.sh "test:${test_name}:${failure_hash}"
   ```

2. 중복이 아닌 경우 이슈 생성:
   ```bash
   gh issue create \
     --title "fix: ${test_name} test failure - ${summary}" \
     --label "{label_prefix}ready" \
     --body "$(cat <<'EOF'
   ## 테스트 실패 분석

   - **Suite**: ${test_name}
   - **Command**: ${command}
   - **Failure type**: ${failure_type}
   - **Failed tests**: ${failure_count}개

   ## 실패 상세

   ${failures_detail}

   ## 수정 제안

   ${suggested_fix}

   ---
   <!-- fingerprint: test:${test_name}:${failure_hash} -->
   EOF
   )"
   ```

### Step 6: CronCreate

**특정 스위트 + interval 지정**: 해당 스위트만 CronCreate 등록
```
CronCreate: /github-autopilot:test-watch {test_name} — interval: {interval}
```

**전체 실행 모드 (인자 없음)**: 각 스위트별 개별 CronCreate 등록
```
CronCreate: /github-autopilot:test-watch {suite.name} — interval: {suite.interval}
```

이렇게 하면 각 스위트가 자신의 interval에 따라 독립적으로 실행됩니다.

### Step 7: 결과 보고

```
## Test Watch 결과

### 실행된 스위트
| Suite | Status | Details |
|-------|--------|---------|
| e2e | failed | 3 tests failed → issue #55 created |
| performance | passed | 42 tests, 2m 30s |

### 생성된 이슈
- #55: fix: e2e test failure - login flow timeout

### CronCreate 등록
- test-watch e2e → 2h interval
- test-watch performance → 6h interval
```

## 주의사항

- issue-label 스킬의 라벨 필수 규칙과 fingerprint 규칙을 반드시 따른다
- environment_error 분류의 실패는 이슈를 생성하지 않는다 (일시적 환경 문제)
- 토큰 최적화: MainAgent는 설정 로딩과 이슈 생성만 수행, 테스트 실행/분석은 모두 Agent에 위임
- 각 스위트가 독립적 CronCreate를 가지므로, 스위트 추가/제거 시 CronDelete로 정리 필요
