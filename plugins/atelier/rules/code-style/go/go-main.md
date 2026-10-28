---
name: go-main
description: Go main 패키지와 cmd 진입점 규칙 — 플래그 정의 위치·이름, 초기화 오류 전파와 종료, 종료 함수의 범위, 서브커맨드 CLI 구성
paths:
  - "**/main.go"
  - "**/cmd/**/*.go"
---

# Go 프로그램 진입점

## 플래그

출처: decisions §Flags

- DO: 플래그는 `main` 패키지(또는 그에 준하는 진입점)에서만 정의한다.
- DON'T: 범용 패키지가 import 부수효과로 플래그를 노출하지 않는다. 라이브러리는 함수 인자나 구조체 필드 같은 Go API 로 설정받는다.
- DO: 플래그를 전역 변수로 두면 import 바로 뒤 자기만의 `var` 그룹에 모은다.
- DO: 플래그 이름은 snake_case, 값을 담는 변수는 같은 뜻의 mixedCaps 로 짓는다.

```go
// DO
var (
    pollInterval = flag.Duration("poll_interval", time.Minute, "Interval to use for polling.")
)

// DON'T
var (
    poll_interval = flag.Int("pollIntervalSeconds", 60, "...")
)
```

## 초기화 오류와 종료

출처: best-practices §Program initialization, §Program checks and panics, decisions §Don't panic

- 권장: 잘못된 플래그·설정 같은 초기화 오류는 `main` 까지 에러로 올린다. `main` 은 어떻게 고치면 되는지 설명하는 메시지를 출력하고 0 이 아닌 코드로 종료한다. 이런 오류에는 스택 트레이스가 도움이 되지 않는다.
- DO: 프로그램을 끝내는 호출(`log.Fatal`, `os.Exit`)은 `main` 과 초기화 코드에만 둔다. 라이브러리는 에러를 반환한다.
- 권장: `os.Exit` 계열은 deferred 함수를 실행하지 않는다. 정리할 자원이 있으면 본문을 `error` 를 반환하는 함수로 빼고, `main` 은 그 결과로 종료만 한다.

```go
// DO
func main() {
    if err := run(context.Background(), os.Args[1:]); err != nil {
        fmt.Fprintf(os.Stderr, "mytool: %v\n", err)
        os.Exit(1)
    }
}

func run(ctx context.Context, args []string) error {
    cfg, err := parseConfig(args)
    if err != nil {
        return fmt.Errorf("invalid config (see --help): %w", err)
    }
    db, err := open(ctx, cfg)
    if err != nil {
        return err
    }
    defer db.Close() // run 이 반환할 때 실행된다
    ...
}

// DON'T: defer 가 실행되지 않는다
func main() {
    db := mustOpen()
    defer db.Close()
    if err := work(db); err != nil {
        log.Fatal(err)
    }
}
```

- 진입점(`main`, `init`)의 루트 context 는 `context.Background()` 로 만든다. side-effect import(`import _`)는 진입점에 모은다. `Must` 함수는 초기화 시점에만 쓴다.
- 권장: 내부 상태가 복구 불가능하게 깨진 경우에만 프로그램을 끝내고, 이때도 `panic` 보다 로그 후 종료를 쓴다.

## 서브커맨드 CLI

출처: best-practices §Complex command-line interfaces

- 권장: 서브커맨드 CLI 라이브러리는 특별한 요구가 없으면 단순한 쪽(`subcommands`)을 고르고, 필요한 기능이 있을 때 `cobra` 를 쓴다.
- 권장: `cobra` 명령 함수는 `context.Background()` 로 새 루트를 만들지 말고 `cmd.Context()` 를 쓴다.
- 권장: 서브커맨드마다 패키지를 나눌 필요는 없다. 패키지 경계는 다른 코드와 같은 기준으로 정한다.
- 권장: 라이브러리로도 쓰일 코드는 CLI 와 분리해, CLI 를 그 라이브러리의 한 클라이언트로 만든다.
