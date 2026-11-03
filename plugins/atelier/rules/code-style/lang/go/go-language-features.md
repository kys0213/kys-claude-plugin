---
name: go-language-features
description: Go 구체 타입 반환, 제네릭 절제, nil 슬라이스, 복사·포인터 인자·리시버, switch, context 전달, crypto/rand
paths:
  - "**/*.go"
---

# Go 언어 기능

출처: decisions §Interfaces, §Generics, §Use any, §Nil slices, §Copying, §Pass values, §Receiver type, §switch and break, §Contexts, §Custom contexts, §crypto/rand

- DO: 필요 전에는 인터페이스를 만들지 않는다. 인자는 인터페이스, 반환은 구체 타입.
- DON'T: 타입이 하나뿐인 제네릭, 제네릭 DSL·에러 처리·assertion 프레임워크. 인터페이스로 풀리면 인터페이스, `any`·타입 스위치가 넘칠 때 제네릭. DO: `interface{}` 대신 `any`, 타입 alias 는 패키지 이전 때만.
- DO: 빈 슬라이스는 `var s []T`, 검사는 `len(s) == 0`. DON'T: nil 과 길이 0 을 구분하는 API.
- DON'T: `sync.Mutex` 등을 가진 값이나 `*T` 메서드 타입을 복사. 몇 바이트 아끼려는 포인터 인자 (`*string`; 큰 구조체는 예외).
- 리시버는 변경·복사 불가 필드·큰 구조체·가변 포인터 필드·애매하면 포인터, 작은 불변 값·맵·함수·채널은 값. DO: 한 타입은 한 종류로.
- DON'T: `switch` 절 끝의 `break` (빈 절은 주석으로). 바깥 `for` 탈출은 라벨 `break`.
- DO: `context.Context` 는 첫 매개변수, 받는 함수는 보통 `error` 도 반환. 불변이라 여러 호출에 같은 값을 넘겨도 된다. DON'T: 구조체 필드 저장 (못 바꾸는 외부 인터페이스 구현만 예외), 중간 계층의 `context.Background()`, 커스텀 context 타입.
- DON'T: 키·토큰을 `math/rand` 로. DO: `crypto/rand` 후 hex·base64.
