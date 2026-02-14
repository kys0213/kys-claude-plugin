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
suggest-workflow query [--project <path>] [--perspective <name>] [--param key=value]... [options]
  # perspective 이름 + 동적 파라미터로 조회
  # --param은 perspective에 정의된 파라미터를 전달 (복수 가능)
  # 결과는 항상 JSON (stdout) → 파이프 체이닝 가능

suggest-workflow query --sql-file <path>
  # 커스텀 .sql 파일 실행 (SELECT만 허용)

suggest-workflow query --list-perspectives
  # 사용 가능한 perspective 목록 + 파라미터 정의 출력

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

사전 정의된 분석 관점(perspective)으로 빠르게 조회.
perspective별 파라미터는 `--param key=value`로 전달한다. (생략 시 Rust 코드에 정의된 default 값 사용)

```bash
# 사용 가능한 perspective 목록 확인
suggest-workflow query --list-perspectives

# 도구 사용 빈도 (Top 10, default)
suggest-workflow query --perspective tool-frequency
suggest-workflow query --perspective tool-frequency --param top=20

# 도구 전이 (특정 도구 기준, tool은 required)
suggest-workflow query --perspective transitions --param tool="Bash:git"

# 주간 트렌드 (default: 2026-01-01부터)
suggest-workflow query --perspective trends
suggest-workflow query --perspective trends --param since=2026-02-01

# 반복/이상치 (default: z-score 2.0)
suggest-workflow query --perspective repetition --param z_threshold=3.0

# 프롬프트 검색 (search는 required)
suggest-workflow query --perspective prompts --param search="테스트"

# 파일 핫스팟
suggest-workflow query --perspective hotfiles --param top=30

# 세션 연결
suggest-workflow query --perspective session-links --param min_overlap=0.5

# 의존성 그래프
suggest-workflow query --perspective dependency-graph --param top=15

# 시퀀스 패턴
suggest-workflow query --perspective sequences --param min_count=3

# 프롬프트 클러스터 (BM25 기반)
suggest-workflow query --perspective clusters --param depth=normal
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
- 빌트인 perspective SQL은 **Rust 코드에 직접 정의** (`register_perspectives()`)
- 모든 쿼리는 파라미터 바인딩(`?1`, `?2`) 사용 (SQL injection 방지)
- 커스텀 쿼리는 Phase 2 에이전트가 **레포별로 `.sql` 파일을 작성**하여 `--sql-file`로 전달

#### Repository trait 정의

```rust
// db/repository.rs — trait 정의 (rusqlite 의존 없음)

use anyhow::Result;
use std::collections::HashMap;
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

/// 쿼리(읽기) 작업 — perspective를 이름 + 파라미터로 동적 디스패치
pub trait QueryRepository {
    /// 등록된 perspective 목록 조회 (--list-perspectives 용)
    fn list_perspectives(&self) -> Result<Vec<PerspectiveInfo>>;

    /// perspective 이름 + 동적 파라미터로 실행
    fn query(&self, perspective: &str, params: &QueryParams) -> Result<serde_json::Value>;

    /// 커스텀 SQL 실행 (.sql 파일에서 읽은 내용 전달, SELECT만 허용)
    fn execute_sql(&self, sql: &str) -> Result<serde_json::Value>;
}

/// perspective 메타데이터 (Rust 코드에서 직접 정의)
pub struct PerspectiveInfo {
    pub name: String,
    pub description: String,
    pub params: Vec<ParamDef>,
    pub sql: String,
}

/// 파라미터 정의
pub struct ParamDef {
    pub name: String,
    pub param_type: ParamType,
    pub required: bool,
    pub default: Option<String>,
    pub description: String,
}

pub enum ParamType {
    Integer,
    Float,
    Text,
    Date,  // YYYY-MM-DD
}

/// 쿼리 실행 시 전달되는 동적 파라미터
pub type QueryParams = HashMap<String, String>;
```

#### SQLite 구현

```rust
// db/sqlite.rs — rusqlite 의존은 이 파일(과 db/ 내부)에만 존재

use rusqlite::Connection;
use crate::db::repository::{IndexRepository, QueryRepository};

pub struct SqliteStore {
    conn: Connection,
    perspectives: Vec<PerspectiveInfo>,
}

impl SqliteStore {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let perspectives = register_perspectives();
        Ok(Self { conn, perspectives })
    }
}

