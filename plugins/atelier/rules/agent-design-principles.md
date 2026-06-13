---
paths:
  - ".claude/{commands,agents,skills}/**/*.md"
  - "{commands,agents,skills}/**/*.md"
---

# Agent Design Principles

> 설계 원리의 단일 출처는 **`agent-design-principles` skill**. 이 rule 은 재서술하지 않고,
> `paths` 에 걸린 명세 파일 편집 시 skill 을 따르도록 강제하는 바인딩이다.

## 적용

Command / Sub-agent / Skill 명세를 만들거나 고칠 때 `agent-design-principles` skill 을 로드해 확인한다:

- 레이어 매핑·안티패턴 체크리스트 통과
- 진입점 분류 (Command vs user-invocable Skill) — skill §3.6
- 항상 적용되는 지식은 skill 아닌 CLAUDE.md/rule — skill §3.5
