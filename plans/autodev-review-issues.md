# Autodev Review Issues 개선 계획

> **Date**: 2026-02-24
> **Scope**: Issue #91 (output 파싱 개선), Issue #92 (exponential backoff)
> **Status**: 설계 검토 중

---

## 1. 요구사항 정리

### 변경 1: SessionOptions 구조체 도입 + json_schema 적용 (Issue #91)

`Claude` trait의 `run_session` 시그니처를 개선한다.

**현재**: `output_format: Option<&str>` 단일 파라미터
**목표**: `SessionOptions` 구조체로 묶고, `json_schema` 필드 추가 + 실제 적용

```rust
// Before
async fn run_session(
    &self, cwd: &Path, prompt: &str, output_format: Option<&str>,
) -> Result<SessionResult>;

// After
async fn run_session(
    &self, cwd: &Path, prompt: &str, opts: &SessionOptions<'_>,
) -> Result<SessionResult>;
```

**json_schema 적용 대상** (JSON 출력을 파싱하는 2곳):
- `analyzer.rs` → `AnalysisResult` 스키마
- `reviewer.rs` → `ReviewResult` 스키마

### 변경 2: Exponential backoff (Issue #92)

Claude CLI 호출 실패 시 재시도 로직 추가. (별도 설계 — 이 문서에서는 변경 1에 집중)

---

## 2. 사이드이펙트 조사

### SessionOptions 도입

| 영향 대상 | 사이드이펙트 | 대응 |
|-----------|-------------|------|
| `Claude` trait (`mod.rs:22-30`) | 시그니처 변경 → 모든 구현체 깨짐 | `RealClaude`, `MockClaude` 동시 수정 |
| `RealClaude` (`real.rs:13-53`) | `output_format` 파라미터 → `opts.output_format` | args 조립 로직에 `--json-schema` 추가 |
| `MockClaude` (`mock.rs:50-73`) | `calls` 타입 `(String, String, Option<String>)` → 변경 필요 | `SessionOptions` 정보를 기록하는 구조로 변경 |
| `analyzer.rs:34` | `Some("json")` → `SessionOptions` | 스키마 포함하여 전달 |
| `reviewer.rs:41` | `Some("json")` → `SessionOptions` | 스키마 포함하여 전달 |
| `pipeline/issue.rs:318` | `None` → `SessionOptions::default()` | 동작 변경 없음 |
| `pipeline/pr.rs:290` | `None` → `SessionOptions::default()` | 동작 변경 없음 |
| `merger.rs:48,90` | `None` → `SessionOptions::default()` | 동작 변경 없음 |
| `knowledge/extractor.rs:45` | `None` → `SessionOptions::default()` | 동작 변경 없음 |
| `knowledge/daily.rs:238` | `None` → `SessionOptions::default()` | 동작 변경 없음 |

### json_schema 적용

| 영향 대상 | 사이드이펙트 | 대응 |
|-----------|-------------|------|
| `AnalysisResult` 구조체 | `schemars::JsonSchema` derive 추가 필요 | `Verdict` enum에도 derive 필요 |
| `ReviewResult` 구조체 | `schemars::JsonSchema` derive 추가 필요 | `ReviewVerdict`, `ReviewComment`에도 derive 필요 |
| Cargo.toml | `schemars` 의존성 추가 | dependencies 섹션에 추가 |
| 파싱 fallback | `--json-schema` 사용 시 Claude가 스키마 준수 출력 → envelope 구조 변경 가능 | 기존 2단계 파싱(envelope → direct) 유지하여 안전 |

---

## 3. 구현 설계

### 3-1. SessionOptions 구조체

```rust
// infrastructure/claude/mod.rs

/// Claude CLI 세션 옵션
#[derive(Debug, Default)]
pub struct SessionOptions<'a> {
    /// --output-format 값 (e.g. "json", "stream-json")
    pub output_format: Option<&'a str>,
    /// --json-schema 값 (JSON schema 문자열)
    pub json_schema: Option<&'a str>,
}
```

### 3-2. Claude trait 시그니처

```rust
#[async_trait]
pub trait Claude: Send + Sync {
    async fn run_session(
        &self,
        cwd: &Path,
        prompt: &str,
        opts: &SessionOptions<'_>,
    ) -> Result<SessionResult>;
}
```

### 3-3. RealClaude — args 조립

