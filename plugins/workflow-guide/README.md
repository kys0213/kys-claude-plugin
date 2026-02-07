# workflow-guide

Skill, Sub-agent, Slash Command 구조화 원칙 가이드 플러그인.

[소프트웨어 3.0 시대를 맞이하며](https://toss.tech/article/software-3-0-era) 블로그의 레이어드 아키텍처 매핑 원칙을 기반으로, 에이전트 워크플로우 설계 시 **깨진 유리창 효과**를 방지합니다.

## 문제

워크플로우를 작성하며 Skill, Sub-agent, Slash Command를 만들다 보면:

- Command에 모든 로직을 직접 구현 (Fat Controller)
- 하나의 Agent가 모든 것을 담당 (God Agent)
- Skill을 과도하게 쪼개서 Context 낭비 (Skill Explosion)
- CLAUDE.md에 동적 정보를 넣어서 토큰 낭비

이런 안티패턴이 하나씩 쌓이면서 **깨진 유리창 효과**가 발생합니다.

## 해결

전통적인 레이어드 아키텍처 원칙을 에이전트 설계에 적용합니다:

| 에이전트 | 레이어 | 원칙 |
|---------|--------|------|
| Slash Command | Controller | 진입점만. 로직 위임 |
| Sub-agent | Service | Skill 조합. 독립 Context |
| Skill | Domain / SRP | 단일 책임. 폭발 주의 |
| MCP | Adapter | 외부 인터페이스 캡슐화 |

## 설치

```bash
/workflow-guide:install
```

프로젝트의 `.claude/rules/agent-design-principles.md`에 설계 원칙 룰을 설치합니다. Claude가 새로운 워크플로우를 만들 때 자동으로 이 원칙을 참조하게 됩니다.

## 구성 요소

### Commands

| 이름 | 설명 |
|------|------|
| `/workflow-guide:install` | 프로젝트에 설계 원칙 룰 설치 |

### Skills

| 이름 | 설명 |
|------|------|
| `agent-design-principles` | 레이어드 아키텍처 기반 설계 원칙 전체 가이드 |

### Agents

| 이름 | model | 설명 |
|------|-------|------|
| `workflow-reviewer` | sonnet | Command/Agent/Skill의 설계 원칙 준수 여부 리뷰 |

## 핵심 원칙 요약

### 1. 레이어를 지켜라

```
Slash Command → 입력 파싱, Sub-agent 위임
Sub-agent    → Skill 조합, 워크플로우 완성
Skill        → 단일 책임, 재사용 가능
```

### 2. 토큰을 아껴라

```
Glob으로 경로만 수집 → Sub-agent가 내용을 읽음
결정적 로직 → 스크립트로 분리
CLAUDE.md → 정적 원칙만 기록
```

### 3. 안티패턴을 감시하라

```
Fat Controller  : Command에 로직 직접 구현
God Agent       : 하나의 Agent가 모든 것을 담당
Skill Explosion : 과도한 분리로 Context 낭비
Chatty Context  : CLAUDE.md에 동적 정보
```
