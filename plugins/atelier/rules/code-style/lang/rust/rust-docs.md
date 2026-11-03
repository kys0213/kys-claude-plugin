---
name: rust-docs
description: Rust 문서 주석의 형식 — ///·//! 구분, 크레이트 문서, Markdown, 문장 형태, Panics·Safety 절, 예제
paths:
  - "**/lib.rs"
---

# Rust 문서 주석

출처: Google Rust Style Guide — When and how to write documentation.

이 룰은 Rust 문서 주석의 형식만 다룬다. 가이드의 "공개 API 에는 문서 주석을 단다(`missing_docs` lint)"는 따르지 않는다 — 공개 여부만으로 문서 주석을 의무화하지 않고, 이름·시그니처로 알 수 없는 사용 계약·전제조건·실패 조건이 있을 때만 쓴다. `missing_docs` lint 를 켠 프로젝트는 프로젝트 룰로 이 룰을 덮어쓴다.

## 문법

- DO: 뒤따르는 항목(함수, 구조체, enum, 트레이트, 타입 별칭)의 문서는 `///` 로 쓴다.
- DO: 모듈·크레이트 자체의 문서는 파일 맨 위에 `//!` 로 쓴다.
- DO: 구현 주석, 사용자에게 의미 없는 TODO, 주석 처리한 코드 등 문서가 아닌 주석은 `//` 로 쓴다.
- DO: 문서 주석과 구현 주석을 한 항목에 함께 써도 된다. 공개할 내용은 `///`, 구현자에게만 필요한 내용은 `//`.
- DO: 문서는 소스에서 바로 읽히게 `///`·`//!` 로 직접 쓴다.
- DON'T: 매크로로 문서를 생성한다.

```rust
/// Returns the first item named `name`, or `None` if absent.
// TODO: take `name` as `&str` instead of `String`.
pub fn find_named(items: &[Item], name: String) -> Option<&Item> { /* ... */ }
```

TODO 에는 티켓 번호 대신 할 일의 내용을 적는다.

## 문장 형태

- DO: Markdown 을 쓴다. 코드 이름은 백틱으로 감싼다.
- DO: 대문자로 시작하고 문장 부호를 갖춘 문장으로 쓴다.
- DO: 공식 Rust Style Guide 의 문서 주석 규칙을 따른다.

## 담아야 하는 것 (문서 주석을 쓰는 경우)

- DO: 잘못된 입력으로 panic 할 수 있는 함수는 panic 조건을 `# Panics` 절에 적는다.
- DO: `unsafe` 함수와 트레이트는 호출자가 지켜야 할 불변식을 `# Safety` 절에 적는다.
- 권장: 크레이트가 어떻게 동작하는지 보여줄 만큼의 예제를 어딘가에 둔다. 모든 함수에 예제가 필요하지는 않다.

```rust
/// # Panics
///
/// Panics if `index >= self.len()`.
pub fn remove(&mut self, index: usize) -> T { /* ... */ }
```

## 쓰지 않는 것

- DON'T: 언어 기초를 설명한다 (`/// A mock implementation of the FileSystem trait` 를 `impl FileSystem for MockFileSystem` 위에).
- DON'T: 함수 이름을 그대로 풀어 쓴다 (`/// Removes an element.` 를 `remove_element` 위에).
- 스스로 설명되는 enum variant 에는 문서를 달지 않는다.
