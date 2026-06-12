# 코드베이스 감지 (detection)

코드베이스의 언어·프레임워크·디렉토리 구조를 감지하는 시그널과 LSP 활용 전략. codebase-analyzer 가 분석 시 로드한다.

## 1. 언어/프레임워크 감지 파일

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

## 2. 프레임워크 세부 감지

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

## 3. 디렉토리 구조 패턴

| 패턴 | 감지 기준 | 예시 |
|---|---|---|
| **Layered** | `controllers/`, `services/`, `repositories/`, `models/` | NestJS, Spring |
| **Domain-driven** | `domain/`, `internal/`, `pkg/` 하위에 도메인별 디렉토리 | Go Hex/Clean |
| **Feature-based** | `features/`, `modules/` 하위에 기능별 디렉토리 | React, Angular |
| **Flat** | 루트에 모든 파일이 혼재 | 소규모 프로젝트 |
| **Monorepo** | `packages/`, `apps/`, `services/` | Turborepo, Nx |

---

## 4. LSP-Enhanced Analysis

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
