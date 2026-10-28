---
name: rust-principles
description: Rust 스타일의 우선순위, 라이브러리/애플리케이션 구분, rustfmt, unstable 금지, 역할별 핵심 한 줄
paths:
  - "**/*.rs"
---

# Rust 원칙

출처: Google Rust Style Guide — Background, Follow public Rust style rules, Apply API design rules in context, Code formatting and aesthetics, No unstable features.

## 우선순위

1. 언어 공통 룰이 Rust 룰보다 우선한다.
2. Rust 룰은 Google Rust 스타일 가이드를 옮긴 것이다.
3. 공개 Rust 스타일(Rust Style Guide, API Guidelines)은 이 가이드와 충돌하지 않는 범위에서 따른다.

## 라이브러리와 애플리케이션

API 설계 규칙은 라이브러리(다른 프로젝트가 쓰는 코드)에 구속력, 애플리케이션에 강한 권장이다.

- DO: 애플리케이션 코드는 단순·직접적인 형태로 당장 필요한 것에 집중한다.
- DON'T: 애플리케이션에서 섣불리 일반화한다 (`impl Into<T>` 인자, 반환용 이터레이터 타입).

## 포맷과 unstable

- DO: rustfmt 로 포맷한다. 권장: `max_width = 100`, `hex_literal_case = "Lower"`, `format_macro_matchers = true`, `imports_granularity = "Module"` (stable 에 없으면 생략).
- DO: 보이지 않는 문자는 `\u{200D}` 처럼 이스케이프한다.
- DON'T: 취향으로 `#[rustfmt::skip]`. 정렬이 곧 가독성인 코드(행렬, 표)와 생성 코드만 허용.
- DON'T: `#![feature(...)]` (nightly 전용). 불가피한 예외는 리뷰에서 대안을 먼저 찾고 승인받는다.

## 역할별 핵심 (어느 파일에서나)

- 테스트: 단위 테스트는 `#[cfg(test)] mod tests;` 로 뺀 파일에 두고, 외부 인프라 의존 테스트만 `#[ignore = "필요한 것"]` 으로 표시한다.
- 진입점: `main()` 전에 코드를 실행하지 않는다 (`ctor` 등).
- 에러 정의: 가를 실패는 enum variant 로 선언하고 원인은 필드나 `source()` 로 보존한다.
- 외부 경계: 트레이트는 제네릭 제약·`impl`/`dyn Trait`·확장 목적에만 정의한다.
- 동시성: poisoned mutex 내부 값에 접근하지 않는다. `Rc`/`Arc` 보다 고유 소유·빌림을 먼저 고른다.
- 공개 API: 표준 트레이트와 같은 동작이면 그 트레이트를 `derive` 우선으로 구현한다. 일반 코드로 되면 매크로를 만들지 않는다.
- 이름: 변환은 `as_`(빌린 view)·`to_`(새 값)·`into_`(소비), getter 는 `get_` 없이 `foo()`, 생성자는 `new()`/`try_new()`. 모를 약어를 쓰지 않는다.
- 문서 주석: 이름·시그니처로 알 수 없는 계약이 있을 때만 `///` 로 쓰고, panic 조건은 `# Panics` 에 적는다.
