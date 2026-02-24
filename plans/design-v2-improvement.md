# Design Phase v2 개선 계획

## 1. 현재 상태 분석 (v1)

### 1.1 현재 아키텍처

```
Phase 0: 요구사항 수집 (HITL)
    │
    ▼
Phase 1: 아키텍처 설계 (3개 LLM 병렬)
    ├── architect-claude (opus, Read/Glob)
    ├── architect-codex  (haiku, Bash/Read → call-codex.sh)
    └── architect-gemini (haiku, Bash/Read → call-gemini.sh)
    │
    ▼
Phase 2: 통합 + ASCII 다이어그램
    │
    ▼
Phase 3: Contract 정의 (Checkpoint + Interface + Test)
    │
    ▼
Step 1.5: 사용자 확인 (HITL)
```

### 1.2 v1의 강점

| # | 강점 | 설명 |
|---|------|------|
| S1 | 명확한 HITL 경계 | 사용자 개입 2곳만 (설계 승인, 최종 PR 리뷰) |
| S2 | Multi-LLM 합의 | 3개 LLM의 관점으로 편향 방지 |
| S3 | 병렬 실행 | 3개 LLM 동시 실행으로 시간 절약 |
| S4 | Contract 기반 병렬화 | Interface + Test로 안전한 병렬 구현 가능 |
| S5 | ASCII 다이어그램 | 외부 도구 불필요, VCS 친화적 |
| S6 | 상태 지속성 | state.json으로 세션 재개/compaction 대응 |
| S7 | Phase Gate 물리 차단 | Hook이 gate 미충족 시 Write/Edit 물리 차단 |

### 1.3 v1의 약점 및 개선 필요 사항

| # | 약점 | 영향도 | 설명 |
|---|------|--------|------|
| W1 | 사이드이펙트 분석 미형식화 | **높음** | CLAUDE.md에 원칙만 명시, `/design`에서 구조화된 단계 부재. 기존 코드 영향을 후반에 발견 |
| W2 | 코드베이스 컨텍스트 미주입 | **높음** | architect 에이전트가 실제 코드를 읽지 않고 진공 상태에서 설계. 기존 패턴/구조 무시 |
| W3 | 설계 산출물 미저장 | **높음** | 설계 결과가 대화 컨텍스트에만 존재. compaction 시 유실, Phase 3에서 참조 불가 |
| W4 | 외부 LLM 장애 시 fallback 미정의 | **중간** | Codex/Gemini 실패 시 암묵적으로 Claude만 사용. 사용자에게 알림 없음 |
| W5 | 설계 반복 추적 부재 | **중간** | state.json이 checkpoint만 추적. 설계 iteration (Phase 2 → Phase 1.2 루프) 미기록 |
| W6 | Contract 자동 검증 부재 | **중간** | 순환 의존성, 파일 겹침, 인터페이스 완전성 등 사전 검증 없이 Phase 2로 넘어감 |
| W7 | 합의 분석이 비구조적 | **낮음** | 3/3, 2/3, 1/3 기준만 있고, 구체적 tie-breaking 로직이나 가중치 없음 |
| W8 | 설계 결정 기록(ADR) 부재 | **낮음** | 왜 A를 선택하고 B를 버렸는지 기록 안 됨. 나중에 같은 질문 반복 |
| W9 | 증분 설계 미지원 | **낮음** | 기존 설계 수정 불가, 전체 재실행만 가능 |
| W10 | 외부 LLM 에이전트 모델 낭비 | **낮음** | haiku 모델이 Bash 스크립트 호출만 수행. 모델 호출 자체가 불필요한 오버헤드 |

---

## 2. Design v2 개선 목표

### 2.1 핵심 목표

```
v1: "요구사항 → 설계 → Contract"  (진공 상태 설계)
v2: "요구사항 → 코드베이스 분석 → 영향 분석 → 설계 → 검증 → Contract"  (컨텍스트 기반 설계)
```

