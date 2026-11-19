---
paths:
  - "**/plugin.json"
---

# Agent Design Principles (repo)

> 설계 원리의 단일 출처는 `agent-design-principles` **skill**. 이 rule 은 재서술하지 않고 참조만 한다.

plugin.json(컴포넌트 등록)을 편집할 때:

- 새 컴포넌트는 Command / Skill 만 등록한다. `agents` 배열에 새 항목을 추가하지 않는다 — 격리 실행이 필요하면 `context: fork` skill 로 만든다 (skill §2).
- 컴포넌트 선택은 skill 의 결정 트리(§6), 진입점 분류는 §3·§4.1, 지식의 자리는 §5 를 따른다.
- 명세 형식 규칙: `plugin-skill.md` / `plugin-command.md` (기존 agent 유지보수 시 `plugin-agent.md`).
- 도구(결정적 변환) vs 지능(판단) 경계: `tool-layer-boundary.md`.
