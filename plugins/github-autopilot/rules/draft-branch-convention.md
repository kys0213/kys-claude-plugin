---
paths:
  - "plugins/github-autopilot/**"
---

# Draft Branch Convention

## 브랜치 네이밍

| 용도 | 패턴 | remote push |
|------|------|-------------|
| `draft/*` | agent 작업용 | 금지 (로컬 only) |
| `feature/*` | PR 생성용 | 허용 |

## 금지 사항

- draft/* 브랜치를 `git push`하지 않는다
- main, develop 브랜치에 직접 커밋하지 않는다
- 기존 feature/* 브랜치를 덮어쓰지 않는다 (이미 존재하면 skip)

## 승격 시 주의

- Quality gate (fmt, lint, test) 통과 후에만 승격한다
- 승격 후 draft 브랜치는 즉시 삭제한다
- PR 라벨에 `{label_prefix}auto`를 반드시 포함한다