impl IndexRepository for SqliteStore { ... }
impl QueryRepository for SqliteStore { ... }
```

#### Perspective 등록: Rust 코드에서 직접 정의

빌트인 perspective는 `register_perspectives()` 함수에서 이름, 설명, 파라미터, SQL을 모두 Rust 코드로 정의한다.
**CLI 바이너리 안에 `.sql` 파일이 포함되지 않는다.** 커스텀 쿼리만 Phase 2 에이전트가 레포별로 `.sql` 파일을 작성하여 `--sql-file`로 전달한다.

```rust
// db/perspectives.rs — 빌트인 perspective 등록

use crate::db::repository::{PerspectiveInfo, ParamDef, ParamType};

pub fn register_perspectives() -> Vec<PerspectiveInfo> {
    vec![
        PerspectiveInfo {
            name: "tool-frequency".into(),
            description: "도구 사용 빈도 (분류명 기준)".into(),
            params: vec![
                ParamDef {
                    name: "top".into(),
                    param_type: ParamType::Integer,
                    required: false,
                    default: Some("10".into()),
                    description: "상위 N개".into(),
                },
            ],
            sql: "\
                SELECT classified_name AS tool, \
                       COUNT(*) AS frequency, \
                       COUNT(DISTINCT session_id) AS sessions \
                FROM tool_uses \
                GROUP BY classified_name \
                ORDER BY frequency DESC \
                LIMIT :top".into(),
        },
        PerspectiveInfo {
            name: "transitions".into(),
            description: "특정 도구 이후 전이 확률".into(),
            params: vec![
                ParamDef {
                    name: "tool".into(),
                    param_type: ParamType::Text,
                    required: true,
                    default: None,
                    description: "기준 도구 (예: Bash:git, Edit)".into(),
                },
            ],
            sql: "\
                SELECT to_tool, count, probability \
                FROM tool_transitions \
                WHERE from_tool = :tool \
                ORDER BY probability DESC".into(),
        },
        PerspectiveInfo {
            name: "trends".into(),
            description: "주간 도구 사용 트렌드".into(),
            params: vec![
                ParamDef {
                    name: "since".into(),
                    param_type: ParamType::Date,
                    required: false,
                    default: Some("2026-01-01".into()),
                    description: "시작 날짜".into(),
                },
            ],
            sql: "\
                SELECT week_start, tool_name, count, session_count \
                FROM weekly_buckets \
                WHERE week_start >= :since \
                ORDER BY week_start, count DESC".into(),
        },
        PerspectiveInfo {
            name: "hotfiles".into(),
            description: "자주 편집되는 파일 핫스팟".into(),
            params: vec![
                ParamDef {
                    name: "top".into(),
                    param_type: ParamType::Integer,
                    required: false,
                    default: Some("20".into()),
                    description: "상위 N개".into(),
                },
            ],
            sql: "\
                SELECT file_path, edit_count, session_count \
                FROM file_hotspots \
                ORDER BY edit_count DESC \
                LIMIT :top".into(),
        },
        PerspectiveInfo {
            name: "repetition".into(),
            description: "반복/이상치 탐지 (z-score 기반)".into(),
            params: vec![
                ParamDef {
                    name: "z_threshold".into(),
                    param_type: ParamType::Float,
                    required: false,
                    default: Some("2.0".into()),
                    description: "z-score 임계값".into(),
                },
            ],
            sql: "/* z-score 기반 이상치 탐지 SQL */".into(),
        },
        // session-links, sequences, prompts, clusters, dependency-graph 등 추가
    ]
}
```

**perspective 추가 방법**: `register_perspectives()`에 `PerspectiveInfo` 항목 1개 추가. 그 외 변경 불필요.

#### QueryRepository 구현

```rust
// db/sqlite.rs

impl QueryRepository for SqliteStore {
    fn list_perspectives(&self) -> Result<Vec<PerspectiveInfo>> {
        Ok(self.perspectives.clone())
    }

