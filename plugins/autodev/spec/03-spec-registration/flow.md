# Flow 3: 스펙 등록 (Spec 모드)

### 시나리오

사용자가 디자인 스펙을 등록하여 자율 구현 루프를 시작한다.

### 등록 방법

스펙 등록은 **대상 레포에서 실행 중인 Claude Code 세션**에서 `/add-spec` 명령으로 수행한다.
autodev plugin이 설치되어 있으면 이 command가 자동으로 사용 가능.

```bash
# 레포에서 작업 중인 Claude Code 세션에서:
/add-spec ./SPEC.md

# 또는 스펙 없이 대화형으로 작성:
/add-spec
```

### 왜 레포 컨텍스트에서 실행하는가

```
CLI 방식 (autodev spec add):
  → 스펙 파일만 읽고 구조 검증
  → 레포의 실제 상태를 모름

/add-spec (레포 Claude 세션에서):
  → 코드베이스를 읽고 기술 스택 자동 감지
  → 기존 테스트 환경 발견 + 자동 구성 제안
  → .claude/rules/ 참조하여 컨벤션 반영
  → 누락 섹션을 대화형으로 보완
```

### 스펙 필수 섹션

스펙 등록 시 **5개 필수 섹션**을 검증한다. 모든 필수 섹션이 있어야 Active 상태로 전환.

| # | 섹션 | 필수 | 목적 | 검증 기준 |
|---|------|------|------|----------|
| 1 | **요구사항 (Requirements)** | ✅ | 무엇을 구현할 것인가 | 기능 목록이 구체적인지 |
| 2 | **아키텍처/컴포넌트** | ✅ | 어떤 구조로 구현할 것인가 | 모듈/인터페이스 정의 존재 |
| 3 | **기술 스택 + 컨벤션** | ✅ | 언어, 프레임워크, 규칙 | 구현 언어/프레임워크 명시 |
| 4 | **테스트 환경 구성** | ✅ | 어떻게 검증할 것인가 | 실행 가능한 테스트 명령 존재 |
| 5 | **Acceptance Criteria** | ✅ | 완료의 정량적 기준 | 검증 가능한 조건 목록 존재 |
| 6 | **의존성/제약사항** | 권장 | 외부 시스템, 선행 조건 | - |

### 왜 4, 5번이 필수인가

```
테스트 환경 없음 → 구현 후 검증 불가 → closed loop 불가
Acceptance Criteria 없음 → Complete 판정 불가 → 자율 루프 종료 불가
```

---

### /add-spec 실행 플로우

```
사용자: /add-spec ./SPEC.md (레포의 Claude 세션에서)
         │
         ▼
  ┌──────────────────────────────────┐
  │  1. 레포 컨텍스트 분석            │
  │     → 코드베이스 구조 스캔        │
  │     → 기술 스택 감지              │
  │     → 기존 테스트 환경 발견        │
  │     → .claude/rules/ 로드         │
  └──────────┬───────────────────────┘
             │
             ▼
  ┌──────────────────────────────────┐
  │  2. 스펙 파싱 + 구조 검증         │
  │                                  │
  │  ✅ 요구사항 존재                  │
  │  ✅ 아키텍처 존재                  │
  │  ⚠️ 기술 스택 — 감지됨, 컨벤션 미명시│
  │  ❌ 테스트 환경 — 누락            │
  │  ❌ Acceptance Criteria — 누락   │
  └──────────┬───────────────────────┘
             │
        누락 항목 있음?
        ├── YES → 대화형 보완 (아래 참조)
        │
        └── NO (전부 통과)
             │
             ▼
  ┌──────────────────────────────────┐
  │  3. claw.enabled 확인             │
  │     false? → "Spec 모드에는 Claw │
  │     가 필요합니다. 활성화할까요?" │
  │     → 사용자 승인 시 자동 활성화  │
  └──────────┬───────────────────────┘
             │
             ▼
  ┌──────────────────────────────────┐
  │  4. autodev spec add 실행        │
  │     → DB 저장 (status: Active)   │
  │     → spec_issues 초기화         │
  │     → force_claw_next_tick       │
  └──────────┬───────────────────────┘
             │
             ▼ (다음 Claw 틱)
  ┌──────────────────────────────────┐
  │  5. Claw(autodev agent): 스펙 분석│
  │     → decompose skill 기반 분해  │
  │     → 각 이슈에 autodev:analyze  │
  │     → 의존성 그래프 기반 순서 결정 │
  └──────────────────────────────────┘
             │
             ▼
  기존 파이프라인 (Analyze → Implement → Review → ...)
```

### 대화형 보완 (누락 섹션)

레포 컨텍스트를 활용하여 누락 섹션을 대화형으로 보완한다.