```rust
// real.rs
async fn run_session(
    &self, cwd: &Path, prompt: &str, opts: &SessionOptions<'_>,
) -> Result<SessionResult> {
    let mut args = vec!["-p".to_string(), prompt.to_string()];

    if let Some(fmt) = opts.output_format {
        args.push("--output-format".to_string());
        args.push(fmt.to_string());
    }

    if let Some(schema) = opts.json_schema {
        args.push("--json-schema".to_string());
        args.push(schema.to_string());
    }

    // ... 나머지 동일
}
```

### 3-4. MockClaude — 호출 기록

```rust
// mock.rs

/// 호출 기록용 구조체
#[derive(Debug)]
pub struct MockCallRecord {
    pub cwd: String,
    pub prompt: String,
    pub output_format: Option<String>,
    pub json_schema: Option<String>,
}

pub struct MockClaude {
    responses: Mutex<Vec<SessionResult>>,
    pub calls: Mutex<Vec<MockCallRecord>>,
}

async fn run_session(
    &self, cwd: &Path, prompt: &str, opts: &SessionOptions<'_>,
) -> Result<SessionResult> {
    self.calls.lock().unwrap().push(MockCallRecord {
        cwd: cwd.display().to_string(),
        prompt: prompt.to_string(),
        output_format: opts.output_format.map(String::from),
        json_schema: opts.json_schema.map(String::from),
    });
    // ... 응답 반환 로직 동일
}
```

### 3-5. schemars 의존성 + JsonSchema derive

```toml
# Cargo.toml
[dependencies]
schemars = "0.8"
```

```rust
// output.rs — 기존 구조체에 JsonSchema derive 추가
use schemars::JsonSchema;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict { ... }

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AnalysisResult { ... }

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict { ... }

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReviewResult { ... }

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReviewComment { ... }
```

### 3-6. 스키마 생성 함수

```rust
// output.rs — 스키마 문자열 생성

use std::sync::LazyLock;

/// AnalysisResult JSON schema (LazyLock으로 한 번만 생성)
pub static ANALYSIS_SCHEMA: LazyLock<String> = LazyLock::new(|| {
    serde_json::to_string(&schemars::schema_for!(AnalysisResult)).unwrap()
});

/// ReviewResult JSON schema
pub static REVIEW_SCHEMA: LazyLock<String> = LazyLock::new(|| {
    serde_json::to_string(&schemars::schema_for!(ReviewResult)).unwrap()
});
```

### 3-7. 호출부 변경

```rust
// analyzer.rs
use crate::infrastructure::claude::output::ANALYSIS_SCHEMA;
use crate::infrastructure::claude::SessionOptions;

let result = self.claude.run_session(wt_path, prompt, &SessionOptions {
    output_format: Some("json"),
    json_schema: Some(&ANALYSIS_SCHEMA),
    ..Default::default()
}).await?;
```

```rust
// reviewer.rs
use crate::infrastructure::claude::output::REVIEW_SCHEMA;
use crate::infrastructure::claude::SessionOptions;

let result = self.claude.run_session(wt_path, prompt, &SessionOptions {
    output_format: Some("json"),
    json_schema: Some(&REVIEW_SCHEMA),
    ..Default::default()
}).await?;
```

```rust
// pipeline/issue.rs, pipeline/pr.rs, merger.rs, knowledge/extractor.rs, knowledge/daily.rs
// 기존 None → SessionOptions::default()
let result = claude.run_session(&wt_path, &prompt, &SessionOptions::default()).await;
```

---

## 4. 구현 순서

```
Step 1: Cargo.toml에 schemars 추가
Step 2: output.rs — JsonSchema derive + ANALYSIS_SCHEMA / REVIEW_SCHEMA 상수
Step 3: mod.rs — SessionOptions 구조체 + Claude trait 시그니처 변경
Step 4: real.rs — --json-schema args 조립
Step 5: mock.rs — MockCallRecord + 시그니처 맞춤
Step 6: analyzer.rs, reviewer.rs — SessionOptions + json_schema 적용
Step 7: 나머지 호출부 6곳 — SessionOptions::default()
```

## 5. 테스트 계획

| 대상 | 테스트 |
|------|--------|
| `ANALYSIS_SCHEMA` | 생성된 스키마가 valid JSON이고 required 필드 포함 확인 |
| `REVIEW_SCHEMA` | 생성된 스키마가 valid JSON이고 required 필드 포함 확인 |
| `MockClaude.calls` | `json_schema` 전달 여부가 `MockCallRecord`에 기록되는지 확인 |
| `RealClaude` (통합) | `--json-schema` args가 올바르게 조립되는지 확인 (unit test로 args 빌드 로직 분리 가능) |
| 기존 테스트 | `output::tests` — 시그니처 변경 영향 없음 (파싱 로직 불변) |
