---
name: go-concurrency
description: Go 고루틴 수명 관리, 동기 함수 선호, context 전달 규칙(첫 매개변수·구조체 저장 금지·커스텀 타입 금지), 채널 방향
paths:
  - "**/*.go"
---

# Go 동시성

## 고루틴 수명

출처: decisions §Goroutine lifetimes

- DO: 고루틴을 띄울 때는 언제, 어떻게 끝나는지가 코드에서 보이게 한다. 동기화 코드는 한 함수 스코프 안에 가두고, 로직은 동기 함수로 뽑는다.
- DO: 띄운 고루틴이 함수보다 오래 살지 않게 기다린다 (`sync.WaitGroup`, 신호 채널 등). 취소는 `context.Context` 로 전달한다.
- DON'T: 끝나는 시점을 모르는 고루틴을 띄우지 않는다. 채널 송수신에 막힌 고루틴은 GC 가 회수하지 않아 누수된다. 그래도 수명이 분명하지 않으면 언제·왜 끝나는지 주석으로 남긴다.
- DON'T: 닫힌 채널에 보내지 않는다 (panic). 결과가 더는 필요 없다고 아직 쓰이는 입력을 수정하지 않는다 (data race).

```go
// DO
func (w *Worker) Run(ctx context.Context) error {
    var wg sync.WaitGroup
    for item := range w.q {
        wg.Add(1)
        go func() {
            defer wg.Done()
            process(ctx, item) // ctx 취소 시 늦어도 그때 반환
        }()
    }
    wg.Wait()
    return nil
}

// DON'T: 언제 끝날지 모른다
func (w *Worker) Run() {
    for item := range w.q {
        go process(item)
    }
}
```

## 동기 함수 선호

출처: decisions §Synchronous functions

- DO: 결과를 직접 반환하고, 콜백·채널 작업을 반환 전에 끝내는 동기 함수를 기본으로 쓴다. 고루틴이 호출 안에 머물러 수명을 따지기 쉽고, 테스트에서 폴링·동기화가 필요 없다.
- DON'T: 비동기 API 를 기본으로 만들지 않는다. 동시성이 필요하면 호출자가 별도 고루틴에서 호출하면 된다. 호출자 쪽에서 불필요한 동시성을 걷어내기는 어렵다.

## context

출처: decisions §Contexts, §Custom contexts

- DO: `context.Context` 는 항상 첫 번째 매개변수로 받는다. 테스트 헬퍼도 같다 (`ctx`, 그다음 `t`).
- 예외: HTTP 핸들러는 `req.Context()`, 스트리밍 RPC 는 스트림의 context, 테스트 함수는 `t.Context()`, 진입점(`main`, `init`)은 `context.Background()` 를 쓴다.
- DON'T: 호출 체인 중간이나 라이브러리에서 `context.Background()` 로 새 루트를 만들지 않는다. 호출자에게서 받는다.
- DON'T: 구조체 필드에 context 를 저장하지 않는다. context 가 필요한 메서드마다 매개변수로 받는다. 시그니처를 바꿀 수 없는 외부 인터페이스를 구현할 때만 예외다.
- DON'T: 커스텀 context 타입이나 `context.Context` 외의 인터페이스를 시그니처에 쓰지 않는다. 예외는 없다.
- DO: context 는 불변이므로 deadline·취소·값을 공유하는 여러 호출에 같은 값을 넘겨도 된다.

```go
func (s *Store) Load(ctx context.Context, id string) (*Item, error) // DO

type Store struct {
    ctx context.Context // DON'T
}
```

## 채널 방향

출처: best-practices §Channel direction

- 권장: 가능한 곳에서는 채널 방향(`<-chan T`, `chan<- T`)을 지정한다. 읽기 전용 채널을 닫는 실수를 컴파일러가 잡고, 소유권이 드러난다.

```go
func sum(values <-chan int) int // DO
func sum(values chan int) int   // DON'T: 실수로 close 할 수 있다
```
