---
name: feature-workflow
description: 기능 개발 워크플로우 가이드라인. 설계 → 리뷰 → 구현 → 머지의 전체 라이프사이클에 대한 원칙과 모범 사례를 제공합니다.
version: 1.0.0
---

# 기능 개발 워크플로우 가이드라인

이 스킬은 `/develop` 워크플로우의 각 Phase에서 따라야 할 원칙과 모범 사례를 제공합니다.

## 상태 감지 (CRITICAL)

### SessionStart 훅

세션 시작/재개 시 `detect-ralph-state.sh` 훅이 자동 실행됩니다.
훅 출력에 "진행 중인 워크플로우 감지"가 포함되면:

1. `.develop-workflow/state.yaml`을 Read tool로 읽기
2. 사용자에게 현재 상태 보고
3. 이어서 진행할지 `AskUserQuestion`으로 확인

### Compaction 대응

컨텍스트 압축 후 RALPH 워크플로우 진행 상황이 불명확해지면:

1. **즉시** `.develop-workflow/state.yaml`을 Read tool로 읽기
2. 현재 Phase, Checkpoint 상태, iteration 확인
3. passed는 건너뛰고 in_progress부터 재개

**감지 신호**: 다음 중 하나라도 해당하면 state.yaml을 재확인합니다:
- RALPH 워크플로우를 실행 중이었는데 현재 Phase/Checkpoint가 기억나지 않을 때
- Checkpoint 번호나 iteration 횟수가 불확실할 때
- "이전 컨텍스트가 요약되었습니다" 류의 메시지가 보일 때

---

## 핵심 원칙

### 1. Human decides What & Why, Agent decides How

- **인간**: 요구사항 정의, 우선순위 결정, 최종 승인
- **에이전트**: 아키텍처 제안, 구현 방법, 기술 선택

### 2. Contract-Based Parallelization

병렬 구현의 핵심은 Contract(Interface + Test Code)입니다:
- Interface가 명확하면 독립 구현 가능
- Test Code가 있으면 자동 검증 가능
- 잘 정의된 Contract = 충돌 없는 병렬화

### 3. Multi-LLM Consensus

단일 LLM보다 다수 LLM의 합의가 더 신뢰할 수 있습니다:
- 3/3 합의 → 높은 신뢰도
- 2/3 동의 → 검토 필요
- 1/3 지적 → 참고 수준

### 4. RALPH Loop

모든 구현은 RALPH 패턴을 따릅니다:
- **R**ead: Contract, 테스트, 기존 코드 읽기
- **A**nalyze: 요구사항 분석, 패턴 파악
- **L**earn: 코드베이스 컨벤션 학습
- **P**atch: 구현 코드 작성
- **H**alt: 검증 실행 → Pass/Fail

## Phase별 가이드라인

### Phase 1: DESIGN

**목표**: 요구사항을 명확히 하고, 합의된 아키텍처를 도출

**Do**:
- 요구사항부터 시작 (코드가 아닌)
- 모호한 부분은 질문으로 명확화
- 대안도 함께 제시
- ASCII 다이어그램으로 시각화

**Don't**:
- 구체적인 코드 작성 (이 단계에서는)
- 단일 해답 강요
- 기술 선택을 요구사항 없이 결정

### Phase 2: REVIEW

**목표**: 설계의 품질을 검증하고 문제를 조기 발견

**Do**:
- 건설적 피드백 (문제 + 대안)
- 컨센서스 기반 판단
- Critical 이슈는 반드시 해결

**Don't**:
- 주관적 취향으로 판단
- 사소한 이슈에 과도한 시간
- 모든 지적을 동일 무게로 처리

### Phase 3: IMPLEMENT

**목표**: Contract를 만족하는 코드를 효율적으로 구현

**전략 선택 기준**:
- Direct: 단순, 소규모, 파일 겹침
- Subagent: 복수 독립 태스크
- Agent Teams: 대규모, 소통 필요

**Do**:
- Contract(테스트)를 먼저 확인
- 기존 코드 패턴 따르기
- Checkpoint별 검증
- 실패 시 체계적 분석

**Don't**:
- 테스트 없이 구현 완료 선언
- 다른 Checkpoint 파일 수정
- 검증 실패 무시

### Phase 4: MERGE

**목표**: 안전하게 코드를 통합하고 배포 준비

**Do**:
- 코드 리뷰 후 머지
- CI 통과 확인
- 충돌 시 원인 분석 후 해결

**Don't**:
- CI 실패 무시
- 리뷰 없이 머지
- 강제 푸시

## 에스컬레이션 기준

다음 상황에서는 사용자에게 에스컬레이션합니다:

1. **설계 결정 필요**: 기술적으로 동등한 대안이 있고 비즈니스 판단이 필요
2. **RALPH 재시도 초과**: 최대 재시도(3회) 후에도 실패
3. **충돌 해결 불가**: 자동 병합/분석으로 해결할 수 없는 충돌
4. **보안 이슈 발견**: 잠재적 보안 취약점

## git-utils 활용 패턴

| 시점 | 커맨드 | 용도 |
|------|--------|------|
| Phase 3 시작 | `/git-branch` | feature 브랜치 생성 |
| Phase 4 시작 | `/commit-and-pr` | 커밋 + PR |
| Phase 4 중 | `/check-ci` | CI 확인 |
| Phase 4 중 | `/git-resolve` | 충돌 해결 |
| Phase 4 완료 | `/merge-pr` | 최종 머지 |
