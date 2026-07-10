<!-- [coding-style:begin] DO NOT REMOVE THIS LINE -->

**단순하게 접근하며 사전에 계획된 작업만 수행한다.**

## 작업 흐름 (Workflow)

요구사항을 분해할 때는 잘 정리된 스킬을 기반으로 한다. `brainstorm` 으로 설계를 만들고 `grill` 로 검증한 뒤 구체화한다.

- **막연한 아이디어 → `brainstorm`**: 무에서 설계를 만드는 발산→수렴 대화. 질문을 하나씩 던져 요구사항과 설계를 분해하고, 설계를 제시해 승인받는다.
- **이미 손에 든 계획·설계 → `grill`**: 있는 계획을 코드 건드리기 전에 집요하게 심문해 빈틈을 드러낸다. `brainstorm` 이 만든 설계를 검증하고 싶을 때도 `grill` 로 핸드오프한다.
- **설계 승인 후 구현·문서 산출물 → `orchestrator`**: 구현뿐 아니라 문서 산출물 생성(리포트, 스펙, 정리 문서)도 직접 Edit/Write 하지 않고 `orchestrator` 로 위임한다. 독립 작업은 병렬 fan-out, 단계 의존은 파이프라인, 장기 작업은 agent-team으로 분해해 수행한다. 예외는 사용자가 명시적으로 직접 수정을 지시한 경우(단일 파일 소규모 수정 등)로 한정한다.

```
✅ 요청 → brainstorm/grill 로 요구사항·설계 분해 → 승인 → orchestrator 로 구현·문서 생성
❌ 요청 → 바로 직접 구현
❌ 문서/리포트 작업이니까 직접 Write/Edit 로 진입
```

## 설계 최우선 원칙 (Design First)

**구현보다 설계가 반드시 먼저 진행되어야 합니다.**

코드를 작성하기 전에 다음을 완료해야 합니다:

1. **요구사항 정리**: 이해한 요구사항을 구조화하여 명확하게 정리
2. **사이드이펙트 조사**: 변경으로 인해 영향받는 기존 코드, 의존성, 동작을 사전에 파악
3. **설계 반영**: 요구사항과 사이드이펙트를 모두 반영한 구조를 설계

```
❌ 요청 → 바로 구현
✅ 요청 → 요구사항 정리 → 사이드이펙트 조사 → 설계 → 승인 → 구현
```

- 막연한 아이디어면 `brainstorm`, 검토할 기존 계획이 있으면 `grill`로 설계를 먼저 다진 뒤 승인을 받고 구현한다
- 구현과 문서 산출물 생성은 `orchestrator`로 위임해 작업을 분해한다 (사용자가 명시적으로 직접 수정을 지시한 경우만 예외)
- 설계 산출물: 인터페이스 정의, 모듈 구조, 의존성 방향
- 구현 중 설계 변경이 필요하면 멈추고 재논의

### 의도 재진술 (Intent Restatement)

`brainstorm`/`grill` 을 거치지 않는 소규모 변경을 위한 경량 경로입니다. 소규모라도 코드를 수정하기 전에 **무엇을**(필드/조건/메서드), **어디서**(파일/위치), **어떻게** 바꾸는지 한 문장으로 재진술하고 사용자 확인을 받는다.

- 특히 diff/비교 로직 변경, rename 인지 동작 변경인지처럼 문구가 중의적인 요청에서는 필수
- 그럴듯한 해석이 하나 떠올랐다고 그것이 사용자의 의도라고 단정하지 않는다

```
❌ "필터 추가해줘" → 쿼리 필터 파라미터 추가로 해석해 바로 구현
✅ "필터 추가해줘" → "diff 비교 조건에 'excluded'를 추가하는 것으로 이해했는데 맞습니까?" → 확인 후 구현
```

## 구현 원칙 (Implementation Principles)

### SOLID

1. **단일 책임 (SRP)**: 하나의 모듈/구조체는 하나의 책임만 갖는다. 변경의 이유가 둘 이상이면 분리한다. (`Extract Class`, `Move Function`)
2. **개방-폐쇄 (OCP)**: 새로운 상태/타입 추가 시 기존 코드를 수정하지 않고 확장할 수 있어야 한다. 동일한 분기가 여러 곳에 흩어져 있으면 다형성이나 전략 패턴으로 전환하여 변경 지점을 하나로 줄인다. (`Replace Conditional with Polymorphism`)
3. **리스코프 치환 (LSP)**: 인터페이스의 모든 구현체는 해당 인터페이스가 정의한 계약을 동일하게 만족해야 한다. 호출하는 쪽은 어떤 구현체가 들어오든 동작이 보장되어야 한다. (`Replace Subclass with Delegate`)
4. **인터페이스 분리 (ISP)**: 인터페이스는 역할별로 분리한다. 하나의 거대한 인터페이스가 아닌, 사용하는 쪽이 필요로 하는 메서드만 포함하는 작은 인터페이스를 정의한다. (`Extract Interface`)
5. **의존성 역전 (DIP)**: 코어 로직은 구체 구현에 직접 의존하지 않는다. 추상(인터페이스)에 의존하고, 구체 구현은 외부에서 주입받는다. (`Parameterize Function`, `Replace Constructor with Factory Function`)

### Decomposition

