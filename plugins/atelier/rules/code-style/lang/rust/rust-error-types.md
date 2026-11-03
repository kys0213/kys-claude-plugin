---
name: rust-error-types
description: Rust 에러 타입 정의 — 호출 쪽이 가를 실패를 enum variant 로 분류, 원인 보존, Copy·must_use
paths:
  - "**/error.rs"
  - "**/errors.rs"
---

# Rust 에러 타입

출처: Google Rust Style Guide — Reporting errors, Traits, Other Rust features(`#[must_use]`).

## enum variant 로 분류한다

- DO: 호출 쪽이 가를 실패 종류는 enum variant 로 선언하고, 처리부는 variant 로 `match` 한다. `to_string()`·`Display` 출력으로 가르지 않는다.
- DO: 하위 에러의 원인은 variant 의 필드나 `source()` 로 보존한다.

```rust
#[derive(Debug)]
pub enum ConfigError {
    Missing { key: &'static str },
    Invalid { key: &'static str, reason: String },
    Io(io::Error),
}

// DO
match load() {
    Err(ConfigError::Missing { key }) => prompt_for(key),
    Err(e) => return Err(e.into()),
    Ok(c) => c,
}

// DON'T
if err.to_string().contains("missing") { /* ... */ }
```

## 트레이트와 속성

- DO: 에러 타입은 가능하면 `Copy` 로 둔다.
- DO: 에러 타입에 `#[must_use]` 를 붙인다.
