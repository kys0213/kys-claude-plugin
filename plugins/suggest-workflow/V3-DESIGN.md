# suggest-workflow v3 설계: SQLite 인덱스 기반 아키텍처

> 설계일: 2026-02-13
> 대상: `plugins/suggest-workflow/cli/` (Rust)
> 현재 버전: v2.0.0 → 목표: v3.0.0

---

## 1. 동기: v2의 한계

### 1-1. 매번 처음부터 재계산

```
v2 흐름:
  JSONL 전체 파싱 → 17개 analyzer 전부 실행 → JSON 캐시 덤프
  (세션 50개, 평균 500줄 = ~25,000줄 매번 파싱)
```

세션이 1개 추가되어도 전체를 다시 계산한다. 인크리멘털 업데이트가 불가능.

### 1-2. 캐시가 조회 불가능한 덩어리

`analysis-snapshot.json`은 분석 결과의 **완성된 스냅샷**. Phase 2 에이전트가 "Bash 관련 전이만 보여줘"라고 하면 JSON 전체를 읽어서 Claude가 직접 필터링해야 한다.

### 1-3. 파이프 체이닝 불가

v2 출력은 분석의 최종 결과물. 중간 데이터에 대해 다른 관점(perspective)으로 재질의하거나, 두 분석을 조합하는 것이 구조적으로 불가능.

### 1-4. 알려진 버그들 (ANALYSIS.md)

- **B2**: Bash 도구 분류 미작동 (input 미전달)
- **B1**: 프로젝트 경로 디코딩 오류
- **B3**: decay 가중치 정렬 미반영
- **P1-P2**: 이중 파싱, bigram 반복 계산

---

## 2. v3 핵심 아이디어

```
v2: JSONL → [매번 전체 분석] → JSON 캐시 (read-only blob)
v3: JSONL → [인크리멘털 인덱싱] → SQLite DB → [유연한 SQL 쿼리]
```

**SQLite를 "분석 가능한 인덱스"로 사용.**

- 세션 데이터를 구조화하여 DB에 적재
- 분석은 SQL 쿼리로 표현 (analyzer 로직의 상당 부분이 SQL로 이동)
- Phase 2 에이전트가 직접 `query` 서브커맨드로 원하는 관점의 데이터를 요청

---

## 3. 아키텍처

### 3-1. 전체 구조

```
┌─────────────────────────────────────────────────────────────┐
│  Phase 1: Rust CLI                                          │
│                                                             │
│  ┌──────────┐    ┌──────────────┐    ┌──────────────────┐  │
│  │  index    │───→│  SQLite DB   │←───│  query           │  │
│  │ (write)   │    │  (index.db)  │    │ (read)           │  │
│  └──────────┘    └──────────────┘    └──────────────────┘  │
│       ↑                                      │              │
│       │                                      ↓              │
│   JSONL files                          JSON (stdout)        │
│   (~/.claude/projects/...)                   │              │
└──────────────────────────────────────────────│──────────────┘
                                               │
                                               ↓
┌──────────────────────────────────────────────────────────────┐
│  Phase 2: Claude Agent                                       │
│                                                              │
│  workflow-insight agent                                       │
│  - query 서브커맨드를 조합하여 원하는 관점의 데이터 획득       │
│  - 시맨틱 해석, 분류, 인사이트 생성                            │
└──────────────────────────────────────────────────────────────┘
```

### 3-2. CLI 서브커맨드 구조

```bash
suggest-workflow <subcommand> [options]

# 인덱스 관리
suggest-workflow index [--project <path>] [--full]
  # 인크리멘털 인덱싱 (기본). --full로 전체 재구축
  # 새로/변경된 세션만 파싱하여 DB에 upsert

# 쿼리
suggest-workflow query [--project <path>] [--perspective <name>] [options]
  # 사전 정의된 perspective 또는 커스텀 필터로 조회
  # 결과는 항상 JSON (stdout) → 파이프 체이닝 가능

# 기존 호환 (v2 → v3 마이그레이션 경로)
suggest-workflow analyze [기존 옵션들...]
  # v2와 동일한 인터페이스. 내부적으로 index → query로 위임
suggest-workflow cache [기존 옵션들...]
  # v2 호환. 내부적으로 index 후 snapshot JSON 생성
```

### 3-3. DB 경로 resolve 규칙

`index`와 `query` 모두 동일한 로직으로 DB 파일 위치를 결정한다:

```
우선순위:
  1. --db <path>          직접 DB 파일 경로 지정 (고급, 테스트/디버깅)
  2. --project <path>     프로젝트 경로 → 인코딩 → DB 경로 계산
  3. (생략)               cwd를 project로 사용 (v2와 동일한 기본값)
```

