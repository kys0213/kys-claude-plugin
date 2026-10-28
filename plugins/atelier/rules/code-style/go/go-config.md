---
name: go-config
description: Go 설정·옵션 파일 — 긴 인자 목록 대신 option struct, variadic option 의 조건과 형태, 기본값 필드
paths:
  - "**/config.go"
  - "**/options.go"
  - "**/config/**/*.go"
---

# Go 설정과 옵션

## 함수 인자 목록

출처: best-practices §Function argument lists, decisions §Function formatting

- 권장: 시그니처가 길어지면 더 단순한 함수 여럿으로 나누거나, option struct 를 마지막 인자로 받는다. option struct 는 필드명으로 자기 문서화되고, 기본값 필드를 생략할 수 있고, 호출부를 깨지 않고 늘어난다.
- DON'T: `context.Context` 를 option struct 에 넣지 않는다. 첫 매개변수로 따로 받는다.
- DON'T: 특정 인자의 의미를 호출부 인라인 주석으로 설명하지 않는다. option struct 의 필드명이 그 역할을 한다.
- 권장: variadic option(`...Option`)은 코드가 많이 들므로 대부분의 호출자가 옵션을 안 쓰고, 옵션 수가 많고, 외부 패키지가 옵션을 정의해야 하는 등 이점이 클 때만 쓴다.
- 권장: 옵션 함수는 존재 여부가 아니라 값을 받는다 (`FailFast(enabled bool)`, not `EnableFailFast()`). 중복 옵션은 마지막 값이 이긴다.

```go
// DON'T
func EnableReplication(ctx context.Context, cfg *Config, primary, readonly []string, replicate, overwrite bool, interval time.Duration, workers int)

// DO
func EnableReplication(ctx context.Context, opts ReplicationOptions)
```

## 설정 값의 출처

출처: decisions §Flags, §Must functions

- DON'T: 범용 패키지가 import 부수효과로 플래그를 노출하지 않는다. 라이브러리는 함수 인자나 구조체 필드 같은 Go API 로 설정받고, 플래그는 진입점에서 정의해 그 값을 넘긴다.
- DO: 설정을 파싱하다 실패하면 멈추는 `MustXxx` 는 프로그램 시작 시점에만 쓴다. 그 밖에서는 에러를 반환한다.