    fn query(&self, perspective: &str, params: &QueryParams) -> Result<serde_json::Value> {
        let info = self.perspectives.iter()
            .find(|p| p.name == perspective)
            .ok_or_else(|| anyhow::anyhow!("unknown perspective: {}", perspective))?;

        // 1. 필수 파라미터 검증
        for param_def in &info.params {
            if param_def.required && !params.contains_key(&param_def.name) {
                anyhow::bail!("missing required param: --param {}=<value>", param_def.name);
            }
        }

        // 2. :name → ?N 치환 + 바인딩 값 배열 구성
        let (bound_sql, bind_values) = bind_named_params(&info.sql, &info.params, params)?;

        // 3. 실행
        let mut stmt = self.conn.prepare(&bound_sql)?;
        let rows = stmt_to_json(&mut stmt, &bind_values)?;
        Ok(rows)
    }
}
```

#### Named parameter 치환

SQL 본문의 `:name`을 `?N`으로 변환하고, 바인딩 값 배열을 구성:

```rust
/// ":top" → "?1", ":tool" → "?2" 등으로 치환
fn bind_named_params(
    sql: &str,
    defs: &[ParamDef],
    params: &QueryParams,
) -> Result<(String, Vec<rusqlite::types::Value>)> {
    let mut bound_sql = sql.to_string();
    let mut values = Vec::new();

    for (i, def) in defs.iter().enumerate() {
        let value = params.get(&def.name)
            .cloned()
            .or_else(|| def.default.clone())
            .ok_or_else(|| anyhow::anyhow!("missing param: {}", def.name))?;

        bound_sql = bound_sql.replace(
            &format!(":{}", def.name),
            &format!("?{}", i + 1),
        );
        values.push(coerce_value(&value, &def.param_type)?);
    }

    Ok((bound_sql, values))
}
```

#### Command에서의 사용

```rust
// commands/query.rs — rusqlite를 import하지 않음
use crate::db::repository::{QueryRepository, QueryParams};

pub fn run_query(
    repo: &dyn QueryRepository,
    perspective: Option<&str>,
    sql_file: Option<&Path>,
    params: QueryParams,           // --param key=value로 수집된 동적 파라미터
) -> Result<()> {
    let result = match (perspective, sql_file) {
        // --sql-file: 커스텀 SQL 파일 실행
        (_, Some(path)) => {
            let sql = std::fs::read_to_string(path)?;
            repo.execute_sql(&sql)?
        }
        // --perspective: 이름으로 동적 디스패치 (match arm 추가 불필요)
        (Some(name), _) => repo.query(name, &params)?,
        (None, None)     => anyhow::bail!("--perspective or --sql-file required"),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

// --list-perspectives: 사용 가능한 perspective 목록 출력
pub fn list_perspectives(repo: &dyn QueryRepository) -> Result<()> {
    let perspectives = repo.list_perspectives()?;
    for p in &perspectives {
        eprintln!("  {} — {}", p.name, p.description);
        for param in &p.params {
            let req = if param.required { "required" } else {
                &format!("default: {}", param.default.as_deref().unwrap_or("none"))
            };
            eprintln!("    --{}: {} ({})", param.name, param.description, req);
        }
    }
    Ok(())
}
```

#### clap 구조 (main.rs)

```rust
// main.rs
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "suggest-workflow", version = "3.0.0")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 세션 데이터 인덱싱
    Index(IndexArgs),
    /// 인덱스 쿼리
    Query(QueryArgs),
    /// [v2 호환] 분석 실행
    Analyze(LegacyAnalyzeArgs),
    /// [v2 호환] 캐시 생성
    Cache(LegacyCacheArgs),
}

#[derive(clap::Args)]
struct IndexArgs {
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    db: Option<PathBuf>,
    /// 전체 재구축 (기존 DB 삭제)
    #[arg(long)]
    full: bool,
}

#[derive(clap::Args)]
struct QueryArgs {
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    db: Option<PathBuf>,

    /// perspective 이름 (예: tool-frequency, transitions)
    #[arg(long)]
    perspective: Option<String>,

    /// 커스텀 SQL 파일 경로
    #[arg(long)]
    sql_file: Option<PathBuf>,

    /// 사용 가능한 perspective 목록 출력
    #[arg(long)]
    list_perspectives: bool,

    /// 동적 파라미터 (복수 가능): --param top=10 --param tool="Bash:git"
    #[arg(long = "param", value_parser = parse_key_val)]
    params: Vec<(String, String)>,

    /// 출력 형식
    #[arg(long, default_value = "json")]
    format: String,
}