| 목표 | 해결하는 약점 | 기대 효과 |
|------|-------------|-----------|
| **코드베이스 인지 설계** | W2 | 기존 패턴/구조를 반영한 현실적 설계 |
| **사이드이펙트 선제 분석** | W1 | 구현 단계 리워크 최소화 |
| **설계 산출물 영속화** | W3, W5, W8 | compaction 내성, Phase 3 참조 가능, 설계 이력 보존 |
| **Contract 사전 검증** | W6 | 잘못된 Contract가 Phase 3까지 전파되는 것 방지 |
| **Graceful degradation** | W4, W10 | 외부 LLM 장애 시 자연스러운 fallback + 투명한 알림 |

### 2.2 비목표 (Non-goals)

- Phase 2 (REVIEW), Phase 3 (IMPLEMENT), Phase 4 (MERGE) 변경
- 새로운 커맨드 추가 (기존 `/design`, `/outline`, `/develop` 유지)
- Multi-LLM 인프라 자체 변경 (call-codex.sh, call-gemini.sh 유지)

---

## 3. v2 아키텍처 설계

### 3.1 새로운 Design Phase 워크플로우

```
Step 0: 요구사항 수집 (HITL) ─── [v1과 동일]
    │
    ▼
Step 1: 코드베이스 스캔 ─── [NEW]
    │  Glob + Grep으로 관련 파일/패턴 수집
    │  → codebase-context.md 생성
    │
    ▼
Step 2: 사이드이펙트 분석 ─── [NEW]
    │  변경 영향 범위 분석
    │  → impact-analysis 섹션 생성
    │
    ▼
Step 3: Multi-LLM 아키텍처 설계 ─── [개선]
    │  코드베이스 컨텍스트를 프롬프트에 포함
    ├── architect-claude (opus, Read/Glob/Grep)
    ├── architect-codex  (haiku, Bash/Read)
    └── architect-gemini (haiku, Bash/Read)
    │
    ▼
Step 4: 통합 + 합의 분석 ─── [개선]
    │  구조화된 합의 매트릭스
    │  LLM 가용성 반영
    │
    ▼
Step 5: Contract 정의 + 자동 검증 ─── [개선]
    │  순환 의존성 검사
    │  파일 소유권 겹침 검사
    │  인터페이스 완전성 검사
    │
    ▼
Step 6: 설계 산출물 저장 ─── [NEW]
    │  .develop-workflow/design.md 영속화
    │  state.json에 설계 메타데이터 기록
    │
    ▼
Step 7: 사용자 확인 (HITL) ─── [v1과 동일]
```

### 3.2 단계별 상세 설계

---

#### Step 0: 요구사항 수집 (변경 없음)

v1의 Phase 0과 동일. 기능/비기능 요구사항, 제약조건, 우선순위 수집.

**변경점**: 없음. 이 단계는 이미 잘 작동함.

---

#### Step 1: 코드베이스 스캔 (NEW)

**목적**: 설계에 필요한 기존 코드베이스 컨텍스트를 수집

**프로세스**:
1. 요구사항에서 키워드 추출 (기능명, 모듈명, 기술명)
2. `Glob`으로 관련 파일 구조 수집
3. `Grep`으로 관련 패턴/인터페이스 검색
4. 결과를 **코드베이스 컨텍스트 요약**으로 정리

**수집 항목**:
```markdown
## 코드베이스 컨텍스트

### 관련 파일 구조
- src/auth/ (인증 관련 기존 모듈)
  - types.ts, service.ts, middleware.ts
- src/api/routes/ (API 라우트)
  - user.ts, product.ts

### 기존 패턴
- 인증: JWT 기반, middleware 패턴
- API: Express + Router 패턴
- DB: TypeORM + Repository 패턴

### 기존 인터페이스 (변경 영향 가능)
- `IAuthService` (src/auth/types.ts:15)
- `UserRouter` (src/api/routes/user.ts:8)

### 의존성 그래프 (요약)
- auth → db, config
- api → auth, db
```

**MainAgent가 직접 수행** (별도 에이전트 불필요):
- `Glob`: 디렉토리 구조 파악
- `Grep`: 인터페이스, 타입, 패턴 검색
- 결과를 markdown 문자열로 정리

**토큰 효율 전략**:
- 파일 내용을 모두 읽지 않음 (Glob으로 경로만, Grep으로 시그니처만)
- 핵심 인터페이스와 타입 정의만 발췌
- 50줄 이내 요약으로 압축

