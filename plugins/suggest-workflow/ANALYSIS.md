# suggest-workflow Rust 로직 개선 분석

> 분석 대상: `plugins/suggest-workflow/cli/src/` (~2,661 LOC)
> 분석 일자: 2026-02-08

---

## 1. 성능 (Performance)

### P1. [HIGH] `workflow.rs` — 세션당 tool extraction 이중 호출

**위치**: `analyzers/workflow.rs:110-123`

`analyze_workflows()` 내에서 각 세션에 대해:
1. `extract_tool_sequences()` 호출 → 내부적으로 `extract_tool_sequence()` 호출
2. 바로 아래에서 다시 `extract_tool_sequence()` 직접 호출 (개별 tool usage 카운트용)

```rust
// 첫 번째 호출 (extract_tool_sequences 내부에서)
let sequences = extract_tool_sequences(entries, min_length, max_length);
// ...
// 두 번째 호출 (같은 데이터 다시 파싱)
let tool_uses = extract_tool_sequence(entries);
```

**영향**: 세션당 2배의 파싱 비용. 세션이 많을수록 선형적 성능 저하.

**개선안**: `extract_tool_sequences`가 원본 `ToolUse` 벡터도 함께 반환하도록 수정하거나, 한 번 추출한 결과를 재사용.

---

### P2. [HIGH] `tacit.rs:cluster_normalized` — char bigram 반복 계산

**위치**: `analyzers/tacit.rs:159-207`

Phase 2 클러스터링에서 `char_bigram_similarity()` 호출 시마다 양쪽 문자열의 bigram을 새로 계산. O(k²) 비교에서 k=500까지 가능.

```rust
for (cluster_repr, cluster_entries) in clusters.iter_mut() {
    let sim = char_bigram_similarity(&repr_text, cluster_repr);  // 매번 재계산
}
```

**영향**: 최악의 경우 500 × 500 = 250,000번의 bigram 재계산.

**개선안**: 각 representative의 bigram을 사전 계산(precompute)하여 `HashMap<String, HashSet<(char, char)>>`으로 캐싱.

---

### P3. [MEDIUM] `workflow.rs:find_common_sequences` — Vec에서 O(n) contains 체크

**위치**: `analyzers/workflow.rs:70`

```rust
if !entry.1.contains(session_id) {
    entry.1.push(session_id.clone());
}
```

세션 중복 체크에 `Vec::contains`를 사용 → O(n) 선형 탐색. 세션 수가 많을수록 성능 저하.

**개선안**: `Vec<String>` 대신 `HashSet<String>` 사용.

---

### P4. [MEDIUM] `bm25.rs:score_query` — 호출마다 HashMap 생성

**위치**: `analyzers/bm25.rs:67`

```rust
let mut tf: HashMap<String, usize> = HashMap::new();
for term in query_tokens {
    *tf.entry(term.clone()).or_insert(0) += 1;
}
```

`score_multi_query`에서 서브쿼리 수만큼 반복 호출되며, 매번 새로운 HashMap 할당.

**개선안**: 재사용 가능한 HashMap을 파라미터로 전달하거나, 작은 쿼리의 경우 Vec 기반 카운팅 사용.

---

### P5. [MEDIUM] `workflow.rs:all_session_ids` — 불필요한 String clone

**위치**: `analyzers/workflow.rs:115`

```rust
all_session_ids.push(session_id.clone());
```

시퀀스마다 `session_id`를 clone. 한 세션에서 수십~수백 개의 시퀀스가 나올 수 있으므로 같은 문자열을 반복 할당.

**개선안**: `Rc<String>` 또는 인덱스 기반으로 변경.

---

### P6. [LOW] `tokenize()` 함수 중복 정의

**위치**: `analyzers/tacit.rs:111-127` / `analyzers/query_decomposer.rs:38-54`

동일한 `tokenize()` 함수가 두 파일에 복사되어 있음. 코드 중복 자체는 성능에 영향 없지만, 최적화 시 두 곳을 모두 수정해야 하는 유지보수 부담.

