---
paths:
  - ".claude/{commands,agents,skills,rules}/**/*.md"
  - "{commands,agents,skills,rules}/**/*.md"
---

# Agent Design Principles

> 설계 원리의 단일 출처는 **`agent-design-principles` skill**. 이 rule 은 재서술하지 않고,
> `paths` 에 걸린 명세 파일 편집 시 skill 을 따르도록 강제하는 바인딩이다.

## 적용

Command / Skill / Rule 명세를 만들거나 고칠 때 `agent-design-principles` skill 을 로드해 확인한다:

- 컴포넌트 선택: 워크플로우는 Command · Skill · Rule 로 구성한다
- 새 agent 정의(`agents/*.md`)는 만들지 않는다. 격리 실행은 `context: fork` skill 로 한다
- 레이어 매핑·안티패턴 체크리스트 통과
- 항상 적용되는 지식은 skill 이 아니라 CLAUDE.md / rule