내부 resolve 흐름:

```rust
fn resolve_db_path(db: Option<&str>, project: Option<&str>) -> Result<PathBuf> {
    // 1) 직접 DB 경로 지정
    if let Some(db_path) = db {
        return Ok(PathBuf::from(db_path));
    }

    // 2) 프로젝트 경로 → DB 경로 계산
    let project_path = match project {
        Some(p) => p.to_string(),
        None => std::env::current_dir()?.to_string_lossy().to_string(), // 3) cwd 사용
    };

    let encoded = resolve_project_path(&project_path)?;
    let encoded_name = encoded.file_name().unwrap().to_str().unwrap();
    let home = std::env::var("HOME")?;
    Ok(PathBuf::from(home)
        .join(".claude")
        .join("suggest-workflow-index")
        .join(encoded_name)
        .join("index.db"))
}
```

사용 예시:

```bash
# 가장 일반적: cwd 기준 자동 resolve
cd /home/user/my-project
suggest-workflow index                                    # → DB 생성
suggest-workflow query --perspective tool-frequency        # → 같은 DB 조회

# 명시적 프로젝트 지정 (cwd와 다른 프로젝트 분석)
suggest-workflow query --project /home/user/other-project --perspective trends

# 직접 DB 경로 (고급: 디버깅, 백업 DB 분석 등)
suggest-workflow query --db /tmp/debug-index.db --perspective hotfiles
```

`--project`와 `--db`를 동시에 지정하면 `--db`가 우선한다 (명시적 > 암묵적).

---

## 4. SQLite 스키마

### 4-1. 핵심 테이블

```sql
-- 메타 정보
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- 초기값: schema_version=3, project_path, last_indexed_at

-- 세션 목록
CREATE TABLE sessions (
    id             TEXT PRIMARY KEY,   -- 세션 파일명 (확장자 제외)
    file_path      TEXT NOT NULL,      -- JSONL 파일 절대 경로
    file_size      INTEGER NOT NULL,   -- bytes (변경 감지용)
    file_mtime     INTEGER NOT NULL,   -- 수정 시각 epoch ms (변경 감지용)
    first_ts       INTEGER,            -- 첫 프롬프트 timestamp ms
    last_ts        INTEGER,            -- 마지막 프롬프트 timestamp ms
    prompt_count   INTEGER NOT NULL DEFAULT 0,
    tool_use_count INTEGER NOT NULL DEFAULT 0,
    indexed_at     TEXT NOT NULL       -- ISO 8601
);

-- 프롬프트
CREATE TABLE prompts (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    text       TEXT NOT NULL,
    timestamp  INTEGER NOT NULL,      -- epoch ms
    char_count INTEGER NOT NULL
);
CREATE INDEX idx_prompts_session ON prompts(session_id);
CREATE INDEX idx_prompts_ts ON prompts(timestamp);

-- 도구 사용 (개별 호출 단위)
CREATE TABLE tool_uses (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq_order       INTEGER NOT NULL,  -- 세션 내 순서 (0-based)
    tool_name       TEXT NOT NULL,     -- 원본 이름 (Bash, Read, Edit, ...)
    classified_name TEXT NOT NULL,     -- 분류된 이름 (Bash:git, Bash:test, ...)
    timestamp       INTEGER,           -- epoch ms
    input_json      TEXT               -- tool input (Bash command 등)
);
CREATE INDEX idx_tool_uses_session ON tool_uses(session_id);
CREATE INDEX idx_tool_uses_tool ON tool_uses(classified_name);
CREATE INDEX idx_tool_uses_ts ON tool_uses(timestamp);

-- 파일 편집 (Edit/Write/NotebookEdit에서 추출)
CREATE TABLE file_edits (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id   TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tool_use_id  INTEGER NOT NULL REFERENCES tool_uses(id) ON DELETE CASCADE,
    file_path    TEXT NOT NULL,
    timestamp    INTEGER
);
CREATE INDEX idx_file_edits_session ON file_edits(session_id);
CREATE INDEX idx_file_edits_path ON file_edits(file_path);

-- 프롬프트 키워드 (토큰화된 단어, stopword 제외)
CREATE TABLE prompt_keywords (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    prompt_id  INTEGER NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    keyword    TEXT NOT NULL
);
CREATE INDEX idx_keywords_keyword ON prompt_keywords(keyword);

-- FTS5 전문 검색 (프롬프트 텍스트)
CREATE VIRTUAL TABLE prompts_fts USING fts5(
    text,
    content=prompts,
    content_rowid=id
);
```

