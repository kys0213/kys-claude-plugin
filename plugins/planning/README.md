# Planning Plugin

상위 레벨 설계를 위한 멀티 LLM 플래닝 플러그인입니다. Claude, OpenAI Codex, Google Gemini 3개 LLM을 병렬로 호출하여 다양한 관점의 설계 의견을 수집하고 통합합니다.

## 특징

- **멀티 LLM 설계**: 3개 LLM이 동일한 요청에 대해 각자의 관점으로 분석
- **상위 레벨 집중**: 구체적인 코드가 아닌 아키텍처/기술선택/트레이드오프 분석
- **통합 요약**: 3개 의견을 하나의 종합 리포트로 정리
- **병렬 실행**: 3개 LLM을 동시에 호출하여 시간 단축

## 설치

### 사전 요구사항

- Claude Code CLI
- OpenAI Codex CLI (`codex` 명령어)
- Google Gemini CLI (`gemini` 명령어)

### 마켓플레이스에서 설치

```bash
claude install kys0213/kys-claude-plugin/planning
```

## 사용법

### 기본 사용

```bash
# 기능 설계
/outline "사용자 인증 시스템 구현"

# 기술 선택
/outline "상태 관리 라이브러리 선택"

# 아키텍처 설계
/outline "마이크로서비스로 분리"

# 참고 파일 지정
/outline "인증 시스템 설계. 참고: src/auth/**"
```

### 워크플로우

1. `/outline` 커맨드 실행
2. 3개 LLM(Claude, Codex, Gemini)이 병렬로 설계 분석 수행
3. 각 LLM의 결과를 수집
4. 통합 요약 리포트 생성

### 출력 예시

```markdown
# 상위 레벨 설계 종합 분석

## 핵심 합의사항 (3개 LLM 공통)
- 권장 아키텍처
- 핵심 컴포넌트

## 접근방식 비교
| 접근방식 | Claude | Codex | Gemini | 종합 |
|----------|--------|-------|--------|------|
| A        | 추천   | 추천  | 중립   | 추천 |

## 트레이드오프 분석
...

## 최종 권장사항
...
```

## 컴포넌트

### 커맨드

| 이름 | 설명 |
|------|------|
| `/outline` | 3개 LLM으로 상위 레벨 설계 수행 |

### 에이전트

| 이름 | 설명 |
|------|------|
| `outline-claude` | Claude를 사용한 설계 분석 |
| `outline-codex` | OpenAI Codex를 사용한 설계 분석 |
| `outline-gemini` | Google Gemini를 사용한 설계 분석 |

### 스킬

| 이름 | 설명 |
|------|------|
| `high-level-design` | 상위 레벨 설계 원칙 및 가이드라인 |

## 기존 Plan Mode와의 차이점

| 항목 | 기존 Plan Mode | /outline |
|------|----------------|----------|
| 초점 | 구체적인 코드 수정 | 아키텍처/기술선택 |
| 출력 | 파일별 수정 계획 | 접근방식 비교 |
| 관점 | 단일 (Claude) | 다중 (3개 LLM) |
| 단계 | How (어떻게) | What & Why (무엇을, 왜) |

## 라이선스

MIT
