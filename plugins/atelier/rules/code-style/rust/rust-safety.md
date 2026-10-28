---
name: rust-safety
description: Rust 안전성 — unsafe 최소화·격리와 SAFETY 근거, poisoned mutex 접근 금지, catch_unwind 금지, main 이전 코드 금지
paths:
  - "**/*.rs"
---

# Rust 안전성

출처: Google Rust Style Guide — Other Rust features(Unsafe code, Do not run code before `main()`), Concurrency(Handling poison), Implementation concerns(Don't call `panic::catch_unwind`). `[chrome]`·`[Fuchsia]` 전용 항목(async 금지, 표준 스레드 금지, `fuchsia_sync::Mutex`)은 제외했다.

## unsafe

- DON'T: 안전한 코드로 같은 기능을 구현할 수 있는데 `unsafe` 를 쓴다.
- DO: 꼭 써야 하면 안전한 추상 안에 가두고 비즈니스 로직에서 최대한 떼어 놓는다. 이상적으로는 표준 라이브러리 외 의존이 적은 별도 크레이트에 둔다. 호출 쪽은 안전한 API 만 본다.
- DO: 모든 `unsafe` 블록에 `// SAFETY:` 주석으로 왜 올바른지 적는다. `unsafe fn`·`unsafe trait` 의 불변식은 문서 주석 `# Safety` 절에 적는다.
- DO: 기술적 근거를 남긴다 — 왜 안전한 동등 코드로 쓰지 않았는가, 성능이 이유라면 벤치마크가 있는가. 블록 단위 또는 파일·빌드 대상 단위로 쓴다.

```rust
pub struct AlignedBuf { ptr: NonNull<u8>, len: usize }

impl AlignedBuf {
    pub fn get(&self, idx: usize) -> Option<u8> {
        if idx >= self.len {
            return None;
        }
        // SAFETY: `idx < self.len` was checked above, and `ptr` points to
        // `len` initialized bytes for the lifetime of `self`.
        Some(unsafe { *self.ptr.as_ptr().add(idx) })
    }
}
```

## 독이 든(poisoned) mutex

- DON'T: poisoned mutex 의 내부 값에 접근한다 (`PoisonError::into_inner`, `get_ref` 등). 어떤 이유로도 하지 않는다. 다른 스레드가 값을 고치던 중 panic 했으므로 불변식이 깨졌을 수 있다.
- DO: lock 실패는 버그로 취급해 전파하거나 panic 한다.

```rust
// DO
let guard = state.lock().expect("state mutex poisoned: a writer panicked");
// DON'T
let guard = state.lock().unwrap_or_else(|e| e.into_inner());
```

## `panic::catch_unwind` 금지

- DON'T: 1st-party 코드에서 `panic::catch_unwind` 로 panic 을 잡아 복구한다. `panic=abort` 환경에서는 동작하지 않고, panic 은 고쳐야 할 버그다.
- 예외: 이미 panic 을 피하려고 할 수 있는 일을 다 하는 코드베이스가, 만일을 대비한 다층 방어로 핵심 진입점을 감싸는 경우. 어떤 성질도 이것에 기대서는 안 되고, `panic=unwind` 로 빌드될 가능성이 있는 코드에만 해당한다.

## main 이전 코드 금지

- DON'T: `main()` 전에 코드가 실행되게 한다 (`ctor` 크레이트 같은 기법). 초기화 순서가 보이지 않고 실패를 다룰 곳이 없다. 초기화는 `main` 에서 명시적으로 호출한다.
