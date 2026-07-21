# 코드베이스 감지 (detection)

코드베이스의 언어·프레임워크·디렉토리 구조를 감지하는 시그널과 LSP 활용 전략. codebase-analyzer 가 분석 시 로드한다.

## 1. 언어/프레임워크 감지 파일

루트의 매니페스트 파일로 언어를 판별합니다 (스캔 체크리스트): `package.json`(Node.js/TS/JS), `tsconfig.json`(TypeScript), `go.mod`(Go), `Cargo.toml`(Rust), `pyproject.toml`/`requirements.txt`/`setup.py`(Python), `pom.xml`/`build.gradle`(Java/Kotlin), `Gemfile`(Ruby), `pubspec.yaml`(Dart/Flutter), `*.csproj`/`*.sln`(C#/.NET).

## 2. 프레임워크 세부 감지

의존성 목록에서 잘 알려진 프레임워크(NestJS, Next.js, React, Vue.js, Express, Gin, Actix Web, Axum, FastAPI, Django 등)는 패키지명으로 바로 식별됩니다. 아래는 이름만으로 유추하기 어려운 항목입니다.

| 조건 | 프레임워크 |
|---|---|
| `package.json`에 `hono` | Hono |
| `go.mod`에 `go-chi/chi` | Chi |
| `go.mod`에 `uber-go/fx` | Fx (DI 프레임워크) |

## 3. 디렉토리 구조 패턴

| 패턴 | 감지 기준 |
|---|---|
| **Layered** | `controllers/`, `services/`, `repositories/`, `models/` |
| **Domain-driven** | `domain/`, `internal/`, `pkg/` 하위에 도메인별 디렉토리 |
| **Feature-based** | `features/`, `modules/` 하위에 기능별 디렉토리 |
| **Flat** | 루트에 모든 파일이 혼재 |
| **Monorepo** | `packages/`, `apps/`, `services/` |

---

## 4. LSP-Enhanced Analysis

코드 구조 분석 시 LSP가 사용 가능하면 Glob/Grep보다 정확한 정보를 얻을 수 있으므로, 가능하면 LSP 기반으로 분석하고 불가능할 때만 Glob/Grep으로 대체합니다.

### 활용 가능한 operation

| LSP operation | 얻는 정보 |
|---|---|
| `documentSymbol` | 레이어 패턴 (클래스/함수/인터페이스 비율) |
| `goToImplementation` | 추상화 수준 (DIP 준수 여부) |
| `findReferences` | 의존 방향, 모듈 경계 |
| `workspaceSymbol` | 전체 레이어 분류 |

### 언어별 LSP

Rust는 rust-analyzer, Go는 gopls, TypeScript/JS는 typescript-language-server, Python은 pylsp를 사용합니다 (설치 명령은 각 도구의 표준 방식을 따름).
