---
paths:
  - "**/internal/**/*.go"
---

# Go Internal 레이어 컨벤션

> `internal/`은 도메인별로 분리된 패키지 집합이다. 인터페이스 정의와 테스트 격리가 핵심이다.

## 원칙

1. **도메인별 패키지 분리**: `internal/spec`, `internal/parser`, `internal/bumper` 처럼 도메인별로 패키지를 분리한다
2. **struct + 메서드**: 관련 상태를 struct로 묶고 메서드로 동작을 정의한다 (예: `Bumper`, `Validator`)
3. **생성자 패턴**: `NewBumper(repoRoot string, dryRun bool) *Bumper` 형태로 생성자를 제공한다
4. **Result struct**: 복수의 반환값이 있으면 named struct로 묶어서 반환한다

## DO

도메인별 struct와 생성자 패턴을 사용하고, Result를 명시적 struct로 표현한다:

```go
package bumper

// BumpType은 버전 범프 종류를 나타냄
type BumpType string

const (
    BumpMajor BumpType = "major"
    BumpMinor BumpType = "minor"
    BumpPatch BumpType = "patch"
)

// BumpResult는 버전 범프 결과
type BumpResult struct {
    Plugin     string `json:"plugin"`
    OldVersion string `json:"old_version"`
    NewVersion string `json:"new_version"`
}

// Bumper는 버전 범프 오퍼레이션을 처리
type Bumper struct {
    RepoRoot        string
    MarketplacePath string
    DryRun          bool
}

// NewBumper는 Bumper 인스턴스를 생성
func NewBumper(repoRoot string, dryRun bool) *Bumper {
    return &Bumper{
        RepoRoot: repoRoot,
        DryRun:   dryRun,
    }
}

// BumpPlugins는 주어진 플러그인들의 버전을 범프
func (b *Bumper) BumpPlugins(plugins []Package, bumpType BumpType) ([]BumpResult, error) {
    // ...
}
```

## DON'T

패키지 전역 함수로만 구성하거나, 여러 도메인을 하나의 패키지에 혼재하지 않는다:

```go
// 전역 함수만 있고 struct 없음 — DON'T (테스트 격리 어려움)
func BumpPlugins(repoRoot string, plugins []Package) ([]BumpResult, error) { ... }

// 여러 도메인 혼재 — DON'T
// internal/utils/utils.go 에 bumper, parser, validator 로직 혼재

// 에러 무시 — DON'T
content, _ := os.ReadFile(filePath) // 에러를 반드시 처리
```

## 체크리스트

- [ ] 패키지 이름이 도메인을 명확히 나타내는가? (`spec`, `bumper`, `parser` 등)
- [ ] 관련 상태를 struct로 묶고 생성자(`New*`)를 제공했는가?
- [ ] 복수의 반환 데이터를 named struct로 표현했는가?
- [ ] 에러를 전파하지 않고 무시하는 코드가 없는가?
- [ ] `internal/` 패키지를 외부 모듈에서 import하지 않는가?