**개선안**: 공용 모듈(예: `utils::tokenize`)로 추출.

---

### P7. [LOW] `analyze.rs` — `load_project_data` / `load_project_data_raw` 중복

**위치**: `commands/analyze.rs:209-255`

두 함수가 거의 동일한 로직을 수행. 하나는 `resolve_project_path` 결과를 받고, 하나는 raw 경로를 받는 차이만 있음.

**개선안**: 공통 로직을 내부 함수로 추출.

---

### P8. [LOW] `projects.rs:parse_session` — 불필요한 필드까지 전체 역직렬화

**위치**: `parsers/projects.rs:58-82`

각 JSONL 라인을 `SessionEntry`로 전체 역직렬화. assistant 메시지의 tool result 등 대용량 필드도 포함되지만, user prompt 추출 시에는 불필요한 데이터.

**개선안**: 필요한 필드만 선택적으로 파싱하는 경량 구조체 사용 (예: `#[serde(skip)]` 적용).

---

## 2. 로직 버그 (Logic Bugs)

### B1. [CRITICAL] `analyze.rs:decode_project_name` — 프로젝트 경로 디코딩 오류

**위치**: `commands/analyze.rs:257-265`

```rust
fn decode_project_name(encoded: &str) -> String {
    if encoded.starts_with('-') {
        format!("/{}", &encoded[1..].replace('-', "/"))
    } else {
        encoded.to_string()
    }
}
```

인코딩: `/` → `-`로 변환. 하지만 디코딩 시 **모든** `-`를 `/`로 역변환.

```
원본:   /home/user/my-project
인코딩: -home-user-my-project
디코딩: /home/user/my/project  ← 오류! 원래의 하이픈이 슬래시로 변환됨
```

이는 글로벌 분석 시 프로젝트 이름 표시에 영향을 미침.

**개선안**: Claude의 실제 인코딩 스킴을 확인하여 정확히 복원하거나, 디코딩이 불가능한 경우 인코딩된 이름을 그대로 사용.

---

### B2. [CRITICAL] `workflow.rs` — tool classifier에 input이 전달되지 않음

**위치**: `analyzers/workflow.rs:25` & `analyzers/workflow.rs:121`

```rust
let classified = classify_tool(&tool.name, None);  // input이 항상 None
```

`ToolUse` 구조체에는 `name`과 `timestamp`만 있고, tool input이 없음. `classify_tool`은 `Bash` 명령을 `git`/`test`/`build`/`lint`로 분류하기 위해 `input.command`를 확인하지만, **항상 `None`이 전달되므로 모든 Bash 도구가 `Bash:other`로 분류됨**.

```rust
// parsers/projects.rs의 ToolUse에는 input 필드가 없음
pub struct ToolUse {
    pub name: String,
    pub timestamp: Option<i64>,
    // input 필드 없음!
}
```

**영향**: Workflow Analysis의 Tool Sequence 결과에서 `Bash:git`, `Bash:test` 등 세부 분류가 전혀 작동하지 않음. 모든 Bash 호출이 `Bash:other`로 표시.

**개선안**: `ToolUse`에 `input: Option<serde_json::Value>` 필드 추가 및 `extract_tool_sequence`에서 input 데이터 전달.

---

### B3. [HIGH] `prompt.rs` — decay 가중치가 정렬에 미반영

**위치**: `analyzers/prompt.rs:64`

```rust
top_prompts.sort_by(|a, b| b.count.cmp(&a.count));
```

`--decay` 플래그를 활성화해도 `weighted_count` 필드만 계산될 뿐, 정렬은 항상 raw `count` 기준. 따라서 **temporal decay 기능이 실질적으로 정렬 결과에 영향을 주지 않음**.

**개선안**: decay 활성화 시 `weighted_count` 기준으로 정렬:
```rust
if decay {
    top_prompts.sort_by(|a, b| b.weighted_count.partial_cmp(&a.weighted_count)...);
} else {
    top_prompts.sort_by(|a, b| b.count.cmp(&a.count));
}
```

