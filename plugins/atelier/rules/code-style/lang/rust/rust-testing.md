---
name: rust-testing
description: Rust 테스트 관용구 — tests.rs 단위 테스트 모듈, tests/ 통합 테스트와 공유 fake, 외부 인프라 테스트의 #[ignore], 단언 문맥
paths:
  - "**/tests/**/*.rs"
  - "**/tests.rs"
---

# Rust 테스트

Google 가이드는 코드베이스별 테스트 프레임워크만 말하므로(전용 항목이라 제외), 아래는 공개 Rust 관용구에서 왔다.

## 배치

- DO: 단위 테스트는 구현 파일 끝에 `#[cfg(test)] mod tests;` 를 선언하고 그 모듈 파일 `tests.rs` 에 둔다. 비공개 항목은 `use super::*;` 로 가져온다 (glob import 의 허용 예외).
- DO: 공개 API 를 바깥에서 쓰는 통합 테스트는 크레이트의 `tests/` 디렉토리에 둔다. 여러 통합 테스트가 공유하는 fake·픽스처는 `tests/` 아래 보조 모듈로 뺀다.

## 실행 구분

- DO: 외부 인프라에 의존하는 테스트는 `#[ignore = "필요한 것"]` 으로 표시해 기본 `cargo test` 에서 빼고, `cargo test -- --ignored` 로 명시 실행한다. 외부 인프라 의존 여부가 테스트를 가르는 유일한 축이고, `#[ignore]` 가 그 표시 수단이다.
- DON'T: 외부 인프라 여부 말고 다른 기준(느림, 중요도)으로 `#[ignore]` 를 쓴다.

## 단언

- DO: 단언에는 왜 성립해야 하는지 문맥 문자열을 단다.

```rust
use super::*;

#[test]
fn rejects_unknown_status() {
    // act
    let result: Result<Status, _> = "archived".parse();
    // assert
    assert!(result.is_err(), "unknown status must not map to a default");
}

#[test]
#[ignore = "requires a running PostgreSQL"]
fn saves_order_to_database() { /* ... */ }
```