### 4-2. 파생 테이블 (인덱싱 시 계산)

```sql
-- 도구 전이 (세션 내 연속 tool_use 쌍에서 집계)
CREATE TABLE tool_transitions (
    from_tool   TEXT NOT NULL,
    to_tool     TEXT NOT NULL,
    count       INTEGER NOT NULL,
    probability REAL NOT NULL,          -- P(to | from)
    PRIMARY KEY (from_tool, to_tool)
);

-- 주간 트렌드 버킷
CREATE TABLE weekly_buckets (
    week_start     TEXT NOT NULL,       -- YYYY-MM-DD (월요일)
    tool_name      TEXT NOT NULL,
    count          INTEGER NOT NULL,
    session_count  INTEGER NOT NULL,
    PRIMARY KEY (week_start, tool_name)
);
CREATE INDEX idx_weekly_week ON weekly_buckets(week_start);

-- 파일 핫스팟 (file_edits에서 집계)
CREATE TABLE file_hotspots (
    file_path     TEXT PRIMARY KEY,
    edit_count    INTEGER NOT NULL,
    session_count INTEGER NOT NULL
);

-- 세션 간 연결 (파일 겹침 기반)
CREATE TABLE session_links (
    session_a       TEXT NOT NULL,
    session_b       TEXT NOT NULL,
    shared_files    INTEGER NOT NULL,
    overlap_ratio   REAL NOT NULL,       -- Jaccard
    time_gap_minutes INTEGER,
    PRIMARY KEY (session_a, session_b)
);
```

### 4-3. 설계 원칙

1. **원시 데이터 테이블** (sessions, prompts, tool_uses, file_edits): 인크리멘털 인서트
2. **파생 테이블** (transitions, weekly_buckets, hotspots, links): 인덱싱 시 전체 재계산 (`DELETE + INSERT`)
3. 파생 테이블은 원시 데이터에서 SQL로도 계산 가능하지만, 자주 사용되는 집계를 미리 물리화(materialize)

---

## 5. 인크리멘털 인덱싱

### 5-1. DB 존재 여부에 따른 분기

```
suggest-workflow index --project /path/to/project

┌─ index.db 존재하는가?
│
├─ NO (첫 실행)
│   1. DB 파일 생성 + 스키마 초기화 (CREATE TABLE)
│   2. JSONL 전체 파싱 → 원시 데이터 INSERT
│   3. 파생 테이블 계산
│   4. 완료 (== --full과 동일)
│
├─ YES + 스키마 버전 일치
│   1. DB 열기
│   2. 변경 감지 → 변경된 세션만 재파싱 (인크리멘털)
│   3. 파생 테이블 재계산
│   4. 완료
│
└─ YES + 스키마 버전 불일치 (v3.0 DB에 v3.1 코드 등)
    1. 자동 마이그레이션 시도 (ALTER TABLE 등)
    2. 마이그레이션 불가 시 → 경고 출력 + --full 안내
    3. --full 시 DB 삭제 후 재생성
```

스키마 버전은 `meta` 테이블의 `schema_version` 값으로 관리:

```sql
SELECT value FROM meta WHERE key = 'schema_version';
-- 기대값: "3"  (v3.0 초기)
-- 불일치 시 마이그레이션 로직 실행
```

### 5-2. v2 JSON 캐시와의 관계

```
~/.claude/suggest-workflow-cache/   ← v2 (JSON, 기존)
~/.claude/suggest-workflow-index/   ← v3 (SQLite, 신규)
```

- **경로가 다르므로 충돌 없이 공존**
- v2 캐시에서 v3 DB로의 마이그레이션은 **불가**:
  - v2 요약본(`*.summary.json`)에는 개별 tool input이 없음 (B2 수정에 필수)
  - v2 snapshot은 집계 결과물이지 원시 데이터가 아님
- v3 첫 `index`는 항상 **JSONL 원본에서 풀 파싱**
- v2 캐시는 v3 안정화 후 Phase D에서 deprecated (삭제는 사용자 판단)

### 5-3. 변경 감지

호출부 (`commands/index.rs`) — Repository trait만 사용:

```rust
// commands/index.rs — rusqlite 의존 없음
fn index_session(repo: &dyn IndexRepository, file_path: &Path) -> Result<bool> {
    let meta = fs::metadata(file_path)?;
    let size = meta.len();
    let mtime = meta.modified()?.duration_since(UNIX_EPOCH)?.as_millis() as i64;

    if !repo.is_session_changed(file_path, size, mtime)? {
        return Ok(false); // 변경 없음 → skip
    }

    let session_data = parse_jsonl(file_path)?;
    repo.upsert_session(&session_data)?;
    Ok(true)
}
```