---

#### Step 2: 사이드이펙트 분석 (NEW)

**목적**: 변경으로 인해 영향받는 기존 코드, 의존성, 동작을 사전에 파악

**프로세스**:
1. Step 0 요구사항에서 **변경 대상** 식별
2. Step 1 코드베이스 컨텍스트에서 **영향 범위** 분석
3. 결과를 **Impact Analysis** 섹션으로 정리

**분석 항목**:

```markdown
## 사이드이펙트 분석

### 직접 변경 대상
| 파일/모듈 | 변경 유형 | 설명 |
|-----------|----------|------|
| src/auth/service.ts | 수정 | OAuth2 프로바이더 추가 |
| src/auth/types.ts | 수정 | AuthProvider 타입 확장 |

### 간접 영향 (Ripple Effect)
| 영향받는 파일 | 영향 유형 | 심각도 | 설명 |
|--------------|----------|--------|------|
| src/api/routes/user.ts | 인터페이스 변경 | 중 | AuthService 시그니처 변경 시 호출부 수정 필요 |
| tests/auth/*.test.ts | 테스트 수정 | 낮 | 기존 테스트 케이스 유지 + 새 케이스 추가 |

### 리스크
- [R1] 기존 JWT 인증 흐름이 깨질 수 있음 → 기존 테스트 통과 필수
- [R2] OAuth2 콜백 URL 설정 필요 → 환경변수 추가

### 마이그레이션 필요 여부
- DB 스키마 변경: 없음 / 있음 (상세: ...)
- 환경변수 추가: 있음 (OAUTH2_CLIENT_ID, OAUTH2_SECRET)
- 설정 파일 변경: 없음
```

**MainAgent가 직접 수행**:
- Step 1의 컨텍스트를 기반으로 분석 (추가 코드 읽기 가능)
- `Grep`으로 변경 대상의 참조자(callers) 검색
- 결과를 표 형태로 구조화

---

#### Step 3: Multi-LLM 아키텍처 설계 (개선)

**변경점**:

1. **프롬프트에 코드베이스 컨텍스트 포함**

```
# 아키텍처 설계 요청

## 요구사항
[Step 0 결과]

## 코드베이스 컨텍스트        ← NEW
[Step 1 결과 (요약)]

## 사이드이펙트 분석           ← NEW
[Step 2 결과 (요약)]

## 설계 요청
위 요구사항과 기존 코드베이스를 고려하여 아키텍처를 설계해주세요.

다음 항목을 포함해주세요:
1. 주요 컴포넌트와 책임
2. 컴포넌트 간 상호작용
3. 데이터 흐름
4. 기술 선택과 근거 (기존 스택과의 일관성 포함)   ← 개선
5. 잠재적 리스크 (사이드이펙트 포함)              ← 개선
6. 기존 코드 변경 최소화 방안                     ← NEW

구체적인 코드가 아닌 상위 레벨 설계를 제공해주세요.
```

2. **architect-claude 에이전트 도구 확장**

```yaml
# 변경 전
tools: ["Read", "Glob"]

# 변경 후
tools: ["Read", "Glob", "Grep"]
```

Claude architect에 `Grep` 추가. 필요 시 기존 코드를 직접 검색하여 설계에 반영.

3. **외부 LLM Graceful Degradation**

```
3개 LLM 병렬 실행
    │
    ├── Claude: 항상 실행 (필수)
    ├── Codex: 실행 시도
    │   ├── 성공 → 결과 수집
    │   └── 실패 → 로그 기록, 계속 진행
    └── Gemini: 실행 시도
        ├── 성공 → 결과 수집
        └── 실패 → 로그 기록, 계속 진행

결과 수집
    │
    ├── 3/3 성공 → 정상 합의 분석
    ├── 2/3 성공 → 2개 LLM으로 합의 분석 + 실패 LLM 명시
    ├── 1/3 성공 (Claude만) → Claude 단독 설계 + 경고 메시지
    └── 0/3 성공 → 에러 (불가능: Claude는 내부 에이전트)
```