---

### B4. [HIGH] `tacit.rs` — examples 순서 비결정적 (non-deterministic)

**위치**: `analyzers/tacit.rs:396-401`

```rust
let examples: Vec<String> = cluster
    .iter()
    .map(|e| e.original.clone())
    .collect::<HashSet<_>>()
    .into_iter()       // HashSet 순서 = 비결정적
    .take(5)
    .collect();
```

`HashSet::into_iter()`는 순서를 보장하지 않으므로, 동일한 입력에 대해 실행할 때마다 다른 예시가 표시될 수 있음.

**개선안**: `BTreeSet` 사용하거나, `Vec`로 수집 후 정렬 + `dedup`.

---

### B5. [MEDIUM] `tacit.rs:calculate_confidence` — 분모에 전체 프롬프트 수 사용

**위치**: `analyzers/tacit.rs:249`

```rust
let frequency_score = (count as f64 / total_prompts as f64).min(1.0);
```

`count`는 **의미 있는(meaningful)** 프롬프트만의 클러스터 크기이지만, `total_prompts`는 **전체 entry 수** (`entries.len()`). non-meaningful 프롬프트까지 분모에 포함되어 frequency_score가 과소평가됨.

**개선안**: 분모를 `meaningful.len()`으로 변경.

---

### B6. [MEDIUM] `tacit.rs:contains_at_boundary` — 한글 구두점 미인식

**위치**: `analyzers/tacit.rs:74,77`

```rust
.ends_with(|c: char| c.is_whitespace() || c.is_ascii_punctuation());
```

`is_ascii_punctuation()`은 ASCII 범위의 구두점만 검사. 한글 구두점(`。`, `、`, `…` 등)이나 유니코드 구두점은 경계로 인식되지 않음.

**개선안**: `char::is_ascii_punctuation()` 대신 유니코드 범주 기반 검사 사용:
```rust
c.is_whitespace() || c.is_ascii_punctuation() || unicode_general_category(c) == Punctuation
```

---

### B7. [MEDIUM] `main.rs` — `current_dir().unwrap()` panic 가능

**위치**: `main.rs:62-66`

```rust
let project_path = cli.project.unwrap_or_else(|| {
    std::env::current_dir()
        .unwrap()  // 현재 디렉토리 접근 실패 시 panic
        .to_string_lossy()
        .to_string()
});
```

작업 디렉토리가 삭제되었거나 권한이 없는 경우 panic 발생.

**개선안**: `unwrap()` 대신 `context("...")?`로 에러 전파.

---

### B8. [LOW] `workflow.rs:find_common_sequences` — 시퀀스 키 충돌 가능성

**위치**: `analyzers/workflow.rs:67`

```rust
let key = seq.join("->");
```

만약 tool 이름에 `"->"` 문자열이 포함되어 있다면 키 충돌 발생 가능. 현실적으로 거의 없지만 robustness 관점에서 취약.

**개선안**: 구분자를 유니코드 특수 문자(예: `\x1F`)로 변경하거나 `Vec<String>` 자체를 키로 사용.

---

## 3. Lint / 타입 검증 도입 방안

### L1. Clippy 도입 (즉시 적용 가능)

`src/main.rs` 최상단에 lint 속성 추가:

```rust
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]  // 필요시
```

**발견 예상 이슈**:
- `&PathBuf` → `&Path` 파라미터 타입 (clippy::ptr_arg)
- `.clone()` 불필요한 사용 (clippy::redundant_clone)
- `for (i, x) in iter.enumerate()` 패턴 체크
- 미사용 필드 경고 (`BM25Ranker::doc_count`)

**Cargo.toml 혹은 `.cargo/config.toml` 에서 설정**:
```toml
# .cargo/config.toml (새로 생성)
[target.'cfg(all())']
rustflags = ["-W", "clippy::all"]
```

---

### L2. `clippy.toml` 프로젝트 전용 설정

