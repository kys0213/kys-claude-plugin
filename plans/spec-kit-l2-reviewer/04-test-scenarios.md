# Test Scenarios — 인용 + 의미 통합 감사 회귀 케이스 + 게이트

## 검증 가능성의 한계

L1 의 mechanical 인용 검증과 달리 **gap-auditor 의 감사는 정답이 단일하지 않다**. 같은 finding 에 대해 사람마다 분류/severity 판단이 갈릴 수 있다. 따라서:

- **단위 테스트로 정량 검증** 은 부적합 (LLM 출력 + 의미 판단)
- **dogfood 정성 검증** + **고정 시나리오 회귀** 의 조합으로 게이트 충족

## 회귀 시나리오 (dogfood 기반)

기존 dogfood 결과 (`05-validation-and-followups.md`) 를 gap-auditor 적용 전/후 비교한다.

### 시나리오 1: belt agent-runtime.md (단일 spec)

| 항목 | gap-auditor 전 | gap-auditor 후 (목표) |
|------|----------------|------------------------|
| L2 findings | 7 (HIGH 1, MEDIUM 2, LOW 1, Notes 4) | 변동 0~1 (대부분 ok) |
| auditor 반복 | N/A | 0~1 회 |
| 사용자 사후 reclassify 요청 | 0 (단일 spec 은 안정) | 0 |

기대: 단일 spec 시나리오는 gap-auditor 가 거의 noop. 비용 증가만 측정.

### 시나리오 2: belt cron-engine.md (다른 도메인)

| 항목 | 전 | 후 |
|------|-----|-----|
| L2 findings | 4 | 변동 0~1 |
| auditor 반복 | N/A | 0~1 회 |

기대: 도메인 안정성 재확인.

### 시나리오 3: belt agent-runtime.md + daemon.md (다중 spec — Test 2 재현)

| 항목 | 전 | 후 (목표) |
|------|-----|-----------|
| spec↔spec gaps | 4 (HIGH 1, MEDIUM 2, LOW 1) | 분류 정정 1~2 건 발생 (M-2) |
| auditor 반복 | N/A | 1~2 회 |
| 사용자 사후 reclassify 요청 | Test 2 에서 1건 (env vars: DEFINITION_CONFLICT → INTERFACE_DRIFT) | 0 (auditor 가 사전 정정) |

**핵심 검증 지점**: Test 2 의 사후 reclassify 1건이 gap-auditor 에 의해 사전에 잡히는지. 잡히면 gap-auditor 의 가치 입증.

### 시나리오 4: 의도적 인용 오류 주입 (M-0)

가짜 L2 output 을 만들어 gap-auditor 가 잡는지 확인:

```
[HIGH] content_type 정의 충돌
- 증거:
  - database.md:S1 — "VARCHAR(255)"
  - proxy.md:S99 — "ENUM('json','multipart','form')"   ← S99 미실재
- 분류: DEFINITION_CONFLICT
```

**기대**: gap-auditor 가 M-0 (INVALID_CITATION) 으로 잡고 인용 정정 또는 finding 제거 권장.

### 시나리오 5: 의도적 misclassify 주입 (M-2)

```
[HIGH] content_type 정의 충돌
- 증거:
  - database.md:S1 — "VARCHAR(255)"
  - proxy.md:S2 — "ENUM('json','multipart','form')"
- 분류: REQUIREMENT_OVERLAP   ← 의도적 오분류
```

**기대**: gap-auditor 가 M-2 (MISCLASSIFICATION) 로 잡고 `DEFINITION_CONFLICT` 로 권장.

### 시나리오 6: 의도적 false positive 주입 (M-6)

```
[HIGH] Rate limiting 미구현 — SPEC_ONLY
- 증거: auth.md:G1 — "all writes are rate-limited"
```

같은 L1 의 `[C5] middleware/throttle.go:30` 이 evidence 로 존재할 때.

**기대**: gap-auditor 가 M-6 (FALSE_POSITIVE) 또는 M-1 (EVIDENCE_CONCLUSION_MISMATCH) 로 잡고 `PARTIAL` 또는 finding 제거 권장.

### 시나리오 7: 의도적 severity 오판 주입 (M-3)

명백한 LOW 사안 (예: 변수명 차이) 을 HIGH 로 표기.