**사용자 알림 형식**:
```
⚠ Codex CLI 실행 실패: codex CLI가 설치되지 않음
ℹ Claude + Gemini 2개 LLM으로 설계를 진행합니다.
```

---

#### Step 4: 통합 + 합의 분석 (개선)

**변경점**: 구조화된 합의 매트릭스 도입

**합의 분석 형식 (v2)**:

```markdown
## 설계 통합 분석

### LLM 가용성
| LLM | 상태 | 비고 |
|-----|------|------|
| Claude | ✅ 성공 | - |
| Codex | ✅ 성공 | - |
| Gemini | ❌ 실패 | CLI 미설치 |

### 합의 매트릭스

| 설계 항목 | Claude | Codex | Gemini | 합의 수준 | 최종 결정 |
|-----------|--------|-------|--------|-----------|----------|
| 인증 방식 | OAuth2 + JWT | OAuth2 + JWT | N/A | 2/2 (높음) | OAuth2 + JWT |
| DB 선택 | PostgreSQL | MongoDB | N/A | 1/2 (분기) | PostgreSQL (기존 스택 일관성) |
| 캐시 전략 | Redis | Redis | N/A | 2/2 (높음) | Redis |

### 합의 수준 판정 기준
- **높음** (전원 동의): 반드시 반영
- **중간** (과반 동의): 기존 코드베이스 일관성 우선 반영
- **분기** (의견 불일치): 기존 스택 일관성 > 성능 > 단순성 순으로 판정
- **단독** (1개 LLM만): Claude 설계를 기본으로 채택, 근거 명시

### 최종 설계 결정 및 근거 (ADR)

#### ADR-1: DB는 PostgreSQL
- **결정**: PostgreSQL
- **대안**: MongoDB (Codex 제안)
- **근거**: 기존 코드베이스가 TypeORM + PostgreSQL. 일관성 유지가 마이그레이션 비용보다 이점
- **리스크**: 없음 (기존 스택 유지)
```

**분기 시 tie-breaking 우선순위**:
1. 기존 코드베이스 일관성 (Step 1 컨텍스트 기반)
2. 사이드이펙트 최소화 (Step 2 분석 기반)
3. 기술적 단순성
4. 성능/확장성

---

#### Step 5: Contract 정의 + 자동 검증 (개선)

**변경점**: Contract 정의 후 자동 검증 단계 추가

**검증 항목**:

| # | 검증 | 방법 | 차단 수준 |
|---|------|------|----------|
| V1 | 순환 의존성 | dependencies 그래프에서 사이클 탐지 | Blocking |
| V2 | 파일 소유권 겹침 | 서로 다른 checkpoint의 interface/tests 파일 교차 검사 | Blocking |
| V3 | 인터페이스 완전성 | 모든 checkpoint에 interface + tests + validation 필드 존재 | Blocking |
| V4 | 의존성 존재 확인 | dependencies에 참조된 checkpoint id가 실제 존재하는지 | Blocking |
| V5 | 기존 파일 충돌 | interface 파일이 기존 파일과 겹칠 때 side-effect 분석에 포함되었는지 | Warning |

**검증 로직 구현 방식**:

MainAgent가 직접 수행 (별도 도구/스크립트 불필요):
- Contract YAML을 파싱하여 각 검증 항목을 체크
- Blocking 이슈 발견 시 자동 수정 후 재검증
- Warning은 설계 산출물에 기록

**검증 출력 예시**:
```
Contract 검증 결과:
  ✅ V1: 순환 의존성 없음
  ✅ V2: 파일 소유권 겹침 없음
  ✅ V3: 인터페이스 완전성 충족
  ✅ V4: 의존성 참조 유효
  ⚠ V5: src/auth/types.ts가 checkpoint-1에서 수정 + 사이드이펙트 분석에 포함됨 (OK)
```

---

#### Step 6: 설계 산출물 저장 (NEW)

**목적**: 설계 결과를 파일로 영속화하여 compaction 내성 확보 + Phase 3 참조

**저장 파일**: `.develop-workflow/design.md`

