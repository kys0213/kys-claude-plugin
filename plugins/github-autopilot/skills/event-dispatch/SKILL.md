---
name: event-dispatch
description: "Monitor 이벤트를 수신하여 적절한 autopilot 커맨드로 디스패치하는 규칙"
---

# Event Dispatch

Monitor 도구에서 수신한 이벤트 라인을 파싱하여 적절한 autopilot 커맨드를 실행합니다.

## 이벤트 형식

모든 이벤트는 `EVENT_TYPE key=value key=value ...` 형식의 단일 라인입니다.

## 디스패치 규칙

### MAIN_UPDATED

origin/main에 새 커밋이 감지되었을 때 발생합니다.

```
MAIN_UPDATED before=<sha> after=<sha> count=<N>
```

**액션**: 다음 두 커맨드를 **순차** 실행합니다:
1. `/github-autopilot:gap-watch`
2. `/github-autopilot:qa-boost {before}`

> gap-watch는 전체 스펙-코드 갭 분석, qa-boost는 `before` SHA 이후 변경분만 분석합니다.

### CI_FAILURE

GitHub Actions workflow가 실패로 완료되었을 때 발생합니다.

```
CI_FAILURE run_id=<id> workflow=<name> branch=<branch>
```

**액션**: 브랜치 종류에 따라 분기합니다:
- **default branch** (main/master): `/github-autopilot:ci-watch --run-id={run_id} --branch={branch}`
- **autopilot branch** (feature/issue-*, draft/issue-*): `/github-autopilot:ci-fix --branch={branch}`

### CI_SUCCESS

GitHub Actions workflow가 성공으로 완료되었을 때 발생합니다.

```
CI_SUCCESS run_id=<id> workflow=<name> branch=<branch>
```

**액션**: autopilot 브랜치인 경우에만 처리합니다:
- **autopilot branch**: `/github-autopilot:merge-prs --branch={branch}`
- **기타**: 무시 (정상 동작)

### NEW_ISSUE

autopilot 라벨이 없는 새 이슈가 감지되었을 때 발생합니다.

```
NEW_ISSUE number=<N> title=<title>
```

**액션**: `/github-autopilot:analyze-issue {number}`

## 주의사항

- 이벤트는 Monitor 알림으로 대화에 도착합니다. 각 이벤트에 대해 해당 커맨드를 즉시 실행합니다.
- 동시에 여러 이벤트가 도착하면 (200ms 이내 배치), 독립적인 이벤트는 병렬로 디스패치합니다.
- 같은 브랜치에 대한 CI_FAILURE와 CI_SUCCESS가 동시에 도착하면, 최신 이벤트(CI_SUCCESS)만 처리합니다.
- pipeline idle check는 각 커맨드 내부에서 수행하므로 디스패치 단계에서는 체크하지 않습니다.
