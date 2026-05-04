# Test Scenarios — 회귀, 정확도, 성능, 마이그레이션

## 1. 환각 회귀 시나리오 (#639 재현)

### 1.1 입력 fixture

`fixtures/issue-639/` 에 다음 배치:

- `spec/concerns/database.md` — 실제 spec (300+ 라인, `mcp_tools.content_type` 컬럼 정의 포함, VARCHAR(255))
- `spec/concerns/proxy.md` — 실제 spec
- `migrations/001.sql` — 실제 schema
- `internal/dao/tool.go` — Go struct 정의
- `internal/handler/embed.go` — embedded resource URI 생성

### 1.2 기대 동작

**L1 (database.md 담당)** 출력 검증:
- `## Spec Claims` 에 `mcp_tools.content_type` 정의가 정확한 라인 인용으로 포함
- 발췌가 spec 원문과 substring 매치
- 환각 없으면 `## Mismatches` 는 일치 표시

**L1 (proxy.md 담당)** 출력 검증:
- proxy.md 가 다른 스펙을 가정하지 않음을 확인 (실제 spec 에 없는 ENUM 주장 금지)
- `database.md` 의 내용을 인용하지 않음 (자기 파일 외 발언권 없음)

**L2** 출력 검증:
- "content_type ENUM 충돌" 같은 가짜 finding 이 발생하지 않음
- 모든 finding 의 증거 인용이 L1 리포트에 실재

### 1.3 환각 주입 테스트

L1 에이전트의 응답을 강제로 환각시킨 mock 출력으로 오케스트레이터 검증:

```markdown
## Spec Claims
- [S1] `spec/concerns/database.md:99999` — "content_type ENUM('json','multipart','form')"
```

기대: 오케스트레이터의 인용 검증이 다음을 수행
- 라인 99999 가 파일 범위를 벗어남 → DROP
- 사용자에게 drop 경고 노출
- L2 입력에서 제거되어 환각 finding 미발생

### 1.4 통과 기준

- 정상 입력: L2 finding 0개 (환각 없음 확인)
- 환각 주입 입력: 100% drop, L2 결과에 영향 없음

## 2. Use case 정확도 회귀

### 2.1 D1-D5 회귀

기존 `cross-reference-checker` 테스트 케이스를 그대로 새 구조에 통과:

| 케이스 | 입력 | 기대 발견 |
|--------|------|-----------|
| D1-1 | 두 spec 이 같은 용어를 다르게 정의 | L2 의 spec↔spec gap (DEFINITION_CONFLICT) |
| D2-1 | 요구사항이 미구현 | L2 의 code↔spec gap (SPEC_ONLY) |
| D5-1 | 인터페이스 시그니처 mismatch | L2 의 spec↔spec gap (INTERFACE_DRIFT) |

### 2.2 A/B/C 회귀

기존 `spec-quality-checker` 테스트 케이스:

| 케이스 | 입력 | 기대 발견 |
|--------|------|-----------|
| A-1 | 모호한 시점 표현 ("적절한 시점") | L1 의 Notes 항목 → L2 의 TERM_AMBIGUITY |
| B-1 | 요구사항 누락 | L1 Gaps SPEC_ONLY → L2 code↔spec gap |
| C-1 | 검증 불가 표현 | L1 Notes |

### 2.3 Code 활용 효과 (신규 능력)

기존 spec-only 검증이 못 잡던 모호함을 code 대조로 잡는지 검증:

- 입력: spec 이 "권한" 만 언급, code 는 permission/role 둘 다 사용
- 기대: L2 가 "권한 정의 부재" 발견 (TERM_AMBIGUITY)
- 기존 구조에서는 spec-only 를 보므로 미발견 → 신규 능력 확인

### 2.4 통과 기준

- D1-D5 / A/B/C 케이스 발견율 ≥ 기존 대비 95% (회귀 없음)
- 신규 능력 케이스 ≥ 5개 발견 (실 spec-set 기준)

## 3. 성능 / 비용

### 3.1 측정 항목

| 지표 | 정의 |
|------|------|
| Wall-clock | 커맨드 시작 → 최종 리포트 출력까지 시간 |
| Token 사용량 | 모든 L1 + L2 호출의 input + output token 합 |
| API 호출 수 | spawn 된 에이전트 수 |
| 비용 | 모델별 단가 적용 (Haiku × N + Sonnet × 1) |

### 3.2 기준선 (기존 구조)

- spec-parser 1회 + checker 들 (cross-reference, quality, gap, reverse-gap, structure-mapper) 5회 = **6 호출**
- 모두 Sonnet/Opus
- 직렬 (의존성 있음)

### 3.3 신구조

- L1 N회 (spec 파일 수, 병렬 가능) + L2 1회 = **N+1 호출**
- L1 = Haiku, L2 = Sonnet
- L1 병렬

### 3.4 가설 / 검증

