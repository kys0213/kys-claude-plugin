---
name: rust-naming
description: Rust 이름 규칙 — as_/to_/into_ 변환, 접근자(foo/foo_mut/set_foo/with_foo), new/try_new/from_foo 생성자, Ext 접미사, 수명·제네릭 이름, 약어 금지
paths:
  - "**/*.rs"
---

# Rust 이름

출처: Google Rust Style Guide — Naming, Other Rust features(Getters and setters).

## 메서드

- DO: 표준 라이브러리의 메서드 이름 관례를 따른다.
- DO: 변환 메서드는 `self` 규약에 맞는 접두사를 쓴다 (Rust API Guidelines, clippy `wrong_self_convention`).

| 접두사 | 비용 | `self` |
|---|---|---|
| `as_` | 공짜, 빌린 view | `&self` |
| `to_` | 비쌈 (새 값 생성) | `&self` (`Copy` 면 `self`) |
| `into_` | 소유권 소비 | `self` |

```rust
fn as_bytes(&self) -> &[u8]
fn to_lowercase(&self) -> String
fn into_inner(self) -> T
```

## 접근자

| 형태 | 용도 |
|---|---|
| `foo()` | `&T` 를 돌려주는 getter (`T: Copy` 면 값). `get_foo()` 로 짓지 않는다 |
| `foo_mut()` | `&mut T` 를 돌려주는 getter |
| `take_foo()` | 값을 비우고(`None` 이나 기본 상태로) 이전 값을 돌려준다 |
| `set_foo(foo)` | 값을 받아 설정한다 |
| `replace_foo(foo)` | 설정하고 이전 값을 돌려준다 |
| `with_foo(f)` / `with_mut_foo(f)` | 클로저로 접근한다. 읽기·쓰기 둘 다 필요할 때만 `with_mut_` 를 나누고, 하나뿐이면 `with_foo` |

- DO: 클로저 접근자(`with_foo`)는 값이 내부 가변성 래퍼(`Mutex`, `RefCell`) 안에 있을 때 쓴다. `MutexGuard` 같은 RAII 객체를 돌려주면 구현 세부가 새어 나간다. 변경 뒤 후속 작업·검증이 필요한데 값을 clone 할 수 없어 `set_foo` 를 쓸 수 없을 때도 쓴다. 클로저의 반환값을 그대로 돌려준다.

```rust
impl Session {
    fn user(&self) -> &User { /* ... */ }
    fn set_user(&mut self, user: User) { /* ... */ }
    fn with_cache<R>(&self, f: impl FnOnce(&Cache) -> R) -> R { /* ... */ }
}
```

## 생성자와 변환 함수

- DO: 생성자는 `new()`. 여러 개면 `new_with_foo()`. `Result` 를 돌려주면 `try_new()`.
- DO: 값→값 변환은 `from_foo()` 로 짓고 대응하는 `From` 구현을 함께 둔다.

## 트레이트

- DO: 확장 트레이트는 `~Ext` 접미사를 붙인다 (`ForeignTypeExt`). 제네릭 제약·동적 디스패치용이 아니라 메서드 추가만이 목적임을 이름으로 알린다.

## 수명과 제네릭 매개변수

- DO: 수명, 특히 타입의 수명에는 의미 있는 이름을 붙인다 (`'conn`, `'src`).
- 예외: `Cow` 처럼 극히 일반적인 추상, 동작하는 곳의 익명 수명 `'_`, 메서드 입력·출력 관계만 나타내거나 지역 고차 수명(`for<'a>`)에 쓰는 한 글자 수명(두 개까지).
- DO: 제네릭 매개변수는 짧게 짓되 가리키는 대상을 암시한다. 한 글자라면 가장 중요한 트레이트 제약에서 따온다 (`R: Read`, `W: Write`). 거의 모든 타입을 받는 API 에서는 `T`, `U` 도 된다.

```rust
struct Parser<'src> { input: &'src str }
fn copy<R: Read, W: Write>(reader: R, writer: W) -> io::Result<u64>
```

## 약어

- DON'T: 프로젝트 밖 사람이 모를 약어(두문자어 포함)를 쓴다. 단어 중간 글자를 빼서 줄이지 않는다 (`cnt`, `msg_q`).
- 예외: 위키백과에 실릴 만큼 흔한 약어, 프로젝트 밖에서도 헷갈리지 않을 약어(`IpAddr`). 그래도 기본은 줄이지 않는 쪽이다.
