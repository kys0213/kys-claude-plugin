---
description: "autopilot 루프를 설정된 인터벌로 모두 시작합니다 (기본 6개 + test_watch)"
argument-hint: ""
allowed-tools: ["Read", "Bash"]
---

# Autopilot

autopilot 루프를 `run-loop.sh` + `Bash(run_in_background)` 기반으로 모두 시작합니다.

## 사용법

```bash
/github-autopilot:autopilot
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 작업 프로세스

### Step 1: 설정 로딩

`github-autopilot.local.md`에서 `default_intervals`와 `test_watch`를 읽습니다.

기본값:
```yaml
default_intervals:
  gap_watch: "30m"
  build_issues: "15m"
  merge_prs: "10m"
  ci_watch: "20m"
  ci_fix: "15m"
  qa_boost: "1h"
test_watch: []
```

### Step 1.5: Preflight Check

`autopilot preflight` CLI로 환경을 검증합니다:

```bash
autopilot preflight --config github-autopilot.local.md --repo-root .
```

- Exit 0: 모든 check PASS (WARN 허용) → 계속 진행
- Exit 1: FAIL 항목 있음 → FAIL 항목을 사용자에게 보여주고 `/github-autopilot:setup` 안내 후 중단

### Step 1.6: Spec Quality Gate

설정에서 `spec_paths`와 `spec_quality_threshold`를 읽습니다 (기본값: `"C"`).

`spec_paths`에 스펙 파일이 있으면, `spec-validator` 에이전트를 호출하여 스펙 품질을 평가합니다:

전달 정보:
- spec_files: `spec_paths`에서 `**/*.md`로 수집한 파일 목록
- spec_quality_threshold: 설정값 (기본: `"C"`)

결과 처리:
- **overall_grade >= threshold**: Step 2로 진행
- **overall_grade < threshold**: AskUserQuestion으로 사용자 확인

```
스펙 품질이 {overall_grade} 등급입니다 (기준: {threshold}).
- A (Big Picture): {grade}
- B (Detail): {grade}
- C (Verification): {grade}

자율주행 결과물의 품질을 보장하기 어렵습니다. 계속 진행하시겠습니까?
```

- **Yes**: 경고 로그를 남기고 Step 2로 진행
- **No**: `/spec-kit:design` 또는 `/spec-kit:spec-review` 안내 후 종료

> `spec_paths`에 파일이 없으면 이 단계를 skip합니다 (preflight에서 이미 확인).

### Step 2: 루프 시작

`PLUGIN_ROOT`를 찾습니다 (이 command 파일이 위치한 플러그인의 루트):

```bash
PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:-$(dirname "$(dirname "$0")")}"
SCRIPT="${PLUGIN_ROOT}/scripts/run-loop.sh"
LABEL_PREFIX="{설정에서 읽은 label_prefix}"
LOG_DIR="/tmp/autopilot-$(basename $(git rev-parse --show-toplevel))/logs"
mkdir -p "$LOG_DIR"
```

기본 6개 루프를 `Bash(run_in_background)`로 시작합니다:

1. `Bash(run_in_background)`: `bash "$SCRIPT" "/github-autopilot:gap-watch" "{gap_watch}" "$LABEL_PREFIX" true "$LOG_DIR"`
2. `Bash(run_in_background)`: `bash "$SCRIPT" "/github-autopilot:build-issues" "{build_issues}" "$LABEL_PREFIX" true "$LOG_DIR"`
3. `Bash(run_in_background)`: `bash "$SCRIPT" "/github-autopilot:merge-prs" "{merge_prs}" "$LABEL_PREFIX" true "$LOG_DIR"`
4. `Bash(run_in_background)`: `bash "$SCRIPT" "/github-autopilot:ci-watch" "{ci_watch}" "$LABEL_PREFIX" true "$LOG_DIR"`
5. `Bash(run_in_background)`: `bash "$SCRIPT" "/github-autopilot:ci-fix" "{ci_fix}" "$LABEL_PREFIX" true "$LOG_DIR"`
6. `Bash(run_in_background)`: `bash "$SCRIPT" "/github-autopilot:qa-boost" "{qa_boost}" "$LABEL_PREFIX" true "$LOG_DIR"`

### Step 2.5: Test Watch 루프 시작

`test_watch` 배열이 비어있지 않으면, 각 스위트별 루프를 추가 시작합니다:

```
# test_watch 배열의 각 항목별
Bash(run_in_background): bash "$SCRIPT" "/github-autopilot:test-watch {suite.name}" "{suite.interval}" "$LABEL_PREFIX" true "$LOG_DIR"
```

### Step 3: 결과 출력

시작된 루프 목록을 테이블로 출력합니다:

```
## Autopilot 시작

| Loop | Command | Interval |
|------|---------|----------|
| Gap Watch | /github-autopilot:gap-watch | 30m |
| Build Issues | /github-autopilot:build-issues | 15m |
| Merge PRs | /github-autopilot:merge-prs | 10m |
| CI Watch | /github-autopilot:ci-watch | 20m |
| CI Fix | /github-autopilot:ci-fix | 15m |
| QA Boost | /github-autopilot:qa-boost | 1h |
| Test: e2e | /github-autopilot:test-watch e2e | 2h |

{N}개 루프가 시작되었습니다.
로그: /tmp/autopilot-{repo}/logs/
PID: /tmp/autopilot-{repo}/pids/
```

## 주의사항

- PID 파일 기반 중복 방지: 같은 루프가 이미 실행 중이면 자동 스킵
- 세션 종료 시 background 프로세스도 종료됨
- 개별 루프 PID 확인: `cat /tmp/autopilot-{repo}/pids/{loop-name}.pid`
- 수동 종료: `kill $(cat /tmp/autopilot-{repo}/pids/{loop-name}.pid)`
