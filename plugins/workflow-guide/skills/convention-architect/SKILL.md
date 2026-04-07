---
name: convention-architect
description: 코드베이스 분석 기반 .claude/rules 구조 설계 지식 — 아키텍처 레이어별 규칙 매핑, paths frontmatter 전략, 언어별 템플릿
user-invocable: false
---

# Convention Architect Skill

> 코드베이스를 분석하여 `.claude/rules/` 구조를 설계하는 데 필요한 도메인 지식

이 스킬은 코드베이스의 언어, 프레임워크, 디렉토리 구조를 기반으로 최적의 `.claude/rules/` 파일 구조를 설계할 때 참조하는 지식을 제공합니다.

---

## 1. 코드베이스 감지 시그널

### 언어/프레임워크 감지 파일

| 시그널 파일 | 언어/프레임워크 |
|---|---|
| `package.json` | Node.js / TypeScript / JavaScript |
| `tsconfig.json` | TypeScript |
| `go.mod` | Go |
| `Cargo.toml` | Rust |
| `pyproject.toml`, `requirements.txt`, `setup.py` | Python |
| `pom.xml`, `build.gradle` | Java / Kotlin |
| `Gemfile` | Ruby |
| `pubspec.yaml` | Dart / Flutter |
| `*.csproj`, `*.sln` | C# / .NET |

### 프레임워크 세부 감지

| 조건 | 프레임워크 |
|---|---|
| `package.json`에 `@nestjs/core` | NestJS |
| `package.json`에 `next` | Next.js |
| `package.json`에 `react` | React |
| `package.json`에 `vue` | Vue.js |
| `package.json`에 `express` | Express |
| `package.json`에 `hono` | Hono |
| `go.mod`에 `go-chi/chi` | Chi |
| `go.mod`에 `uber-go/fx` | Fx |
| `go.mod`에 `gin-gonic/gin` | Gin |
| `Cargo.toml`에 `actix-web` | Actix Web |
| `Cargo.toml`에 `axum` | Axum |
| `pyproject.toml`에 `fastapi` | FastAPI |
| `pyproject.toml`에 `django` | Django |

### 디렉토리 구조 패턴

| 패턴 | 감지 기준 | 예시 |
|---|---|---|
| **Layered** | `controllers/`, `services/`, `repositories/`, `models/` | NestJS, Spring |
| **Domain-driven** | `domain/`, `internal/`, `pkg/` 하위에 도메인별 디렉토리 | Go Hex/Clean |
| **Feature-based** | `features/`, `modules/` 하위에 기능별 디렉토리 | React, Angular |
| **Flat** | 루트에 모든 파일이 혼재 | 소규모 프로젝트 |
| **Monorepo** | `packages/`, `apps/`, `services/` | Turborepo, Nx |

---

## 2. 아키텍처 레이어 → 규칙 파일 매핑

### 공통 관심사 (언어 무관)

모든 프로젝트에 적용할 수 있는 보편적 관심사:

| 관심사 | 규칙 파일명 | 핵심 원칙 |
|---|---|---|
| 진입점 (Entrypoint) | `handler.md` 또는 `controller.md` | HTTP/CLI 파싱만, 로직 위임 |
| 비즈니스 로직 | `service.md` | 인터페이스 정의, DI, 트랜잭션 조율 |
| 데이터 접근 | `repository.md` | 추상화, 쿼리 캡슐화 |
| 도메인 모델 | `entity.md` 또는 `model.md` | 불변식, 밸류 오브젝트 |
| DTO / 계약 | `dto.md` | 입출력 타입 분리, 직렬화 |
| 모듈 경계 | `module-boundary.md` | 단방향 의존, 순환 금지 |
| 테스트 | `testing.md` | mock 전략, 테스트 구조 |
| 에러 처리 | `error-handling.md` | 에러 타입, 전파 규칙 |

### 프론트엔드 전용 관심사

