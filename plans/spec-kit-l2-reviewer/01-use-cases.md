# Use Cases — 어떤 의미 오류를 잡는가

reviewer 가 잡아야 하는 시나리오를 카테고리별로 정리한다. 모든 예시는 dogfood 또는 가상 시나리오에 기반한다. major/minor 분류 기준의 근거이다.

## Major (루프 발생)

### M-1. 증거-결론 불일치

L2 가 인용한 L1 항목이 실제로는 L2 의 결론을 뒷받침하지 않는 경우.

**예시**:

```
[HIGH] content_type 정의 충돌
- 증거:
  - database.md:S1 — "content_type 컬럼은 VARCHAR(255)"
  - proxy.md:S2 — "content_type 은 ENUM('json','multipart','form')"
- 분류: DEFINITION_CONFLICT
```

여기서 인용은 정확하지만, proxy.md:S2 가 실제로는 "HTTP request body 의 content type" 을 의미하고 database.md:S1 은 "DB 컬럼" 을 의미한다면 (L1 의 다른 항목에서 그 맥락이 분명하다면) 두 인용은 같은 도메인이 아니다 → 충돌이 아님.

reviewer 는 L1 reports 의 주변 항목을 보고 컨텍스트 일치 여부를 확인한다.

### M-2. 분류 오류

`Code↔Spec`: `SPEC_ONLY` / `CODE_ONLY` / `PARTIAL` / `DIVERGENT`
`Spec↔Spec`: `DEFINITION_CONFLICT` / `INTERFACE_DRIFT` / `TERM_AMBIGUITY` / `REQUIREMENT_OVERLAP`

**예시** (Test 2 의 실제 사례):

```
[HIGH] 환경변수 주입 범위 충돌
- 분류: DEFINITION_CONFLICT
```

실제는 daemon.md 가 자식 프로세스 환경변수 책임을 명시하지 않은 것이고, code 는 추가 변수를 주입한다. → `INTERFACE_DRIFT` (책임 경계 표류) 가 더 정확. reviewer 가 분류 기준 (03-detailed-spec.md) 에 맞춰 비평한다.

### M-3. 심각도 오판

severity 는 finding 이 사용자에게 주는 의미를 결정한다. reviewer 는 다음 휴리스틱으로 검토:

- HIGH: production 동작에 직접 영향, 데이터/보안 위험, spec 의 핵심 약속 위반
- MEDIUM: 동작 영향은 제한적이지만 spec 와 code 가 의미 있게 발산
- LOW: 표현/명명 불일치, 누락된 doc, 무해한 변동

L2 가 LOW 사안을 HIGH 로 보고하거나 그 반대인 경우 → major.

### M-4. False positive (실재하지 않는 갭)

L1 에서 cited 된 evidence 만 보면 갭처럼 보이지만, 같은 L1 의 다른 항목에 해당 구현이 명시되어 있는 경우.

**예시**:

```
[HIGH] Rate limiting 미구현 — SPEC_ONLY
- 증거: auth.md:G1 — "all writes are rate-limited"
```

같은 L1 리포트의 `[C5] middleware/throttle.go:30` 이 `func ThrottleWriteRequests(...)` 를 가리키고 있다면 SPEC_ONLY 가 아니다 → `PARTIAL` 또는 finding 자체 제거.

reviewer 는 인용된 L1 항목 외에 **같은 L1 리포트의 모든 항목** 을 cross-check.

### M-5. False negative (명백한 누락)

L1 에 갭 evidence 가 있으나 L2 가 finding 을 만들지 않은 경우.

**예시**:

L1 의 `[G3] CODE_ONLY — internal/auth/api_key.go:20` 이 spec 미언급으로 표시되어 있는데 L2 의 Code↔Spec Gaps 에 해당 finding 이 없다면 reviewer 가 "추가 권장" 으로 표시.

이 시나리오는 **보수적으로 처리** — 명백한 L1 evidence 가 있을 때만 reviewer 가 missing-finding 을 major 로 표시. 모호한 경우는 minor 로.

### M-6. 중복 finding

서로 다른 finding ID 가 사실상 같은 사안인 경우. reviewer 는 dedupe 권장.

## Minor (루프 미발생, 노출만)

### m-1. 표현 모호

권장 액션이 불명확 ("적절히 수정"), 증거 인용이 부족하지만 결론 자체는 타당.

### m-2. 추가 증거 권장

현재 인용으로 충분하지만 L1 에 더 강한 evidence 가 있어 추가하면 좋음.

### m-3. 권장 액션 부정확

분석은 타당한데 권장 조치가 비현실적이거나 더 나은 대안이 있음.

## 비-issue (reviewer 가 건드리지 않음)

- L2 가 명확하게 잘 분류한 finding
- 인용이 정확하고 결론이 증거에 부합
- severity 가 합리적 범위

## 대표 시나리오 매트릭스

| 시나리오 | 종류 | reviewer 의 대응 |
|----------|------|------------------|
| dogfood Test 2 #1 (env vars) | M-2 | INTERFACE_DRIFT 로 reclassify 권장 |
| 가상 content_type | M-1 | 도메인 컨텍스트 mismatch — finding 제거 권장 |
| 가상 rate-limiting (false positive) | M-4 | SPEC_ONLY → PARTIAL 또는 제거 |
| 가상 missed api_key | M-5 | finding 추가 권장 |
| dogfood Test 2 의 severity | M-3 | HIGH → MEDIUM 권장 |

## 가드레일

reviewer 는 다음을 **하지 않는다**:

- 새로운 raw 파일 인용 (L1 의 인용만 재사용)
- L1 의 사실 자체 의심 (L1 인용 검증은 별도 단계에서 끝남)
- 직접 finding 수정 (수정 권한은 L2 만)
- 무한 nitpicking — minor 만 누적되면 종료
