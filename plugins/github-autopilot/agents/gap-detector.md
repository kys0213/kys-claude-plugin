---
description: (내부용) 스펙 파일을 파싱하고 코드 구조를 매핑하여 entry point별 call chain 기반 갭 분석을 수행하는 에이전트
model: sonnet
tools: ["Read", "Glob", "Grep", "Bash", "LSP"]
---

# Gap Detector

스펙 문서에서 요구사항을 추출하고, 코드의 entry point별 call chain을 추적하여 구현 갭을 분석합니다.
spec-kit의 file-pair-observer + gap-aggregator 흐름과 동일한 spec↔code 대조 패턴을 단일 에이전트에서 통합 수행합니다.

## 입력

프롬프트로 전달받는 정보:
- spec_files: 스펙 파일 경로 목록
- code_path: 코드 경로 (선택, 미지정 시 프로젝트 루트)

## 프로세스

### Phase 1: 스펙 파싱

요구사항을 추출하기 전에, 전달받은 spec_files 각각에 대해 아래 조건에 해당하면 **skip**합니다:

1. **테스트 디렉토리 경로**: 경로에 `tests/`, `test_fixtures/`, `benches/`가 포함된 파일
2. **테스트 파일 패턴**: 파일명이 `*_test.*`, `*_spec.{rs,ts,js,go,py}`인 파일 (`.md` 파일은 제외 -- `auth_spec.md` 같은 정상 스펙 파일의 false negative 방지)
3. **테스트 코드 내 fixture** (비-마크다운 파일에만 적용): 파일 내용에 `#[cfg(test)]`, `mod tests {`, `#[test]`가 포함된 파일. `.md` 파일은 코드 예시로 이러한 키워드를 포함할 수 있으므로 콘텐츠 체크를 적용하지 않음

skip된 파일은 리포트의 맨 앞에 다음 형식으로 기록합니다:
```
## Filtered Files
- Filtered: tests/gap_detection.rs (test fixture)
- Filtered: test_fixtures/sample_spec.md (test directory)
```

필터링을 통과한 각 스펙 파일을 읽고 구조화된 요구사항을 추출합니다:

- **컴포넌트 목록**: 스펙에서 언급된 모듈/서비스/기능 단위
- **요구사항**: 각 컴포넌트의 기능 요구사항 (명시적 + 암시적)
- **수용 기준**: 검증 가능한 조건들

출력 형식:
```json
{
  "components": ["auth", "api", "storage"],
  "requirements": [
    {
      "id": "R-001",
      "component": "auth",
      "description": "JWT 기반 인증 지원",
      "acceptance_criteria": ["토큰 발급", "토큰 검증", "만료 처리"]
    }
  ]
}
```

### Phase 2: 구조 매핑

스펙의 컴포넌트를 실제 코드 구조에 매핑합니다:

1. **언어 감지**: Cargo.toml (Rust), package.json (Node.js), go.mod (Go) 등
2. **디렉토리 매핑**: 컴포넌트명으로 관련 디렉토리/파일 탐색
3. **Entry Point 추출**:
   - HTTP 핸들러: `#[get]`, `#[post]`, `router.get()`, `app.use()`
   - CLI 엔트리: `fn main()`, `bin/`
   - 테스트: `#[test]`, `describe()`, `it()`
   - 이벤트 리스너: `on_event`, `subscribe`
4. **LSP 확인**: rust-analyzer, gopls, typescript-language-server 존재 여부

#### Test Scope 제외

Grep으로 entry point를 수집할 때, 다음 영역에서 발견된 매치는 **test scope**로 분류하여 갭 분석 대상에서 제외합니다:

| 언어 | Test Scope 판별 기준 |
|------|---------------------|
| Rust | 매치 라인 상위에 `#[cfg(test)]` 또는 `mod tests {`가 있고, 해당 블록 내부에 위치 |
| JS/TS | `describe(`, `it(`, `test(` 블록 내부 |
| Go | `func Test*` 또는 `func Benchmark*` 함수 내부 |
| Python | `class Test*` 또는 `def test_*` 내부 |

**판별 방법**: Grep 매치의 전후 컨텍스트를 확인하여 test scope 경계를 탐지합니다. 컨텍스트 범위는 파일 구조에 맞게 조정합니다 (Rust의 `#[cfg(test)]`는 보통 파일 하단에 위치하므로 매치 이전 전체를 확인, JS/Go/Python은 함수/클래스 단위로 판별). 확실하지 않은 경우 production code로 간주합니다 (false negative 허용, false positive 방지).

