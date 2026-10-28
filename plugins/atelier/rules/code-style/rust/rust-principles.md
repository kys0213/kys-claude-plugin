---
name: rust-principles
description: Rust 스타일의 우선순위(언어 공통 룰 > 이 가이드 > 공개 Rust 스타일), 라이브러리/애플리케이션 구분, rustfmt, unstable 금지, 테스트 관용구
paths:
  - "**/*.rs"
---

# Rust 원칙

출처: Google Rust Style Guide — Background, Follow public Rust style rules, Apply API design rules in context, Code formatting and aesthetics, No unstable features.

## 우선순위

1. 언어 공통 룰(common)이 이 Rust 룰들보다 우선한다. 충돌하면 언어 공통 룰을 따른다.
2. Rust 룰들은 Google Rust 스타일 가이드를 옮긴 것이다.
3. 공개 Rust 스타일(Rust Style Guide, Rust API Guidelines)은 이 가이드와 충돌하지 않는 범위에서 따른다. "충돌"은 이 가이드가 권하는 패턴을 공개 스타일이 말리거나 그 반대인 경우다.

가이드의 목표: 쓰기 쉬움보다 읽기 쉬움·유지보수성, 자동화(대규모 일괄 변경)에 유리한 일관성, Rust 생태계와의 일관성.

## 라이브러리 코드와 애플리케이션 코드를 구분한다

| 구분 | 정의 | API 설계 규칙 |
|---|---|---|
| 라이브러리 | 다른 프로젝트가 쓰도록 만든, 비교적 일반적인 문제를 푸는 코드 | 구속력 있음 |
| 애플리케이션 | 한 애플리케이션 안에서만 쓰는, 구체적 문제를 푸는 코드 | 강한 권장 |

- DO: 애플리케이션 코드는 범용 재사용보다 단순하고 직접적인 형태를 고른다. 당장 필요한 것에 집중한다.
- DON'T: 애플리케이션 코드에서 섣불리 일반화·추상화한다 (예: `impl Into<T>` 인자, 반환용 이터레이터 타입).

## 포맷

- DO: rustfmt 로 포맷한다. 수동 포맷 논쟁을 하지 않는다.
- 권장 rustfmt 설정: `max_width = 100`, `hex_literal_case = "Lower"`, `format_macro_matchers = true`, `imports_granularity = "Module"`. stable 에서 쓸 수 없는 옵션은 stable 빌드를 지키려면 생략해도 된다.
- DO: 문자열에 보이지 않는 문자는 이스케이프(`\u{200D}`)로 쓴다.
- DON'T: 개인 취향으로 `#[rustfmt::skip]` 을 붙인다. 허용되는 경우는 둘뿐이다 — 정렬 자체가 가독성인 코드(행렬, 나란한 표), 생성된 코드.

```rust
#[rustfmt::skip]
const IDENTITY: [[f32; 3]; 3] = [
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
];
```

## unstable 기능 금지

- DON'T: nightly 전용 unstable 기능(`#![feature(...)]`)을 쓴다. 쓸 수밖에 없는 임베디드 등 예외는 리뷰에서 대안을 먼저 찾고 승인받는다.

## 테스트 관용구

Google 가이드는 코드베이스별 테스트 프레임워크만 말하므로(전용 항목이라 제외), 아래는 공개 Rust 관용구에서 왔다. Rust 단위 테스트는 구현 파일 안 `#[cfg(test)]` 모듈에 있어 테스트 경로로 가를 수 없으므로, 언어 공통 테스트 원칙(의존성 주입, AAA·한 테스트 한 행동, fake 는 본문 밖, 외부 인프라 의존 여부 하나로 기본 실행 포함을 가름)을 구현 파일 안의 테스트 모듈에도 그대로 적용한다.

- DO: 단위 테스트는 같은 파일 끝의 `#[cfg(test)] mod tests` 에 둔다. 비공개 항목은 `use super::*;` 로 가져온다 (glob import 의 허용 예외).
- DO: 공개 API 를 바깥에서 쓰는 통합 테스트는 크레이트의 `tests/` 디렉토리에 둔다. 여러 통합 테스트가 공유하는 fake·픽스처는 `tests/` 아래 보조 모듈로 뺀다.
- DO: 외부 인프라에 의존하는 테스트는 `#[ignore = "필요한 것"]` 으로 표시해 기본 `cargo test` 에서 빼고, `cargo test -- --ignored` 로 명시 실행한다. 외부 인프라 의존 여부가 테스트를 가르는 유일한 축이고, `#[ignore]` 가 그 표시 수단이다.
- DO: 단언에는 왜 성립해야 하는지 문맥 문자열을 단다.
- DON'T: 외부 인프라 여부 말고 다른 기준(느림, 중요도)으로 `#[ignore]` 를 쓴다.

```rust
#[cfg(test)]
mod tests {
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
}
```
