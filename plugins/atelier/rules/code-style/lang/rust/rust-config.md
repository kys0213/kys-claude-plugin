---
name: rust-config
description: Rust 설정·옵션 타입 — 옵션 묶음 구조체의 Default 와 갱신 문법, 필수 값이 있는 설정 타입, non_exhaustive 예외
paths:
  - "**/config.rs"
  - "**/options.rs"
  - "**/config/**/*.rs"
---

# Rust 설정·옵션

출처: Google Rust Style Guide — Traits, Other Rust features(`#[non_exhaustive]`).

- DO: 함수 옵션 묶음으로 쓰는 전부-공개 필드 구조체는 `Default` 를 구현해 `..Default::default()` 갱신 문법을 쓸 수 있게 한다.
- DON'T: 필수 값이 있는 설정 타입에 `Default` 를 구현한다. 빠진 필수 값이 기본값으로 조용히 채워진다.
- 사용자가 `..Default::default()` 초기화를 써야 하는 공개 필드 구조체에는 `#[non_exhaustive]` 를 붙이지 않는다 (붙이면 바깥에서 구조체 리터럴을 쓸 수 없다).

```rust
#[derive(Default)]
pub struct ExportOptions {
    pub compress: bool,
    pub max_rows: Option<usize>,
}

let opts = ExportOptions { compress: true, ..Default::default() };
```
