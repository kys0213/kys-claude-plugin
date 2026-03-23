---
paths:
  - "plugins/github-autopilot/**"
---

# Always Pull First

github-autopilot의 모든 agent와 command는 작업 전 반드시 최신 변경사항을 가져와야 합니다.

## 규칙

작업 시작 시 아래 명령을 실행합니다:

```bash
git fetch origin
```

현재 브랜치가 remote tracking 브랜치가 있는 경우:

```bash
git pull --rebase origin $(git branch --show-current)
```

## 적용 대상

- 모든 slash command (gap-watch, build-issues, merge-prs, ci-watch, qa-boost)
- 모든 agent (gap-detector, issue-implementer, pr-merger, etc.)

## 이유

autopilot은 주기적으로 실행되므로, 이전 실행 이후 다른 agent나 사람이 변경한 내용을 반영하지 않으면 충돌이나 중복 작업이 발생합니다. 항상 최신 상태에서 판단해야 정확한 분석과 구현이 가능합니다.
