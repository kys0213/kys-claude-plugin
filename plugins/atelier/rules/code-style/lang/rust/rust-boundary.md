---
name: rust-boundary
description: Rust 외부 시스템 경계 — 트레이트를 정의하는 목적, fake 주입 경계, 제네릭 vs dyn
paths:
  - "**/client.rs"
  - "**/*_client.rs"
  - "**/*_adapter.rs"
  - "**/adapter/**/*.rs"
  - "**/*_repository.rs"
  - "**/*_handler.rs"
  - "**/middleware.rs"
  - "**/*_middleware.rs"
---

# Rust 외부 경계와 트레이트

출처: Google Rust Style Guide — Traits.

## 트레이트를 정의할 때

- DO: 트레이트는 다음 목적에만 정의한다 — 제네릭 프로그래밍(`T: Trait` 제약), `impl Trait`/`dyn Trait` 로 받거나 돌려주기, 확장 트레이트. 테스트용 fake 를 주입하는 경계는 제네릭 제약이나 `dyn Trait` 로 받으므로 허용 목적에 해당한다.
- DON'T: 코드를 정리하려는 목적만으로 트레이트를 만든다. 그 밖의 경우는 고유 구현(`impl Type`)을 쓴다.
- DO (라이브러리): 함수 매개변수는 `dyn Trait` 보다 제네릭(`T: Trait`, `impl Trait`)을 받는다. 간접 호출 비용이 없고 인라인이 가능하다. 코드 크기가 중요하면 예외.

```rust
pub trait OrderStore {
    fn save(&self, order: &Order) -> Result<(), StoreError>;
}

// 생산 코드는 실제 구현을, 테스트는 fake 를 넣는다
pub struct Checkout<S: OrderStore> { store: S }
```
