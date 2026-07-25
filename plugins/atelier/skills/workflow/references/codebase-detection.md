# 코드베이스 감지 (detection)

코드베이스의 언어·프레임워크·디렉토리 구조를 감지하는 시그널. codebase-analyzer 가 분석 시 로드한다.

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
