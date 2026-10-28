---
name: go-testing
description: Go 테스트 관용구 — 표준 testing 패키지, 실패 메시지(got/want·diff), cmp 비교, 에러 검사, t.Error 와 t.Fatal, 테이블·서브테스트, 테스트 헬퍼, 셋업 범위
paths:
  - "**/*_test.go"
---

# Go 테스트

이 룰은 Go 테스트 관용구만 다룬다.

## 프레임워크

출처: decisions §Use package testing, §Assertion libraries, best-practices §Leave testing to the Test function

- DO: 표준 `testing` 패키지만 쓴다. 비교는 `cmp` 와 `fmt` 로 한다.
- DON'T: assertion 라이브러리나 서드파티 테스트 프레임워크를 쓰거나 만들지 않는다. 테스트를 실패시키는 판단과 메시지는 `Test` 함수 안에 둔다.
- 권장: 여러 테스트가 같은 검증을 쓰면 `*testing.T` 를 받아 실패시키는 헬퍼 대신, 값이나 `error` 를 돌려주는 검증 함수(또는 `cmp.Option`)를 만들고 실패 판단은 `Test` 에서 한다.

```go
// DON'T
assert.StringEq(t, "obj.Type", obj.Type, "post")

// DO
if got, want := obj.Type, "post"; got != want {
    t.Errorf("Load(%q).Type = %q, want %q", id, got, want)
}
```

## 실패 메시지

출처: decisions §Useful test failures

테스트 소스를 읽지 않고도 실패를 진단할 수 있어야 한다.

- DO: 실패한 함수와 입력을 밝힌다. 형식은 `YourFunc(%v) = %v, want %v`. 입력이 크면 케이스 설명을 붙여 출력한다.
- DO: 실제 값을 먼저, 기대 값을 나중에 쓴다. "actual/expected" 대신 "got/want" 를 쓴다.
- DO: 큰 값은 diff 로 출력하고 방향을 적는다 (`(-want +got)`). diff 앞에 줄바꿈을 넣는다. `cmp.Diff` 인자 순서는 주변 코드에 맞추되 메시지의 방향 표기와 일치시킨다.
- DO: 문자열은 `%q`, 작은 구조체는 `%+v` 로 출력한다. 셋업 실패는 간단히 적어도 된다.

```go
if diff := cmp.Diff(want, got); diff != "" {
    t.Errorf("Parse(%q) mismatch (-want +got):\n%s", in, diff)
}
```

## 비교

출처: decisions §Equality comparison and diffs, §Full structure comparisons, §Compare stable results

- DO: 구조체·슬라이스·맵은 필드를 하나씩 비교하지 말고 기대 값을 만들어 `cmp.Equal`/`cmp.Diff` 로 통째로 비교한다. 필요하면 `cmpopts` 로 조정한다.
- DON'T: `reflect.DeepEqual` 로 비교하지 않는다. 비공개 필드 등 구현 세부에 민감하다. `cmp` 는 테스트 전용이므로 프로덕션 코드에서 쓰지 않는다.
- DON'T: 내가 소유하지 않은 패키지의 출력 안정성에 기대지 않는다. JSON 바이트를 문자열로 비교하지 말고 파싱해서 의미로 비교한다.
- DO: 반환값이 여럿이면 구조체로 묶지 말고 각각 비교한다.

## 에러 검사

출처: decisions §Test error semantics

- DON'T: 에러 종류를 메시지 문자열로 검사하지 않는다. 메시지 문구가 바뀌면 깨지는 변경 감지 테스트가 된다.
- DO: 의미 비교는 `errors.Is` 또는 `cmp` 와 `cmpopts.EquateErrors` 로 한다.
- DO: 에러 발생 여부만 중요하면 `wantErr bool` 로 두고 `(err != nil) != tc.wantErr` 로 비교한다.

## t.Error 와 t.Fatal

출처: decisions §Keep going, best-practices §t.Error vs. t.Fatal, §Don't call t.Fatal from separate goroutines

- DO: 한 번 실행으로 실패를 모두 보도록 불일치는 `t.Error` 로 보고하고 계속 진행한다.
- DO: 이후 검사가 무의미하거나 오해를 낳을 때(셋업 실패, 다음 단계의 입력이 틀림)만 `t.Fatal` 을 쓴다.
- DO: 테이블 테스트에서 한 행만 진행할 수 없으면, 서브테스트가 없을 때는 `t.Error` 후 `continue`, `t.Run` 안이면 `t.Fatal` 을 쓴다.
- DON'T: 테스트가 띄운 고루틴 안에서 `t.Fatal`·`t.FailNow` 를 부르지 않는다. `t.Error` 후 `return` 한다.