| 관심사 | 규칙 파일명 | 핵심 원칙 |
|---|---|---|
| 컴포넌트 | `component.md` | props 설계, 합성, 접근성 |
| 훅/컴포저블 | `hook.md` | 상태 캡슐화, 재사용 |
| 상태 관리 | `store.md` | 전역/로컬 분리, 구독 패턴 |
| 페이지/라우팅 | `page.md` | 데이터 페칭, 레이아웃 |

---

## 3. `paths:` Frontmatter 전략

### Lazy Context Injection 원칙

규칙 파일은 `paths:` frontmatter를 통해 관련 파일 수정 시에만 Claude 컨텍스트에 로드됩니다. 이를 통해 토큰 낭비를 방지합니다.

```yaml
---
paths:
  - "**/*handler*.go"        # 파일명 패턴
  - "**/*.controller.ts"     # 확장자 + 접미사 패턴
  - "**/controllers/**"      # 디렉토리 패턴
---
```

### 패턴 작성 가이드

| 전략 | 패턴 예시 | 적합한 상황 |
|---|---|---|
| **파일명 접미사** | `**/*.service.ts` | NestJS 등 명명 규칙이 있는 프레임워크 |
| **파일명 포함** | `**/*service*.go` | Go 등 접미사 규칙이 없는 언어 |
| **디렉토리 기반** | `**/services/**` | 디렉토리로 분류하는 프로젝트 |
| **복합** | `["**/*.service.ts", "**/services/**"]` | 혼용 프로젝트 |

### 모노레포에서의 paths

모노레포에서는 패키지 레벨 prefix를 제거하고 범용 패턴을 사용합니다 (Section 7 참조):

```yaml
# Bad: 패키지명 하드코딩
paths:
  - "packages/api/**/*handler*.ts"
  - "apps/web/**/components/**"

# Good: 범용 패턴
paths:
  - "**/*handler*.ts"
  - "**/components/**"
```

단, 패키지 간 컨벤션이 다른 경우에는 패키지별 규칙 파일을 분리합니다.

---

## 4. 규칙 파일 템플릿

### 표준 구조

모든 규칙 파일은 다음 구조를 따릅니다:

```markdown
---
paths:
  - "<glob pattern>"
---

# <레이어명> Convention

> 한 줄 요약

## 원칙

1. **원칙 1**: 설명
2. **원칙 2**: 설명

## DO

- 구체적인 좋은 예시 (코드 포함)

## DON'T

- 구체적인 나쁜 예시 (코드 포함)

## 체크리스트

- [ ] 확인 항목 1
- [ ] 확인 항목 2
```

### DO/DON'T 예시 (레이어별)

#### Handler / Controller

```markdown
## DO

- HTTP 파싱과 응답 변환만 담당
- 에러를 적절한 HTTP 상태 코드로 매핑
- 입력 유효성 검사는 DTO/스키마에 위임

## DON'T

- 비즈니스 로직을 핸들러에 직접 구현
- 데이터베이스를 직접 호출
- 다른 핸들러를 호출
```

#### Service

```markdown
## DO

- 인터페이스를 먼저 정의하고 구현
- 생성자 주입으로 의존성 받기
- 트랜잭션 경계를 서비스 레이어에서 관리

## DON'T

- HTTP 요청/응답 객체에 의존
- 구체 구현에 직접 의존 (DIP 위반)
- 하나의 서비스에 5개 이상 메서드 (SRP 위반 가능성)
```

#### Repository

```markdown
## DO

- 데이터 접근 로직만 캡슐화
- 쿼리를 메서드로 표현 (FindByID, ListByStatus 등)
- 인터페이스로 정의하여 mock 가능하게

## DON'T

- 비즈니스 로직을 리포지토리에 구현
- SQL/쿼리를 서비스 레이어에 노출
- 여러 테이블을 조인하는 복잡한 로직 (서비스로 분리)
```

---

## 5. Gap 분석 모드

기존 `.claude/rules/` 파일이 있을 때 gap 분석을 수행합니다.

### 분석 기준

