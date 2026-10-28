---
name: rust-code-organization
description: Rust 모듈·파일 구성과 use 규칙 — 주 항목 이름의 파일, 비리프 모듈, glob import 금지, 모듈 단위 use, 한정 규칙
paths:
  - "**/*.rs"
---

# Rust 코드 구성

출처: Google Rust Style Guide — Code organization (`[chrome]` 전용 제외).

## 파일과 모듈

- DO: 리프 모듈에 주 항목(예: `Foo`)이 있으면 모듈·파일을 그 이름으로 짓는다 (`mod foo`, `foo.rs`). 주 항목이 없으면 자유롭게 짓는다.
- DO: 비리프 모듈에는 모듈 선언과 `pub use` 재노출만 둔다. 하위 모듈을 엮는 소량의 코드는 예외이고, `mod tests`·언어 간 바인딩 모듈은 리프 판정에서 뺀다.

## `use`

- DON'T: `use foo::*;` (출처가 안 보이고 일괄 변경을 막는다). 예외: 인라인 모듈·테스트 모듈의 `use super::*`, enum 메서드 본문이나 주로 그 enum 을 다루는 함수 안의 `use MyEnum::*`, 비리프 모듈의 `pub use submodule::*`.
- DON'T: prelude 모듈을 새로 정의한다. 서드파티 prelude 는 리뷰어 판단.
- DO: `use` 한 문장은 한 모듈의 항목만 가져온다. `{..}` 안에 `::` 가 없다 (`use foo::bar::{Bar, Baz};` O, `use foo::{Foo, bar::Bar};` X).
- DO: 트레이트는 쓰는 함수 본문 안에서 `use` 한다. 파일 전체가 많이 쓸 때만 최상단에 둔다.
- DO: 다른 모듈의 함수는 모듈을 가져와 `mem::take()` 처럼 한정해 부른다. 한정 없는 `foo()` 는 같은 모듈의 것으로 읽힌다. 예외: 이름만으로 구분되는 함수, 모듈 이름이 충돌하기 쉬운 경우.
- DO: 타입·트레이트는 `use` 로 가져와 한정 없이 쓴다 (`regex::Regex` 가 아니라 `Regex`). 예외: 이름 충돌(`io::Error`, `as IoError`), 큰 타입 군, 매크로·생성 코드, 항목이 지나치게 많을 때.
