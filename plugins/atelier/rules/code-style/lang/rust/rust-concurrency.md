---
name: rust-concurrency
description: Rust 동시성 — Rc/Arc 회피, poisoned mutex 접근 금지, 내부 가변성 값의 클로저 접근자
paths:
  - "**/worker.rs"
  - "**/*_worker.rs"
  - "**/*_pool.rs"
---

# Rust 동시성

출처: Google Rust Style Guide — Ownership and borrowing, Concurrency(Handling poison), Other Rust features(Getters and setters). `[chrome]`·`[Fuchsia]` 전용 항목(async 금지, 표준 스레드 금지, `fuchsia_sync::Mutex`)은 제외했다.

## 공유 소유

- DO: 메모리는 고유 소유와 빌림으로 관리한다. Rust 는 공유 소유 스마트 포인터 없이도 대부분의 객체 그래프를 표현할 수 있고, `Rc`/`Arc` 는 런타임 비용과 고유한 복잡성을 더한다.
- 예외: 소유·빌림 구조가 지나치게 복잡해져 쓰기·이해·유지보수가 어려우면 `Rc`/`Arc` 를 쓴다. 판단은 리뷰에서 논의한다.

## 독이 든(poisoned) mutex

- DON'T: poisoned mutex 의 내부 값에 접근한다 (`PoisonError::into_inner`, `get_ref` 등). 어떤 이유로도 하지 않는다. 다른 스레드가 값을 고치던 중 panic 했으므로 불변식이 깨졌을 수 있다.
- DO: lock 실패는 버그로 취급해 전파하거나 panic 한다.

```rust
// DO
let guard = state.lock().expect("state mutex poisoned: a writer panicked");
// DON'T
let guard = state.lock().unwrap_or_else(|e| e.into_inner());
```

## 내부 가변성 값의 접근자

- DO: 값이 `Mutex`·`RefCell` 같은 내부 가변성 래퍼 안에 있으면 `MutexGuard` 같은 RAII 객체를 돌려주지 말고 클로저 접근자(`with_foo(f)`)로 노출한다. 가드를 돌려주면 구현 세부가 새어 나간다. 클로저의 반환값을 그대로 돌려준다.

```rust
fn with_cache<R>(&self, f: impl FnOnce(&Cache) -> R) -> R {
    let cache = self.cache.lock().expect("cache mutex poisoned");
    f(&cache)
}
```
