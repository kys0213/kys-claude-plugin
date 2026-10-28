---
name: rust-traits-and-api
description: Rust 트레이트와 공개 API 설계 — 표준 트레이트·derive, 제네릭 vs dyn, 트레이트를 정의할 때, 매크로 절제, non_exhaustive, must_use
paths:
  - "**/*.rs"
---

# Rust 트레이트와 API

출처: Google Rust Style Guide — Traits, Macros, Other Rust features(Implement expected API sets together, `#[doc(hidden)]`, `const` functions, Avoid overloading, `#[non_exhaustive]`, `#[must_use]`).

## 트레이트 구현

- DO: 저수준 라이브러리는 사용자가 필요로 할 트레이트를 미리 구현한다. 애플리케이션 코드는 필요해질 때 추가한다.
- DO: 메서드가 표준 라이브러리 트레이트의 목적·동작과 같으면 그 트레이트를 구현한다 (`fn from_str` 메서드 대신 `impl FromStr`; clippy `should_implement_trait`). 발견성을 위해 메서드를 함께 둬도 된다.
- DO: `#[derive(...)]` 로 표현할 수 있으면 수동 구현하지 않는다.
- DO: 짝을 이루는 API 는 함께 구현한다 — `len()` 이 있으면 `is_empty()` 도 (clippy `len_without_is_empty`).

표준 트레이트별 기준:

| 트레이트 | 기준 |
|---|---|
| `Clone` | 대부분 구현한다. 외부 자원을 나타내는 타입은 예외 |
| `Copy` | 앞으로도 `Copy` 로 남을 것이 분명할 때 구현한다. 에러 타입은 가능하면 `Copy` 로 둔다 |
| `Default` | `new()` 가 있거나 기본 상태가 있으면 구현한다 (clippy `new_without_default`). 단 필수 값이 있는 설정 타입에는 구현하지 않는다 |
| `PartialEq`/`Eq` | `==` 가 목적이면 `PartialEq` 를 우선한다. `Eq` 는 반사성 요구를 알고 구현한다 |
| `PartialOrd`/`Ord` | `<`·`>` 가 목적이면 `PartialOrd` 를 우선한다. `Ord` 는 전순서를 요구한다 |
| `Hash`+`Eq` (+`Ord`) | 맵 키·캐시 키로 쓰일 것 같으면 구현한다 |
| 공개 필드 타입 | 모든 하위 필드가 구현한 트레이트를 구현하는 것을 고려한다 |

- DO: 주요 생성 경로인 `From` 구현에는 명시적 생성자(`new()`, `from_foo()`)를 짝지어 둔다.
- DO: 함수 옵션 묶음으로 쓰는 전부-공개 필드 구조체는 `Default` 를 구현해 `..Default::default()` 갱신 문법을 쓸 수 있게 한다.

## 트레이트를 정의할 때

- DO: 트레이트는 다음 목적에만 정의한다 — 제네릭 프로그래밍(`T: Trait` 제약), `impl Trait`/`dyn Trait` 로 받거나 돌려주기, 확장 트레이트. 테스트용 fake 를 주입하는 경계는 제네릭 제약이나 `dyn Trait` 로 받으므로 허용 목적에 해당한다.
- DON'T: 코드를 정리하려는 목적만으로 트레이트를 만든다. 그 밖의 경우는 고유 구현(`impl Type`)을 쓴다.
- DO (라이브러리): 함수 매개변수는 `dyn Trait` 보다 제네릭(`T: Trait`, `impl Trait`)을 받는다. 간접 호출 비용이 없고 인라인이 가능하다. 코드 크기가 중요하면 예외.
- DO: 확장 트레이트(다른 크레이트의 타입에 메서드만 더하는 트레이트)는 `~Ext` 접미사를 붙이고, 원래 타입을 소유한 팀이 소유한다.

```rust
trait ForeignTypeExt {
    fn summary(&self) -> String;
}
impl ForeignTypeExt for foreign_crate::ForeignType { /* ... */ }
```

## 매크로

- DO: API 표현 수단은 이 순서로 고른다 — 일반 함수·타입·트레이트·모듈 → `macro_rules!` → 다른 코드 생성 → `proc_macro`.
- DON'T: 일반 코드로 되는 것을 매크로로 만든다. 매크로는 IDE·코드 검색을 약하게 하고, 언어 방언을 만들어 읽는 사람이 할 수 있는 가정을 줄인다.

## 공개 API 속성

- DO: 다른 팀이 쓰는 공개 enum 에 `#[non_exhaustive]` 를 붙인다. 예외: 절대 바뀌지 않는 enum(기존 체계를 그대로 옮긴 `Color`, bool 같은 두 상태), 깨짐을 직접 고칠 각오가 된 경우, 사용처가 적은 enum.
- DO: 다른 팀이 쓰는 공개 필드 구조체에 `#[non_exhaustive]` 를 붙인다. 예외: 필드가 바뀔 일이 없을 때, 깨짐을 직접 고칠 때, 사용자가 `..Default::default()` 초기화를 써야 할 때.
- DO: 반환값을 무시하면 올바르게 쓸 수 없는 함수(순수 함수 포함), 반환값이 에러나 중요한 정보인 함수에 `#[must_use]` 를 붙인다. 반환 타입이 이미 `#[must_use]`(`Result` 등)면 생략한다.
- DO: 에러 타입, 지연 평가되는 계산(반환값을 써야 일이 일어나는 타입)에 `#[must_use]` 를 붙인다.
- DO: 공개 함수는 앞으로도 `const` 로 남을 것이 확실할 때만 `const fn` 으로 만든다. `const` 를 떼는 것은 깨지는 변경이다.
- DON'T: 연산자 오버로딩을 수학적·논리적 의미가 아닌 문법 설탕으로 쓴다.
- DON'T: 소유하지 않은 `#[doc(hidden)]` 항목에 접근한다. 비공개로 취급한다.

```rust
#[must_use]
pub fn with_timeout(self, timeout: Duration) -> Self { /* ... */ }

#[non_exhaustive]
pub enum ExportFormat { Csv, Json }
```