1. **누락된 레이어**: 코드베이스에 해당 레이어가 존재하지만 규칙이 없는 경우
2. **paths 미설정**: 규칙 파일에 `paths:` frontmatter가 없어 항상 로드되는 경우
3. **과도한 범위**: `paths:` 패턴이 너무 넓어 불필요하게 자주 로드되는 경우
4. **언어 미지원**: 모노레포에서 특정 언어의 규칙이 누락된 경우

---

## 6. 다중 언어/프레임워크 레포 전략

### Prefix 전략

2개 이상의 언어/프레임워크가 혼용된 레포에서는 prefix로 구분합니다:

```
.claude/rules/
├── go-handler.md          # Go 핸들러 규칙
├── go-service.md          # Go 서비스 규칙
├── ts-controller.md       # TypeScript 컨트롤러 규칙
├── ts-service.md          # TypeScript 서비스 규칙
├── react-component.md     # React 컴포넌트 규칙
└── testing.md             # 공통 테스트 규칙 (언어 무관)
```

### 공통 규칙 vs 언어별 규칙

- **공통**: `module-boundary.md`, `error-handling.md`, `testing.md`
- **언어별**: prefix를 붙여 분리 (`go-`, `ts-`, `py-`, `rs-`)

---

## 7. 범용 패턴 변환 전략 (Generalization)

### 절대 경로 → 와일드카드 경로 변환

`paths:` frontmatter에는 특정 프로젝트/패키지 경로가 아닌 범용 와일드카드 패턴을 사용합니다. 이를 통해 규칙이 코드베이스 전체에 일관되게 적용됩니다.

| Before (특정 경로) | After (범용 패턴) |
|---|---|
| `plugins/git-utils/src/core/git.ts` | `**/src/core/**/*.ts` |
| `plugins/git-utils/src/commands/commit.ts` | `**/src/commands/**/*.ts` |
| `plugins/git-utils/src/cli.ts` | `**/src/cli.ts` |
| `packages/api/handlers/user_handler.go` | `**/handlers/**/*.go` |
| `internal/auth/service/auth_service.go` | `**/*service*.go` |
| `apps/web/src/components/Button.tsx` | `**/components/**/*.tsx` |

### 레이어 구조의 공통성 판단

동일한 아키텍처 레이어가 여러 패키지/모듈에 걸쳐 나타나면, 하나의 범용 패턴으로 통합합니다:

```
# 여러 패키지에 handlers/ 디렉토리가 존재
packages/auth/handlers/login.go
packages/user/handlers/profile.go
packages/order/handlers/checkout.go

# → 하나의 범용 패턴으로 통합
paths: ["**/handlers/**/*.go"]
```

### 단일 위치 범용화 원칙

현재 한 곳에만 존재하는 레이어라도 `**/` prefix를 사용합니다. 코드베이스가 확장되어 동일 레이어가 추가되면 규칙이 자동으로 적용됩니다.

```yaml
# Bad: 특정 위치에 고정
paths: ["src/services/**/*.ts"]

# Good: 향후 확장 대응
paths: ["**/services/**/*.ts"]
```

### 모노레포에서의 범용화 패턴

모노레포에서는 패키지 레벨 prefix를 제거하되, 레이어 디렉토리는 보존합니다:

```yaml
# Bad: 패키지명 하드코딩
paths:
  - "packages/api/src/controllers/**"
  - "packages/admin/src/controllers/**"

# Good: 패키지명 제거, 레이어만 보존
paths:
  - "**/src/controllers/**"
```

단, 패키지 간 컨벤션이 다른 경우에는 패키지별 규칙 파일을 분리합니다.

### 체크리스트

규칙 파일의 `paths:` 패턴을 작성한 후 아래 항목을 확인합니다:

- [ ] 모든 `paths:` 패턴이 `**/`로 시작하는가? (특정 패키지명 하드코딩 금지)
- [ ] 플러그인/패키지명이 패턴에 포함되어 있지 않은가?
- [ ] 동일 레이어의 여러 경로가 하나의 범용 패턴으로 통합되었는가?
- [ ] Glob으로 매칭 테스트하여 의도한 파일만 매칭되는가?
- [ ] 패턴이 너무 넓어 관련 없는 파일까지 매칭되지 않는가?