/// "key=value" 문자열을 (String, String) 튜플로 파싱
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s.find('=')
        .ok_or_else(|| format!("invalid param (expected key=value): '{}'", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Index(args) => {
            let db_path = resolve_db_path(args.db.as_deref(), args.project.as_deref())?;
            let store = SqliteStore::open(&db_path)?;
            commands::index::run_index(&store, args.full)
        }
        Command::Query(args) => {
            let db_path = resolve_db_path(args.db.as_deref(), args.project.as_deref())?;
            let store = SqliteStore::open(&db_path)?;

            if args.list_perspectives {
                return commands::query::list_perspectives(&store);
            }

            // Vec<(String, String)> → HashMap<String, String>
            let params: QueryParams = args.params.into_iter().collect();

            commands::query::run_query(
                &store,
                args.perspective.as_deref(),
                args.sql_file.as_deref(),
                params,
            )
        }
        Command::Analyze(args) => { /* v2 호환: 내부적으로 index → query 위임 */ }
        Command::Cache(args)   => { /* v2 호환: 내부적으로 index → snapshot 추출 */ }
    }
}
```

### 6-3. CLI 사용 예시 (end-to-end)

```bash
# ── 인덱싱 ──
suggest-workflow index                              # cwd 기준 인크리멘털
suggest-workflow index --project /path/to/repo      # 다른 프로젝트
suggest-workflow index --full                       # DB 삭제 후 전체 재구축

# ── perspective 목록 확인 ──
suggest-workflow query --list-perspectives
#   tool-frequency — 도구 사용 빈도 (분류명 기준)
#     --top: 상위 N개 (default: 10)
#   transitions — 특정 도구 이후 전이 확률
#     --tool: 기준 도구 (required)
#   trends — 주간 도구 사용 트렌드
#     --since: 시작 날짜 (default: 2026-01-01)
#   ...

# ── perspective 쿼리 ──
suggest-workflow query --perspective tool-frequency                     # default 파라미터 사용
suggest-workflow query --perspective tool-frequency --param top=5       # top 오버라이드
suggest-workflow query --perspective transitions --param tool="Bash:git"  # required 파라미터
suggest-workflow query --perspective trends --param since=2026-02-01

# ── 커스텀 SQL ──
suggest-workflow query --sql-file /tmp/my-query.sql

# ── 파이프 체이닝 ──
suggest-workflow query --perspective tool-frequency --param top=5 \
  | jq '.[].tool' -r

# ── v2 호환 ──
suggest-workflow analyze --scope project --format json   # 기존 인터페이스 유지
suggest-workflow cache                                    # v2 스냅샷 생성
```

`--param`은 각 perspective의 Rust 코드(`register_perspectives()`)에 정의된 `ParamDef`에 대응한다.
공통적으로 자주 쓰이는 파라미터(top, since, tool 등)도 perspective별로 개별 선언하므로,
별도의 "공통 필터"가 아닌 **perspective가 필요한 파라미터를 스스로 선언**하는 구조.

### 6-4. 파이프 체이닝

```bash
# 1단계: git 관련 도구 전이 확인
suggest-workflow query --perspective transitions --tool "Bash:git" \
| # 2단계: 결과를 jq로 가공
  jq '.[] | select(.probability > 0.3)' \
| # 3단계: 다른 관점으로 추가 분석
  xargs -I {} suggest-workflow query --perspective sequences --include-tool {}
```

### 6-5. 커스텀 SQL (Phase 2 에이전트용)

빌트인 perspective만으로는 레포별 특성에 맞는 분석이 어려울 수 있다.
Phase 2 에이전트(Claude)가 **분석 대상 레포에 맞는 커스텀 쿼리를 직접 작성**하여 `--sql-file`로 전달한다.

**CLI 내부에는 `.sql` 파일이 없다.** `.sql` 파일은 오직 Phase 2 에이전트가 런타임에 생성하는 것.

```bash
suggest-workflow query --sql-file /tmp/custom-query.sql
```

Phase 2 에이전트의 사용 흐름:

```
1. 빌트인 perspective 결과를 먼저 확인
2. 더 깊은 분석이 필요하면 Write 도구로 .sql 파일 작성 (쉘 이스케이핑 걱정 없음)
3. --sql-file로 전달 → JSON 결과 수신
4. 레포 특성에 맞는 인사이트 도출
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

# Step 2: 사용 가능한 perspective 확인
suggest-workflow query --list-perspectives

# Step 3: 필요한 관점별로 쿼리 (--param으로 동적 파라미터 전달)
TOOL_FREQ=$(suggest-workflow query --perspective tool-frequency --param top=15)
TRANSITIONS=$(suggest-workflow query --perspective transitions --param tool="Edit")
TRENDS=$(suggest-workflow query --perspective trends --param since=2026-01-01)
HOTFILES=$(suggest-workflow query --perspective hotfiles --param top=10)
REPETITION=$(suggest-workflow query --perspective repetition)
CLUSTERS=$(suggest-workflow query --perspective clusters)

