---
description: "autopilot 루프를 설정된 인터벌로 모두 시작합니다 (기본 6개 + test_watch)"
argument-hint: ""
allowed-tools: ["Read", "Bash", "CronCreate"]
---

# Autopilot

autopilot 루프를 `CronCreate` 기반으로 모두 시작합니다.

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

설정의 인터벌을 cron 표현식으로 변환합니다:

| 인터벌 | cron 표현식 |
|--------|------------|
| `"10m"` | `"*/10 * * * *"` |
| `"15m"` | `"*/15 * * * *"` |
| `"20m"` | `"*/20 * * * *"` |
| `"30m"` | `"*/30 * * * *"` |
| `"1h"` | `"7 * * * *"` |
| `"2h"` | `"7 */2 * * *"` |
| `"6h"` | `"7 */6 * * *"` |

> 분 단위(`Nm`)는 `*/N * * * *`, 시간 단위(`Nh`)는 `7 */N * * *` (`:00` 회피).

기본 6개 루프를 `CronCreate`로 등록합니다:

1. `CronCreate(cron: "{gap_watch_cron}", prompt: "/github-autopilot:gap-watch")`
2. `CronCreate(cron: "{build_issues_cron}", prompt: "/github-autopilot:build-issues")`
3. `CronCreate(cron: "{merge_prs_cron}", prompt: "/github-autopilot:merge-prs")`
4. `CronCreate(cron: "{ci_watch_cron}", prompt: "/github-autopilot:ci-watch")`
5. `CronCreate(cron: "{ci_fix_cron}", prompt: "/github-autopilot:ci-fix")`
6. `CronCreate(cron: "{qa_boost_cron}", prompt: "/github-autopilot:qa-boost")`

### Step 2.5: Test Watch 루프 시작

`test_watch` 배열이 비어있지 않으면, 각 스위트별 루프를 추가 등록합니다:

```
# test_watch 배열의 각 항목별
CronCreate(cron: "{suite_interval_cron}", prompt: "/github-autopilot:test-watch {suite.name}")
```

### Step 3: 결과 출력

시작된 루프 목록을 테이블로 출력합니다:

```
## Autopilot 시작

| Loop | Command | Interval | Cron |
|------|---------|----------|------|
| Gap Watch | /github-autopilot:gap-watch | 30m | */30 * * * * |
| Build Issues | /github-autopilot:build-issues | 15m | */15 * * * * |
| Merge PRs | /github-autopilot:merge-prs | 10m | */10 * * * * |
| CI Watch | /github-autopilot:ci-watch | 20m | */20 * * * * |
| CI Fix | /github-autopilot:ci-fix | 15m | */15 * * * * |
| QA Boost | /github-autopilot:qa-boost | 1h | 7 * * * * |
| Test: e2e | /github-autopilot:test-watch e2e | 2h | 7 */2 * * * |

{N}개 루프가 등록되었습니다.
CronList로 확인 가능합니다.
```

## 주의사항

- CronCreate는 REPL이 idle일 때만 실행 — 이전 prompt 실행 중에는 자동 대기
- 세션 종료 시 모든 cron job 자동 삭제
- 7일 후 자동 만료 — 장기 운영 시 재등록 필요
- 수동 해제: `CronDelete(id)` 또는 `CronList`로 확인