```toml
# plugins/suggest-workflow/cli/clippy.toml
too-many-arguments-threshold = 10     # 현재 run() 함수가 8개 파라미터
cognitive-complexity-threshold = 30
```

---

### L3. String 기반 타입 → Rust enum 전환

현재 여러 판별자(discriminator)가 `String`으로 처리됨:

| 현재 | 개선안 |
|------|--------|
| `entry_type: String` ("user", "assistant", "tool_use") | `enum EntryType { User, Assistant, ToolUse }` |
| `item_type: String` ("text", "tool_use") | `enum ContentItemType { Text, ToolUse }` |
| `pattern_type: String` ("directive", "general", ...) | `enum PatternType { Directive, Convention, ... }` |
| `scope: String` / `format: String` (main.rs) | clap의 `ValueEnum` derive 활용 |

**clap ValueEnum 예시**:
```rust
#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}
```

장점: 컴파일 타임에 잘못된 값 방지, match exhaustiveness 보장.

---

### L4. `#[must_use]` 속성 추가

반환값을 무시하면 안 되는 함수들:

```rust
#[must_use]
pub fn analyze_workflows(...) -> WorkflowAnalysisResult { ... }

#[must_use]
pub fn analyze_prompts(...) -> PromptAnalysisResult { ... }

#[must_use]
pub fn analyze_tacit_knowledge(...) -> TacitAnalysisResult { ... }
```

---

### L5. `once_cell` → `std::sync::LazyLock` 마이그레이션

Rust 1.80+ 표준 라이브러리에 `LazyLock`이 포함됨. 외부 의존성 제거 가능:

```rust
// Before (once_cell)
use once_cell::sync::Lazy;
static STOPWORDS: Lazy<HashSet<&str>> = Lazy::new(|| { ... });

// After (std)
use std::sync::LazyLock;
static STOPWORDS: LazyLock<HashSet<&str>> = LazyLock::new(|| { ... });
```

---

### L6. CI 파이프라인 구성 제안

```yaml
# .github/workflows/rust-lint.yml
name: Rust Lint & Test
on: [push, pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: plugins/suggest-workflow/cli
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test
```

---

### L7. `cargo fmt` 포맷팅 통일

`rustfmt.toml` 추가:

```toml
# plugins/suggest-workflow/cli/rustfmt.toml
max_width = 120
use_field_init_shorthand = true
```

---

### L8. 에러 타입 개선 (선택적)

현재 `anyhow` 전면 사용. CLI 도구에는 적절하지만, 라이브러리 부분에는 `thiserror` 기반 커스텀 에러 도입 검토:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    #[error("no sessions found")]
    NoSessions,
    #[error("parse error at line {line}: {source}")]
    ParseError { line: usize, source: serde_json::Error },
}
```

---

## 개선 우선순위 요약

| 우선순위 | ID | 카테고리 | 설명 | 난이도 |
|---------|-----|---------|------|-------|
| 🔴 1 | B2 | Bug | tool classifier에 input 미전달 (Bash 분류 불능) | Medium |
| 🔴 2 | B1 | Bug | decode_project_name 하이픈-슬래시 혼동 | Low |
| 🔴 3 | B3 | Bug | decay 가중치 정렬 미반영 | Low |
| 🟠 4 | P1 | Perf | 세션당 tool extraction 이중 호출 | Low |
| 🟠 5 | P2 | Perf | cluster bigram 반복 계산 | Medium |
| 🟠 6 | L1 | Lint | Clippy 도입 | Low |
| 🟠 7 | L3 | Type | String → enum 전환 | Medium |
| 🟡 8 | B4 | Bug | examples 비결정적 순서 | Low |
| 🟡 9 | B5 | Bug | confidence 분모 불일치 | Low |
| 🟡 10 | P3 | Perf | Vec contains → HashSet | Low |
| 🟡 11 | L5 | Lint | once_cell → std LazyLock | Low |
| 🟡 12 | L6 | CI | Rust lint/test CI 구성 | Low |