# Step 3: Claude가 결과를 종합하여 시맨틱 해석
```

### 7-3. 장점

1. **토큰 절약**: 필요한 데이터만 가져옴 (analysis-snapshot.json 전체 대비 1/10~1/5)
2. **탐색적 분석**: 한 쿼리 결과를 보고 "이 부분 더 파보자" 가능
3. **새 관점 추가 용이**: `register_perspectives()`에 항목 1개 추가 = 새 perspective

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
cli/
├── Cargo.toml
└── src/
    ├── main.rs                    # clap subcommand 라우팅 + 의존성 조립(wiring)
    ├── types.rs                   # 공통 타입 (SessionData, Transition 등)
    ├── db/                        # [신규] 저장소 레이어
    │   ├── mod.rs                 # pub use repository, sqlite, perspectives
    │   ├── repository.rs          # trait 정의 (IndexRepository, QueryRepository)
    │   │                          #   → rusqlite 의존 없음, 순수 인터페이스
    │   ├── sqlite.rs              # SqliteStore: trait 구현체
    │   │                          #   → rusqlite는 이 파일에서만 import
    │   ├── perspectives.rs        # [신규] register_perspectives() — 빌트인 perspective 등록
    │   │                          #   → 이름, 설명, 파라미터, SQL을 Rust 코드로 정의
    │   ├── schema.rs              # 테이블 생성 DDL
    │   └── migrate.rs             # 스키마 버전 관리
    ├── commands/                  # rusqlite 의존 없음 — trait만 사용
    │   ├── mod.rs
    │   ├── index.rs               # fn run_index(repo: &dyn IndexRepository, ...)
    │   ├── query.rs               # fn run_query(repo: &dyn QueryRepository, ...)
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
                    ↑
              db/perspectives.rs (빌트인 SQL + 파라미터 정의)
```

`commands/`는 `db/repository.rs`의 trait만 알고, 구체 구현(`sqlite.rs`)과 `rusqlite`는 모른다.
`main.rs`만 구체 구현을 알고 조립(wiring)한다.
빌트인 perspective SQL은 `perspectives.rs`에서 Rust 코드로 직접 정의한다. **CLI 바이너리 안에 `.sql` 파일 없음.**
커스텀 SQL 파일은 Phase 2 에이전트가 레포별로 런타임에 작성 → `--sql-file`로 전달.

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
3. `db/schema.rs` — DDL 정의
4. `db/sqlite.rs` — `SqliteStore` 구현체 (스키마 초기화 + 원시 데이터 INSERT)
5. `commands/index.rs` — 기본 인덱싱 (`&dyn IndexRepository`만 의존)

### Sprint 2: 쿼리 (perspective 시스템)

6. `db/perspectives.rs` — `register_perspectives()` 빌트인 perspective 등록 (SQL + 파라미터를 Rust 코드에 직접 정의)
7. `db/sqlite.rs` — `QueryRepository` 구현 (named param 치환 + 동적 디스패치)
8. `commands/query.rs` — 쿼리 커맨드 (`--perspective`, `--param`, `--list-perspectives`, `--sql-file`)
9. `main.rs` — 서브커맨드 라우팅 + `SqliteStore` 조립(wiring)

### Sprint 3: 인크리멘털 + 파생

10. 인크리멘털 인덱싱 (변경 감지, 선택적 파싱)
11. 파생 테이블 계산 (transitions, weekly_buckets, hotspots, links)
12. `db/migrate.rs` — 스키마 버전 관리

### Sprint 4: 통합

13. `analyze` 커맨드 내부를 DB 기반으로 전환
14. `cache` 커맨드 → index + snapshot 추출로 전환
15. Phase 2 에이전트 업데이트 (workflow-insight.md)
16. B2/B3 등 기존 버그 수정

### Sprint 5: 고급 기능

