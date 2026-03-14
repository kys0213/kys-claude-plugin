# Flow 4: 다중 스펙 우선순위

### 시나리오

하나의 레포에 여러 스펙이 Active 상태로 존재한다.

### 우선순위 결정

```
Case 1: 독립적인 스펙들
  → Claw가 concurrency 범위 내에서 병렬 진행
  → 각 스펙의 이슈가 서로 다른 파일/모듈을 수정하면 충돌 없음

Case 2: 의존 관계 있는 스펙들
  → Claw가 의존성 판단 → 선행 스펙 우선 처리
  → 후행 스펙은 Paused 또는 이슈 생성 보류

Case 3: 충돌하는 스펙들
  → 같은 파일/모듈을 수정하는 스펙
  → Claw: HITL 요청 "두 스펙이 충돌합니다. 우선순위를 지정해주세요."
```

### 사용자 개입 방법

```bash
# 수동 우선순위 지정
autodev spec prioritize <spec-id-1> <spec-id-2> ...

# 특정 스펙 일시정지
autodev spec pause <spec-id>
```

### Claw의 판단 기준

Claw는 claw-workspace의 `skills/prioritize/SKILL.md`에 정의된 전략을 기반으로 판단한다.
기본 제공되는 판단 기준:

1. **의존성**: 스펙 A의 결과물이 스펙 B의 전제조건인지
2. **충돌**: 같은 파일을 수정하는 이슈가 있는지
3. **리소스**: 현재 concurrency 여유가 있는지
4. **진행도**: 이미 이슈가 생성/진행 중인 스펙 우선

이 기준은 `prioritize/SKILL.md`를 수정하여 자연어로 커스터마이즈할 수 있다.
