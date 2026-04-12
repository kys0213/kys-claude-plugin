---
paths:
  - "**/agents/*.md"
---

# Plugin Agent 명세 컨벤션

> Agent는 단일 책임을 가진 서비스 레이어다. 입출력 계약, model 선택, 실패 처리를 명세에서 명확히 한다.

## 원칙

1. **단일 책임**: agent 하나는 하나의 역할만 수행한다. 이름이 역할을 즉시 드러내야 한다
2. **최소 권한**: `tools`에는 실제로 필요한 도구만 선언한다 (읽기 전용 agent에 Write 불필요)
3. **model 적합성**: 복잡한 추론은 `opus`, 코드 분석/리뷰는 `sonnet`, 단순 분류/변환은 `haiku`
4. **입출력 계약 명시**: 호출하는 측이 무엇을 전달하고 무엇을 기대하는지 명세에 포함한다

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

모든 도구를 허용하거나, 입출력 계약 없이 애매한 역할을 정의하지 않는다:

```markdown
---
description: 도움을 주는 에이전트  ← 역할 불명확
model: opus                          ← 단순 작업에 opus는 낭비
tools: ["Read", "Write", "Bash", "Glob", "Grep", "Task", "Edit"]  ← 과도한 권한
---

# Helper

뭐든 도와드립니다.  ← 입출력 계약 없음, 단일 책임 위반
```

## 체크리스트

- [ ] `description`이 어떤 command에서 호출되는지, 무슨 역할인지 명시하는가?
- [ ] `model`이 작업 복잡도에 적합한가? (분류/파싱 → haiku, 코드 분석 → sonnet, 설계 → opus)
- [ ] `tools`에 실제로 필요한 도구만 포함되었는가? (Write 없이 읽기만 하면 Read/Glob/Grep만)
- [ ] 입력 형식과 출력 형식이 명세에 포함되어 있는가?
- [ ] 실패/예외 케이스 처리가 명세에 언급되어 있는가?
