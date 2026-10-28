---
name: rust-errors-and-safety
description: Rust 실패 보고와 안전성 — Option/Result/panic 선택, expect, match 완전성, 단언 문맥, catch_unwind 금지, unsafe 와 SAFETY 주석
paths:
  - "**/*.rs"
---

# Rust 실패와 안전성

출처: Google Rust Style Guide — Reporting errors, Implementation concerns, Other Rust features(Unsafe code).

## Option, Result, panic

| 수단 | 쓰는 경우 |
|---|---|
| `Option` | 원인이 자명한 실패 (값 없음, 형식 불일치) |
| `Result` | 호출 쪽이 원인별로 처리할 복구 가능한 실패. 주로 막을 수 없는 I/O·네트워크 |
| `panic!` | 버그, 깨진 불변식, 전제조건 위반 |

- 프로젝트가 panic 을 전면 금지하고 불변식 위반까지 `Result` 로 보고하기로 했다면 따른다.
- DO: panic 하지 않음은 `unwrap` 대신 패턴 매칭(`if let Some(v) = opt`)으로 증명한다.
- DO: 타입으로 표현 못 한 불변식은 `expect("이유")` 로 남긴다.
- DON'T: 에러 메시지 문자열(`to_string()`)을 비교해 종류를 가른다. variant 로 `match` 한다.
- DON'T: 1st-party 코드에서 `panic::catch_unwind` 로 복구한다 (`panic=abort` 에서 안 되고, panic 은 고칠 버그).

## `match` 와 단언

- DO: `match` 는 아는 variant 를 모두 나열한다. `#[non_exhaustive]` enum 은 관심 없는·미래 variant 만 와일드카드 팔에 둔다 (`Shape::Triangle | _ => {}`).
- DON'T: 아는 variant 를 `_ =>` 로 덮는다. 일부만 관심 있으면 `if let`·`let ... else` 를 쓰고, `match` 가 더 깔끔할 때만 허용한다.
- DO: `assert!` 에 왜 성립해야 하는지, 실패 값, 오용인지 버그인지를 문맥 문자열로 적는다. 조건을 되풀이하지 않는다.

## unsafe

- DON'T: 안전한 코드로 될 일에 `unsafe` 를 쓴다.
- DO: 안전한 추상 안에 가두고 비즈니스 로직에서 떼어 놓는다. 이상적으로는 의존이 적은 별도 크레이트에 둔다.
- DO: 모든 `unsafe` 블록 바로 위에 `// SAFETY:` 로 어떤 불변식이 어디서 보장되는지 적는다. `unsafe fn`·`unsafe trait` 의 불변식은 `# Safety` 문서 절에 적는다.
- DO: 왜 안전한 코드로 쓰지 않았는지(성능이면 벤치마크 유무)를 블록 또는 파일·빌드 대상 단위로 남긴다.