17. FTS5 프롬프트 검색
18. 프롬프트 클러스터링 (BM25 → DB 기반)
19. `--sql-file` 커스텀 쿼리 — Phase 2 에이전트가 레포별 `.sql` 작성
20. 글로벌 인덱스

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
| 빌트인 perspective | Rust 코드에 직접 정의 (`register_perspectives()`) | Repository에서 프로그래밍적으로 파라미터 결정, `.sql` 파일 불필요 |
| 커스텀 쿼리 | `--sql-file` (Phase 2 에이전트가 레포별 작성) | CLI 안에 `.sql` 없음, 에이전트가 분석 레포에 맞게 쿼리 작성 |
| 파라미터 전달 | `--param key=value` (동적) | perspective별 고유 파라미터를 유연하게 지원 |
| 쿼리 안전성 | named param (`:name` → `?N`) 바인딩 | SQL injection 방지 |
| 인크리멘털 전략 | size + mtime 변경 감지 | 단순하고 신뢰성 있음 |
| 파생 테이블 | 전체 재계산 | 데이터 규모가 작아 충분히 빠름 |
| v2 호환 | 기존 CLI 인터페이스 유지 | 점진적 마이그레이션 |
| Phase 2 연동 | query 서브커맨드 | 에이전트가 필요한 것만 요청 |

---

## 16. 테스트 계획 (블랙박스)

v3 CLI는 **블랙박스 테스트** 중심으로 검증한다.
내부 구현(Repository, SQLite 스키마 등)을 직접 검증하지 않고, CLI 바이너리의 **입력(fixture + 옵션) → 출력(stdout JSON, stderr, exit code)** 만 확인한다.

### 16-1. 테스트 의존성

```toml
[dev-dependencies]
assert_cmd = "2.0"      # CLI 바이너리 실행 + 검증
predicates = "3.1"      # stdout/stderr 매칭
tempfile = "3.14"       # 임시 디렉토리 (테스트 격리)
serde_json = "1.0"      # JSON 출력 파싱
```

### 16-2. Fixture 전략

```
cli/tests/
├── fixtures/
│   ├── sessions/                    # 테스트용 JSONL 파일
│   │   ├── minimal.jsonl            # 프롬프트 1개, tool_use 1개 (최소)
│   │   ├── multi_tool.jsonl         # 다양한 도구 사용 (Edit, Bash, Read 등)
│   │   ├── bash_classified.jsonl    # Bash 분류 가능 (git, test, npm 등)
│   │   ├── file_edits.jsonl         # Edit/Write로 파일 편집 포함
│   │   ├── empty.jsonl              # 빈 파일 (프롬프트 0개)
│   │   └── malformed.jsonl          # 잘못된 JSON 라인 포함
│   └── custom_queries/
│       ├── valid_select.sql         # 유효한 SELECT 쿼리
│       └── has_insert.sql           # INSERT 포함 (거부돼야 함)
```

fixture는 **실제 Claude 세션 JSONL 구조를 최소화**한 것.
각 fixture는 **어떤 테스트 시나리오를 위한 것인지** 파일명에 드러나야 한다.

### 16-3. 테스트 헬퍼

```rust
// tests/helpers/mod.rs

use assert_cmd::Command;
use tempfile::TempDir;
use std::path::PathBuf;

/// 임시 프로젝트 디렉토리 생성 + fixture JSONL 복사
pub fn setup_project(fixtures: &[&str]) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let sessions_dir = tmp.path().join(".claude").join("projects").join("test");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    for fixture in fixtures {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sessions")
            .join(fixture);
        let dest = sessions_dir.join(fixture);
        std::fs::copy(&src, &dest).unwrap();
    }

    let project_path = tmp.path().to_path_buf();
    (tmp, project_path)
}

/// suggest-workflow CLI 커맨드 빌드
pub fn cli() -> Command {
    Command::cargo_bin("suggest-workflow").unwrap()
}
```

### 16-4. 테스트 케이스

#### A. `index` 서브커맨드

| # | 시나리오 | 입력 | 검증 |
|---|---------|------|------|
| A1 | 첫 인덱싱 | fixture 3개 + `index` | exit 0, stderr에 "Indexed: 3 new" 포함 |
| A2 | 인크리멘털 (변경 없음) | A1 후 다시 `index` | stderr에 "0 new, 0 updated, 3 unchanged" |
| A3 | 인크리멘털 (1개 변경) | A1 후 fixture 1개 수정 → `index` | stderr에 "1 updated, 2 unchanged" |
| A4 | 인크리멘털 (1개 추가) | A1 후 fixture 1개 추가 → `index` | stderr에 "1 new, 3 unchanged" |
| A5 | 인크리멘털 (1개 삭제) | A1 후 fixture 1개 삭제 → `index` | stderr에 "1 deleted, 2 unchanged" |
| A6 | `--full` 재구축 | A1 후 `index --full` | stderr에 "Indexed: 3 new" (전체 재구축) |
| A7 | 빈 프로젝트 | fixture 0개 + `index` | exit 0, stderr에 "0 new" |
| A8 | malformed JSONL | `malformed.jsonl` + `index` | exit 0 (에러 세션 skip), stderr에 경고 |

