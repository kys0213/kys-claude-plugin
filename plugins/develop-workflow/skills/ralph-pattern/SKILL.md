---
name: ralph-pattern
description: RALPH 구현-검증 루프 패턴. Read → Analyze → Learn → Patch → Halt 사이클과 재시도 정책, 에스컬레이션 조건을 정의합니다.
version: 1.0.0
---

# RALPH 패턴

모든 구현 전략(Direct/Subagent/Agent Teams)에서 공통으로 적용되는 구현-검증 루프입니다.

## RALPH Loop

```
┌─→ R: Read     - Contract, 테스트, 기존 코드 읽기
│   A: Analyze  - 요구사항 분석, 패턴 파악
│   L: Learn    - 코드베이스 컨벤션 학습
│   P: Patch    - 구현 코드 작성
│   H: Halt     - 검증 실행
│       │
│       ├── Pass → 완료
│       └── Fail → 원인 분석
│                   │
└───────────────────┘ (최대 3회)
```

## 각 단계 상세

### R: Read (읽기)

**읽어야 할 것**:
1. Contract의 Interface 파일: 구현해야 할 인터페이스
2. Test 파일: 통과해야 할 테스트 (목표 이해)
3. 관련 기존 코드: 패턴과 컨벤션 파악

**핵심**: 코드를 쓰기 전에 충분히 읽기

### A: Analyze (분석)

**분석할 것**:
1. 테스트가 요구하는 동작은 무엇인가?
2. Interface가 기대하는 시그니처는?
3. 엣지 케이스는 무엇인가?
4. 의존성은 어떻게 주입되는가?

**핵심**: 코드를 쓰기 전에 충분히 이해하기

### L: Learn (학습)

**학습할 것**:
1. 프로젝트의 네이밍 컨벤션 (camelCase? snake_case?)
2. 에러 처리 패턴 (throw? Result? Option?)
3. 파일/디렉토리 구조 관습
4. 테스트 작성 스타일
5. import/export 패턴

**핵심**: 기존 코드와 일관된 스타일 유지

### P: Patch (구현)

**구현 순서**:
1. 타입/인터페이스 정의 (있다면)
2. 핵심 로직 구현
3. 에러 핸들링 추가
4. export/연결 코드

**원칙**:
- 가장 단순한 방법으로 시작
- 테스트를 통과하는 최소 구현
- 기존 패턴 따르기
- 불필요한 추상화 금지

### H: Halt (검증)

**검증 절차**:
1. Checkpoint의 `validation.command` 실행
2. 결과를 `validation.expected`와 비교
3. Pass → 다음 Checkpoint으로
4. Fail → 원인 분석

**실패 시 분석**:
1. 에러 메시지 확인
2. 실패한 테스트 케이스 식별
3. 원인 분류:
   - `NOT_IMPLEMENTED`: 미구현 함수/메서드
   - `LOGIC_ERROR`: 로직 오류
   - `TYPE_ERROR`: 타입/컴파일 오류
   - `DESIGN_MISMATCH`: Contract 불일치 (에스컬레이션)
   - `ENV_ISSUE`: 환경 문제 (에스컬레이션)
4. 수정 후 R부터 다시 시작

## 재시도 정책

| 시도 | 범위 | 실패 시 |
|------|------|---------|
| 1회차 | 에러 메시지 기반 수정 | 2회차로 |
| 2회차 | 관련 코드 넓게 분석 후 수정 | 3회차로 |
| 3회차 | Test Oracle 에이전트 호출 | 에스컬레이션 |

## 에스컬레이션 조건

다음의 경우 사용자에게 에스컬레이션합니다:
- 3회 재시도 후에도 실패
- `DESIGN_MISMATCH`: Contract 자체에 문제 가능성
- `ENV_ISSUE`: 환경 설정 문제
- 보안 취약점 발견
