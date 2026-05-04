# Use Cases — 새 구조에서 기존 검증 시나리오 처리

기존 spec-kit 의 검증 분류(D1-D5, A/B/C, gap, reverse-gap)와 트리거 사건(#639) 이 새 2-layer 구조에서 어떻게 처리되는지 시나리오 기반으로 검증한다.

## U1. D1 — 용어 일관성

### 기존 처리
`cross-reference-checker` 가 `spec-parser` 의 `glossary` JSON 을 받아 용어별 정의 충돌을 탐색.

### 새 처리

**L1 출력 예** (`database.md` 담당 에이전트):

```markdown
## Spec Claims
- [S3] `database.md:80-85` — "Tool: 외부 시스템과의 통합 단위. ID 는 namespace + name 으로 식별"
```

**L1 출력 예** (`proxy.md` 담당 에이전트):

```markdown
## Spec Claims
- [S2] `proxy.md:40-44` — "Tool: 사용자가 호출 가능한 단일 엔드포인트. ID 는 UUID"
```

**L2 가 발견**:

```markdown
### [HIGH] 용어 "Tool" 의 정의 충돌
- 증거: database.md report [S3] vs proxy.md report [S2]
- 양쪽 모두 동일 code 영역(`internal/types/tool.go`)을 가리키지만 서로 다른 식별 방식 주장
- 둘 중 하나(또는 둘 다)는 실 구현과 불일치
```

**핵심**: D1 을 위한 별도 검증 로직 불필요. L2 가 L1 리포트들에서 같은 항목에 대한 다른 주장을 발견하면 자동으로 표면화.

## U2. D2 — 요구사항 ↔ 컴포넌트 매핑

### 기존 처리
`cross-reference-checker` 가 `requirements[].component` 와 `components[]` 리스트를 cross-check.

### 새 처리

**L1 출력**:

```markdown
## Spec Claims
- [R1] `auth.md:120` — "Auth 모듈은 JWT 토큰 검증을 수행"

## Code Observations
- [C1] `internal/auth/jwt.go:50` — `func ValidateToken(token string) error`

## Mismatches
- [R1] vs [C1] — 일치
```

**Orphan requirement (구현 누락)**:

```markdown
## Gaps
- [G1] spec `auth.md:200` "Refresh token 회전" 언급 → 해당 영역 code 없음
```

**Orphan code (spec 누락)**:

```markdown
## Gaps
- [G2] code `internal/auth/session.go:30` "session expiry tracking" → spec 미언급
```

**L2 종합**:

```markdown
### [MEDIUM] Auth 모듈 요구사항 누락
- 증거: auth.md report [G1]
- spec 만 있고 code 없음. 우선순위 협의 필요
```

## U3. D5 — 인터페이스 일치

### 기존 처리
`cross-reference-checker` 가 spec 들이 선언한 인터페이스 시그니처를 cross-check.

### 새 처리

**L1 출력 (proxy.md 담당)**:

```markdown
## Spec Claims
- [S1] `proxy.md:60-70` — "GET /tools/{name} → ToolInfo { id, name, version }"

## Code Observations
- [C1] `internal/handler/tool.go:40` — `r.GET("/tools/:name", getToolInfo)` returns `ToolInfo { ID, Name, Version, Deprecated }`

## Mismatches
- [S1] vs [C1] — 응답에 `Deprecated` 필드 추가 (spec 미언급)
```

**L1 출력 (api.md 담당)**:

```markdown
## Spec Claims
- [S2] `api.md:30` — "ToolInfo 응답: { id, name, version, deprecated_at? }"
```

**L2 종합**:

```markdown
### [HIGH] ToolInfo 응답 스키마 충돌
- 증거: proxy.md report [S1, C1] vs api.md report [S2]
- proxy.md 는 deprecated 필드 미언급, api.md 는 `deprecated_at` 명시
- code 는 `Deprecated` (boolean) 사용 → 두 spec 과 다름
```

## U4. A/B/C — Spec Quality

### 기존 처리
`spec-quality-checker` 가 spec 자체를 읽고 모호성, 누락, 검증 불가 항목을 식별.

### 새 처리

기존 처리는 "spec 만 보고" 모호성을 판단하려 했다. 새 구조에서는 **code 와의 대조 결과** 가 모호성을 드러내는 객관적 증거가 된다.

**Mode A. 직접 모호 (기존과 유사)**

여전히 일부 모호함은 spec 만 봐도 보임. L1 이 다음과 같이 표시:

```markdown
## Notes
- [N1] `auth.md:150` — "적절한 시점에 토큰 갱신" 시점 미명시 (모호)
```

**Mode B. Code 와의 대조로 드러나는 모호함 (새 능력)**

```markdown
## L1 from auth.md
- [S5] `auth.md:200` — "사용자는 권한에 따라 접근 제한"

## L1 from rbac.md
- [S2] `rbac.md:80` — "역할 기반 접근 제어"
```

**L2 발견**:

```markdown
### [MEDIUM] auth.md "권한" 정의 부재
- 증거: auth.md report [S5] (권한 언급) + rbac.md report [S2] (역할 정의)
- auth.md 는 "권한" 의 의미를 정의하지 않음. rbac.md 는 별개 개념인 "역할" 만 다룸
- code 는 두 개념을 모두 사용 (`internal/auth/permission.go`, `internal/rbac/role.go`)
- spec 에 "권한 vs 역할" 정의 누락
```

이것이 "**code 를 외부 ground truth 로 활용**" 의 실제 효과. 기존 spec-only 검증이 잡지 못하는 모호함이 드러남.

## U5. Spec ↔ Code Gap (gap-analyzer 의 일)

### 기존 처리
`gap-analyzer` 가 spec 의 요구사항 리스트와 code 를 대조해 미구현 항목 식별.

### 새 처리

L1 의 `## Gaps` 섹션이 그대로 이 역할. L2 가 우선순위만 부여.

**L1 출력**:

```markdown
## Gaps
- [G1] `auth.md:300` "rate limiting" → 해당 영역 code 없음
- [G2] `auth.md:250` "audit logging" → `internal/audit/log.go` 존재하지만 auth 호출처 없음 (부분 구현)
```

**L2**:

```markdown
### [HIGH] Rate limiting 완전 미구현
- 증거: auth.md report [G1]

### [MEDIUM] Audit logging 부분 구현
- 증거: auth.md report [G2]
- 모듈은 있으나 auth 영역에서 호출 없음
```

## U6. Code ↔ Spec Reverse Gap (reverse-gap-analyzer 의 일)

### 기존 처리
`reverse-gap-analyzer` 가 code 에 있는데 spec 에 없는 것을 식별.

### 새 처리

L1 이 양방향을 동시에 본다. `## Gaps` 의 다른 방향:

```markdown
## Gaps
- [G3] `internal/auth/api_key.go:20` "API key 인증" → spec 미언급
```

L2 종합 시 reverse gap 으로 분류:

```markdown
### [LOW] API key 인증 spec 누락
- 증거: auth.md report [G3]
- code 는 있으나 spec 어디에도 정의 없음
- 결정 필요: spec 추가 vs code 제거
```

## U7. #639 회귀 시나리오 (환각 검출)

### 가상 재현

새 구조에서 같은 종류의 환각이 시도된다고 가정.

**가정**: L1 (`database.md` 담당) 이 hallucinate.

가짜 출력:

```markdown
## Spec Claims
- [S99] `database.md:99999` — "content_type ENUM('json','multipart','form')"
```

### 차단 메커니즘

1. **인용 검증 (오케스트레이터)**: `database.md` 의 라인 99999 가 존재하는지 확인 → 없음 → finding drop + 경고
2. **substring 검증**: 인용된 텍스트가 `database.md:99999` 에 실제 존재하는지 → 없음 → drop
3. **L2 영향 차단**: 검증 실패한 finding 은 L2 입력에서 제거되므로 환각이 cross-file 결론으로 번지지 않음

### 환각 전파 단절

- L1 한 에이전트의 환각은 그 파일 검증 단계에서 차단됨
- 다른 파일 L1 들은 영향 없음 (서로 격리)
- L2 는 검증 통과한 항목만 봄

### 검증 못 잡는 케이스 (잔여 위험)

- 인용 라인이 실재하고 substring 도 매치하지만, **해석이 왜곡** 된 경우
- 예: 실제로 "VARCHAR(255)" 인데 L1 이 "VARCHAR(255) (=ENUM 의 일종)" 로 적음
- 잔여 대응: L1 의 출력 형식을 "**원문 그대로 인용 + 한 줄 분류**" 로 강제하여 해석을 최소화

## 결론

새 구조는 기존 6개 에이전트의 모든 use case 를 cover 하며, 추가로 다음 능력을 얻는다:

- code 를 외부 ground truth 로 활용한 모호함 발견 (U4 Mode B)
- 환각 차단 메커니즘 (U7)
- L1 병렬화로 성능 향상

다음 문서(`02-architecture.md`)에서 L1/L2 인터페이스를 명시한다.
