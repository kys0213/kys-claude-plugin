# autodev 플러그인: 코드 구조를 디자인 문서에 맞게 리팩토링

## 목표

현재 코드 구조를 DESIGN.md에 명시된 디렉토리/모듈 구조로 변경한다.
**블랙박스 테스트 기반**: 기존 통합 테스트(5개 파일, ~60개 테스트)가 리팩토링 전후로 모두 통과해야 한다.

---

## 현재 구조 vs 디자인 구조: 차이점

| # | 현재 (Code) | 디자인 (DESIGN.md) | 변경 유형 |
|---|-------------|-------------------|----------|
| 1 | `consumer/` | `processor/` | 디렉토리 rename |
| 2 | `consumer/github.rs` | `github/mod.rs` | 파일 이동 → 독립 모듈 |
| 3 | `lib.rs`에서 `pub mod consumer` | `pub mod processor` | 모듈명 변경 |
| 4 | `scanner/issues.rs`, `scanner/pulls.rs` | `scanner/mod.rs` (단일) | 디자인은 단일이지만, 코드의 분리가 더 좋으므로 **디자인 업데이트** |
| 5 | `active.rs` (독립) | 디자인에 없음 | 유지 (런타임 필수) |
| 6 | `client/mod.rs` (CLI handlers) | 디자인에 없음 | 유지 (CLI 구현 디테일) |
| 7 | `config/loader.rs` | 디자인에 없음 | 유지 (config 구현 디테일) |
| 8 | `queue/models.rs` | 디자인에 없음 | 유지 (데이터 모델) |

---

## 리팩토링 범위

### 코드 변경 (3건)

1. **`consumer/` → `processor/` 디렉토리 rename**
   - `consumer/mod.rs` → `processor/mod.rs`
   - `consumer/issue.rs` → `processor/issue.rs`
   - `consumer/pr.rs` → `processor/pr.rs`
   - `consumer/merge.rs` → `processor/merge.rs`

2. **`consumer/github.rs` → `github/mod.rs` 독립 모듈화**
   - `consumer/github.rs` → `github/mod.rs`
   - processor 내부에서 `crate::github::*`로 import 변경

3. **모듈 선언 업데이트**
   - `lib.rs`: `pub mod consumer` → `pub mod processor` + `pub mod github` 추가
   - `main.rs`: `mod consumer` → `mod processor` + `mod github` 추가
   - 모든 내부 참조 (`consumer::` → `processor::`) 업데이트

### 디자인 문서 업데이트 (2건)

4. **DESIGN.md 디렉토리 구조 업데이트**
   - `scanner/issues.rs`, `scanner/pulls.rs` 반영 (현재 코드가 더 좋은 분리)
   - `queue/models.rs` 반영
   - `active.rs`, `client/mod.rs`, `config/loader.rs` 반영
   - `lib.rs` 반영

5. **README.md 동기화** (DESIGN.md 변경 반영)

---

## 실행 계획

### Phase 0: 블랙박스 테스트 베이스라인 확립

기존 테스트가 모두 통과하는지 확인한다.

```bash
cd plugins/autodev/cli && cargo test 2>&1
```

**통과 조건**: 모든 5개 테스트 파일의 ~60개 테스트가 pass

---

### Phase 1: `consumer/` → `processor/` + `github/` 독립 모듈

#### Step 1-1: 파일 이동

```
consumer/mod.rs    → processor/mod.rs
consumer/issue.rs  → processor/issue.rs
consumer/pr.rs     → processor/pr.rs
consumer/merge.rs  → processor/merge.rs
consumer/github.rs → github/mod.rs
```

#### Step 1-2: 모듈 선언 수정

**`src/lib.rs`**:
```rust
pub mod active;
pub mod config;
pub mod github;       // NEW (consumer/github.rs에서 이동)
pub mod processor;    // RENAMED from consumer
pub mod queue;
pub mod scanner;
pub mod session;
pub mod workspace;
```