구현부 (`db/sqlite.rs`) — rusqlite 사용은 여기서만:

```rust
// db/sqlite.rs
impl IndexRepository for SqliteStore {
    fn is_session_changed(&self, file_path: &Path, size: u64, mtime: i64) -> Result<bool> {
        match self.conn.query_row(
            "SELECT file_size, file_mtime FROM sessions WHERE file_path = ?1",
            params![file_path.to_str()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        ) {
            Ok((saved_size, saved_mtime)) =>
                Ok(size as i64 != saved_size || mtime != saved_mtime),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true), // 신규 세션
            Err(e) => Err(e.into()),
        }
    }
}
```

### 5-4. 인덱싱 흐름 (인크리멘털)

```
suggest-workflow index --project /path/to/project

1. DB 파일 열기 (없으면 생성 + 스키마 초기화)
2. 스키마 버전 확인 → 필요 시 마이그레이션
3. 세션 파일 목록 스캔
4. 각 세션에 대해:
   a. 변경 감지 (size + mtime)
   b. 변경된 세션만:
      - DB에서 해당 세션 데이터 DELETE (CASCADE)
      - JSONL 파싱
      - 원시 데이터 INSERT (sessions, prompts, tool_uses, file_edits, keywords)
   c. 변경 없는 세션 → skip (로그에 "N sessions unchanged" 표시)
5. 삭제된 세션 감지 (DB에 있지만 파일 없음) → DELETE
6. 파생 테이블 재계산 (전체)
   - tool_transitions: tool_uses에서 집계
   - weekly_buckets: tool_uses + prompts에서 집계
   - file_hotspots: file_edits에서 집계
   - session_links: file_edits에서 Jaccard 계산
7. FTS5 인덱스 리빌드
8. meta.last_indexed_at 업데이트
9. stderr 요약: "Indexed: 3 new, 1 updated, 46 unchanged, 0 deleted"
```

### 5-5. `--full` 옵션

```bash
suggest-workflow index --project /path --full
# 기존 DB 파일 삭제 후 처음부터 재생성.
# 용도: 스키마 변경, DB 손상, 디버깅
```

---

## 6. 쿼리 시스템

### 6-1. Perspective 기반 쿼리

사전 정의된 분석 관점(perspective)으로 빠르게 조회:

```bash
# 도구 사용 빈도 (Top 10)
suggest-workflow query --perspective tool-frequency --top 10

# 도구 전이 (특정 도구 기준)
suggest-workflow query --perspective transitions --tool "Bash:git"

# 주간 트렌드
suggest-workflow query --perspective trends --since 2026-01-01

# 반복/이상치
suggest-workflow query --perspective repetition --z-threshold 2.0

# 프롬프트 검색
suggest-workflow query --perspective prompts --search "테스트"

# 파일 핫스팟
suggest-workflow query --perspective hotfiles --top 20

# 세션 연결
suggest-workflow query --perspective session-links --min-overlap 0.5

# 의존성 그래프
suggest-workflow query --perspective dependency-graph --top 15

# 시퀀스 패턴
suggest-workflow query --perspective sequences --min-count 3

# 프롬프트 클러스터 (BM25 기반)
suggest-workflow query --perspective clusters --depth normal
```

### 6-2. Repository 패턴

`rusqlite` 의존성을 `db/` 모듈 안에 격리하고, CLI commands는 trait만 의존한다.

#### 설계 원칙

```
commands/index.rs  ──→  IndexRepository (trait)  ←── db/sqlite.rs (impl)
commands/query.rs  ──→  QueryRepository (trait)  ←── db/sqlite.rs (impl)
                                                      ↓
                                                  rusqlite (여기서만 import)
```

- `rusqlite`는 `db/` 모듈 **외부에서 절대 import하지 않는다**
- Commands는 Repository trait의 메서드만 호출
- SQL은 `.sql` 파일로 분리하여 `include_str!`로 로드
- 모든 쿼리는 파라미터 바인딩(`?1`, `?2`) 사용 (SQL injection 방지)

#### Repository trait 정의

