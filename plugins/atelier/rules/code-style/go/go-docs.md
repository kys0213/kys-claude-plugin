---
name: go-docs
description: Go doc 주석·패키지 주석의 형식, 이름 붙은 반환값, 문서에 담을 계약(context·동시성·정리·에러), signal boosting
paths:
  - "**/*.go"
---

# Go 문서 주석

이 룰은 doc 주석의 **형식**만 다룬다. 주석에는 코드가 말하지 못하는 why·제약·함정만 적는다. 그래서 공개 이름마다 doc 주석을 의무로 달지 않는다.

## doc 주석 형식

출처: decisions §Doc comments, §Comment sentences

- DO: doc 주석은 설명하는 이름으로 시작하는 완전한 문장으로 쓴다. 앞에 관사(a, an, the)가 와도 된다. 비공개 이름의 doc 주석도 같은 형식을 쓴다 — 나중에 공개로 바꿀 때 이름만 바꾸면 된다.
- DO: 완전한 문장은 대문자로 시작하고 마침표로 끝낸다. 구조체 필드의 줄 끝 주석은 필드를 주어로 한 짧은 구절이면 된다.
- DO: 구조체 안의 주석은 다음 필드 또는 필드 묶음에 적용된다. 묶음 주석으로 필드를 무리 지을 수 있다.

```go
// DO
// Encode writes the JSON encoding of req to w.
func Encode(w io.Writer, req *Request) error

// DON'T: 이름으로 시작하지 않는다
// This function writes JSON.
func Encode(w io.Writer, req *Request) error

type Options struct {
    PageLength int // lines per page (optional; default: 20)
}
```

## 주석 줄 길이

출처: decisions §Comment line length

- DO: 주석 줄 길이 제한은 없지만, 긴 문단은 자동 줄바꿈 없는 도구에서도 읽히게 나눈다 (80~100 칸이 흔하다). 한 파일 안에서 일관되게 한다.
- DON'T: 긴 문단을 한 줄에 몰아 쓰지 않는다. 반대로 긴 URL 처럼 쪼개면 해로운 것은 쪼개지 않는다.

## 패키지 주석

출처: decisions §Package comments

- DO: 패키지 주석은 `package` 절 바로 위에, 빈 줄 없이 둔다.
- DON'T: 한 패키지에 패키지 주석을 둘 이상 두지 않는다. 여러 파일 중 정확히 한 파일에만 둔다. 길면 주석과 `package` 절만 있는 `doc.go` 에 둔다.
- DO: 형식은 `// Package name ...`, `main` 패키지는 바이너리 이름으로 시작한다 (`// The seed_generator command ...`, `// Command seed_generator ...`).
- DO: 파일 전체에 적용되는 유지보수자용 주석은 import 선언 뒤에 둔다. 이것은 패키지 문서에 노출되지 않는다.

## 이름 붙은 반환값

출처: decisions §Named result parameters

- DO: 같은 타입 반환값이 둘 이상이거나, 호출자가 특정 반환값으로 해야 할 일이 있을 때(예: `cancel` 호출) 이름을 붙인다.
- DO: deferred 클로저에서 반환값을 바꿔야 하면 이름을 붙여도 된다.
- DON'T: 반복만 낳는 이름(`(node *Node)`)을 붙이지 않는다. 함수 안 변수 선언을 아끼거나 naked return 을 쓰려고 이름을 붙이지 않는다. naked return 은 작은 함수에서만 쓴다.

```go
func (n *Node) Children() (left, right *Node, err error) // DO
func (n *Node) Parent() (node *Node, err error)          // DON'T
```

## 문서에 담을 계약

출처: best-practices §Documentation conventions, decisions §Examples

문서를 쓴다면 관례로 알 수 있는 것은 빼고, 관례에서 벗어나는 계약만 적는다.

- 권장: 매개변수·필드를 하나씩 나열하지 않는다. 틀리기 쉽거나 비자명한 것만 왜 중요한지 적는다.
- 권장: context 취소가 함수를 중단하고 `ctx.Err()` 를 돌려준다는 사실은 다시 적지 않는다. 취소 시 다른 에러를 돌려주거나, context 외의 중단 수단이 있거나, context 에 특별한 요구(값·deadline)가 있을 때만 적는다.
- 권장: 읽기 작업은 동시에 써도 안전하고 변경 작업은 아니라는 기본 가정은 적지 않는다. 읽기처럼 보이지만 내부 상태를 바꾸는 경우, API 가 동기화를 제공하는 경우, 사용자 구현 인터페이스에 동시성 요구가 있는 경우만 적는다.
- 권장: 호출자가 해야 하는 정리(`Stop`, `Close`)는 반드시 적는다.
- 권장: 호출자가 분기할 수 있도록 반환하는 sentinel 에러·에러 타입을 적고, 타입이면 포인터인지 밝힌다.
- 권장: 사용 예는 주석 속 코드보다 테스트 파일의 runnable example(`ExampleXxx`)로 둔다.

```go
// DO: 관례에서 벗어나는 계약만 적는다
// Lookup returns the data associated with the key.
//
// Lookup is not safe for concurrent use: a hit reorders the LRU list.
func (c *Cache) Lookup(key string) ([]byte, bool)

// DON'T: 관례를 되풀이한다
// Run runs the loop. It stops when ctx is cancelled and returns an error.
func (w *Worker) Run(ctx context.Context) error
```

## Signal boosting

출처: guide §Concision, best-practices §Signal boosting

- 권장: 흔한 관용구와 거의 같지만 미묘하게 다른 줄에는 짧은 주석으로 차이를 드러낸다.

```go
if err := doSomething(); err == nil { // if NO error
    ...
}
```
