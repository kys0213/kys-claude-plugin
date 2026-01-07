---
name: review-codex
description: OpenAI Codex를 사용하여 문서를 리뷰합니다
argument-hint: "[리뷰 요청 사항]"
allowed-tools: ["Task", "Glob"]
---

# Codex 리뷰 커맨드

OpenAI Codex를 사용하여 문서를 자연어 기반으로 리뷰합니다.

## 사용법

```bash
/review-codex [자유로운 리뷰 요청]
```

## 워크플로우

1. **사용자 요청 파싱**: 관점, 대상 파일, 컨텍스트 추출
2. **Glob으로 파일 경로 수집** (내용 안 읽음!)
3. **자연어 프롬프트 구성**
4. **plan-reviewer-codex 에이전트에 전달**

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
/review-codex
```
→ 기본 관점(기술 리뷰어)으로 plans/*.md를 리뷰

### 다른 관점
```bash
/review-codex "비즈니스 의사결정자 입장에서 plans를 평가해줘"
```
→ 비즈니스 관점으로 plans/*.md를 평가

### 소설 리뷰
```bash
/review-codex "웹소설 작가 관점에서 1화를 분석해줘"
```
→ 작가 관점으로 1화 분석

## 장점

- **다른 관점**: Claude와 다른 OpenAI 관점 제공
- **자연어**: 말하는 대로 요청
- **토큰 최적화**: 파일 경로만 전달, 스크립트가 파일 읽기

## 주의사항

- **CLI 필요**: codex CLI 설치 필요
- **실행 시간**: 30초-2분 소요