**`src/main.rs`**:
```rust
mod active;
mod client;
mod config;
mod daemon;
mod github;           // NEW
mod processor;        // RENAMED from consumer
mod queue;
mod scanner;
mod session;
mod tui;
mod workspace;
```

#### Step 1-3: 내부 참조 수정

- `processor/mod.rs` 내 `mod github` 제거 → `use crate::github`
- `processor/issue.rs`, `pr.rs`, `merge.rs` 내 `super::github::` → `crate::github::`
- `daemon/mod.rs` 내 `consumer::` → `processor::` 호출
- `main.rs` 내 `consumer::` → `processor::`

#### Step 1-4: 테스트 파일 참조 수정

- `tests/daemon_consumer_tests.rs`:
  - `autodev::consumer::issue::` → `autodev::processor::issue::`
  - `autodev::consumer::pr::` → `autodev::processor::pr::`
  - `autodev::consumer::process_all` → `autodev::processor::process_all`

---

### Phase 2: 블랙박스 테스트 검증

```bash
cd plugins/autodev/cli && cargo test 2>&1
```

**통과 조건**: Phase 0과 동일한 테스트가 모두 pass

---

### Phase 3: DESIGN.md 업데이트

DESIGN.md의 디렉토리 구조 섹션을 실제 코드와 일치하도록 업데이트:

```
plugins/autodev/
├── cli/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── lib.rs              # 라이브러리 export
│       ├── active.rs           # ActiveItems (in-memory dedup)
│       ├── client/
│       │   └── mod.rs          # CLI 서브커맨드 핸들러
│       ├── daemon/
│       │   ├── mod.rs
│       │   └── pid.rs
│       ├── scanner/
│       │   ├── mod.rs          # scan_all 오케스트레이터
│       │   ├── issues.rs       # 이슈 스캐너
│       │   └── pulls.rs        # PR 스캐너
│       ├── processor/          # (구 consumer/)
│       │   ├── mod.rs          # process_all 오케스트레이터
│       │   ├── issue.rs        # Issue 처리 (분석 → 구현)
│       │   ├── pr.rs           # PR 처리 (리뷰 → 개선)
│       │   └── merge.rs        # Merge 처리
│       ├── github/             # GitHub API 헬퍼
│       │   └── mod.rs
│       ├── queue/
│       │   ├── mod.rs          # Database wrapper
│       │   ├── models.rs       # 데이터 모델
│       │   ├── schema.rs       # SQLite 스키마
│       │   └── repository.rs   # 데이터 접근 레이어
│       ├── session/
│       │   ├── mod.rs
│       │   └── output.rs
│       ├── workspace/
│       │   └── mod.rs
│       ├── config/
│       │   ├── mod.rs
│       │   ├── models.rs
│       │   └── loader.rs       # YAML 로딩/머지
│       └── tui/
│           ├── mod.rs
│           ├── views.rs
│           └── events.rs
```

---

### Phase 4: 최종 검증

1. `cargo test` — 전체 테스트 통과 확인
2. `cargo build --release` — 릴리스 빌드 성공 확인
3. 커밋 및 푸시

---

## 리스크 & 사이드이펙트

| 리스크 | 대응 |
|--------|------|
| 테스트에서 `autodev::consumer::` 경로 하드코딩 | 테스트 파일 내 import 경로 일괄 변경 |
| daemon 모듈 내 consumer 참조 | `consumer::` → `processor::` 일괄 변경 |
| 외부에서 lib crate 사용하는 곳 | lib.rs의 re-export로 처리 (현재 외부 사용처 없음) |
| `consumer/github.rs`가 processor 내부에서만 사용 | 독립 모듈로 분리해도 가시성 문제 없음 (pub fn) |

## 변경하지 않는 것

- `active.rs`: 디자인에 없지만 런타임 필수 → 유지
- `client/mod.rs`: CLI 구현 디테일 → 유지
- `config/loader.rs`: config 내부 구현 → 유지
- `queue/models.rs`: 데이터 모델 분리 → 유지
- `scanner/issues.rs`, `scanner/pulls.rs`: 코드 분리가 더 좋음 → 유지 (디자인 업데이트)
