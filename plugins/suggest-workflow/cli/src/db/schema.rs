pub const SCHEMA_VERSION: u32 = 4;

pub const DDL: &str = "
-- 메타 정보
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- 세션 목록
CREATE TABLE IF NOT EXISTS sessions (
    id             TEXT PRIMARY KEY,
    file_path      TEXT NOT NULL,
    file_size      INTEGER NOT NULL,
    file_mtime     INTEGER NOT NULL,
    first_ts       INTEGER,
    last_ts        INTEGER,
    prompt_count   INTEGER NOT NULL DEFAULT 0,
    tool_use_count INTEGER NOT NULL DEFAULT 0,
    first_prompt_snippet TEXT,
    indexed_at     TEXT NOT NULL
);

-- 프롬프트
CREATE TABLE IF NOT EXISTS prompts (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    text       TEXT NOT NULL,
    timestamp  INTEGER NOT NULL,
    char_count INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_prompts_session ON prompts(session_id);
CREATE INDEX IF NOT EXISTS idx_prompts_ts ON prompts(timestamp);

-- 도구 사용
CREATE TABLE IF NOT EXISTS tool_uses (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq_order       INTEGER NOT NULL,
    tool_name       TEXT NOT NULL,
    classified_name TEXT NOT NULL,
    timestamp       INTEGER,
    input_json      TEXT
);
CREATE INDEX IF NOT EXISTS idx_tool_uses_session ON tool_uses(session_id);
CREATE INDEX IF NOT EXISTS idx_tool_uses_tool ON tool_uses(classified_name);
CREATE INDEX IF NOT EXISTS idx_tool_uses_ts ON tool_uses(timestamp);

-- 파일 편집
CREATE TABLE IF NOT EXISTS file_edits (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id   TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tool_use_id  INTEGER REFERENCES tool_uses(id) ON DELETE CASCADE,
    file_path    TEXT NOT NULL,
    timestamp    INTEGER
);
CREATE INDEX IF NOT EXISTS idx_file_edits_session ON file_edits(session_id);
CREATE INDEX IF NOT EXISTS idx_file_edits_path ON file_edits(file_path);

-- 도구 전이 (파생)
CREATE TABLE IF NOT EXISTS tool_transitions (
    from_tool   TEXT NOT NULL,
    to_tool     TEXT NOT NULL,
    count       INTEGER NOT NULL,
    probability REAL NOT NULL,
    PRIMARY KEY (from_tool, to_tool)
);

-- 주간 트렌드 (파생)
CREATE TABLE IF NOT EXISTS weekly_buckets (
    week_start     TEXT NOT NULL,
    tool_name      TEXT NOT NULL,
    count          INTEGER NOT NULL,
    session_count  INTEGER NOT NULL,
    PRIMARY KEY (week_start, tool_name)
);
CREATE INDEX IF NOT EXISTS idx_weekly_week ON weekly_buckets(week_start);

-- 파일 핫스팟 (파생)
CREATE TABLE IF NOT EXISTS file_hotspots (
    file_path     TEXT PRIMARY KEY,
    edit_count    INTEGER NOT NULL,
    session_count INTEGER NOT NULL
);

-- 세션 간 연결 (파생)
CREATE TABLE IF NOT EXISTS session_links (
    session_a       TEXT NOT NULL,
    session_b       TEXT NOT NULL,
    shared_files    INTEGER NOT NULL,
    overlap_ratio   REAL NOT NULL,
    time_gap_minutes INTEGER,
    PRIMARY KEY (session_a, session_b)
);
";
