---
paths:
  - "**/agents/*.md"
---

# Plugin Agent 명세 컨벤션

> Agent 명세 파일의 작성 형식 규칙. 설계 원칙(단일 책임, 최소 권한, model 선택)은 `agent-design-principles.md` 참조.

## 원칙

1. **입출력 계약 명시**: 호출하는 측이 무엇을 전달하고 무엇을 기대하는지 명세에 포함한다
2. **description에 호출 컨텍스트 명시**: 어떤 command에서 호출되는지 `(내부용)` prefix로 표기한다
3. **실패/예외 케이스**: 명세에 예외 상황 처리를 포함한다

## DO

역할, model, tools를 적절히 설정하고 입출력 형식을 명시한다:

```markdown
---
description: (내부용) /design 커맨드에서 호출되는 Claude 아키텍처 설계 에이전트
model: opus
color: purple
tools: ["Read", "Glob"]
---

# Claude 아키텍처 설계 에이전트

## 입력 형식

MainAgent로부터 다음 형식의 프롬프트를 받습니다:

\`\`\`
# 아키텍처 설계 요청

## 요구사항
- [FR-1] ...
\`\`\`

## 출력 형식

\`\`\`markdown
# Claude 아키텍처 설계

## 설계 개요
## 주요 컴포넌트
## 리스크 및 고려사항
\`\`\`
```

## DON'T

입출력 계약 없이 명세를 작성하지 않는다:

```markdown
---
description: 코드 분석 에이전트
model: sonnet
tools: ["Read", "Glob"]
---

# Code Analyzer

코드를 분석합니다.
# ↑ 입력 형식, 출력 형식, 예외 처리 모두 누락
```

## 체크리스트

- [ ] `description`이 `(내부용)` prefix와 함께 호출 컨텍스트를 명시하는가?
- [ ] 입력 형식과 출력 형식이 명세에 포함되어 있는가?
- [ ] 실패/예외 케이스 처리가 명세에 언급되어 있는가?
