---
paths:
  - "**/skills/**/SKILL.md"
---

# Plugin Skill 명세 컨벤션

> SKILL.md 파일의 작성 형식 규칙. 설계 원칙(SRP, 정적 지식, Skill 폭발 방지)은 `agent-design-principles.md` 참조.

## 원칙

1. **Frontmatter 필수**: `name`, `description`, `version`을 반드시 설정한다
2. **섹션 체계**: 원칙 → 프로세스 → 예시 → 출력 형식 순서로 구조화한다

## DO

frontmatter에 메타데이터를 명시하고, 번호 있는 섹션으로 도메인 지식을 구조화한다:

```markdown
---
name: review
description: 문서 및 코드 리뷰 가이드라인. 건설적이고 구체적인 리뷰를 위한 원칙, 관점별 평가 기준, 출력 형식을 제공합니다.
version: 1.0.0
---

# 리뷰 가이드라인

## 리뷰 철학

### 5대 원칙

1. **건설적 (Constructive)**: 문제 지적 시 반드시 대안 제시
2. **구체적 (Specific)**: 모호한 표현 대신 수치와 예시

## 기본 평가 기준

| 기준 | 설명 | 배점 |
|------|------|------|
| 완성도 | 필요한 정보가 모두 있는가 | 20 |

## 출력 형식

\`\`\`markdown
## 리뷰 결과
### 점수: XX/100
\`\`\`
```

## DON'T

Skill을 과도하게 세분화하거나, 동적 정보를 SKILL.md에 포함하지 않는다:

```markdown
---
name: jwt-validation  ← 너무 좁은 범위, auth-patterns으로 통합해야 함
description: JWT 검증
version: 1.0.0
---

# JWT 검증

현재 진행 중인 이슈: AUTH-123  ← 동적 정보 포함 금지
오늘의 우선순위: 토큰 만료 처리  ← 대화로 전달해야 할 정보
```

## validate 요구사항

CI의 `validate` 도구가 다음을 검증합니다:
- `name`, `description` frontmatter 필수
- 콘텐츠 최소 50줄 이상, 최대 500줄 이하
- 민감 데이터 패턴(password, api_key, token, secret) 포함 금지

## 체크리스트

- [ ] `name`, `description`, `version` frontmatter가 모두 설정되어 있는가?
- [ ] Skill이 하나의 도메인만 다루는가? (과도한 분리는 없는가?)
- [ ] 동적으로 변하는 정보가 없는가?
- [ ] 콘텐츠가 50줄 이상, 500줄 이하인가?
- [ ] 민감 데이터가 포함되지 않았는가?