- 가설: 토큰량은 증가, **비용은 감소** (Haiku 단가 ≪ Sonnet)
- 가설: wall-clock 은 **감소** (병렬 + L1 짧은 추론)
- N=5 spec set 기준 측정 후 비교
- N=20 spec set 으로 확장성 측정

### 3.5 통과 기준

- 비용: 기존의 70% 이하
- Wall-clock: 기존의 80% 이하
- 만족하지 못하면 L1 batch (한 Haiku 가 여러 파일 처리) 옵션 검토

## 4. 검증 알고리즘 단위 테스트

### 4.1 인용 검증 unit tests

오케스트레이터의 검증 로직을 단위 테스트:

| 케이스 | 입력 | 기대 |
|--------|------|------|
| V1 | 정상 인용 (file 존재, 라인 유효, 발췌 매치) | PASS |
| V2 | file 미존재 | DROP "file not found" |
| V3 | 라인 범위 초과 | DROP "line out of range" |
| V4 | 발췌가 substring 미매치 | DROP "excerpt mismatch" |
| V5 | 공백/줄바꿈 차이만 있는 발췌 | PASS (정규화 후 매치) |
| V6 | `...` 절단된 발췌의 prefix 매치 | PASS |
| V7 | L1 항목 ID 충돌 | DROP "duplicate id" |
| V8 | Mismatch/Gap 의 참조 ID 가 같은 리포트에 없음 | DROP "dangling reference" |
| V9 | L2 의 인용된 L1 리포트가 검증 통과 리스트에 없음 | DROP "phantom report" |
| V10 | L2 인용 항목 ID 가 L1 에 없음 | DROP "phantom item" |

### 4.2 통과 기준

- 모든 unit test 통과
- 정규화 함수 (공백, 줄바꿈) 가 false positive/negative 없음

## 5. 마이그레이션 호환성

### 5.1 호출자 인벤토리

다음 명령으로 의존성 매트릭스 작성:

```bash
grep -rn "spec-parser\|cross-reference-checker\|spec-quality-checker\|gap-analyzer\|reverse-gap-analyzer\|structure-mapper" plugins/
```

각 호출 지점별로:
- 마이그레이션 필요 여부
- 신구조에서의 대체 호출
- Phase 3 에서 변경

### 5.2 Phase 별 회귀 테스트

| Phase | 테스트 |
|-------|--------|
| Phase 2 (L1/L2 신설) | 단일 spec 입력으로 L1, L2 단독 실행 정상 동작 (legacy 에이전트는 그대로) |
| Phase 3 (아토믹 마이그레이션) | 재작성된 `/spec-kit:spec-review` / `/spec-kit:gap-detect` 가 동일 spec-set 에서 v1 과 동등 발견. legacy 6개 에이전트 0회 호출 (grep 확인). github-autopilot 의 doc 참조 갱신 확인 |

### 5.3 통과 기준

- Phase 3 비교 테스트: 동일 spec-set 에서 finding 의 90% 이상 일치 (10% 신규 발견은 허용)
- Phase 3 e2e: 모든 호출자 (spec-review, gap-detect, github-autopilot/gap-detector 의 doc 참조) 회귀 없음
- Phase 3 후: 기존 6개 에이전트 (`spec-parser`, `cross-reference-checker`, `spec-quality-checker`, `gap-analyzer` (legacy), `reverse-gap-analyzer`, `structure-mapper`) 0회 호출 + 파일 삭제 (grep 확인)

## 6. 사용자 시나리오 dogfooding

### 6.1 본 레포의 spec 으로 테스트

`plans/github-autopilot/`, `plans/spec-kit-redesign/` 자체를 입력으로 새 구조 실행:

- 신규 능력 케이스 발견 (이 프로젝트의 spec 모호함)
- 출력의 가독성 / 클릭 가능 인용 / 우선순위 정렬 점검

### 6.2 외부 spec set (선택)

가능하면 협업 프로젝트의 실 spec 으로 검증. 사용자 직접 점검.

## 7. 통과 기준 종합

Phase 3 (아토믹 마이그레이션) 진입 게이트:

- [ ] 환각 회귀 (1.x): 100% drop, finding 미발생
- [ ] 정확도 (2.x): D1-D5/A/B/C 95% 이상 회귀, 신규 케이스 5+
- [ ] 비용 (3.5): 기존 70% 이하
- [ ] Wall-clock (3.5): 기존 80% 이하
- [ ] 검증 unit test (4.x): 전체 통과
- [ ] Phase 3 비교 (5.3): 90% 일치
- [ ] dogfooding (6.x): 사용자 만족

게이트 미달 시 설계 보완 후 재측정.

## 8. 미해결 / 후속 검토

- L1 batch 모드 (Haiku 한 호출에 여러 파일) 의 정확도 영향
- `related_paths` 자율 탐색의 false positive 비율 (관련 없는 code 를 끌어오는 경우)
- 매우 큰 spec 파일 (1000+ 라인) 의 L1 처리 전략 (라인 범위 분할 vs 전체 처리)
- 인용 검증의 i18n / non-ASCII 처리 (한국어 spec 의 substring 매치)
