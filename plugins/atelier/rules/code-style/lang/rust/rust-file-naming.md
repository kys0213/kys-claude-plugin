---
name: rust-file-naming
description: Rust 파일 이름 컨벤션 — 역할별 코드를 정해진 이름의 파일에 두고, 새 역할 파일을 만들기 전에 같은 역할의 기존 파일을 먼저 Read 한다
paths:
  - "**/*.rs"
---

# Rust 파일 이름

역할별 상세 규칙은 파일 이름에 따라 그 파일을 Read 할 때 주입된다. 역할 코드가 엉뚱한 이름의 파일에 있으면 그 역할의 규칙 없이 작업하게 된다. 그래서 역할마다 아래 이름의 파일에 코드를 둔다.

| 역할 | 파일 이름 |
|---|---|
| 테스트 | 단위 테스트는 `#[cfg(test)] mod tests;` 로 선언한 `tests.rs`, 통합 테스트는 크레이트의 `tests/` 아래 |
| 진입점 | `main.rs`, `bin/` 아래 |
| 에러 정의 | `error.rs`, `errors.rs` |
| 설정·옵션 | `config.rs`, `options.rs`, `config/` 아래 |
| 외부 시스템 경계 | `client.rs`, `*_client.rs`, `*_adapter.rs`, `adapter/` 아래, `*_repository.rs`, `*_handler.rs`, `middleware.rs`, `*_middleware.rs` |
| 동시성(작업자·풀) | `worker.rs`, `*_worker.rs`, `*_pool.rs` |
| 크레이트 문서·공개 API | `lib.rs` |

- DO: 역할 코드는 위 이름의 파일에 둔다. 역할이 섞이면 나눈다.
- DO: 새 역할 파일을 만들기 전에 같은 역할의 기존 파일을 하나 Read 한다. 규칙은 Read 할 때만 주입되고 새 파일 Write 로는 주입되지 않는다. 기존 파일이 없으면 위 이름으로 만든 뒤 Read 한다.
