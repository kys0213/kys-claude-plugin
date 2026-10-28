---
name: rust-code-organization
description: Rust 모듈·파일 구성과 use 규칙 — 주 항목 이름의 파일, 비리프 모듈은 선언·재노출만, glob import 금지, 모듈 단위 use
paths:
  - "**/*.rs"
---

# Rust 코드 구성

출처: Google Rust Style Guide — Code organization. `[chrome]` 전용 항목(작은 크레이트, 평평한 파일 배치, `chromium::import!`)은 제외했다. 이 룰은 Rust 모듈·`use` 의 형태만 다룬다.

## 파일과 모듈

- DO: 리프 모듈에 주 사용자 정의 항목(예: `Foo`)이 있으면 모듈과 파일을 그 이름으로 짓는다 (`mod foo`, `foo.rs`). 파일 이름만 보고 내용을 알 수 있다. 특정 추상에 속하지 않는 리프 모듈은 자유롭게 짓는다.
- DO: 비리프 모듈(하위 모듈을 가진 모듈)에는 모듈 선언과 재노출(`pub use`)만 둔다. 구조와 내용을 섞지 않아야 찾기 쉽다.
- 예외: 하위 모듈을 엮는 소량의 코드는 비리프 모듈에 있어도 된다. `mod tests`, 언어 간 바인딩 모듈은 리프 판정에서 제외한다.

```rust
// DO: src/order/mod.rs
mod line;
mod order;
pub use line::Line;
pub use order::Order;

// DON'T: src/order/mod.rs 안에 Order 구현 수백 줄
```

## glob import 금지

- DON'T: `use foo::*;` 를 쓴다. 타입이 어디서 왔는지 코드만 보고 알 수 없고, 대규모 일괄 변경을 어렵게 한다.
- 허용 예외 (선택):
  - 인라인 모듈의 `use super::*` (테스트 모듈 포함)
  - enum 의 메서드 본문, 또는 주로 그 enum 을 다루는 함수 안의 `use MyEnum::*`
  - 비리프 모듈의 `pub use submodule::*`
- DON'T: prelude 모듈을 새로 정의한다. 서드파티 크레이트의 prelude 사용은 리뷰어 판단에 맡긴다.

## `use` 는 한 문장에 한 모듈

- DO: `use` 한 문장은 한 모듈의 항목만 가져온다. `{..}` 안에 `::` 가 없어야 한다 (rustfmt `imports_granularity = "Module"`).

```rust
// DO
use foo::Foo;
use foo::bar::{Bar, Baz};

// DON'T
use foo::{Foo, bar::Bar};
```

## 트레이트는 지역에서 가져온다

- DO: 트레이트는 쓰는 함수 본문 안에서 `use` 한다. 파일 전체에서 그 트레이트 메서드를 많이 쓸 때만 모듈 최상단에 둔다.

```rust
fn parse(s: &str) -> Result<u8, std::num::ParseIntError> {
    use std::str::FromStr;
    u8::from_str(s)
}
```

## 함수는 모듈로 한정하고, 타입은 한정하지 않는다

- DO: 다른 모듈·크레이트의 최상위 함수는 모듈을 가져와 `module::function()` 으로 부른다. 한정 없는 소문자 호출 `foo()` 는 같은 모듈의 것으로 읽힌다.
- DO: 타입과 트레이트는 `use` 로 가져와 한정 없이 쓴다 (`Regex`, `regex::Regex` 가 아니라).
- 함수 한정의 예외: 이름만으로 충분히 구분되는 함수(C++ 에서 온 함수 등), 모듈 이름 자체가 충돌하기 쉬운 경우.
- 타입 한정의 예외: 이름이 겹칠 때(`io::Error` 로 쓰거나 `use std::io::Error as IoError`), 큰 타입 군을 모듈째 쓰는 경우, 매크로·생성 코드, 가져올 항목이 지나치게 많은 경우.

```rust
// DO
use std::mem;
use regex::Regex;
let x = mem::take(&mut v);
let re = Regex::new(pattern)?;

// DON'T
use std::mem::take;
let x = take(&mut v);
let re = regex::Regex::new(pattern)?;
```
