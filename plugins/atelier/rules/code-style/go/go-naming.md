---
name: go-naming
description: Go 식별자·패키지·리시버·상수·약어·함수 이름 규칙과 반복·섀도잉 방지
paths:
  - "**/*.go"
---

# Go 이름

이름은 쓰이는 자리에서 반복처럼 느껴지지 않아야 하고, 문맥이 이미 말하는 개념을 되풀이하지 않는다.

## 대소문자와 밑줄

출처: guide §MixedCaps, decisions §Underscores, §Constant names

- DO: 여러 단어 이름은 `MixedCaps`/`mixedCaps` 로 쓴다. 지역 변수는 비공개로 취급한다.
- DON'T: 이름에 밑줄을 넣지 않는다. 예외는 `*_test.go` 의 Test·Benchmark·Example 함수, 생성 코드만 import 하는 패키지, OS·cgo 저수준 식별자뿐이다. 파일명은 식별자가 아니므로 밑줄을 써도 된다.
- DON'T: 상수에 `MAX_LENGTH`, `kMaxLength` 같은 형식을 쓰지 않는다.
- DO: 상수는 값이 아니라 역할로 이름 짓는다. 값 말고 역할이 없으면 상수로 만들지 않는다.

```go
const MaxPacketSize = 512   // DO
const MAX_PACKET_SIZE = 512 // DON'T
const Twelve = 12           // DON'T: 값에서 따온 이름
```

## 패키지 이름

출처: decisions §Package names

- DO: 소문자와 숫자만, 여러 단어는 붙여 쓴다 (`tabwriter`, `oauth2`).
- DON'T: 밑줄·대문자를 쓰지 않는다. 밑줄 있는 패키지를 import 하면 규칙에 맞는 이름으로 바꿔 import 한다.
- DON'T: 흔한 지역 변수명과 겹쳐 가려질 이름을 피한다 (`count` 보다 `usercount`).

## 리시버 이름

출처: decisions §Receiver names

- DO: 타입을 줄인 한두 글자로, 그 타입의 모든 메서드에서 같은 이름을 쓴다.
- DON'T: `this`, `self`, 긴 이름, `_` 를 쓰지 않는다. 쓰지 않으면 이름을 생략한다.

```go
func (w *ReportWriter) Flush() error    // DO
func (this *ReportWriter) Flush() error // DON'T
```

## 약어

출처: decisions §Initialisms

- DO: 약어는 한 덩어리로 같은 대소문자를 쓴다: `URL`/`url`, `ID`/`id`, `DB`/`db`, `XMLAPI`/`xmlAPI`.
- DO: 소문자가 섞인 약어는 산문 표기를 따르되, 공개 여부 때문에 첫 글자를 바꾸면 전체를 맞춘다: `iOS`→`IOS`, `gRPC`→`GRPC`.
- DON'T: `Url`, `Id`, `XmlApi`, `Grpc`.

## Getter 와 함수 이름

출처: decisions §Getters, best-practices §Function and method names

- DON'T: `Get` 접두사를 붙이지 않는다 (`Counts`, not `GetCounts`). 개념 자체가 get 인 경우(HTTP GET)만 예외다.
- DO: 오래 걸리거나 실패할 수 있는 계산·원격 호출은 `Compute`, `Fetch` 처럼 그 사실이 드러나는 동사를 쓴다.
- 권장: 값을 돌려주는 함수는 명사형, 동작하는 함수는 동사형으로 짓는다.
- 권장: 타입만 다른 같은 함수는 타입을 끝에 붙인다 (`ParseInt`, `ParseInt64`). 기본판이 분명하면 그쪽은 생략한다.

## 반복 없애기

출처: decisions §Repetition, best-practices §Avoid repetition

이름은 사용처에서 평가한다. 패키지명·리시버 타입·인자·반환 타입이 이미 주는 정보를 이름에 다시 넣지 않는다.

```go
widget.NewWidget(...)               // DON'T → widget.New(...)
yamlconfig.ParseYAMLConfig(in)      // DON'T → yamlconfig.Parse(in)
func (c *Config) WriteConfigTo(...) // DON'T → WriteTo
func (p *Project) ProjectName()     // DON'T → Name
```

같은 이름끼리 구분이 필요할 때만 정보를 더한다 (`WriteTextTo`, `WriteBinaryTo`).

## 변수 이름

출처: decisions §Variable names, §Single-letter variable names

- DO: 이름 길이는 스코프 크기에 비례하고, 그 스코프에서 쓰이는 횟수에 반비례한다.
- DO: 값이 어디서 왔는지보다 지금 무엇을 담고 어떻게 쓰이는지로 짓는다.
- DON'T: 타입을 이름에 넣지 않는다 (`users`, not `userSlice`; `count`, not `numUsers`). 같은 값이 두 형태로 있을 때만 `limitRaw`/`limit` 처럼 구분한다.
- DON'T: 타이핑을 아끼려고 글자를 빼지 않는다 (`Sandbox`, not `Sbx`).
- 한 글자 이름은 리시버, 루프 인덱스·좌표, 관용 이름(`r` io.Reader·요청, `w` io.Writer·응답)에만 쓴다.

## 섀도잉

출처: best-practices §Shadowing

- 권장: 새 블록 안의 `:=` 는 바깥 변수를 가리는 새 변수를 만든다. 바깥 값을 바꾸려는 의도면 `=` 를 쓴다.

```go
// DON'T: 블록이 끝나면 ctx 는 다시 원래 값이다
if short {
    ctx, cancel := context.WithTimeout(ctx, d)
    defer cancel()
}

// DO
if short {
    var cancel context.CancelFunc
    ctx, cancel = context.WithTimeout(ctx, d)
    defer cancel()
}
```

- 권장: 아주 작은 스코프가 아니면 표준 패키지 이름(`url` 등)을 변수명으로 쓰지 않는다.

## 테스트 더블 이름

출처: best-practices §Test double and helper packages

- 권장: 테스트 더블 패키지는 원본 이름 뒤에 `test` 를 붙인다 (`creditcard` → `creditcardtest`).
- 권장: 더블이 한 종류면 `Stub` 처럼 짧게, 여러 행동이면 `AlwaysCharges`/`AlwaysDeclines` 처럼 행동으로, 여러 타입이면 `StubService` 처럼 타입을 붙인다.
- 권장: 테스트 안에서 더블을 담는 변수는 실제 타입과 구분되게 짓는다 (`spyCC`).
