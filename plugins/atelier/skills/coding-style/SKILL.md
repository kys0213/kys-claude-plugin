---
name: coding-style
description: 코딩 스타일·컨벤션 원칙. 코드를 작성하거나 리뷰할 때 CLAUDE.md 코딩 원칙을 적용하거나, 변경 후 단순화(/simplify)를 제안할 때 사용합니다.
---

# coding-style

프로젝트의 코딩 스타일과 컨벤션 원칙을 제공합니다. atelier 의 다른 skill/agent(예: `workflow`, `codebase-analyzer`)가 코딩 원칙을 참조할 때 이 skill 을 로드합니다.

## 코딩 원칙 템플릿

설치 시 `~/.claude/CLAUDE.md` 에 병합되는 코딩 원칙 템플릿 원본:

- `${CLAUDE_PLUGIN_ROOT}/templates/claude-md/CLAUDE.md`

atelier setup 의 `style` 모듈이 워터마크 기반 중복 확인 후 사용자 CLAUDE.md 에 병합합니다.

## 변경 후 단순화 제안

`suggest-simplify.sh` (Stop hook) 가 변경을 감지하면 `/simplify` 워크플로우를 제안합니다.
hook 원본: `${CLAUDE_PLUGIN_ROOT}/hooks/suggest-simplify.sh`

## 적용 지침

- 새 코드/리뷰 시 템플릿의 원칙(네이밍·주변 코드 일관성·SRP 등)을 따릅니다.
- 변경량이 많아지면 단순화 가능 지점을 우선 점검합니다.
