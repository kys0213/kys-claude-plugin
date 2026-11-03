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

- DO: 호출 쪽이 가를 필요가 있는 실패 종류는 에러를 정의한 곳이 enum variant 로 선언한다. 처리부는 variant 로 `match` 한다.
- DON'T: 에러 메시지 문자열(`to_string()`, `Display` 출력)을 비교·검색해 종류를 가른다. 메시지 문구를 고치면 분류가 조용히 깨진다.
- DO: 하위 에러를 감쌀 때 원인을 variant 의 필드나 `source()` 로 보존해 호출 쪽이 판별할 수 있게 한다.

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
