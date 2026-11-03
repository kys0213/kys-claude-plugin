---
name: go-boundary
description: Go 외부 경계(client·adapter·repository·handler·middleware) — 소비자 정의 인터페이스, 경계의 context, 경계 에러 변환, 로깅(중복·레벨·PII·비용)
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

- DO: 실제 필요가 생기기 전에는 인터페이스를 만들지 않는다. "service", "repository" 같은 패턴 이름이 인터페이스를 정당화하지 않는다. 행동과 구체 구현을 먼저 쓴다.
- DO: 인터페이스는 쓰는 쪽(소비자) 패키지가 정의하고, 실제로 호출하는 메서드만 담는다. 패키지 안에서만 쓰면 비공개로 둔다.
- DO: 인자는 인터페이스로 받고 반환은 구체 타입으로 한다. 인터페이스 반환은 `error`, 런타임에 구체 타입을 고르는 팩토리·전략·체이닝, 순환 import 회피, 위험한 메서드를 감추는 캡슐화일 때만.
- 권장: 생산자가 인터페이스를 내보내는 경우는 인터페이스 자체가 제품(`io.Writer` 같은 공통 프로토콜)이거나, 같은 인터페이스가 여러 소비자에 복제되는 것을 막을 때다.
- 권장: 순환 의존을 끊으려고 인터페이스를 들이는 것은 패키지 경계가 잘못됐다는 신호인 경우가 많다. 먼저 패키지 구성을 다시 본다.
- 권장: 테스트용 협력자는 전역 치환이 아니라 생성자·인자 주입으로 바꿔 넣는다.

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

## 경계의 context 와 에러

출처: decisions §Contexts, best-practices §Adding information to errors

- DO: HTTP 핸들러는 `req.Context()`, 스트리밍 RPC 는 스트림의 context 를 쓴다. 새 루트(`context.Background()`)를 만들지 않는다.
- 권장: 시스템 경계(RPC·IPC·저장소)에서 내부 에러를 그대로 노출하지 말고, `%v` 로 정해진 에러 공간의 새 에러로 옮긴다.

## 로깅

출처: decisions §Logging, best-practices §Logging errors, §Custom verbosity levels

- DO: 프로젝트가 정한 하나의 로깅 패키지를 쓴다. 포맷할 것이 없으면 포맷하지 않는 함수(`Info(v)`)를 쓴다.
- 권장: 로그 메시지는 좋은 테스트 실패 메시지처럼 무엇이 잘못됐는지와 진단 정보를 담는다.
- 권장: 에러를 반환하는 함수는 그 에러를 직접 로그하지 않는다. 반환과 로그를 둘 다 하면 같은 에러가 여러 번 찍힌다. 기록 여부·빈도 제한은 호출자가 정한다.
- 권장: 개인정보(PII)를 로그에 남기지 않는다. 많은 로그 저장소가 민감한 사용자 정보에 맞지 않는다.
- 권장: ERROR 레벨은 아껴 쓴다. "더 심각함"이 아니라 "누군가 조치해야 함"을 뜻한다.
- 권장: 상세 로그의 인자를 만드는 비용이 크면 레벨이 켜졌는지 먼저 확인한다.
- DON'T: 라이브러리·경계 코드에서 로그 후 종료하는 함수(`log.Fatal` 등)를 쓰지 않는다. `os.Exit` 를 불러 deferred 함수가 실행되지 않는다. 에러를 반환한다.

```go
// DO
if logger.Enabled(ctx, slog.LevelDebug) {
    logger.Debug("plan", "explain", q.Explain())
}

// DON'T: 레벨이 꺼져 있어도 Explain 이 실행된다
logger.Debug("plan", "explain", q.Explain())
```