```rust
#[test]
fn a1_first_index_creates_db() {
    let (_tmp, project) = setup_project(&[
        "minimal.jsonl",
        "multi_tool.jsonl",
        "bash_classified.jsonl",
    ]);

    cli().args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicates::str::contains("3 new"));
}

#[test]
fn a2_incremental_skips_unchanged() {
    let (_tmp, project) = setup_project(&["minimal.jsonl"]);

    // 첫 인덱싱
    cli().args(["index", "--project", project.to_str().unwrap()])
        .assert().success();

    // 두 번째 — 변경 없음
    cli().args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicates::str::contains("0 new"))
        .stderr(predicates::str::contains("1 unchanged"));
}
```

#### B. `query --perspective` (빌트인 perspective)

| # | 시나리오 | 입력 | 검증 |
|---|---------|------|------|
| B1 | tool-frequency 기본 | `query --perspective tool-frequency` | exit 0, stdout가 valid JSON 배열, `tool` + `frequency` 키 존재 |
| B2 | tool-frequency --param top=3 | `--param top=3` | JSON 배열 길이 ≤ 3 |
| B3 | transitions (required param) | `--perspective transitions --param tool=Edit` | exit 0, `to_tool` + `probability` 키 존재 |
| B4 | transitions (param 누락) | `--perspective transitions` (--param 없음) | exit ≠ 0, stderr에 "missing required param" |
| B5 | trends 기본 | `--perspective trends` | exit 0, `week_start` 키 존재 |
| B6 | trends --param since | `--param since=2026-02-01` | 반환된 모든 `week_start` ≥ 2026-02-01 |
| B7 | hotfiles | `--perspective hotfiles` | exit 0, `file_path` + `edit_count` 키 존재 |
| B8 | 존재하지 않는 perspective | `--perspective nonexistent` | exit ≠ 0, stderr에 "unknown perspective" |
| B9 | 빈 DB 쿼리 | 인덱싱 없이 바로 `query` | exit ≠ 0 또는 빈 배열 |

```rust
#[test]
fn b1_tool_frequency_returns_valid_json() {
    let (_tmp, project) = setup_project(&["multi_tool.jsonl"]);
    cli().args(["index", "--project", project.to_str().unwrap()])
        .assert().success();

    let output = cli()
        .args(["query", "--project", project.to_str().unwrap(),
               "--perspective", "tool-frequency"])
        .assert()
        .success()
        .get_output().stdout.clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty());
    assert!(arr[0].get("tool").is_some());
    assert!(arr[0].get("frequency").is_some());
}

#[test]
fn b4_transitions_missing_required_param() {
    let (_tmp, project) = setup_project(&["multi_tool.jsonl"]);
    cli().args(["index", "--project", project.to_str().unwrap()])
        .assert().success();

    cli().args(["query", "--project", project.to_str().unwrap(),
                "--perspective", "transitions"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("missing required param"));
}
```

#### C. `query --list-perspectives`

| # | 시나리오 | 검증 |
|---|---------|------|
| C1 | 목록 출력 | exit 0, 모든 빌트인 perspective 이름이 출력에 포함 |
| C2 | 파라미터 정보 | 각 perspective의 `--param` 이름과 required/default 정보 표시 |

```rust
#[test]
fn c1_list_perspectives_shows_all_builtins() {
    let (_tmp, project) = setup_project(&["minimal.jsonl"]);
    cli().args(["index", "--project", project.to_str().unwrap()])
        .assert().success();

    let assert = cli()
        .args(["query", "--project", project.to_str().unwrap(),
               "--list-perspectives"])
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(stderr.contains("tool-frequency"));
    assert!(stderr.contains("transitions"));
    assert!(stderr.contains("trends"));
    assert!(stderr.contains("hotfiles"));
}
```

#### D. `query --sql-file` (커스텀 SQL)

| # | 시나리오 | 입력 | 검증 |
|---|---------|------|------|
| D1 | 유효한 SELECT | `valid_select.sql` | exit 0, stdout가 valid JSON |
| D2 | INSERT 포함 SQL | `has_insert.sql` | exit ≠ 0, stderr에 거부 메시지 |
| D3 | 존재하지 않는 파일 | `--sql-file /tmp/nope.sql` | exit ≠ 0, stderr에 파일 에러 |
| D4 | 빈 SQL 파일 | 빈 `.sql` 파일 | exit ≠ 0, stderr에 에러 |

