---
paths:
  - "plugins/atelier/rules/code-style/**"
---

# code-style 룰 작성 지침

atelier 의 code-style 룰 원본은 세 층이다. 각 층은 설치될 때 그대로 복사되고, 룰은 자기 `paths:` 에 걸린 파일을 Read 할 때만 로드된다.

| 층 | 원본 위치 | 담는 것 |
|---|---|---|
| base | `base/` | 언어·앱 성격과 무관한 원칙. 모든 설치에 함께 깔린다 |
| 언어 | `lang/<언어>/` | 그 언어의 관용구와 메커니즘 |
| 앱 성격 | `app/<성격>/` | cli·library 처럼 애플리케이션 성격에 따라 달라지는 원칙 |

## 어느 층에 둘까

- DO: 다른 언어로 옮겨 써도 문장이 그대로 맞으면 base 에 둔다. 예시는 의사코드로 쓴다.
- DO: 언어 기능·표준 라이브러리 이름이 있어야 말이 되는 문장만 언어 층에 둔다 (`%w`, `errors.Is`, `?`, `#[must_use]`). 원칙은 base 에, 그 원칙을 그 언어로 지키는 방법은 언어 층에 둔다.
- DO: CLI 에는 맞고 라이브러리에는 틀린 것처럼, 애플리케이션 성격에 따라 답이 달라지면 앱 성격 층에 둔다 (종료 코드, 전역 상태, 출력).
- DON'T: 같은 원칙을 두 층에 다시 쓴다. 겹치면 base 쪽만 남기고 언어 층에는 메커니즘만 남긴다.
- DON'T: 특정 언어 관용과 부딪히는 원칙을 base 에 올린다 (예: Go 의 `Get` 접두사 금지는 Java 관용과 부딪힌다). 언어 층에 둔다.
- 원본 그룹 디렉토리에 README 같은 설명 `.md` 를 두지 않는다. 룰로 설치된다.

## base·앱 성격 룰의 `paths:` 매핑

base·앱 성격 룰은 지원하는 모든 언어의 경로를 `paths:` 에 직접 적는다. 아래 표에서 룰이 다루는 분류의 행을 골라 모든 열의 경로를 적는다.

| 분류 | Go | Rust |
|---|---|---|
| 소스 전체 | `**/*.go` | `**/*.rs` |
| 테스트 | `**/*_test.go` | `**/tests/**/*.rs`, `**/tests.rs` |
| 진입점 | `**/main.go`, `**/cmd/**/*.go` | `**/main.rs`, `**/bin/**/*.rs` |
| 설정 | `**/config.go`, `**/options.go`, `**/config/**/*.go` | `**/config.rs`, `**/options.rs`, `**/config/**/*.rs` |
| 외부 경계 | `**/*_client.go`, `**/*_adapter.go`, `**/*_repository.go`, `**/*_handler.go`, `**/*_middleware.go` | `**/client.rs`, `**/*_client.rs`, `**/*_adapter.rs`, `**/adapter/**/*.rs`, `**/*_repository.rs`, `**/*_handler.rs`, `**/middleware.rs`, `**/*_middleware.rs` |
| 동시성 | `**/*_worker.go`, `**/*_pool.go`, `**/*_scheduler.go` | `**/worker.rs`, `**/*_worker.rs`, `**/*_pool.rs` |

- 표의 경로는 각 언어의 파일명 규칙 룰(`<언어>-file-naming.md`)과 같아야 한다. 한쪽을 바꾸면 다른 쪽도 바꾼다.
- 중괄호 glob(`*.{go,rs}`)은 쓰지 않고 펼쳐 적는다.

## 언어를 더할 때

1. `lang/<언어>/` 를 만들고 `<언어>-file-naming.md` 로 역할별 파일 이름을 정한다.
2. 위 표에 그 언어의 열을 더한다.
3. base·앱 성격 룰 전부의 `paths:` 에 표의 해당 경로를 더한다.

앱 성격을 더할 때는 `app/<성격>/` 디렉토리만 만들면 된다. CLI 는 디렉토리 이름으로 언어와 앱 성격을 찾는다.