```rust
// db/repository.rs — trait 정의 (rusqlite 의존 없음)

use crate::types::*;
use anyhow::Result;
use std::path::Path;

/// 인덱싱(쓰기) 작업
pub trait IndexRepository {
    /// DB 초기화 (스키마 생성, 마이그레이션)
    fn initialize(&self) -> Result<()>;

    /// 세션 변경 여부 확인 (size + mtime 비교)
    fn is_session_changed(&self, file_path: &Path, size: u64, mtime: i64) -> Result<bool>;

    /// 세션 데이터 upsert (기존 데이터 DELETE 후 INSERT)
    fn upsert_session(&self, session: &SessionData) -> Result<()>;

    /// 삭제된 세션 제거 (DB에 있지만 파일 없음)
    fn remove_stale_sessions(&self, existing_paths: &[&Path]) -> Result<u64>;

    /// 파생 테이블 재계산 (transitions, weekly_buckets, hotspots, links)
    fn rebuild_derived_tables(&self) -> Result<()>;

    /// 메타 정보 업데이트
    fn update_meta(&self, key: &str, value: &str) -> Result<()>;

    /// 스키마 버전 조회
    fn schema_version(&self) -> Result<u32>;
}

/// 쿼리(읽기) 작업 — 각 perspective가 하나의 메서드
pub trait QueryRepository {
    fn tool_frequency(&self, opts: &QueryOpts) -> Result<serde_json::Value>;
    fn transitions(&self, tool: &str, opts: &QueryOpts) -> Result<serde_json::Value>;
    fn trends(&self, opts: &QueryOpts) -> Result<serde_json::Value>;
    fn repetition(&self, z_threshold: f64, opts: &QueryOpts) -> Result<serde_json::Value>;
    fn prompts(&self, search: &str, opts: &QueryOpts) -> Result<serde_json::Value>;
    fn hotfiles(&self, opts: &QueryOpts) -> Result<serde_json::Value>;
    fn session_links(&self, min_overlap: f64, opts: &QueryOpts) -> Result<serde_json::Value>;
    fn dependency_graph(&self, opts: &QueryOpts) -> Result<serde_json::Value>;
    fn sequences(&self, min_count: u32, opts: &QueryOpts) -> Result<serde_json::Value>;
    fn clusters(&self, depth: &str, opts: &QueryOpts) -> Result<serde_json::Value>;
    /// 커스텀 SQL 실행 (.sql 파일에서 읽은 내용 전달, SELECT만 허용)
    fn execute_sql(&self, sql: &str) -> Result<serde_json::Value>;
}

/// 공통 필터 옵션
pub struct QueryOpts {
    pub since: Option<String>,
    pub until: Option<String>,
    pub tool: Option<String>,
    pub session: Option<String>,
    pub top: Option<u32>,
    pub min_count: Option<u32>,
}
```

#### SQLite 구현

```rust
// db/sqlite.rs — rusqlite 의존은 이 파일(과 db/ 내부)에만 존재

use rusqlite::Connection;
use crate::db::repository::{IndexRepository, QueryRepository, QueryOpts};

pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub fn open(db_path: &Path) -> Result<Self> { ... }
}

impl IndexRepository for SqliteStore { ... }
impl QueryRepository for SqliteStore { ... }
```

#### SQL 파일 분리

```rust
// db/sqlite.rs 내부에서 SQL 로드
const TOOL_FREQUENCY_SQL: &str = include_str!("queries/tool_frequency.sql");
const TRANSITIONS_SQL: &str = include_str!("queries/transitions.sql");
const TRENDS_SQL: &str = include_str!("queries/trends.sql");
// ...

impl QueryRepository for SqliteStore {
    fn tool_frequency(&self, opts: &QueryOpts) -> Result<serde_json::Value> {
        let top = opts.top.unwrap_or(10);
        let mut stmt = self.conn.prepare(TOOL_FREQUENCY_SQL)?;
        let rows = stmt.query_map(params![top], |row| { ... })?;
        Ok(serde_json::to_value(rows.collect::<Vec<_>>()?)?)
    }
}
```

```sql
-- db/queries/tool_frequency.sql
SELECT classified_name AS tool,
       COUNT(*) AS frequency,
       COUNT(DISTINCT session_id) AS sessions
FROM tool_uses
GROUP BY classified_name
ORDER BY frequency DESC
LIMIT ?1
```

```sql
-- db/queries/transitions.sql
SELECT to_tool, count, probability
FROM tool_transitions
WHERE from_tool = ?1
ORDER BY probability DESC
```

```sql
-- db/queries/trends.sql
SELECT week_start, tool_name, count, session_count
FROM weekly_buckets
WHERE week_start >= ?1
ORDER BY week_start, count DESC
```

#### Command에서의 사용