**파일 구조**:
```markdown
# 설계 결과

> 생성: {timestamp}
> 기능: {feature 요약}
> LLM: Claude ✅, Codex ✅, Gemini ❌

## 1. 요구사항 요약
[Step 0 결과]

## 2. 코드베이스 컨텍스트
[Step 1 결과]

## 3. 사이드이펙트 분석
[Step 2 결과]

## 4. 아키텍처 설계 (통합)
[Step 4 결과 - 합의 매트릭스 포함]

### 컴포넌트 다이어그램 (ASCII)
[다이어그램]

### 데이터 흐름도 (ASCII)
[다이어그램]

## 5. 기술 스택
[테이블]

## 6. 설계 결정 기록 (ADR)
[Step 4 ADR 목록]

## 7. Checkpoints (Contract)
[Step 5 결과 - 검증 결과 포함]

## 8. 리스크 및 고려사항
[통합 리스크 목록]
```

**state.json 확장**:
```json
{
  "phase": "DESIGN",
  "feature": "OAuth2 인증 추가",
  "design": {                          // ← NEW
    "iteration": 1,
    "llm_status": {
      "claude": "success",
      "codex": "success",
      "gemini": "failed"
    },
    "contract_validation": "passed",
    "artifact_path": ".develop-workflow/design.md"
  },
  "gates": { ... },
  "checkpoints": { ... }
}
```

**Phase 3에서의 참조**:
- IMPLEMENT 진입 시 `.develop-workflow/design.md`를 Read하여 컨텍스트 확보
- compaction 후에도 설계 문서로 컨텍스트 복원 가능

---

#### Step 7: 사용자 확인 (변경 없음)

v1의 Step 1.5와 동일. AskUserQuestion으로 설계 승인.

**개선점**: 설계 산출물이 `.develop-workflow/design.md`에 저장되어 있으므로 사용자에게 파일 경로도 안내.

---

### 3.3 v1 → v2 변경 영향 분석

#### 변경되는 파일

| 파일 | 변경 유형 | 설명 |
|------|----------|------|
| `commands/design.md` | **대폭 수정** | Step 1-6 반영, 프롬프트 템플릿 개선 |
| `commands/outline.md` | **수정** | Step 1 (코드베이스 스캔) 추가, 산출물 저장은 제외 |
| `commands/develop.md` | **소폭 수정** | Phase 1 설명을 v2로 업데이트, Phase 3에서 design.md 참조 추가 |
| `agents/architect-claude.md` | **수정** | tools에 Grep 추가, 프롬프트 입력 형식에 컨텍스트 추가 |
| `agents/architect-codex.md` | **수정** | 에러 처리 명확화 (fallback 메시지) |
| `agents/architect-gemini.md` | **수정** | 에러 처리 명확화 (fallback 메시지) |
| `skills/feature-workflow/SKILL.md` | **소폭 수정** | Phase 1 가이드라인에 Step 1-2 추가 |
| `skills/high-level-design/SKILL.md` | **소폭 수정** | 코드베이스 인지 설계 원칙 추가 |
| `hooks/develop-phase-gate.cjs` | **변경 없음** | Gate 로직은 그대로 유지 |
| `plugin.json` | **수정** | 버전 범프 |

#### 변경되지 않는 영역

- Phase 2 (REVIEW): 변경 없음
- Phase 3 (IMPLEMENT): design.md 참조 로직만 추가 (구현 전략 자체는 변경 없음)
- Phase 4 (MERGE): 변경 없음
- state.json Gate 시스템: 변경 없음
- develop-phase-gate.cjs Hook: 변경 없음
- 외부 LLM 스크립트 (call-codex.sh, call-gemini.sh): 변경 없음

#### 하위 호환성

- `/design` 기존 사용법 유지 (인자 형식 동일)
- `/outline` 기존 사용법 유지
- `/develop` 기존 사용법 유지
- state.json의 기존 필드 유지 (`design` 필드만 추가)

---

## 4. 구현 계획

### 4.1 구현 순서 (의존성 기반)

