---
name: rust-error-and-impl
description: Rust 에러 보고(Option/Result/panic 선택, enum variant 로 분류)와 구현 관용구 — 패턴 매칭, expect, 단언 문맥, 이터레이터 체인 절제, 타입 추론, format!
paths:
  - "**/*.rs"
---

# Rust 에러와 구현

출처: Google Rust Style Guide — Reporting errors, Implementation concerns.

## Option, Result, panic 을 계약에 맞게 고른다

| 수단 | 쓰는 경우 |
|---|---|
| `Option` | 원인이 자명하고 호출 쪽이 더 알 필요가 없는 단순 실패 (값 없음, 형식 불일치) |
| `Result` | 호출 쪽이 원인을 보고 처리해야 하는 복구 가능한 실패. 주로 I/O·네트워크처럼 올바른 코드도 미리 막을 수 없는 실패 |
| `panic!` | 버그와 깨진 불변식. 전제조건을 지키지 않은 호출처럼 코드에서 고쳐야 할 결함 |

- 프로젝트가 panic 을 전면 금지하고 불변식 위반까지 `Result` 로 보고하기로 했다면 그 결정을 따른다.
- 예외: 개발자용 바이너리는 치명적이지 않은 오류에도 panic 을 쓸 수 있다. 테스트의 단언은 panic 이다.

```rust
fn checked_add(a: u32, b: u32) -> Option<u32>            // 원인 자명
fn open_config(path: &Path) -> Result<Config, ConfigError> // 원인별 처리 필요
```

## 에러는 enum variant 로 분류한다

- DO: 호출 쪽이 가를 필요가 있는 실패 종류는 에러를 정의한 곳이 enum variant 로 선언한다. 처리부는 variant 로 `match` 한다.
- DON'T: 에러 메시지 문자열(`to_string()`, `Display` 출력)을 비교·검색해 종류를 가른다. 메시지 문구를 고치면 분류가 조용히 깨진다.
- DO: 하위 에러를 감쌀 때 원인을 variant 의 필드나 `source()` 로 보존해 호출 쪽이 판별할 수 있게 한다.

```rust
#[derive(Debug)]
pub enum ConfigError {
    Missing { key: &'static str },
    Invalid { key: &'static str, reason: String },
    Io(io::Error),
}

// DO
match load() {
    Err(ConfigError::Missing { key }) => prompt_for(key),
    Err(e) => return Err(e.into()),
    Ok(c) => c,
}

// DON'T
if err.to_string().contains("missing") { /* ... */ }
```

## panic 을 피하는 구현

- DO: 입력이 panic 하지 않음을 보장할 때는 `unwrap` 대신 패턴 매칭으로 컴파일러에 증명한다.
- DO: 불변식을 타입으로 표현할 수 없어 panic 이 필요하면 `unwrap` 보다 `expect` 로 이유를 남긴다.

```rust
// DO
if let Some(value) = option { use_value(value); }
let port = env_port.expect("validated by Config::load");

// DON'T
if option.is_some() { use_value(option.unwrap()); }
```

## `match` 와 `#[non_exhaustive]` enum

- DO: 닫힌 enum 을 `match` 할 때는 모든 variant 를 나열한다.
- DO: `#[non_exhaustive]` enum 도 아는 variant 는 모두 나열하고, 관심 없는 variant 와 미래 variant 만 와일드카드 팔에 둔다 (`Shape::Triangle | _ => {}`). 특정 variant 만 관심 있으면 `if let` 이나 `let ... else` 를 쓴다.
- DON'T: 아는 variant 를 생략하고 `_ =>` 하나로 덮는다. 단, 특정 variant 만 다루고 `match` 가 `if let` 보다 깔끔하면 허용한다.

## 단언에는 문맥 문자열

- DO: `assert!` 에 왜 성립해야 하는지, 실패 시 어떤 값이었는지, API 오용인지 구현 버그인지를 문맥 문자열로 적는다. 주석 대신 쓰면 로그에도 남는다.
- DON'T: 조건을 그대로 반복하는 문맥 문자열을 쓴다.

```rust
// DO
assert!(refund > 0, "refund must be positive, got {refund:?}");
assert!(self.stream.is_some(), "call connect() before executing queries");
// DON'T
assert!(values.is_sorted(), "values should always be sorted");
```

## 읽히는 구현

- DO: 코드베이스를 아는 사람이 추론된 타입을 알 수 있을 만큼 명시 타입을 쓴다. `.into()`, `.parse()` 처럼 반환 타입이 제네릭인 호출과 복잡한 이터레이터 체인의 결과에 특히 붙인다. turbofish 보다 변수 선언에 붙이고, 강제될 때는 `_` 를 쓴다.
- DO: 짧은 체인, 또는 결합자가 로직을 대신해 주는 경우(`any`, `all`, `sum`)에 이터레이터 결합자를 쓴다.
- DON'T: 조건 분기·외부 상태 변경이 섞인 길고 복잡한 결합자 체인을 쓴다. `for` 루프로 쓴다.
- DON'T: `for` 헤더 안에서 체인을 만든다. 의도를 말하는 이름의 변수에 담은 뒤 순회한다.
- DON'T: 특별한 이유 없이 `Iterator::for_each` 를 쓴다. `for` 루프가 대개 더 읽기 쉽다.
- DO: 셋 이상의 문자열 연결과 리터럴 치환은 `format!` / `write!` 로 한다 (`a + b + c`, `str::replace` 대신). 자체 템플릿 문법을 만들지 않는다.

```rust
// DO
let urgent_bugs = bugs.iter().filter(|bug| bug.priority <= Priority::P1);
for bug in urgent_bugs { assign(bug); }
let port: u16 = raw.parse()?;

// DON'T
for bug in bugs.iter().filter(|bug| bug.priority <= Priority::P1) { assign(bug); }
```
