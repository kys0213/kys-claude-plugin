---
name: plan-to-architect
description: /outline 결과를 team-claude architect로 전달 - Multi-LLM 설계를 Contract 기반 구현으로 연결
argument-hint: "[outline 결과 파일 경로 또는 세션ID]"
allowed-tools: ["Read", "Bash", "Task"]
---

# Plan to Architect

`/outline`으로 생성한 Multi-LLM 아키텍처 설계를 `/team-claude:architect`의 입력으로 전달합니다.

## 사용법

```bash
# outline 결과 파일 직접 지정
/plan-to-architect "plans/architecture-2024-01-15.md"

# 가장 최근 outline 결과 사용
/plan-to-architect

# outline 실행 후 바로 architect로 연결
/outline "채팅 시스템 설계" && /plan-to-architect
```

## 워크플로우

```
┌───────────────────────────────────────────────────────────────────────┐
│  /outline (Planning 플러그인)                                          │
│                                                                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                            │
│  │  Claude  │  │  Codex   │  │  Gemini  │  ← 3개 LLM 병렬 설계        │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘                            │
│       └─────────────┼─────────────┘                                   │
│                     ▼                                                  │
│           ┌─────────────────┐                                         │
│           │ 통합 아키텍처   │  → plans/architecture-*.md              │
│           │ + ASCII 다이어그램│                                        │
│           └────────┬────────┘                                         │
└────────────────────┼──────────────────────────────────────────────────┘
                     │
                     ▼
┌───────────────────────────────────────────────────────────────────────┐
│  /plan-to-architect (이 명령어)                                        │
│                                                                        │
│  • outline 결과 파싱                                                   │
│  • 요구사항/컴포넌트/데이터 흐름 추출                                  │
│  • team-claude:architect 입력 형식으로 변환                            │
└────────────────────┬──────────────────────────────────────────────────┘
                     │
                     ▼
┌───────────────────────────────────────────────────────────────────────┐
│  /team-claude:architect (Team Claude 플러그인)                         │
│                                                                        │
│  • Contract (Interface + Test) 정의                                    │
│  • Task 분할 및 의존성 그래프                                          │
│  • Checkpoint 생성                                                     │
│                                                                        │
│  → .team-claude/sessions/{session-id}/                                 │
│     ├── architecture.md (outline 결과 포함)                            │
│     ├── contracts/                                                     │
│     └── checkpoints/                                                   │
└───────────────────────────────────────────────────────────────────────┘
```

## 작업 프로세스

### Step 1: Outline 결과 찾기

인자가 없으면 가장 최근 outline 결과 검색:

```bash
# plans/ 디렉토리에서 최신 architecture 파일 찾기
ls -t plans/architecture-*.md 2>/dev/null | head -1
```

인자가 있으면 해당 파일 사용

### Step 2: Outline 결과 파싱

```
Read plans/architecture-2024-01-15.md
```

추출할 정보:
- **요구사항 요약**: 기능/비기능 요구사항
- **컴포넌트 목록**: 주요 컴포넌트와 책임
- **데이터 흐름**: 컴포넌트 간 상호작용
- **기술 스택**: 선택된 기술과 근거
- **리스크**: 식별된 리스크

### Step 3: Architect 입력 형식 생성

```markdown
# Team Claude Architect 입력

## 출처
- 생성 방식: Multi-LLM /outline
- 원본 파일: plans/architecture-2024-01-15.md
- 합의 수준: Claude/Codex/Gemini 3개 LLM

## 요구사항

### 기능 요구사항
[outline에서 추출]

### 비기능 요구사항
[outline에서 추출]

## 아키텍처 개요

### 컴포넌트
[outline에서 추출]

### 데이터 흐름
[outline에서 추출]

## 기술 스택
[outline에서 추출]

## 리스크 및 고려사항
[outline에서 추출]

---

위 설계를 기반으로 Contract(Interface + Test)를 정의하고
Task로 분할해주세요.
```

### Step 4: team-claude:architect 호출 안내

변환된 입력을 표시하고 다음 단계 안내:

```
# Outline → Architect 변환 완료

위 내용이 /team-claude:architect 입력으로 준비되었습니다.

다음 단계:
1. 위 내용을 검토하세요
2. /team-claude:architect를 실행하세요
3. 또는 자동으로 진행하려면 'y'를 입력하세요
```

## Multi-LLM 설계의 장점

### Outline 단계 (3개 LLM)
- 다양한 아키텍처 관점
- 기술 선택의 합의
- 리스크 다각도 분석

### Architect 단계 (Main Agent)
- 합의된 설계 기반 Contract 정의
- TDD 방식 테스트 코드 선작성
- 명확한 Task 분할

### Worker 단계 (RALPH + 3개 LLM 리뷰)
- 구현 후 Multi-LLM 코드 리뷰
- 테스트 전 품질 검증
- 자율 피드백 루프

## 주의사항

- **outline 먼저**: /outline 결과가 있어야 사용 가능
- **수정 가능**: 변환 결과를 검토/수정 후 architect 진행
- **세션 연결**: architect가 생성한 세션에 outline 출처 기록