```
Phase A: 핵심 인프라 (의존성 없음)
    ├── A1: design.md 커맨드 재작성 (Step 0-7 전체)
    ├── A2: architect-claude.md 에이전트 개선
    └── A3: architect-codex/gemini.md 에러 처리 개선

Phase B: 연동 업데이트 (Phase A 의존)
    ├── B1: outline.md 커맨드 업데이트
    ├── B2: develop.md 커맨드 업데이트
    └── B3: feature-workflow SKILL.md 업데이트

Phase C: 보조 업데이트 (Phase A 의존)
    ├── C1: high-level-design SKILL.md 업데이트
    └── C2: plugin.json 버전 범프
```

### 4.2 작업 상세

#### A1: design.md 커맨드 재작성

**작업 범위**: `plugins/develop-workflow/commands/design.md` 전면 재작성

**핵심 변경**:
1. Step 1 (코드베이스 스캔) 섹션 추가
   - Glob/Grep 패턴 예시
   - 수집 항목 템플릿
   - 토큰 효율 전략 명시

2. Step 2 (사이드이펙트 분석) 섹션 추가
   - 직접 변경 대상 식별 방법
   - Ripple Effect 분석 방법
   - 리스크/마이그레이션 체크리스트

3. Step 3 프롬프트 템플릿 개선
   - 코드베이스 컨텍스트 섹션 추가
   - 사이드이펙트 섹션 추가
   - "기존 코드 변경 최소화" 요청 추가

4. Step 4 합의 매트릭스 형식 추가
   - LLM 가용성 테이블
   - 구조화된 합의 매트릭스
   - ADR 형식
   - Tie-breaking 우선순위

5. Step 5 Contract 검증 로직 추가
   - 5개 검증 항목 (V1-V5) 체크리스트
   - Blocking vs Warning 분류
   - 자동 수정 + 재검증 흐름

6. Step 6 설계 산출물 저장 명세
   - `.develop-workflow/design.md` 형식
   - state.json 확장 필드

7. Graceful Degradation 흐름 추가
   - 외부 LLM 실패 시 처리 흐름
   - 사용자 알림 형식

#### A2: architect-claude.md 에이전트 개선

**작업 범위**: `plugins/develop-workflow/agents/architect-claude.md`

**핵심 변경**:
1. `tools`에 `"Grep"` 추가
2. 입력 형식에 코드베이스 컨텍스트 + 사이드이펙트 섹션 추가
3. 설계 프로세스에 "기존 코드 패턴 확인" 단계 추가
4. 출력 형식에 "기존 코드 변경 최소화 방안" 섹션 추가

#### A3: architect-codex/gemini.md 에러 처리 개선

**작업 범위**: `plugins/develop-workflow/agents/architect-codex.md`, `architect-gemini.md`

**핵심 변경**:
1. 에러 발생 시 구조화된 에러 응답 형식 정의
2. MainAgent가 에러를 파싱할 수 있도록 명확한 형식

```markdown
## 에러 응답 형식

성공 시: 설계 결과 텍스트 그대로 반환
실패 시:
  ```
  [ERROR] {llm_name} 설계 실패
  원인: {error_message}
  ```
```

#### B1: outline.md 커맨드 업데이트

**작업 범위**: `plugins/develop-workflow/commands/outline.md`

**핵심 변경**:
1. Step 1 (코드베이스 스캔) 추가 (design.md와 동일하되 경량 버전)
2. 프롬프트에 코드베이스 컨텍스트 포함
3. Graceful Degradation 흐름 추가
4. 산출물 저장은 제외 (outline은 가볍게 유지)

#### B2: develop.md 커맨드 업데이트

**작업 범위**: `plugins/develop-workflow/commands/develop.md`

**핵심 변경**:
1. Phase 1 설명을 v2 워크플로우로 업데이트
2. Phase 3 진입 시 `.develop-workflow/design.md` 참조 로직 추가
3. state.json에 `design` 필드 추가 설명

#### B3: feature-workflow SKILL.md 업데이트

**작업 범위**: `plugins/develop-workflow/skills/feature-workflow/SKILL.md`

**핵심 변경**:
1. Phase 1 가이드라인에 코드베이스 스캔 + 사이드이펙트 분석 추가
2. "Do" 목록에 "기존 코드 패턴 확인" 추가
3. "Don't" 목록에 "코드베이스 무시하고 설계" 추가

