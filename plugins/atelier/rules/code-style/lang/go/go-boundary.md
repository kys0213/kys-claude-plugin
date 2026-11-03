---
name: go-boundary
description: Go 외부 경계(client·adapter·repository·handler·middleware) — 인터페이스의 위치와 반환 예외, 경계의 context, slog 레벨 확인
paths:
  - "**/*_client.go"
  - "**/*_adapter.go"
  - "**/*_repository.go"
  - "**/*_handler.go"
  - "**/*_middleware.go"
---

# Go 외부 경계

## 인터페이스

출처: decisions §Interfaces, best-practices §Interfaces

- DO: 인터페이스는 쓰는 쪽(소비자) 패키지가 정의한다. 패키지 안에서만 쓰면 비공개로 둔다.
- DO: 인터페이스 반환은 `error`, 런타임에 구체 타입을 고르는 팩토리·전략·체이닝, 순환 import 회피, 위험한 메서드를 감추는 캡슐화일 때만.
- 권장: 생산자가 인터페이스를 내보내는 경우는 인터페이스 자체가 제품(`io.Writer` 같은 공통 프로토콜)이거나, 같은 인터페이스가 여러 소비자에 복제되는 것을 막을 때다.
- 권장: 순환 의존을 끊으려고 인터페이스를 들이는 것은 패키지 경계가 잘못됐다는 신호인 경우가 많다. 먼저 패키지 구성을 다시 본다.

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

## 경계의 context

출처: decisions §Contexts

- DO: HTTP 핸들러는 `req.Context()`, 스트리밍 RPC 는 스트림의 context 를 쓴다. 새 루트(`context.Background()`)를 만들지 않는다.

## 로깅

출처: decisions §Logging, best-practices §Custom verbosity levels

- DO: 포맷할 것이 없으면 포맷하지 않는 함수(`Info(v)`)를 쓴다.
- DO: 비싼 인자를 만들기 전에 `logger.Enabled` 로 레벨을 확인한다.
- DON'T: 경계 코드에서 `log.Fatal` 계열을 쓴다. `os.Exit` 를 불러 deferred 함수가 실행되지 않는다.

```go
// DO
if logger.Enabled(ctx, slog.LevelDebug) {
    logger.Debug("plan", "explain", q.Explain())
}

// DON'T: 레벨이 꺼져 있어도 Explain 이 실행된다
logger.Debug("plan", "explain", q.Explain())
```