```rust
#[test]
fn d1_custom_sql_file_returns_json() {
    let (_tmp, project) = setup_project(&["multi_tool.jsonl"]);
    cli().args(["index", "--project", project.to_str().unwrap()])
        .assert().success();

    // 커스텀 SQL 파일 작성
    let sql_file = _tmp.path().join("custom.sql");
    std::fs::write(&sql_file,
        "SELECT classified_name, COUNT(*) as cnt FROM tool_uses GROUP BY classified_name"
    ).unwrap();

    let output = cli()
        .args(["query", "--project", project.to_str().unwrap(),
               "--sql-file", sql_file.to_str().unwrap()])
        .assert()
        .success()
        .get_output().stdout.clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json.as_array().is_some());
}

#[test]
fn d2_sql_file_with_insert_rejected() {
    let (_tmp, project) = setup_project(&["minimal.jsonl"]);
    cli().args(["index", "--project", project.to_str().unwrap()])
        .assert().success();

    let sql_file = _tmp.path().join("bad.sql");
    std::fs::write(&sql_file,
        "INSERT INTO sessions (id) VALUES ('hacked')"
    ).unwrap();

    cli().args(["query", "--project", project.to_str().unwrap(),
                "--sql-file", sql_file.to_str().unwrap()])
        .assert()
        .failure();
}
```

#### E. DB 경로 resolve

| # | 시나리오 | 입력 | 검증 |
|---|---------|------|------|
| E1 | `--project` 지정 | `--project /tmp/test-proj` | 해당 프로젝트의 인코딩된 경로에 DB 생성 |
| E2 | `--db` 직접 지정 | `--db /tmp/test.db` | 지정된 경로에 DB 생성 |
| E3 | `--project` 생략 | cwd에서 실행 | cwd 기준 DB 경로 사용 |
| E4 | `--db` + `--project` 동시 | 둘 다 지정 | `--db`가 우선 |

#### F. 출력 형식 + 파이프

| # | 시나리오 | 검증 |
|---|---------|------|
| F1 | stdout는 valid JSON | 모든 query 출력이 `serde_json::from_slice` 성공 |
| F2 | stderr는 사람 읽기용 | 로그, 경고, 에러는 stderr로 출력 |
| F3 | exit code 규칙 | 성공=0, 사용자 에러(param 누락 등)=1, 내부 에러=2 |

#### G. v2 호환 (Sprint 4 이후)

| # | 시나리오 | 검증 |
|---|---------|------|
| G1 | `analyze` 커맨드 | v2와 동일한 JSON 형식 출력 |
| G2 | `cache` 커맨드 | `analysis-snapshot.json` 생성 |

### 16-5. 테스트 실행

```bash
# 전체 블랙박스 테스트
cargo test --test '*'

# 특정 카테고리
cargo test --test blackbox_index      # A: index 테스트
cargo test --test blackbox_query      # B+C+D: query 테스트
cargo test --test blackbox_resolve    # E: DB 경로 테스트

# 특정 테스트
cargo test --test blackbox_query b4_transitions_missing_required_param
```

### 16-6. 테스트 파일 구조

```
cli/tests/
├── fixtures/
│   ├── sessions/
│   │   ├── minimal.jsonl
│   │   ├── multi_tool.jsonl
│   │   ├── bash_classified.jsonl
│   │   ├── file_edits.jsonl
│   │   ├── empty.jsonl
│   │   └── malformed.jsonl
│   └── custom_queries/
│       ├── valid_select.sql
│       └── has_insert.sql
├── helpers/
│   └── mod.rs                      # setup_project(), cli()
├── blackbox_index.rs               # A1~A8
├── blackbox_query.rs               # B1~B9, C1~C2, D1~D4
├── blackbox_resolve.rs             # E1~E4
├── blackbox_output.rs              # F1~F3
└── blackbox_v2_compat.rs           # G1~G2 (Sprint 4 이후)
```

### 16-7. 스프린트별 테스트 범위

| 스프린트 | 테스트 케이스 | 비고 |
|---------|-------------|------|
| Sprint 1 | A1, A7, A8 | 기본 인덱싱만 |
| Sprint 2 | B1~B9, C1~C2, D1~D4 | perspective + 커스텀 SQL |
| Sprint 3 | A2~A6, E1~E4 | 인크리멘털 + 경로 resolve |
| Sprint 4 | G1~G2, F1~F3 | v2 호환 + 출력 규격 |
