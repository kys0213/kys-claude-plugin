---
description: "5개 autopilot 루프를 설정된 인터벌로 모두 시작합니다"
argument-hint: ""
allowed-tools: ["Read", "CronCreate"]
---

# Autopilot

5개 autopilot 루프를 설정된 기본 인터벌로 모두 등록합니다.

## 사용법

```bash
/github-autopilot:autopilot
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 작업 프로세스

### Step 1: 설정 로딩

`github-autopilot.local.md`에서 `default_intervals`를 읽습니다.

기본값:
```yaml
default_intervals:
  gap_watch: "30m"
  build_issues: "15m"
  merge_prs: "10m"
  ci_watch: "20m"
  qa_boost: "1h"
```

### Step 2: CronCreate 등록

5개 루프를 순차적으로 CronCreate에 등록합니다:

1. CronCreate: `/github-autopilot:gap-watch` — interval: `{gap_watch}`
2. CronCreate: `/github-autopilot:build-issues` — interval: `{build_issues}`
3. CronCreate: `/github-autopilot:merge-prs` — interval: `{merge_prs}`
4. CronCreate: `/github-autopilot:ci-watch` — interval: `{ci_watch}`
5. CronCreate: `/github-autopilot:qa-boost` — interval: `{qa_boost}`

### Step 3: 결과 출력

등록된 루프 목록을 테이블로 출력합니다:

```
## Autopilot 시작

| Loop | Command | Interval |
|------|---------|----------|
| Gap Watch | /github-autopilot:gap-watch | 30m |
| Build Issues | /github-autopilot:build-issues | 15m |
| Merge PRs | /github-autopilot:merge-prs | 10m |
| CI Watch | /github-autopilot:ci-watch | 20m |
| QA Boost | /github-autopilot:qa-boost | 1h |

5개 루프가 등록되었습니다. CronList로 상태를 확인할 수 있습니다.
```

## 주의사항

- 이미 등록된 동일 루프가 있으면 중복 등록하지 않음
- 세션이 종료되면 모든 cron이 삭제됨 (세션 스코프)
- 개별 루프를 중단하려면 CronDelete 사용