#### C1: high-level-design SKILL.md 업데이트

**작업 범위**: `plugins/develop-workflow/skills/high-level-design/SKILL.md`

**핵심 변경**:
1. "코드베이스 인지 설계" 원칙 추가
2. 설계 품질 기준에 "기존 코드 일관성" 추가

#### C2: plugin.json 버전 범프

**작업 범위**: `plugins/develop-workflow/plugin.json`

**변경**: 버전을 minor 범프 (새 기능 추가이므로)
- `0.6.1` → `0.7.0`

---

## 5. 검증 계획

### 5.1 변경 후 검증 체크리스트

| # | 검증 항목 | 방법 |
|---|----------|------|
| T1 | plugin.json 스펙 유효성 | `make validate-specs` |
| T2 | 마크다운 경로 유효성 | `make validate-paths` |
| T3 | 버전 형식 유효성 | `make validate-versions` |
| T4 | ESLint (hooks 파일) | `npm run lint` |
| T5 | 기존 Rust 프로젝트 빌드 | `cargo fmt --check && cargo clippy && cargo test` |
| T6 | design.md 커맨드 frontmatter 유효성 | 수동 확인 |
| T7 | agent md frontmatter 유효성 | 수동 확인 |

### 5.2 수동 시나리오 테스트

| # | 시나리오 | 기대 결과 |
|---|---------|-----------|
| S1 | `/design "인증 기능 추가"` 실행 | Step 0-7 순서대로 진행, design.md 생성 |
| S2 | Codex CLI 미설치 상태에서 `/design` | Claude + Gemini로 진행, 실패 알림 표시 |
| S3 | `/develop` 실행 후 Phase 3 진입 | design.md 참조하여 컨텍스트 확보 |
| S4 | 세션 compaction 후 재개 | state.json + design.md로 컨텍스트 복원 |

---

## 6. 리스크 및 완화

| # | 리스크 | 확률 | 영향 | 완화 방안 |
|---|--------|------|------|----------|
| R1 | Step 1-2 추가로 설계 시간 증가 | 높음 | 낮음 | Glob/Grep 병렬 실행으로 최소화. 실제 토큰은 요약만 사용 |
| R2 | 코드베이스 컨텍스트가 너무 큰 경우 | 중간 | 중간 | 50줄 이내 요약 제한. 키워드 기반 선별적 수집 |
| R3 | 기존 `/design` 동작 변경으로 혼란 | 낮음 | 낮음 | 사용법(인자 형식)은 동일. 내부 프로세스만 개선 |
| R4 | design.md 파일이 너무 큰 경우 | 낮음 | 중간 | 핵심 정보만 기록. 상세 LLM 출력은 저장하지 않음 |

---

## 7. 요약

### v1 → v2 핵심 변경 요약

| 영역 | v1 | v2 |
|------|----|----|
| **코드베이스 인지** | 없음 (진공 설계) | Glob/Grep으로 기존 코드 컨텍스트 수집 |
| **사이드이펙트** | 원칙만 명시 | 구조화된 분석 단계 (Step 2) |
| **설계 프롬프트** | 요구사항만 | 요구사항 + 코드베이스 + 사이드이펙트 |
| **합의 분석** | 비구조적 | 합의 매트릭스 + ADR + tie-breaking 기준 |
| **Contract 검증** | 없음 | 5개 자동 검증 항목 (V1-V5) |
| **산출물 저장** | 대화 컨텍스트만 | `.develop-workflow/design.md` 영속화 |
| **외부 LLM 장애** | 암묵적 fallback | Graceful degradation + 사용자 알림 |
| **state.json** | phase/gates/checkpoints | + design 메타데이터 (iteration, llm_status 등) |

### 예상 효과

1. **구현 단계 리워크 50% 이상 감소**: 사이드이펙트를 설계에서 사전 파악
2. **설계 품질 향상**: 기존 코드 패턴을 반영한 현실적 설계
3. **compaction 내성 확보**: design.md로 설계 컨텍스트 항상 복원 가능
4. **투명성 향상**: 외부 LLM 장애 시 명확한 알림 + 합의 과정 가시화
