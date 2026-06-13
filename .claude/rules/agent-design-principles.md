---
paths:
  - "**/plugin.json"
---

# Agent Design Principles (repo)

> 설계 원리의 단일 출처는 `agent-design-principles` **skill**. 이 rule 은 재서술하지 않고 참조만 한다.

plugin.json(컴포넌트 등록)을 편집할 때:

- 새 Command / Agent / Skill 은 skill 의 레이어 매핑·안티패턴 체크리스트·진입점 분류(§3.6)·지식 자리(§3.5)를 따른다.
- 명세 형식 규칙: `plugin-skill.md` / `plugin-command.md` / `plugin-agent.md`.
- 도구(결정적 변환) vs 지능(판단) 경계: `tool-layer-boundary.md`.
