use anyhow::Result;
use rusqlite::Connection;

pub fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        BEGIN EXCLUSIVE;

        CREATE TABLE IF NOT EXISTS repositories (
            id          TEXT PRIMARY KEY,
            url         TEXT NOT NULL UNIQUE,
            name        TEXT NOT NULL,
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS scan_cursors (
            repo_id     TEXT NOT NULL REFERENCES repositories(id),
            target      TEXT NOT NULL,
            last_seen   TEXT NOT NULL,
            last_scan   TEXT NOT NULL,
            PRIMARY KEY (repo_id, target)
        );

        CREATE TABLE IF NOT EXISTS consumer_logs (
            id          TEXT PRIMARY KEY,
            repo_id     TEXT NOT NULL REFERENCES repositories(id),
            queue_type  TEXT NOT NULL,
            queue_item_id TEXT NOT NULL,
            worker_id   TEXT NOT NULL,
            command     TEXT NOT NULL,
            stdout      TEXT,
            stderr      TEXT,
            exit_code   INTEGER,
            started_at  TEXT NOT NULL,
            finished_at TEXT,
            duration_ms INTEGER
        );

        CREATE INDEX IF NOT EXISTS idx_consumer_logs_repo ON consumer_logs(repo_id, started_at);

        CREATE TABLE IF NOT EXISTS token_usage (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            log_id              TEXT NOT NULL REFERENCES consumer_logs(id),
            repo_id             TEXT NOT NULL REFERENCES repositories(id),
            queue_type          TEXT NOT NULL,
            queue_item_id       TEXT NOT NULL,
            input_tokens        INTEGER NOT NULL DEFAULT 0,
            output_tokens       INTEGER NOT NULL DEFAULT 0,
            cache_write_tokens  INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens   INTEGER NOT NULL DEFAULT 0,
            created_at          TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_token_usage_repo ON token_usage(repo_id, created_at);

        COMMIT;
        ",
    )?;
    Ok(())
}
