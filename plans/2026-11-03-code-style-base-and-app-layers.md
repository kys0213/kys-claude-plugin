# code-style 룰을 base · 언어 · 앱 성격 세 층으로 나눈다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-03 · 브랜치: feat/code-style-language-profiles · 이슈: 없음 · 승인 출처: 사용자 승인

## 배경

같은 브랜치의 직전 설계(언어 프로필)는 공통 룰 원본에 `roles:` 만 적고, 언어마다 둔 `profile.json` 으로 CLI 가 `paths:` 를 채웠다. 사용자 피드백은 이랬다.

- 설정이 한 겹 더 있어 복잡하다. 언어를 고르면 그대로 등록되는 편이 낫다.
- 언어 룰 안에 언어와 무관한 원칙이 섞여 있다. 같은 원칙이 Go·Rust 양쪽에 반복된다 (예: "에러 종류를 메시지 문자열로 가르지 않는다"가 5곳).
- 지금 룰은 CLI 애플리케이션을 전제로 쓴 부분이 있다. 애플리케이션 성격별 공통 컨벤션이 필요하다.

## 결정

### 1. 세 층과 원본 구조

```
rules/code-style/
  base/              # 언어·앱 성격과 무관한 원칙 (구 common)
  lang/go/ lang/rust/  # 언어별 관용구
  app/cli/ app/library/  # 앱 성격별 원칙
```

- 원본은 `lang/`·`app/` 아래로 나눠, 디렉토리 이름만으로 언어인지 앱 성격인지 안다. 앱 성격이나 언어를 늘릴 때 CLI 를 고치지 않는다.
- 설치는 평면이다: `.claude/rules/atelier/code-style/{base,go,cli}/`. 설치된 디렉토리 목록이 곧 설치된 것이다.
- 언어 이름과 앱 성격 이름이 겹치면 CLI 오류다.

### 2. 경로 매핑은 룰 파일에 직접 적는다 (방안 A)

base·앱 룰의 `paths:` 에 지원 언어 경로를 모두 적는다. 설치는 그대로 복사한다. `profile.json`·`roles:`·렌더링은 없앤다.

| 분류 | Go | Rust |
|---|---|---|
| 소스 전체 | `**/*.go` | `**/*.rs` |
| 테스트 | `**/*_test.go` | `**/tests/**/*.rs`, `**/tests.rs` |
| 진입점 | `**/main.go`, `**/cmd/**/*.go` | `**/main.rs`, `**/bin/**/*.rs` |
| 설정 | `**/config.go`, `**/options.go`, `**/config/**/*.go` | `**/config.rs`, `**/options.rs`, `**/config/**/*.rs` |
| 외부 경계 | `**/*_client.go`, `**/*_adapter.go`, `**/*_repository.go`, `**/*_handler.go`, `**/*_middleware.go` | `**/client.rs`, `**/*_client.rs`, `**/*_adapter.rs`, `**/adapter/**/*.rs`, `**/*_repository.rs`, `**/*_handler.rs`, `**/middleware.rs`, `**/*_middleware.rs` |
| 동시성 | `**/*_worker.go`, `**/*_pool.go`, `**/*_scheduler.go` | `**/worker.rs`, `**/*_worker.rs`, `**/*_pool.rs` |

- 이 표는 룰 작성 지침이라 레포 룰(`.claude/rules/`)로 둔다. 원본 그룹 디렉토리에 README 를 두면 룰로 설치되므로 두지 않는다.
- Go 만 쓰는 프로젝트에도 Rust 경로가 함께 설치된다. 그 확장자 파일이 없으면 로드되지 않으니 해가 없다.
- TS·Java·Kotlin 은 이번에 뺀다 (사용자: "rust, go 위주로, 나머지는 차차"). 언어를 더할 때는 `lang/<언어>/` 를 만들고 표에 열을 더해 base·앱 룰의 `paths:` 에 반영한다.

### 3. 앱 성격: `cli`, `library`

- `cli`: 종료 코드, stdout(결과)·stderr(진단), 기계가 읽을 출력, 비대화형 실행, 사용자에게 보이는 오류 메시지, CLI 는 인자 파싱 → 호출 → 출력만 하는 얇은 층.
- `library`: 공개 API 호환성, 전역 상태·싱글톤 API 금지, 로그·종료·출력은 호출자 몫, 설정은 인자로, 의존성 최소화.
- 둘 다 소스 전체 경로에 건다. 앱 성격을 고른 것 자체가 그 프로젝트에 해당한다는 신호다.
- `server`·`frontend` 는 다음에 쌓는다.

