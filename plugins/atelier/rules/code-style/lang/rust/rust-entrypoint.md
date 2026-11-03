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

- DON'T: `ctor` 크레이트 같은 기법으로 `main()` 전에 코드를 실행한다.
- DO: `std::process::exit` 는 drop 을 실행하지 않는다. 본문은 `Result` 를 돌려주는 함수로 빼고, `main` 은 그 결과로 종료 코드를 정한다 (`fn main() -> ExitCode` 도 된다).

## panic

- 예외: 개발자용 바이너리는 치명적이지 않은 오류에도 panic 을 쓸 수 있다.
- DON'T: 1st-party 코드에서 `panic::catch_unwind` 로 panic 을 잡아 복구한다.
- 예외: 이미 panic 을 피하려고 할 수 있는 일을 다 하는 코드베이스가, 만일을 대비한 다층 방어로 핵심 진입점을 감싸는 경우. 어떤 성질도 이것에 기대서는 안 되고, `panic=unwind` 로 빌드될 가능성이 있는 코드에만 해당한다.
