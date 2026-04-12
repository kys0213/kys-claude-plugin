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

모노레포에서의 범용화 패턴은 Section 7을 참조합니다.

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

## 7. paths 범용화 원칙

### 핵심 원칙

paths는 **레이어 구조**(역할)를 표현해야 하며, **위치**(컨테이너 경로)를 표현해서는 안 됩니다.

### 판별 기준

- **레이어**: 파일의 역할을 나타내는 디렉토리 (`core/`, `commands/`, `handlers/`, `service/` 등)
- **위치**: 레이어가 속한 컨테이너 (`plugins/git-utils/`, `packages/api/`, `apps/web/` 등)
- paths에는 레이어만 남기고, 위치는 `**/`로 대체합니다.

### 범용화 알고리즘

1. **레이어 식별**: 수집된 경로에서 역할을 나타내는 디렉토리를 구분
2. **컨테이너 추상화**: 레이어 상위의 컨테이너 경로를 `**/`로 대체
3. **교차 검증**: 범용 패턴이 동일 레이어의 파일만 매칭하는지 Glob 테스트
4. **범위 조정**: 의도하지 않은 파일이 포함되면 패턴을 좁히거나 예외 명시

### 예시 (원칙 적용)

| 수집된 경로 | 레이어 | 컨테이너 | 범용 패턴 |
|---|---|---|---|
| `plugins/git-utils/src/core/git.ts` | `core/` | `plugins/git-utils/src/` | `**/core/**/*.ts` |
| `apps/web/handlers/auth.go` | `handlers/` | `apps/web/` | `**/handlers/**/*.go` |
| `internal/auth/service/auth_svc.go` | `service/` | `internal/auth/` | `**/service/**/*.go` |

### 예외 (범용화하지 않는 경우)

의도적으로 범위를 제한해야 하는 paths는 특정 경로를 그대로 사용합니다:

- 자동 생성 코드 (`generated/`, `__generated__/`)
- 마이그레이션 파일 (`migrations/`)
- 루트 설정 파일 (`*.config.ts`, `*.toml`)

### 체크리스트

- [ ] paths가 레이어(역할)를 표현하는가, 위치(컨테이너)를 표현하는가?
- [ ] 컨테이너 경로가 `**/`로 대체되었는가? (예외 제외)
- [ ] 동일 레이어가 복수 위치에 존재할 때 하나의 패턴으로 통합되었는가?
- [ ] Glob 테스트로 의도한 파일만 매칭되는지 확인했는가?
- [ ] 의도적 범위 제한이 필요한 경우 예외로 명시했는가?

---

## 8. 프로젝트 가치관 인터뷰 카테고리

scaffold-rules에서 프로젝트 가치관을 수집할 때 다음 3개 카테고리를 다룹니다.
자동감지 가능한 항목은 먼저 채우고, 사용자에게는 확인만 받습니다.
트레이드오프는 양자택일이 아닌 스펙트럼으로 제시합니다.

### A. 프로젝트 맥락

| 항목 | 자동감지 소스 |
|------|-------------|
| 프로젝트 유형 | `package.json`의 `main`/`types` → 라이브러리, `bin` → CLI, deploy 스크립트 → 제품 |
| 팀 구성 | `.github/CODEOWNERS`, `git shortlog -sn` 커밋터 수 |
| 주요 독자 | document-analyzer 결과 (톤, 설명 깊이) |

### B. 엔지니어링 트레이드오프

| 트레이드오프 | 왼쪽 | 오른쪽 | 기본값 |
|---|---|---|---|
| 가독성 ↔ 성능 | 가독성 우선 | 성능 우선 | 가독성 (명백한 병목 제외) |
| 명시성 ↔ 간결성 | 보일러플레이트 허용 | DRY/매직 허용 | 명시적 |
| 안정성 ↔ 속도 | 테스트/리뷰 필수 | 빠른 이터레이션 | 안정성 |
| 추상화 시점 | 이른 추상화 | 명시적 중복 | Rule of 3 (3회 반복 후 추상화) |

### C. 문서화 컨벤션

| 항목 | 자동감지 소스 |
|------|-------------|
| 문서 독자 | document-analyzer 톤/설명 깊이 분석 |
| 톤 | document-analyzer 종결어미/격식 분석 |
| 언어 | document-analyzer 언어 혼용 패턴 |

---

## 9. CLAUDE.md vs .claude/rules/ 배치 기준

Section 8의 인터뷰 결과를 어디에 저장할지 결정하는 기준입니다.

| 내용 | 위치 | 이유 |
|------|------|------|
| 프로젝트 정체성 (유형, 단계, 팀) | CLAUDE.md | 항상 필요, 거의 변하지 않음 |
| 엔지니어링 가치 (트레이드오프) | CLAUDE.md | 모든 코드에 적용, 레이어 무관 |
| 문서화 컨벤션 | CLAUDE.md | 모든 문서에 적용, 경로 무관 |
| 아키텍처 레이어별 코딩 규칙 | `.claude/rules/` + `paths:` | 특정 파일 수정 시에만 필요 |

CLAUDE.md 섹션의 구체적인 템플릿과 예시는 scaffold-rules Command가 인터뷰 결과를 기반으로 생성합니다.

---

## 10. LSP-Enhanced Analysis

코드 구조 분석 시 LSP가 사용 가능하면 Glob/Grep보다 정확한 정보를 얻을 수 있습니다.

### 전략

```
1. LSP 사용 가능 여부 확인 (which rust-analyzer, gopls, etc.)
   ├─ 가능 → LSP 기반 정밀 분석
   └─ 불가 → Glob/Grep fallback + 설치 안내
```

### 활용 가능한 operation

| LSP operation | 얻는 정보 |
|---|---|
| `documentSymbol` | 레이어 패턴 (클래스/함수/인터페이스 비율) |
| `goToImplementation` | 추상화 수준 (DIP 준수 여부) |
| `findReferences` | 의존 방향, 모듈 경계 |
| `workspaceSymbol` | 전체 레이어 분류 |

### 언어별 LSP 설치 안내

| 언어 | LSP | 설치 명령 |
|------|-----|----------|
| Rust | rust-analyzer | `rustup component add rust-analyzer` |
| Go | gopls | `go install golang.org/x/tools/gopls@latest` |
| TypeScript/JS | typescript-language-server | `npm i -g typescript-language-server typescript` |
| Python | pylsp | `pip install python-lsp-server` |