```rust
// commands/query.rs — rusqlite를 import하지 않음
use crate::db::repository::QueryRepository;

pub fn run_query(
    repo: &dyn QueryRepository,
    perspective: Option<&str>,
    sql_file: Option<&Path>,
    opts: QueryOpts,
) -> Result<()> {
    let result = match (perspective, sql_file) {
        // --sql-file: 파일에서 SQL 읽어서 실행
        (_, Some(path)) => {
            let sql = std::fs::read_to_string(path)?;
            repo.execute_sql(&sql)?
        }
        // --perspective: 사전 정의 쿼리
        (Some("tool-frequency"), _) => repo.tool_frequency(&opts)?,
        (Some("transitions"), _)    => repo.transitions(opts.tool.as_deref().unwrap_or(""), &opts)?,
        (Some("trends"), _)         => repo.trends(&opts)?,
        // ...
        (Some(name), _) => anyhow::bail!("unknown perspective: {}", name),
        (None, None)     => anyhow::bail!("--perspective or --sql-file required"),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
```

```rust
// main.rs — 조립(wiring)만 여기서
use crate::db::sqlite::SqliteStore;

fn main() -> Result<()> {
    let args = Cli::parse();
    let db_path = resolve_db_path(&args)?;
    let store = SqliteStore::open(&db_path)?;

    match args.command {
        Command::Index(opts) => commands::index::run_index(&store, opts),
        Command::Query(opts) => commands::query::run_query(&store, &opts.perspective, opts.into()),
        // ...
    }
}
```

### 6-3. 공통 필터 옵션

모든 perspective에 적용 가능한 공통 필터:

```bash
--since YYYY-MM-DD        # 시작 날짜
--until YYYY-MM-DD        # 종료 날짜
--tool <name>             # 도구 이름 필터 (LIKE 패턴 지원)
--session <id>            # 특정 세션만
--top N                   # 상위 N개
--min-count N             # 최소 빈도
--format json|csv|table   # 출력 형식 (기본: json)
```

### 6-4. 파이프 체이닝

```bash
# 1단계: git 관련 도구 전이 확인
suggest-workflow query --perspective transitions --tool "Bash:git" \
| # 2단계: 결과를 jq로 가공
  jq '.[] | select(.probability > 0.3)' \
| # 3단계: 다른 관점으로 추가 분석
  xargs -I {} suggest-workflow query --perspective sequences --include-tool {}
```

### 6-5. 커스텀 SQL (고급)

Phase 2 에이전트가 사전 정의 perspective로 부족할 때, `.sql` 파일을 작성하여 전달:

```bash
suggest-workflow query --sql-file /tmp/custom-query.sql
```

Phase 2 에이전트의 사용 흐름:

```
1. Write 도구로 .sql 파일 작성 (쉘 이스케이핑 걱정 없음)
2. --sql-file로 전달
3. JSON 결과 수신
```

예시 `.sql` 파일:

```sql
-- /tmp/custom-query.sql
SELECT t1.classified_name as tool,
       COUNT(*) as freq,
       AVG(CASE WHEN t2.classified_name LIKE 'Bash:%' THEN 1.0 ELSE 0.0 END) as bash_follow_rate
FROM tool_uses t1
LEFT JOIN tool_uses t2
  ON t1.session_id = t2.session_id AND t2.seq_order = t1.seq_order + 1
GROUP BY t1.classified_name
HAVING freq >= 5
ORDER BY freq DESC
```

실행:

```bash
suggest-workflow query --sql-file /tmp/custom-query.sql
# → JSON 출력 (stdout)
```

**보안**: `--sql-file`은 SELECT만 허용 (INSERT/UPDATE/DELETE/DROP 차단). 파일 내용을 파싱하여 DML/DDL 포함 시 즉시 에러 반환.

---

## 7. Phase 2 에이전트 인터페이스 변경

### 7-1. v2 → v3 비교

```
v2: agent가 analysis-snapshot.json 전체를 Read → 메모리에서 해석
v3: agent가 query 서브커맨드를 필요한 만큼 호출 → 필요한 관점만 획득
```

### 7-2. workflow-insight agent 사용 예시 (v3)

```bash
# Step 1: 인덱스 업데이트
suggest-workflow index --project "$(pwd)"

# Step 2: 필요한 관점별로 쿼리
TOOL_FREQ=$(suggest-workflow query --perspective tool-frequency --top 15)
TRANSITIONS=$(suggest-workflow query --perspective transitions --tool "Edit")
TRENDS=$(suggest-workflow query --perspective trends --since 2026-01-01)
HOTFILES=$(suggest-workflow query --perspective hotfiles --top 10)
REPETITION=$(suggest-workflow query --perspective repetition)
CLUSTERS=$(suggest-workflow query --perspective clusters)

# Step 3: Claude가 결과를 종합하여 시맨틱 해석
```