### 4. base 로 옮기는 언어 무관 원칙

| base 룰 | 경로 분류 | 출처 |
|---|---|---|
| errors (신규) | 소스 전체 | go-errors, go-error-definitions, go-boundary, rust-error-types, rust-errors-and-safety |
| logging (신규) | 소스 전체 | go-boundary, go-errors |
| concurrency (신규) | 동시성 | go-concurrency, rust-concurrency |
| abstractions (신규) | 소스 전체 | go-boundary, go-language-features, go-config, rust-boundary, rust-ownership-and-types, rust-principles |
| naming (신규) | 소스 전체 | go-naming, rust-naming |
| entrypoint (신규) | 진입점 | go-main, rust-entrypoint |
| testing (보강) | 테스트 | go-testing, rust-errors-and-safety |

- 언어 룰에는 그 언어의 관용구만 남기고, base 로 옮긴 문장은 지운다.
- 앱 성격에 따라 달라지는 원칙(전역 상태, 라이브러리의 로그·종료)은 base 가 아니라 앱 룰로 보낸다.
- Java·Kotlin·TS 관용과 부딪히는 원칙(`Get` 접두사 금지, "에러는 반환한다")은 base 로 올리지 않는다. base 는 "호출자에게 알린다(반환이든 예외든)"로 쓴다.

### 5. CLI

- `install --lang <L>... [--app <A>...]`: 언어는 하나 이상 필요, 앱은 선택.
- `update`·`check`: `--lang`·`--app` 둘 다 없으면 설치된 전부. 지정하면 그것과 base.
- `uninstall --lang/--app`: 지정한 그룹을 백업 후 지운다. 언어가 하나도 안 남으면 base·앱·기록도 지운다.
- `restore --rule <그룹>/<파일>`: 그룹은 `base` 또는 설치된 언어·앱.
- `installed.json`: `{"langs": [...], "apps": [...], "rules": [...]}`. EXCLUDED 판정은 그대로다.
- 기록이 없는 이전 설치본: 설치 루트의 디렉토리를 전부 언어로 본다. 구 `common/` 은 플러그인에 없는 언어로 판정돼 ORPHANED 로 보고되고 update 에서 백업 후 지워진다. base 는 NEW 로 설치된다.

## 사이드이펙트

| 대상 | 영향 | 처리 |
|---|---|---|
| main 의 설치본 (`common/`·`go/`·`rust/`) | 첫 update 에서 `common/*` ORPHANED, `base/*` NEW | 정상 경로. 커맨드 문서에 적는다 |
| 이 브랜치의 프로필 코드 | `profile.rs`·`render.rs`·`role.rs`·`profile.json` 이 쓸모없어진다 | 지운다. 머지 전이라 호환 부담이 없다 |
| 이 브랜치의 `ts`·`java`·`kotlin` | 범위에서 빠진다 | 지운다 |
| `commands/code-style.md`, README | 그룹 구조·`--app`·마이그레이션 | 함께 고친다 |
| 레포 `.claude/rules/` | 매핑 표를 담을 작성 지침이 없다 | `code-style-authoring.md` 를 추가한다 |

## 버린 대안

| 대안 | 이유 |
|---|---|
| 언어 프로필로 CLI 가 경로를 채운다 (직전 설계) | 설정이 한 겹 더 있어 복잡하다 (사용자) |
| base 룰에 `paths:` 를 두지 않고 항상 로드 | 테스트·경계 룰까지 모든 작업에 로드된다 |
| 원본도 평면(`rules/code-style/go`, `.../cli`) | 디렉토리가 언어인지 앱 성격인지 알려면 CLI 에 목록을 박아야 한다. 앱 성격을 늘릴 때마다 CLI 를 고치게 된다 |
| 설치도 `lang/`·`app/` 로 중첩 | 한 단계 깊어져 설치된 것을 한눈에 보기 어렵다 |

## 구현 순서

1. CLI 테스트를 새 구조로 바꾼다 (fail) → 구현 (pass). 렌더링 코드를 지운다.
2. 원본 이동: `common/` → `base/` (경로는 지금과 같은 Go·Rust 목록), `go/`·`rust/` → `lang/`, 프로필과 ts·java·kotlin 삭제.
3. base 신규·보강 룰, `app/cli`·`app/library` 작성, 언어 룰에서 옮긴 문장 정리.
4. 작성 지침 룰, 커맨드 문서·README.
5. `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `make validate`.
