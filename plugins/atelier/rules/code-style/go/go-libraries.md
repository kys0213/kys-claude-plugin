---
name: go-libraries
description: Go 로깅(중복·레벨·PII·비용·종료 함수)과 crypto/rand 사용 규칙
paths:
  - "**/*.go"
---

# Go 공통 라이브러리

## 로깅

출처: decisions §Logging, best-practices §Logging errors, §Custom verbosity levels

- DO: 프로젝트가 정한 하나의 로깅 패키지를 쓴다. 포맷할 것이 없으면 포맷하지 않는 함수(`Info(v)`)를 쓴다.
- 권장: 로그 메시지는 좋은 테스트 실패 메시지처럼 무엇이 잘못됐는지와 진단에 필요한 정보를 담는다.
- 권장: 에러를 반환하는 함수는 그 에러를 직접 로그하지 않는다. 기록 여부·빈도 제한은 호출자가 정한다.
- 권장: 개인정보(PII)를 로그에 남기지 않는다. 많은 로그 저장소가 민감한 사용자 정보에 맞지 않는다.
- 권장: ERROR 레벨은 아껴 쓴다. ERROR 는 "더 심각함"이 아니라 "누군가 조치해야 함"을 뜻한다.
- 권장: 상세(verbose) 로그의 인자를 만드는 비용이 크면, 레벨이 켜졌는지 먼저 확인하고 계산한다. 꺼진 로그를 위해 비싼 함수를 부르지 않는다.

```go
// DO
if logger.Enabled(ctx, slog.LevelDebug) {
    logger.Debug("plan", "explain", q.Explain())
}

// DON'T: 레벨이 꺼져 있어도 Explain 이 실행된다
logger.Debug("plan", "explain", q.Explain())
```

- DO: 로그 후 종료하는 함수(`log.Fatal` 등)는 `os.Exit` 를 부르므로 deferred 함수가 실행되지 않는다. `main` 과 초기화 코드에서만 쓰고, 라이브러리는 에러를 반환한다.

## crypto/rand

출처: decisions §crypto/rand

- DON'T: 키·토큰·세션 ID 처럼 예측되면 안 되는 값을, 버려질 값이라도 `math/rand` 로 만들지 않는다. 시드가 없으면 완전히 예측되고, 시각으로 시드해도 엔트로피가 몇 비트뿐이다.
- DO: `crypto/rand` 를 쓰고, 텍스트가 필요하면 hex·base64 로 인코딩한다.

```go
// DO
func Key() (string, error) {
    buf := make([]byte, 16)
    if _, err := rand.Read(buf); err != nil { // crypto/rand
        return "", err
    }
    return hex.EncodeToString(buf), nil
}
```
