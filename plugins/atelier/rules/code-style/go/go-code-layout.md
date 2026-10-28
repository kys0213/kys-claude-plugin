---
name: go-code-layout
description: Go import 그룹·리네임·blank·dot import, 복합 리터럴, 함수·조건문 줄바꿈, 변수 선언, 문자열 조합, 패키지·파일 크기
paths:
  - "**/*.go"
---

# Go 코드 배치

## import

출처: decisions §Imports

- DO: import 는 다음 그룹 순서로 빈 줄을 두고 나눈다: 표준 라이브러리 → 그 밖의(프로젝트·외부) 패키지 → protobuf 등 생성 코드 → side-effect(`_`) import.
- DON'T: 이유 없이 import 를 리네임하지 않는다. 리네임은 이름 충돌(더 지역적인 쪽을 바꾼다), 밑줄 든 생성 패키지(`foopb`), 정보 없는 이름(`v1`)일 때만 한다. 리네임한 이름도 패키지 이름 규칙을 따르고, 같은 패키지는 어디서나 같은 이름으로 리네임한다.
- DO: 흔한 변수명과 겹치는 패키지를 리네임해야 하면 `pkg` 접미사를 쓴다 (`urlpkg`).
- DON'T: `import _` 는 `main` 패키지나 그것이 필요한 테스트에서만 쓴다. 라이브러리 패키지에서는 쓰지 않는다 (`//go:embed` 용 `embed` 는 예외).
- DON'T: `import .` 을 쓰지 않는다. 식별자가 어디서 왔는지 안 보인다.

```go
import (
    "fmt"
    "os"

    "github.com/example/lib"

    foopb "example.com/proj/foo_go_proto"

    _ "example.com/proj/plugins/dial"
)
```

## 복합 리터럴

출처: decisions §Literal formatting

- DO: 다른 패키지에 정의된 타입의 구조체 리터럴은 필드명을 쓴다. 필드 순서·전체 목록은 공개 API가 아니다. 패키지 내부 타입도 필드가 많으면 필드명을 쓴다.
- DO: 닫는 괄호는 여는 괄호와 같은 들여쓰기의 줄에 둔다. 여러 줄 리터럴의 마지막 값 뒤에 쉼표를 붙이고 줄을 바꾼다.
- DO: 슬라이스·맵 리터럴의 반복되는 타입명은 생략한다 (`gofmt -s`).
- DO: 명료성을 해치지 않으면 0값 필드를 생략해, 지정한 옵션이 눈에 띄게 한다.

```go
r := csv.Reader{Comma: ',', Comment: '#'}    // DO
r := csv.Reader{',', '#', 4, false, false}   // DON'T

items := []*Item{{Key: "a"}, {Key: "b"}}      // DO
items := []*Item{&Item{Key: "a"}, &Item{Key: "b"}} // DON'T
```

## 함수·조건문 줄바꿈

출처: decisions §Indentation confusion, §Function formatting, §Conditionals and loops

- DO: 함수 시그니처는 한 줄에 둔다. 줄바꿈된 인자가 본문처럼 보이기 때문이다.
- DON'T: 호출 인자를 줄 길이만으로 나누지 않는다. 지역 변수를 뽑아 줄인다. 나눠야 하면 의미 단위로 묶는다.
- DON'T: 특정 인자에 인라인 주석을 달지 않는다. option struct 를 쓰거나 함수 문서에 적는다.
- DON'T: `if` 조건을 여러 줄로 나누지 않는다. 단락 평가가 필요 없으면 조건을 지역 변수로 뽑는다. `switch`/`case` 도 한 줄을 유지한다.
- DON'T: 상수를 왼쪽에 두지 않는다 (`if "foo" == x`). 변수를 왼쪽에 둔다.

```go
// DON'T
if db.InTx() &&
    db.KeyMatches(row) {
    ...
}

// DO
inTx := db.InTx()
keysMatch := db.KeyMatches(row)
if inTx && keysMatch {
    ...
}
```

## 변수 선언

출처: best-practices §Variable declarations

- 권장: 0이 아닌 값으로 초기화하면 `:=` 를 쓴다 (`i := 42`, not `var i = 42`).
- 권장: 나중에 채울 빈 값은 0값 선언(`var buf Point`)으로 쓴다. 빈 복합 리터럴로 0값을 흉내 내지 않는다.
- 권장: 초기 원소를 알면 복합 리터럴로 선언한다. 0값 포인터가 필요하면 `new(T)` 또는 `&T{}`.
- 권장: 맵은 쓰기 전에 반드시 초기화한다. 0값 맵 읽기는 괜찮다.
- 권장: 크기 힌트(`make([]T, 0, n)`)는 측정으로 필요가 확인된 성능 경로에서만 쓴다.

## 문자열 조합

출처: best-practices §String concatenation, decisions §Use %q

- 권장: 몇 개 이어 붙이기는 `+`, 포맷이 있으면 `fmt.Sprintf`, 조각을 반복해 쌓으면 `strings.Builder` 를 쓴다.
- 권장: 결과를 `io.Writer` 로 보낼 거면 임시 문자열 없이 `fmt.Fprintf` 로 바로 쓴다.
- 권장: 여러 줄 상수 문자열은 백틱 리터럴로 쓴다.
- DO: 사람이 읽는 출력에서 문자열을 따옴표로 감쌀 때는 `%q` 를 쓴다. 빈 문자열이 `""` 로 드러난다.

```go
fmt.Printf("value %q is invalid", s)     // DO
fmt.Printf("value \"%s\" is invalid", s) // DON'T
```

## 패키지와 파일 크기

출처: best-practices §Package size

- 권장: 함께 import 해야만 의미 있게 쓸 수 있는 두 패키지는 하나로 합친다. 구현이 긴밀히 얽힌 타입은 같은 패키지에 두어 비공개 식별자로 결합한다.
- 권장: 개념적으로 구별되는 것은 작은 패키지로 나눈다. 패키지명과 타입명이 함께 의미 있는 식별자가 되게 한다 (`bytes.Buffer`).
- 권장: "타입 하나에 파일 하나" 규칙은 없다. 수천 줄짜리 파일도, 아주 작은 파일 여럿도 피하고, 어느 파일에 무엇이 있는지 유지보수자가 짐작할 수 있게 관련 코드를 파일로 묶는다.
