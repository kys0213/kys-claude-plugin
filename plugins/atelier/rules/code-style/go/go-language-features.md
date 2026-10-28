---
name: go-language-features
description: Go 인터페이스 설계·소유, 구체 타입 반환, 제네릭 절제, nil 슬라이스, 값 복사·포인터 인자·리시버 타입, 전역 상태, 함수 인자 목록과 option struct
paths:
  - "**/*.go"
---

# Go 언어 기능

## 인터페이스

출처: decisions §Interfaces, best-practices §Interfaces

- DO: 실제 필요가 생기기 전에는 인터페이스를 만들지 않는다. "service", "repository" 같은 패턴 이름이 인터페이스를 정당화하지 않는다. 행동과 구체 구현을 먼저 쓴다.
- DO: 인터페이스는 쓰는 쪽(소비자) 패키지가 정의하고, 실제로 호출하는 메서드만 담는다. 작을수록 구현·조합이 쉽다.
- DO: 패키지 안에서만 쓰는 인터페이스는 비공개로 둔다.
- DO: 인자는 인터페이스로 받고 반환은 구체 타입으로 한다. 인터페이스 반환은 `error`, 런타임에 여러 구체 타입 중 하나를 고르는 팩토리·전략·체이닝, 순환 import 회피, 위험한 메서드를 감춰야 하는 캡슐화일 때만 한다.
- 권장: 생산자가 인터페이스를 내보내는 경우는 인터페이스 자체가 제품(공통 프로토콜, 예: `io.Writer`)이거나, 같은 인터페이스가 여러 소비자에 복제되는 것을 막아야 할 때다.
- 권장: 순환 의존을 끊으려고 인터페이스를 들이는 것은 패키지 경계가 잘못됐다는 신호인 경우가 많다. 먼저 패키지 구성을 다시 본다.
- 권장: 테스트 더블을 실제 구현과 같은 API 패키지에서 공개하지 않는다. 테스트용 협력자는 전역 치환이 아니라 생성자·인자 주입으로 바꿔 넣는다.

```go
// DO: 소비자가 필요한 메서드만
package report

type sink interface {
    Write(p []byte) (int, error)
}

func Render(s sink, r Report) error { ... }

// DON'T: 구현 패키지가 쓰이지도 않을 큰 인터페이스를 미리 정의
package storage

type Storage interface { Get(...); Put(...); Delete(...); List(...); Watch(...) }
```

## 제네릭

출처: decisions §Generics, §Use any

- DO: 제네릭은 요구사항을 충족할 때만 쓴다. 슬라이스·맵·인터페이스로 충분하면 쓰지 않는다.
- DON'T: 실제로 쓰이는 타입이 하나뿐이면 제네릭을 쓰지 않는다. 그 타입으로 먼저 작성한다.
- DON'T: 제네릭으로 DSL, 에러 처리 프레임워크, assertion 라이브러리를 만들지 않는다.
- DO: 공통 인터페이스로 풀리면 인터페이스로, 그렇지 않고 `any` 와 타입 스위치가 넘치면 제네릭을 검토한다.
- DO: 새 코드는 `interface{}` 대신 `any` 를 쓴다. 타입 alias(`type A = B`)는 패키지 이전 같은 드문 경우에만 쓴다.

## nil 슬라이스

출처: decisions §Nil slices

- DO: 빈 슬라이스 지역 변수는 `var s []T` 로 선언한다 (`s := []T{}` 아님).
- DON'T: nil 과 길이 0 슬라이스를 구분해야 하는 API 를 만들지 않는다. 비었는지는 `len(s) == 0` 으로 검사한다.

## 복사, 포인터 인자, 리시버 타입

출처: decisions §Copying, §Pass values, §Receiver type

- DON'T: `sync.Mutex` 처럼 복사하면 안 되는 필드를 가진 값, 메서드가 `*T` 에 붙은 타입 `T` 의 값을 복사하지 않는다. 이런 구조체는 포인터로 주고받는다.
- DON'T: 몇 바이트 아끼려고 포인터 인자를 쓰지 않는다. `*string`, `*io.Reader` 처럼 `*x` 로만 읽는 인자는 값으로 받는다. 큰 구조체·커질 구조체는 예외다.
- 리시버는 정확성 기준으로 고른다.
  - 포인터: 리시버를 변경할 때, 복사 불가 필드가 있을 때, 큰 구조체일 때, 변경될 수 있는 포인터 필드를 가질 때, 판단이 서지 않을 때.
  - 값: 재할당하지 않는 슬라이스, 맵·함수·채널, 변경하지 않는 기본 타입, 포인터·가변 필드 없는 작은 값 타입.
- DO: 한 타입의 메서드는 모두 포인터 리시버거나 모두 값 리시버로 맞춘다.

```go
type Counter struct {
    mu    sync.Mutex
    total int
}

func (c *Counter) Inc() { ... } // DO
func (c Counter) Inc()  { ... } // DON'T: mu 가 복사된다
```

## switch

출처: decisions §switch and break

- DON'T: `switch` 절 끝에 라벨 없는 `break` 를 두지 않는다. Go 의 `case` 는 자동으로 끝난다. 빈 절의 의도는 주석으로 적는다.
- DO: `switch` 안에서 바깥 `for` 를 빠져나가려면 라벨을 붙인 `break` 를 쓴다.

## 전역 상태

출처: best-practices §Global state

- 권장: 라이브러리는 전역 상태에 기대는 API 를 강제하지 않는다. 상태가 있으면 인스턴스 타입(`New() *Registry`)을 제공하고, 클라이언트가 그 인스턴스를 명시적 의존성으로 넘기게 한다.
- 권장: 패키지 수준 변수, 전역 서비스 로케이터, 콜백 레지스트리, 지연 생성 싱글톤 클라이언트를 API 로 노출하지 않는다.
- 권장: 전역 상태가 안전한 경우는 논리적 상수이거나, 관측 가능한 동작이 무상태(호출자가 구분할 수 없는 캐시)이거나, 프로그램 밖으로 새지 않을 때다. 서로 독립적인 테스트가 전역 상태로 얽히거나 테스트가 전역 값을 갈아끼우고 싶어지면 안전하지 않다.
- 권장: 편의를 위해 기본 인스턴스를 제공해야 하면, 인스턴스 API 의 얇은 프록시로만 두고 바이너리에서만 쓴다.

## 함수 인자 목록

출처: best-practices §Function argument lists, decisions §Function formatting

- 권장: 시그니처가 길어지면 더 단순한 함수 여럿으로 나누거나, option struct 를 마지막 인자로 받는다. option struct 는 필드명으로 자기 문서화되고, 기본값 필드를 생략할 수 있고, 호출부를 깨지 않고 늘어난다.
- DON'T: `context.Context` 를 option struct 에 넣지 않는다.
- 권장: variadic option(`...Option`)은 코드가 많이 들므로 대부분의 호출자가 옵션을 안 쓰고, 옵션 수가 많고, 외부 패키지가 옵션을 정의해야 하는 등 이점이 클 때만 쓴다. 옵션 함수는 존재 여부가 아니라 값을 받는다 (`FailFast(enabled bool)`, not `EnableFailFast()`). 중복 옵션은 마지막 값이 이긴다.

```go
// DON'T
func EnableReplication(ctx context.Context, cfg *Config, primary, readonly []string, replicate, overwrite bool, interval time.Duration, workers int)

// DO
func EnableReplication(ctx context.Context, opts ReplicationOptions)
```
