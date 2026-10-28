---
name: rust-entrypoint
description: Rust 진입점 — main 이전 코드 금지, 개발자용 바이너리의 panic 허용, catch_unwind 다층 방어 예외
paths:
  - "**/main.rs"
  - "**/bin/**/*.rs"
---

# Rust 진입점

출처: Google Rust Style Guide — Other Rust features(Do not run code before `main()`), Reporting errors, Implementation concerns(Don't call `panic::catch_unwind`).

## main 이전 코드 금지

- DON'T: `main()` 전에 코드가 실행되게 한다 (`ctor` 크레이트 같은 기법). 초기화 순서가 보이지 않고 실패를 다룰 곳이 없다.
- DO: 초기화는 `main` 에서 명시적으로 호출한다.

## panic

- 예외: 개발자용 바이너리는 치명적이지 않은 오류에도 panic 을 쓸 수 있다.
- DON'T: 1st-party 코드에서 `panic::catch_unwind` 로 panic 을 잡아 복구한다.
- 예외: 이미 panic 을 피하려고 할 수 있는 일을 다 하는 코드베이스가, 만일을 대비한 다층 방어로 핵심 진입점을 감싸는 경우. 어떤 성질도 이것에 기대서는 안 되고, `panic=unwind` 로 빌드될 가능성이 있는 코드에만 해당한다.
