---
name: review-claude
description: Claude를 사용하여 문서를 리뷰합니다
argument-hint: "[리뷰 요청 사항]"
allowed-tools: ["Task", "Glob"]
---

# Claude 리뷰 커맨드

Claude를 사용하여 문서를 자연어 기반으로 리뷰합니다.

## 사용법

```bash
/review-claude [자유로운 리뷰 요청]
```

## 워크플로우

1. **사용자 요청 파싱**: 관점, 대상 파일, 컨텍스트 추출
2. **Glob으로 파일 경로 수집** (내용 안 읽음!)
3. **자연어 프롬프트 구성**
4. **plan-reviewer-claude 에이전트에 전달**

### 프롬프트 구성

```
컨텍스트:
- 프로젝트: 소설 집필 시스템
- 관점: [파악한 관점]

대상 파일:
- plans/file1.md
- plans/file2.md

사용자 요청:
[원래 사용자 요청]

위 파일들을 리뷰해주세요.
```

## 예시

### 기본 리뷰
```bash
/review-claude
```
→ 기본 관점(기술 리뷰어)으로 plans/*.md를 리뷰

### 관점 지정
```bash
/review-claude "엔지니어 관점으로 리뷰해줘"
```
→ 엔지니어 관점으로 plans/*.md를 리뷰

### 파일 및 관점 지정
```bash
/review-claude "50대 독자 입장에서 1화 소설을 평가해줘"
```
→ 50대 독자 관점으로 novels/*/1화/*.md를 평가

### 복합 요청
```bash
/review-claude "staff+ 엔지니어 관점으로 plans를 리뷰해줘. 우리는 3명 스타트업 팀이야"
```
→ staff+ 엔지니어 관점 + 스타트업 컨텍스트 반영

## 장점

- **빠름**: 외부 API 없이 즉시 실행
- **자연스러움**: 말하는 대로 요청
- **무료**: 추가 비용 없음
- **토큰 최적화**: MainAgent는 파일 경로만 수집