### 7-3. 장점

1. **토큰 절약**: 필요한 데이터만 가져옴 (analysis-snapshot.json 전체 대비 1/10~1/5)
2. **탐색적 분석**: 한 쿼리 결과를 보고 "이 부분 더 파보자" 가능
3. **새 관점 추가 용이**: SQL 템플릿 하나 추가 = 새 perspective

---

## 8. v2 버그 수정 계획

v3 인덱싱 파이프라인 구현 시 자연스럽게 해결되는 항목:

| ID | 버그 | v3 해결 방식 |
|----|------|------------|
| B2 | Bash tool input 미전달 | `tool_uses.input_json`에 input 저장, 인덱싱 시 classify |
| B1 | 프로젝트 경로 디코딩 | v2에서 이미 수정됨 (인코딩 이름 그대로 사용) |
| B3 | decay 정렬 미반영 | SQL `ORDER BY`에서 decay 가중치 컬럼 사용 |
| B4 | examples 비결정적 순서 | DB에서 `ORDER BY timestamp` |
| B5 | confidence 분모 불일치 | SQL `COUNT(*)` 기반으로 정확한 분모 사용 |
| P1 | 이중 tool extraction | 인덱싱 시 한 번만 파싱 후 DB 저장 |
| P2 | bigram 반복 계산 | 클러스터링은 DB에서 pre-computed similarity 활용 |

---

## 9. 의존성 변경

### 9-1. Cargo.toml 추가

```toml
[dependencies]
# 기존 유지
clap = { version = "4.5", features = ["derive", "subcommand"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1.0"
rayon = "1.10"
regex = "1.10"
walkdir = "2.5"

# v3 신규
rusqlite = { version = "0.32", features = ["bundled", "serde_json"] }

# 선택적 (기존)
lindera = { version = "2.1", features = ["lindera-ko-dic", "embed-ko-dic"], optional = true }
```

### 9-2. 바이너리 크기 영향

| 구성 요소 | 예상 크기 |
|-----------|----------|
| v2 바이너리 (현재) | ~2-3 MB |
| rusqlite (bundled) | +~600 KB |
| v3 바이너리 (예상) | ~3-4 MB |

release 프로필 (`opt-level=3, lto=true, strip=true`)로 최소화. CLI 도구로서 충분히 가벼움.

---

## 10. 마이그레이션 전략

### 10-1. 하위 호환성

```bash
# v2 명령어 → v3에서도 동작
suggest-workflow --project $(pwd) --format json
# 내부적으로: index → analyze 위임

suggest-workflow --cache
# 내부적으로: index → analysis-snapshot.json 생성 (v2 형식)
```

### 10-2. 단계적 전환

```
Phase A: index + query 서브커맨드 추가 (기존 analyze/cache 유지)
Phase B: analyze 내부를 DB 기반으로 전환
Phase C: Phase 2 에이전트가 query 사용하도록 업데이트
Phase D: v2 캐시 형식 deprecated
```

---

## 11. 파일 구조 변경

```
cli/src/
├── main.rs                    # clap subcommand 라우팅 + 의존성 조립(wiring)
├── types.rs                   # 공통 타입 (SessionData, Transition 등)
├── db/                        # [신규] 저장소 레이어
│   ├── mod.rs                 # pub use repository, sqlite
│   ├── repository.rs          # trait 정의 (IndexRepository, QueryRepository)
│   │                          #   → rusqlite 의존 없음, 순수 인터페이스
│   ├── sqlite.rs              # SqliteStore: trait 구현체
│   │                          #   → rusqlite는 이 파일에서만 import
│   ├── schema.rs              # 테이블 생성 DDL (include_str!로 로드)
│   ├── migrate.rs             # 스키마 버전 관리
│   └── queries/               # [신규] SQL 파일 분리
│       ├── schema.sql         # CREATE TABLE DDL
│       ├── tool_frequency.sql
│       ├── transitions.sql
│       ├── trends.sql
│       ├── repetition.sql
│       ├── hotfiles.sql
│       ├── session_links.sql
│       ├── sequences.sql
│       └── ...
├── commands/                  # rusqlite 의존 없음 — trait만 사용
│   ├── mod.rs
│   ├── index.rs               # [신규] fn run_index(repo: &dyn IndexRepository, ...)
│   ├── query.rs               # [신규] fn run_query(repo: &dyn QueryRepository, ...)
│   ├── analyze.rs             # [유지] v2 호환
│   └── cache.rs               # [유지] v2 호환
├── analyzers/                 # 기존 유지 (Phase B에서 점진적 DB 기반 전환)
│   ├── ...
├── parsers/                   # 기존 유지
│   ├── ...
└── tokenizer/                 # 기존 유지
    ├── ...
```