## 테이블 테스트와 서브테스트

출처: decisions §Table-driven tests, §Subtests, §Identifying the row, best-practices §Use field names in struct literals

- DO: 같은 검증 로직으로 여러 입력을 볼 때 테이블 테스트를 쓴다. 행마다 다른 분기 로직이 필요해지면 테스트 함수를 나눈다 (예: 정상 출력용 테이블과 에러용 테이블).
- DON'T: 행을 인덱스로 식별하지 않는다 (`"case #%d"`). 이름 필드나 입력을 실패 메시지에 넣는다.
- 권장: 테스트 케이스 구조체는 필드명을 써서 초기화하고, 케이스와 무관한 0값 필드는 생략한다.
- DO: 서브테스트는 서로의 실행 결과나 상태에 기대지 않는다. 각각 단독으로 `-run` 될 수 있어야 한다.
- DO: 서브테스트 이름은 짧은 식별자처럼 짓는다. 공백·슬래시를 피하고, 긴 설명은 별도 필드에 둔다.

```go
tests := []struct {
    name    string
    in      string
    want    int
    wantErr bool
}{
    {name: "simple", in: "42", want: 42},
    {name: "empty", in: "", wantErr: true},
}
for _, tc := range tests {
    t.Run(tc.name, func(t *testing.T) {
        got, err := Parse(tc.in)
        if (err != nil) != tc.wantErr {
            t.Fatalf("Parse(%q) error = %v, want error presence = %t", tc.in, err, tc.wantErr)
        }
        if got != tc.want {
            t.Errorf("Parse(%q) = %d, want %d", tc.in, got, tc.want)
        }
    })
}
```

## 테스트 헬퍼

출처: decisions §Test helpers, §Must functions, best-practices §Error handling in test helpers

- DO: 테스트 헬퍼는 셋업·정리만 맡는다. 헬퍼의 실패는 테스트 대상이 아닌 환경 실패다.
- DO: `*testing.T` 를 받는 헬퍼는 `t.Helper()` 를 부른다. 매개변수 순서는 `ctx` 다음 `t`, 그다음 나머지다.
- 권장: 셋업 헬퍼는 에러를 반환하지 말고 무엇이 실패했는지 담아 `t.Fatalf` 한다. 이름은 `mustXxx` 로 짓는다. 정리는 `t.Cleanup` 으로 등록한다.
- 권장: 실패시킬 일이 없는 헬퍼는 `t` 를 받지 않는다.
- DO: 테스트의 context 는 `context.Background()` 대신 `t.Context()` 를 쓴다.

## 셋업 범위와 테스트 패키지

출처: best-practices §Keep setup code scoped to specific tests, decisions §Test package

- 권장: 셋업은 필요한 테스트에서 명시적으로 호출한다. `init` 이나 패키지 변수로 모든 테스트에 셋업을 걸지 않는다 — 단독 실행이 느려지고 실패하기 쉽다.
- 권장: 비싸고 일부 테스트만 쓰며 정리가 필요 없는 셋업은 `sync.Once` 로 공유한다. 패키지 전체가 정리까지 필요한 비싼 자원을 쓸 때만 `TestMain` 을 검토한다.
- DO: 테스트는 같은 패키지(`package foo`)에 둘 수 있다. 공개 API 만 검사하거나 순환 의존이 생기면 `package foo_test` 를 쓴다.

## 테스트 더블

출처: best-practices §Test double and helper packages, §Interfaces

- 권장: 테스트 더블을 실제 구현과 같은 API 패키지에서 공개하지 않는다. 더블 패키지는 원본 이름 뒤에 `test` 를 붙인다 (`creditcard` → `creditcardtest`).
- 권장: 더블이 한 종류면 `Stub` 처럼 짧게, 여러 행동이면 `AlwaysCharges`/`AlwaysDeclines` 처럼 행동으로, 여러 타입이면 `StubService` 처럼 타입을 붙인다.
- 권장: 테스트 안에서 더블을 담는 변수는 실제 타입과 구분되게 짓는다 (`spyCC`).
- 권장: 협력자는 전역 치환이 아니라 생성자·인자 주입으로 바꿔 넣는다.