```
🔍 스펙 검증 결과:

✅ 요구사항 — 3개 기능 정의됨
✅ 아키텍처 — 모듈 4개 정의됨
⚠️ 기술 스택 — Rust + Axum 감지됨, 컨벤션 미명시
❌ 테스트 환경 — 누락
❌ Acceptance Criteria — 누락

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

[1/3] 기술 스택 보완

레포 분석 결과:
  언어: Rust (Cargo.toml 감지)
  프레임워크: Axum (dependencies에서 감지)
  DB: PostgreSQL + Redis (docker-compose.yml 감지)
  기존 .claude/rules/: 2개 파일 존재

컨벤션에 추가할 내용이 있나요?
[1: 감지된 내용으로 자동 구성 (Recommended)]
[2: 직접 작성]

> 1

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

[2/3] 테스트 환경 구성

레포에서 감지된 테스트 설정:
  - Cargo.toml: [dev-dependencies] tokio, mockall
  - docker-compose.yml: postgres:15, redis:7
  - tests/ 디렉토리: 3개 테스트 파일 존재
  - 기존 패턴: #[tokio::test] + mockall

제안하는 테스트 환경:
  unit: cargo test -p auth
  integration: docker compose up -d && cargo test --test integration
  e2e: docker compose -f docker-compose.test.yml up -d && cargo test --test e2e

수정할 부분이 있나요?
[1: 제안대로 적용 (Recommended)]
[2: 수정 후 적용]

> 1

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

[3/3] Acceptance Criteria

요구사항 기반으로 제안합니다:
  - [ ] POST /auth/login → JWT 반환 (200)
  - [ ] 만료 토큰으로 요청 → 401 반환
  - [ ] POST /auth/refresh → 새 토큰 반환 (200)
  - [ ] 무효 토큰으로 요청 → 403 반환
  - [ ] cargo test -p auth 전체 통과
  - [ ] e2e 테스트 전체 통과

추가하거나 수정할 항목이 있나요?

> rate limiting 테스트도 추가해줘

업데이트:
  - [ ] POST /auth/login 5회 연속 실패 → 429 반환 (추가)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

✅ 스펙 검증 완료!

  요구사항: 3개 기능
  아키텍처: 4개 모듈
  기술 스택: Rust + Axum + PostgreSQL + Redis
  테스트 환경: unit + integration + e2e
  Acceptance Criteria: 7개 항목

autodev에 등록합니다...
→ autodev spec add --title "Auth Module v2" --file /tmp/spec-validated.md --repo org/repo
✅ 등록 완료. Claw가 다음 틱에서 이슈를 분해합니다.
```

### /add-spec 대화형 작성 (파일 없이)

스펙 파일 없이 `/add-spec`만 실행하면 처음부터 대화형으로 작성:

```
> /add-spec

스펙을 새로 작성합니다. 이 레포에서 무엇을 구현하고 싶으신가요?

> JWT 기반 인증 모듈을 만들고 싶어. 토큰 발급, 갱신, 검증이 필요하고 세션은 Redis에 저장

레포를 분석하여 스펙 초안을 작성합니다...

[요구사항, 아키텍처, 기술 스택을 자동 생성]

이 초안을 기반으로 진행할까요?
[1: 진행 (Recommended)]
[2: 수정]

> 1

[이후 테스트 환경 + Acceptance Criteria 대화형 보완]
```

---

### 스펙 예시 (유효)

```markdown
# Auth Module v2 Spec

## 1. 요구사항
- JWT 기반 인증 미들웨어
- 토큰 발급/갱신/검증 API
- 세션 스토리지 어댑터 (Redis)

## 2. 아키텍처
- `auth/middleware.rs` — 미들웨어
- `auth/token.rs` — JWT 발급/검증
- `auth/session.rs` — 세션 스토리지 trait + Redis impl

## 3. 기술 스택
- Rust, Axum, jsonwebtoken crate
- Redis (세션)
- 컨벤션: CLAUDE.md 준수

## 4. 테스트 환경
- unit: `cargo test -p auth`
- e2e: `docker compose -f docker-compose.test.yml up -d && cargo test --test e2e_auth`
- docker-compose.test.yml: postgres + redis + app

## 5. Acceptance Criteria
- [ ] POST /auth/login → JWT 반환 (200)
- [ ] 만료 토큰 → 401 반환
- [ ] POST /auth/refresh → 새 토큰 반환 (200)
- [ ] 무효 토큰 → 403 반환
- [ ] `cargo test -p auth` 전체 통과
- [ ] e2e 테스트 전체 통과
```

### 스펙 예시 (거부 → 대화형 보완)

```markdown
# Auth Module

## 요구사항
- 인증을 구현한다
```

```
→ /add-spec 결과:

❌ 검증 실패:
  ⚠️ 요구사항 — "인증을 구현한다"는 너무 추상적입니다
  ❌ 아키텍처 — 누락
  ❌ 기술 스택 — 누락 (레포에서 Rust+Axum 감지됨)
  ❌ 테스트 환경 — 누락
  ❌ Acceptance Criteria — 누락

구체적으로 어떤 인증을 구현하시려는 건가요?
  - JWT? 세션 기반? OAuth?
  - 어떤 API 엔드포인트가 필요한가요?

> JWT 기반으로. 로그인, 토큰 갱신, 로그아웃이 필요해

[이후 대화형으로 5개 필수 섹션 완성]
```