**의존성 방향** (단방향):
```
commands/ ──→ db/repository.rs (trait)
                    ↑
main.rs ──→ db/sqlite.rs (impl) ──→ rusqlite
```

`commands/`는 `db/repository.rs`의 trait만 알고, 구체 구현(`sqlite.rs`)과 `rusqlite`는 모른다.
`main.rs`만 구체 구현을 알고 조립(wiring)한다.

---

## 12. DB 파일 위치

```
~/.claude/suggest-workflow-index/
├── {project-encoded}/
│   └── index.db               # 프로젝트별 SQLite DB
└── global/
    └── index.db               # 글로벌 (크로스 프로젝트) DB
```

---

## 13. 구현 순서 (권장)

### Sprint 1: 기반 (Repository 스캐폴딩)

1. `rusqlite` 의존성 추가 + 빌드 확인
2. `db/repository.rs` — `IndexRepository`, `QueryRepository` trait 정의
3. `db/queries/schema.sql` — DDL 분리
4. `db/sqlite.rs` — `SqliteStore` 구현체 (스키마 초기화 + 원시 데이터 INSERT)
5. `commands/index.rs` — 기본 인덱싱 (`&dyn IndexRepository`만 의존)

### Sprint 2: 쿼리

6. `db/queries/*.sql` — perspective별 SQL 파일 작성
7. `db/sqlite.rs` — `QueryRepository` 구현 (`include_str!` + 파라미터 바인딩)
8. `commands/query.rs` — 쿼리 커맨드 (`&dyn QueryRepository`만 의존)
9. `main.rs` — 서브커맨드 라우팅 + `SqliteStore` 조립(wiring)

### Sprint 3: 인크리멘털 + 파생

8. 인크리멘털 인덱싱 (변경 감지, 선택적 파싱)
9. 파생 테이블 계산 (transitions, weekly_buckets, hotspots, links)
10. `db/migrate.rs` — 스키마 버전 관리

### Sprint 4: 통합

11. `analyze` 커맨드 내부를 DB 기반으로 전환
12. `cache` 커맨드 → index + snapshot 추출로 전환
13. Phase 2 에이전트 업데이트 (workflow-insight.md)
14. B2/B3 등 기존 버그 수정

### Sprint 5: 고급 기능

15. FTS5 프롬프트 검색
16. 프롬프트 클러스터링 (BM25 → DB 기반)
17. `--sql` 커스텀 쿼리 지원
18. 글로벌 인덱스

---

## 14. 성능 기대치

| 작업 | v2 (현재) | v3 (예상) | 개선 |
|------|----------|----------|------|
| 첫 인덱싱 (50 세션) | N/A | ~500ms | - |
| 인크리멘털 (1 세션 추가) | 전체 재계산 ~2s | ~100ms | **20x** |
| 도구 빈도 조회 | JSON 전체 읽기 ~200ms | SQL 쿼리 ~5ms | **40x** |
| 전이 필터링 | JSON 읽기 + Claude 필터 | SQL WHERE ~3ms | **토큰 절약** |
| Phase 2 토큰 사용 | snapshot 전체 (~10K 토큰) | 필요한 쿼리만 (~2K) | **5x 절약** |

---

## 15. 결정 사항 요약

| 결정 | 선택 | 근거 |
|------|------|------|
| 스토리지 | SQLite (rusqlite bundled) | 유연한 SQL, ~600KB, 단일 파일 |
| DB 접근 패턴 | Repository 패턴 (trait 추상화) | rusqlite 의존성 격리, commands에서 DB 구현 모름 |
| SQL 관리 | `.sql` 파일 분리 + `include_str!` | 에디터 하이라이팅, 관심사 분리 |
| 쿼리 안전성 | 파라미터 바인딩 (`?1`, `?2`) | SQL injection 방지 |
| 쿼리 인터페이스 | perspective 기반 + 커스텀 SQL | 일반 사용은 쉽게, 고급은 자유롭게 |
| 인크리멘털 전략 | size + mtime 변경 감지 | 단순하고 신뢰성 있음 |
| 파생 테이블 | 전체 재계산 | 데이터 규모가 작아 충분히 빠름 |
| v2 호환 | 기존 CLI 인터페이스 유지 | 점진적 마이그레이션 |
| Phase 2 연동 | query 서브커맨드 | 에이전트가 필요한 것만 요청 |