**기대**: gap-auditor 가 M-3 (SEVERITY_MISJUDGMENT) 로 잡고 `LOW` 권장.

### 시나리오 8: 의도적 누락 주입 (M-5)

L1 의 명백한 `CODE_ONLY` gap (`[G3]`) 을 L2 가 누락한 가짜 output.

**기대**: gap-auditor 가 M-5 로 잡고 finding 추가 권장.

## 게이트

| 게이트 | 측정 | 통과 기준 |
|--------|------|-----------|
| gap-auditor 통합 후 회귀 (시나리오 1~3) | 기존 dogfood 와 finding 차이 | 2건 이내 (분류/severity 정정) |
| 의도적 오류 검출 (시나리오 4~8) | gap-auditor 가 잡은 비율 | 5건 모두 잡음 (1회 호출 내) |
| 평균 추가 호출 비용 | gap-auditor + L2 재호출 | sonnet 2~3회 추가 |
| 무한 루프 회귀 | 고의 모순 입력으로 3회 도달 | 종료 후 drop log 노출 |
| 사용자 가시 drop | 잔여 major 가 사용자에게 표시 | 노출 |
| mechanical L2 검증 제거 회귀 | 기존 dogfood 의 L2 인용 drop 케이스가 M-0 으로 잡히는지 | 기존 drop 케이스의 ≥ 80% M-0 으로 검출 |

게이트 충족은 **dogfood 1회 + 의도 시나리오 5건** 통과로 합격 처리.

## 비용/지연 측정

gap-auditor 추가는 비용 증가가 있다. 측정 항목:

- L2 호출 횟수 (1 → 평균 1.x)
- gap-auditor 호출 횟수 (0 → 1.x)
- wall-clock (Step 5 → Step 6 → Step 5 재호출 등)
- 토큰 (가능하면 — Task tool 의 응답에서 추출 가능 여부 후속 확인)

이 측정은 별도 follow-up (C3) 에서 wall-clock + 호출 횟수 footer 와 묶어 처리.

## dogfood 절차

1. C6 plan 머지 후 PR 로 gap-auditor 에이전트 + Step 6 통합 게이트 구현
2. belt 의 `agent-runtime.md`, `cron-engine.md`, `agent-runtime.md + daemon.md` 3개 시나리오 재실행
3. 의도 시나리오 4~8 은 가짜 L1 reports + 가짜 L2 output 을 fixture 로 만들지 않고, 실제 dogfood 중 발견된 finding 의 일부를 손으로 변형해서 orchestrator 에 다시 입력하여 gap-auditor 단독 호출로 검증
4. 결과는 별도 dogfood note 로 기록

## 무한 루프 회귀

gap-auditor 가 고의로 같은 major 를 반복 보고하는 경우 (또는 L2 가 fix 를 거부하는 경우):

- iter == 3 도달 시 강제 종료
- `prev_major_ids == curr_major_ids` 시 종료
- 종료 후 잔여 major 는 drop log 로 노출

회귀 테스트: 모순된 prompt (예: "분류는 X 인데 X 가 아니다") 를 강제로 만들어 종료 조건이 발동하는지.

## 실패 시 fallback

gap-auditor 단계 자체가 실패 (호출 오류, 출력 파싱 실패 등) 시:

1. 1회 retry
2. 2회째도 실패 → gap-auditor 단계 skip + 사용자에게 "감사 단계 미수행" 알림
3. L2 의 raw output 을 그대로 최종 리포트로 출력 (mechanical 검증도 제거됐으므로 이때는 검증 부재)
4. 사용자에게 "이번 실행은 audit 미수행" 명시적 노출

gap-auditor 가 옵션 단계가 되도록 설계 — 실패가 전체 명령 실패로 이어지지 않음. 단 사용자에게 검증 부재를 분명히 알려야 한다.

## 다음 단계 (구현 PR 시)

- gap-auditor.md 에이전트 작성
- spec-review.md / gap-detect.md 의 Step 6 (mechanical L2 인용 검증) 을 gap-auditor 호출로 교체
- plugin.json 에이전트 목록 갱신
- belt dogfood 1회 + 의도 시나리오 5건
- 결과를 spec-kit-l2-reviewer/05-validation.md 로 추가 PR 또는 본 PR 에 합산
