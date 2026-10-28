---
name: go-error-definitions
description: Go 에러 정의 파일 — sentinel·에러 타입 선언과 분류, %w 래핑 계약과 위치, 정보 덧붙이기, panic·Must 함수·recover 의 경계
paths:
  - "**/errors.go"
  - "**/error.go"
  - "**/*_errors.go"
---

# Go 에러 정의

## 에러 분류

출처: best-practices §Error structure, decisions §Test error semantics, §Returning errors

에러 분류는 에러를 정의한 쪽이 선언하고, 처리하는 쪽은 그 선언으로 가른다.

- DO: 호출자가 조건을 구분해야 하면 sentinel 값(`var ErrNotFound = errors.New(...)`)이나 에러 타입으로 구조를 준다. 호출자가 필요로 하는 추가 정보는 타입의 필드로 준다.
- DO: 구분은 `errors.Is`(sentinel)·`errors.As`(타입)로 한다. 래핑되지 않음이 보장될 때만 `==`.
- DON'T: 공개 함수의 반환 타입을 `*MyError` 로 두지 않는다. nil 포인터가 인터페이스에 담겨 non-nil 에러가 된다.
- DON'T: 문자열(`err.Error()`, 메시지 비교·정규식)로 가르지 않는다 — 프로덕션·테스트 모두. 의존성이 구분 수단을 주지 않으면 파싱하지 말고 그쪽 API 개선을 요청한다.

```go
// DO
switch {
case errors.Is(err, ErrDuplicate):
    ...
case errors.As(err, &pathErr):
    ...
}

// DON'T
if strings.Contains(err.Error(), "duplicate") { ... }
```

## 에러 문자열

출처: decisions §Error strings

- DO: 소문자로 시작하고(고유명사·공개 이름·약어 제외) 구두점으로 끝내지 않는다 — 다른 문맥에 이어 붙어 출력된다. 로그·테스트 실패·UI 처럼 완성된 메시지는 대문자로 시작해도 된다.

```go
fmt.Errorf("something bad happened")  // DO
fmt.Errorf("Something bad happened.") // DON'T
```

## 정보 덧붙이기와 래핑

출처: best-practices §Adding information to errors, §Placement of %w in errors

- 권장: 아래 에러가 이미 가진 정보(예: `os` 에러의 경로)를 반복하지 않는다. 실패 사실만 알리는 "failed" 는 붙이지 말고 그냥 반환한다.
- 권장: 호출자가 아래 에러를 `errors.Is`/`errors.As` 로 검사해야 하면 `%w`. 이렇게 노출한 아래 에러는 패키지 계약이므로 문서화하고 테스트한다.
- 권장: 사람에게 보여주기만 하거나, 시스템 경계(RPC·IPC·저장소)에서 내부 에러를 감추고 정해진 에러 공간으로 옮길 때는 `%v` 로 새 에러를 만든다.
- 권장: `%w` 는 끝(`"...: %w"`)에 둬 출력 순서가 체인 순서(새 것 → 오래된 것)와 같게 한다. 범주를 나타내는 sentinel 만 앞(`"%w: ..."`)에 둔다.

```go
return fmt.Errorf("launch codes unavailable: %w", err)     // DO
return fmt.Errorf("could not open settings.txt: %v", err)  // DON'T: 경로 중복
return fmt.Errorf("failed: %v", err)                       // DON'T: 정보 없음
return fmt.Errorf("%w: invalid header: %v", ErrParse, err) // DO: 범주 sentinel 은 앞
```

## panic 과 Must 함수

출처: decisions §Don't panic, §Must functions, best-practices §Program checks and panics, §When to panic

- DON'T: 일반 에러 처리에 `panic` 을 쓰지 않는다. `error` 와 다중 반환을 쓴다.
- DO: 실패 시 프로그램을 멈추는 설정 헬퍼는 `MustXxx` 로 이름 짓고 프로그램 시작 시점(패키지 변수 초기화 등)에만 호출한다. 사용자 입력처럼 일반 에러 처리가 가능한 곳에서는 쓰지 않는다.
- DO: 테스트에서 `t.Fatal` 로 현재 테스트만 멈추는 `mustXxx` 헬퍼는 허용한다.
- 권장: 내부 상태가 복구 불가능하게 깨진 불변식 위반만 프로그램을 끝낸다. deferred 함수가 교착·추가 손상을 일으킬 수 있는 `panic` 보다 로그 후 종료를 쓴다.
- 권장: 크래시를 피하려고 `recover` 로 panic 을 삼키지 않는다. 깨진 상태가 퍼진다.
- 권장: 파서처럼 깊게 얽힌 내부 구현이 panic 으로 탈출하는 패턴은 허용하되, 공개 API 경계에서 `recover` 해 에러로 바꾸고 자기가 일으킨 panic 이 아니면 다시 panic 한다.
- 권장: 잘못된 플래그·설정 같은 초기화 오류는 panic 대신 `main` 까지 에러로 올린다.

```go
var defaultVersion = MustParse("1.2.3") // DO: 초기화 시점

func Version(o *Object) (*Version, error) {
    v := MustParse(o.VersionString()) // DON'T: 에러를 반환할 수 있는 곳
    ...
}
```
