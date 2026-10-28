---
name: rust-ownership-and-types
description: Rust 소유권·입력·타입·구현 관용구 — clone 우회 금지, 참조 입력, bool 대신 enum, newtype, 명시 타입, 체인 절제, format!
paths:
  - "**/*.rs"
---

# Rust 소유권·타입·구현

출처: Google Rust Style Guide — Ownership and borrowing, Function inputs and outputs, Types, Implementation concerns.

- DON'T: 빌림 오류를 피하려고 `.clone()` 한다. 깊은 복사가 필요할 때, 빌림이 과도하게 복잡할 때만 쓴다 (`Rc`/`Arc` clone 은 예외).
- DO: 불변 입력은 데이터의 참조로 받는다 — `&T`·`&[T]`·`&str`·`Option<&T>` (`&Vec<T>`·`&String` 등 X).
- DO: 호출부에서 의미가 안 보이는 `bool` 인자는 enum 으로 바꾼다 (`Recursion::Recursive`). 주석으로 때우지 않는다.
- DO: 의미가 다르거나, 자기 API 가 필요하거나, 기반 타입을 바꿀 여지를 두거나, 섞이기 쉬운 값(`UserId`/`OrderId`)이면 타입 별칭 대신 newtype.
- DO: 튜플보다 이름 있는 필드. 예외: 필드 하나인 newtype, 아주 지역적인 문맥.
- DO: 추론 타입을 알 수 있게 명시한다. `.into()`·`.parse()`·복잡한 체인 결과에 특히, turbofish 보다 변수 선언에, 강제될 때는 `_` (`let port: u16 = raw.parse()?;`).
- DO: 결합자는 짧은 체인이나 `any`·`all`·`sum` 에. DON'T: 분기·상태 변경이 섞인 긴 체인(→ `for`), `for` 헤더 안의 체인(→ 이름 있는 변수), 이유 없는 `for_each`.
- DO: 셋 이상의 연결·리터럴 치환은 `format!`/`write!`. 자체 템플릿 문법을 만들지 않는다.
