---
name: rust-ownership-and-types
description: Rust 소유권·함수 입출력·타입 설계 — Rc/Arc 회피, clone 우회 금지, 유연한 참조 입력, bool 대신 enum, newtype, 이름 있는 필드
paths:
  - "**/*.rs"
---

# Rust 소유권과 타입

출처: Google Rust Style Guide — Ownership and borrowing, Function inputs and outputs, Types.

## 소유권

- DO: 메모리는 고유 소유와 빌림으로 관리한다. Rust 는 공유 소유 스마트 포인터 없이도 대부분의 객체 그래프를 표현할 수 있고, `Rc`/`Arc` 는 런타임 비용과 고유한 복잡성을 더한다.
- 예외: 소유·빌림 구조가 지나치게 복잡해져 쓰기·이해·유지보수가 어려우면 `Rc`/`Arc` 를 쓴다. 판단은 리뷰에서 논의한다.
- DO: `.clone()` 은 깊은 복사가 정말 필요할 때 쓴다.
- DON'T: 빌림 오류를 피하려고 `.clone()` 으로 우회한다. 소유권 규칙에 자연스럽게 맞도록 설계한다. 빌림 관계가 과도하게 복잡해질 때만 실용적 선택으로 clone 한다.
- `Rc`/`Arc` 포인터의 clone 은 깊은 복사가 아니므로 이 제한 대상이 아니다.

## 함수 입력

- DO: 불변 데이터를 빌려 받을 때는 스마트 포인터·컨테이너·래퍼가 아닌 데이터 자체의 참조를 받는다. 호출 쪽이 어떤 형태로 들고 있든 복사 없이 넘길 수 있다 (clippy `borrowed_box`, `ptr_arg`).

| 쓴다 | 쓰지 않는다 |
|---|---|
| `&T` | `&Box<T>` |
| `&[T]` | `&Vec<T>` |
| `&str` | `&String` |
| `Option<&T>` | `&Option<T>` |

- DO (라이브러리): 입력을 순회만 한다면 `impl IntoIterator` 나 이터레이터를 받는다. 애플리케이션 코드는 코드가 단순해지면 `&[T]` 를 받아도 된다.
- 권장 (라이브러리): 호출 쪽이 여러 타입을 들고 있을 가능성이 크고 호출부 편의가 중요하면 `impl Into<T>` 를 받는다. 코드 크기·추론 비용과 저울질한다.
- DON'T (애플리케이션): `impl Into<T>` 인자를 쓴다. 애플리케이션 코드에서는 거의 쓰지 않는다.

```rust
// DO
fn total(prices: &[Price]) -> Price { /* ... */ }
fn greet(name: &str) { /* ... */ }

// DON'T
fn total(prices: &Vec<Price>) -> Price { /* ... */ }
fn greet(name: &String) { /* ... */ }
```

## 함수 출력

- 권장 (라이브러리): `Vec<T>` 보다 이터레이터(구체 타입 또는 `impl Iterator`)를 반환한다. 호출 쪽이 수집·순회·연결을 고를 수 있다.
- 다음이면 `Vec<T>` 반환이 맞다: 대부분의 호출 쪽이 어차피 벡터를 만든다, 구현이 자연스럽게 벡터를 만든다, 임의 인덱싱이 필요하다, `for` 루프가 더 맞고 할당 비용이 문제되지 않는다, 새 컬렉션 생성이 함수의 목적이다, 이미 있는 벡터를 돌려준다.
- 애플리케이션 코드는 이터레이터의 유연성·성능이 분명히 필요하지 않으면 `Vec<T>` 를 반환해도 된다.

## bool 대신 enum

- DO: 호출부에서 의미가 드러나지 않는 `bool` 매개변수는 `enum` 으로 바꾼다. 인자 목록의 `/* recursive= */` 같은 설명 주석으로 때우지 않는다.

```rust
// DO
pub enum Recursion { Recursive, Shallow }
remove_dir(path, Recursion::Recursive);

// DON'T
remove_dir(path, true);
```

## newtype 과 타입 별칭

- DO: 다음이면 타입 별칭(`type`) 대신 newtype 을 쓴다.
  - 의미가 기반 타입과 뚜렷이 다르다.
  - 자기 API 가 필요하다.
  - 나중에 기반 타입을 바꿀 여지를 두고 싶다 (특히 모든 사용처를 통제하지 못할 때).
  - 섞이기 쉬운 값을 타입으로 구분해야 한다 (예: `Day`, `Month`, `Year`).

```rust
pub struct UserId(u64);
pub struct OrderId(u64);
fn cancel(user: UserId, order: OrderId) { /* ... */ } // 인자 순서 실수가 컴파일 오류가 된다
```

## 이름 있는 필드

- DO: 튜플·다중 필드 튜플 구조체보다 이름 있는 필드를 쓴다. `.0`, `.1` 은 금세 헷갈린다.
- 예외: 필드가 하나인 newtype 튜플 구조체, 헷갈릴 여지가 없는 아주 지역적인 문맥.

```rust
// DO
struct Range { start: u32, end: u32 }
// DON'T
struct Range(u32, u32);
fn bounds() -> (u32, u32, bool) { /* ... */ }
```