- 함수 내 지역 변수가 늘어나면 추출 (`Extract Variable` → `Extract Function`)
- 인라인 로직보다 의도를 드러내는 이름의 서브루틴 선호 (`Replace Inline Code with Function Call`)
- 여러 단계 수행 함수는 단계별 분리 (`Split Phase`, `Replace Loop with Pipeline`)
- 중첩 블록(try-catch, if-else, 루프 안 루프)이 쌓이면 서브루틴 추출. 특히 루프 내 try-catch 결과 분류는 리터럴 유니온 반환 서브루틴으로

### Fail Fast

- 외부 의존성 응답이 스펙과 다르면 fallback으로 우회하지 말고 즉시 실패
- 방어 로직(텍스트 파싱 fallback, 기본값 대체)은 스펙 불일치를 숨겨 디버깅을 어렵게 만듦
- 정상 경로 하나를 신뢰, 그 경로가 실패하면 에러를 명확히

### Debugging (Reproduce-then-Fix)

증상만 보고 추측하지 않고, 재현과 증거로 원인을 확정한 뒤에만 수정합니다.

- 버그 수정 전 **로컬 재현**으로 정확한 에러를 캡처한다
- 가설은 grep/로그/DB 쿼리 등 **증거로 실증**한 뒤에만 수정을 제안한다
- 실증되지 않은 설명(배포 지연, 캐시, 타이밍 등)을 결론으로 채택하지 않는다
- 수정에 앞서 **실제 버그인지 설계된 동작(working-as-designed)인지** 먼저 구분한다
- 원인 확정 후 실패하는 회귀 테스트 작성 → 수정 → 통과 확인 (Testing의 Review-Driven Testing 섹션 참조)

```
❌ 증상 관찰 → "배포 지연 때문인 것 같다" 추측성 진단 → 바로 수정
✅ 증상 관찰 → 로컬 재현으로 에러 캡처 → grep/로그/쿼리로 가설 실증 → 회귀 테스트 (Red) → 수정 (Green)
```

### Testing

#### Review-Driven Testing

- 코드 리뷰에서 지적된 버그 수정 시 재현 테스트 함께 작성
- 수정 전 코드에서 Red, 수정 후 Green 확인
- 리뷰 이슈 수정 시 테스트 없이 코드만 변경하지 않는다
- 원인 확정까지의 절차는 Debugging (Reproduce-then-Fix) 섹션을 따른다

#### Black-box TDD

코어 로직 구현 전에 블랙박스 테스트를 먼저 작성한다. 공개 API 기준으로만 검증하고 내부 구현에 결합하지 않는다.

```
❌ 코어 로직 구현 → 나중에 테스트 추가
✅ 인터페이스 정의 → 테스트용 구현 → 테스트 작성 (fail) → 코어 구현 (pass) → 실제 구현 연결
```

Red → Green → Refactor 순서를 따른다.

#### Testability First

- 테스트가 어려운 코드는 테스트 기법이 아닌 **프로덕션 코드를 리팩터링**으로 해결 (DI 전환)
- 단, Request Context 수준 암묵적 상태(thread-local, AsyncLocalStorage)는 리팩터링 대상 아님 → 테스트 harness 구성

#### Mock

- 외부 의존성(DB, HTTP, 외부 서비스)은 추상화로 정의 + mock으로 대체
- mock은 테스트 전용 디렉토리 또는 테스트 파일 내 정의

#### E2E

- 컨테이너로 외부 의존성 환경 구성
- E2E와 단위 테스트는 디렉토리 또는 파일명으로 분리

#### 적용 범위

- **적용**: 외부 시스템과 상호작용하는 기능 구현 (API 호출, DB 접근, CLI 실행 등)
- **제외**: 단순 버그 수정, 설정 변경, 문서 수정 등 외부 의존성이 없는 작업

## Code Comments

- 코드 주석에는 **의도(why)** 와 **비자명한 제약**만 적는다
- PR 번호, 버전 시점, 변경 이력 narration 금지 — git blame과 PR description 영역
- 예: `// v5.7부터 X로 동작`, `// PR #721에서 추가됨`, `// 이전엔 Y를 사용했음` 같은 표현은 stale
- 예외: `spec/`, `changelog/` 디렉토리는 변경 이력 보존이 목적이라 narration 허용

## Refactoring Findings

- `/simplify`, `/review` 등이 발견한 finding 중 "의도된 디자인이라 skip"한 항목은 **commit body 또는 PR description의 별도 섹션에 사유 한 줄 명시**
- 형식: `Simplify findings (skipped):` 또는 `Review findings (deferred):` 섹션
- Why: 다음 분석 실행 시 동일 finding 재검토 방지

## Verification Before Action

- import 경로, 클래스명, 모듈 export를 작성하기 전에 실제 소스에서 Grep/Read로 확인
- 존재하지 않는 API, 미export된 심볼을 추측으로 사용 안 함
- 편집 후 빌드/타입 체크 가능하면 실행

## 코드 품질 게이트 (Quality Gate)

lint, format, test가 실패하면 변경된 부분이 아니더라도 반드시 수정해야 합니다.

```
❌ 내가 변경한 파일이 아니니까 무시
✅ pre-push hook 또는 CI에서 실패하면 해당 오류를 모두 수정한 뒤 push
```

## 작업 마무리 습관

코드나 문서 변경 작업 후에는 `/simplify`를 실행하여 재사용성, 품질, 효율성을 검토한다.

<!-- [coding-style:end] DO NOT REMOVE THIS LINE -->
