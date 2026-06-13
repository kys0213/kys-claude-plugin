---
paths:
  - ".claude/{commands,agents,skills}/**/*.md"
  - "{commands,agents,skills}/**/*.md"
---

# Agent Design Principles

> 설계 원리(레이어드 매핑·토큰 관리·안티패턴·skill↔rule 경계·진입점 분류)의 단일 출처는
> **`agent-design-principles` skill** 이다. 이 rule 은 그 원리를 *재서술하지 않고*,
> `paths` 에 걸린 명세 파일을 편집할 때 skill 을 따르도록 강제하는 얇은 바인딩이다.
> (skill ↔ rule 을 이렇게 두는 이유 자체가 skill §3.5 의 "암묵지 vs 컨벤션" 경계다.)

## 적용

Command / Sub-agent / Skill 명세를 만들거나 고칠 때:

1. **레이어 확인** — `agent-design-principles` skill 을 로드해 레이어 매핑과 안티패턴 체크리스트를 통과하는지 본다.
2. **진입점 분류** — 새 진입점은 "모델이 자동 트리거해도 되나?"로 Command(User 전용) vs user-invocable Skill(User+모델)을 가른다 (skill §3.6).
3. **지식의 자리** — 항상 적용돼야 하는 코딩 원칙은 skill 이 아니라 CLAUDE.md/rule(자동 주입)에 둔다 (skill §3.5).