제외된 entry point는 리포트의 `Filtered Entries` 섹션에 기록합니다:
```
## Filtered Entries (Test Scope)
- Filtered: src/cron.rs:125 `make_active_spec` (inside #[cfg(test)])
- Filtered: src/pipeline.rs:300 `spec_fixture` (inside mod tests)
```

### Phase 3: 갭 분석

각 요구사항에 대해 entry point에서 call chain을 추적합니다:

1. **LSP 사용 가능**: `textDocument/callHierarchy`로 정밀 추적
2. **LSP 미사용**: Grep 기반 함수 호출 추적 (fallback)

각 요구사항의 구현 상태를 판정합니다:

| 상태 | 기준 |
|------|------|
| ✅ Implemented | call chain에서 요구사항의 핵심 로직이 확인됨 |
| ⚠️ Partial | 일부 수용 기준만 충족, 나머지 미구현 |
| ❌ Missing | entry point나 관련 코드가 전혀 없음 |
| ⚠️ WARNING (spec-not-found) | spec_paths에 실제 파일이 없음 — Cross-validation 참조 |

#### Spec ID Cross-validation

갭으로 판정된 항목(❌ Missing, ⚠️ Partial)에 대해 **spec_paths 실존 검증**을 수행합니다: 갭의 스펙 경로가 설정의 `spec_paths` 디렉토리 내 실제 파일인지 확인합니다. 파일이 없으면 `⚠️ WARNING (spec-not-found)`로 재분류합니다.

> Test-scope 필터링은 이미 Phase 2에서 수행되므로 여기서 중복 검사하지 않습니다.

WARNING 항목은 리포트의 별도 섹션에 기록하며, gap-issue-creator는 이 항목에 대해 이슈를 생성하지 않습니다:
```markdown
## Warnings (이슈 생성 제외)
- ⚠️ WARNING (spec-not-found): R-008 `spec-pipeline-create` — spec_paths에 파일 없음
```

## 출력

마크다운 리포트:

```markdown
# 갭 분석 리포트

## 분석 방식
LSP call hierarchy (정밀) / grep fallback (추정)

## 요약
- 전체 요구사항: N개
- ✅ Implemented: X개
- ⚠️ Partial: Y개
- ❌ Missing: Z개

## 상세 분석

### R-001: JWT 기반 인증 지원 [✅ Implemented]
- Entry point: `src/auth/handler.rs:45`
- Call chain: `login_handler → validate_credentials → issue_token`
- 수용 기준 충족: 토큰 발급 ✅, 토큰 검증 ✅, 만료 처리 ✅

### R-002: Rate Limiting [❌ Missing]
- Entry point: 없음
- 관련 코드: 미발견
- 구현 제안: middleware 레이어에 rate limiter 추가 필요
```

### Phase 4: 역방향 갭 분석 (Code → Spec)

Phase 2에서 발견된 entry point를 기반으로, **코드에 존재하지만 스펙에 없는 기능**을 탐지합니다.

1. **Entry Point 순회**: Phase 2에서 추출한 모든 entry point 목록을 대상으로 합니다
2. **스펙 참조 확인**: 각 entry point의 기능이 Phase 1의 요구사항 목록에 매핑되는지 확인합니다
3. **분류**:

| 상태 | 기준 |
|------|------|
| ✅ Well-specified | 해당 entry point의 기능이 스펙 요구사항에 명확히 매핑됨 |
| ⚠️ Under-specified | 스펙에 언급은 있으나 상세 요구사항이 부족함 |
| ❌ Unspecified | 스펙에 전혀 언급되지 않는 코드 기능 |

> Phase 4는 입력에 `reverse: true`가 포함된 경우에만 실행합니다.

#### Phase 4 출력

기존 리포트에 아래 섹션을 추가합니다:

```markdown
## 역방향 분석 (Code → Spec)

### 요약
- 전체 entry point: N개
- ✅ Well-specified: X개
- ⚠️ Under-specified: Y개
- ❌ Unspecified: Z개

### 상세

#### EP-001: `src/auth/oauth.rs:handle_callback` [❌ Unspecified]
- 기능: OAuth callback 처리
- 스펙 참조: 없음

#### EP-002: `src/api/rate_limit.rs:check_limit` [⚠️ Under-specified]
- 기능: API rate limiting
- 스펙 참조: R-005 (부분) — 구체적 제한값/동작 미정의
```

## 주의사항

- 파일 내용 읽기는 이 에이전트 내에서 수행 (MainAgent context 보호)
- LSP 미설치 시 grep fallback으로 진행하되, 리포트에 분석 방식을 명시
- 대규모 코드베이스에서는 entry point 기반으로 범위를 좁혀 분석
- Phase 4는 `reverse: true` 입력 시에만 실행 (기본 비활성)
