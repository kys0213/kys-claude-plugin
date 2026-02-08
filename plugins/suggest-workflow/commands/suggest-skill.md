---
description: 프롬프트에서 암묵지를 추출하여 skill로 제안
---

# Suggest Skill

사용자 입력 패턴을 분석하여 암묵적 규칙, 컨벤션, 선호사항을 발견하고 skill로 제안합니다.

## 사용법

Rust CLI를 사용합니다:

```bash
# 바이너리가 없으면 자동 다운로드/빌드
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh

${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow skill \
  --project "$(pwd)" \
  --threshold 3 \
  --top 10 \
  --similarity 0.7
```

## 옵션

- `--threshold N`: 최소 반복 횟수 (기본: 3)
- `--top N`: 상위 N개 결과 (기본: 10)
- `--project PATH`: 특정 프로젝트만 분석
- `--report`: 마크다운 리포트 생성
- `--no-clustering`: 유사도 클러스터링 비활성화
- `--similarity N`: 유사도 임계값 (0-1, 기본: 0.7)

## 분석 대상

암묵지(tacit knowledge)는 명시적으로 문서화되지 않은 사용자의 선호, 규칙, 작업 방식을 의미합니다.

| 유형 | 감지 패턴 | 예시 |
|------|----------|------|
| **directive** | "항상", "반드시", "절대", "~해줘" | "항상 타입을 명시해줘" |
| **convention** | "컨벤션", "규칙", "스타일", "포맷" | "커밋은 conventional commit으로" |
| **preference** | "~가 좋아", "~로 해줘", "선호" | "한국어로 답변해줘" |
| **correction** | "아니", "다시", "그게 아니라", "수정" | "아니 Result 패턴으로 해" |

## 실행 흐름

1. **프롬프트 수집**: 프로젝트 세션에서 사용자 입력 추출
2. **패턴 감지**: 정규식 기반 패턴 매칭으로 유형 분류
3. **클러스터링**: 레벤슈타인 거리 기반 유사 의도 그룹화
4. **필터링**: threshold 이상 반복된 패턴만 선택
5. **신뢰도 계산**: 빈도 + 시간적 일관성 기반 confidence 점수
6. **대화형 선택**: AskUserQuestion으로 사용자 확인
7. **Skill 생성**: 선택된 항목을 skill 파일로 저장

## 신뢰도 계산

```
confidence = (frequency_score * 0.7) + (consistency_score * 0.3)

- frequency_score: 전체 프롬프트 중 발생 빈도 (정규화)
- consistency_score: 시간적 분포의 균일성 (낮은 분산 = 높은 일관성)
```

## 출력 예시

```
=== Tacit Knowledge Analysis ===

총 프롬프트: 156개
감지된 패턴: 12개

# 상위 암묵지 패턴

| # | 암묵지 | 유형 | 빈도 | 신뢰도 |
|---|--------|------|------|--------|
| 1 | 한국어로 응답 | preference | 23회 | 95% |
| 2 | TDD 워크플로우 적용 | directive | 12회 | 88% |
| 3 | Conventional Commit 형식 | convention | 8회 | 82% |
| 4 | Result 패턴 사용 | preference | 7회 | 78% |
| 5 | 타입 명시 필수 | directive | 6회 | 75% |

예시:
  - "항상 한국어로 답변해줘"
  - "응답은 한국어로"
  - "한국어가 좋아"

다음 항목을 skill로 생성하시겠습니까?
[y/n] >
```

## Skill 생성 형식

선택된 암묵지는 다음과 같은 skill 파일로 저장됩니다:

**위치**: `.claude/skills/[skill-name].md`

**내용**:
```markdown
---
description: [패턴 설명]
trigger: auto
---

# [Skill 이름]

## 적용 규칙

[추출된 암묵지 내용]

## 발견된 예시

- [예시 1]
- [예시 2]
- [예시 3]

## 자동 적용

이 규칙은 다음과 같은 경우 자동으로 적용됩니다:
[적용 조건]
```

## 활용 사례

### 1. 개인 작업 스타일 자동화
반복적으로 요청하는 작업 방식을 skill로 만들어 매번 명시하지 않아도 자동 적용

### 2. 팀 컨벤션 발견
여러 팀원의 히스토리를 분석하여 공통된 컨벤션 추출

### 3. 프로젝트별 규칙 추출
특정 프로젝트에서만 사용하는 규칙이나 패턴 발견

### 4. 학습 및 개선
자신의 작업 패턴을 분석하여 더 효율적인 워크플로우 설계
