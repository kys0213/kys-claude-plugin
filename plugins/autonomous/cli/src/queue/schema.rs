use anyhow::Result;
use rusqlite::Connection;

pub fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS repositories (
            id          TEXT PRIMARY KEY,
            url         TEXT NOT NULL UNIQUE,
            name        TEXT NOT NULL,
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS repo_configs (
            repo_id             TEXT PRIMARY KEY REFERENCES repositories(id),
            scan_interval_secs  INTEGER NOT NULL DEFAULT 300,
            scan_targets        TEXT NOT NULL DEFAULT '[\"issues\",\"pulls\"]',
            issue_concurrency   INTEGER NOT NULL DEFAULT 1,
            pr_concurrency      INTEGER NOT NULL DEFAULT 1,
            merge_concurrency   INTEGER NOT NULL DEFAULT 1,
            model               TEXT NOT NULL DEFAULT 'sonnet',
            issue_workflow      TEXT NOT NULL DEFAULT 'multi-llm',
            pr_workflow         TEXT NOT NULL DEFAULT '/multi-review',
            filter_labels       TEXT DEFAULT NULL,
            ignore_authors      TEXT DEFAULT '[\"dependabot\",\"renovate\"]',
            workspace_strategy  TEXT NOT NULL DEFAULT 'worktree',
            gh_host             TEXT DEFAULT NULL
        );

        CREATE TABLE IF NOT EXISTS scan_cursors (
            repo_id     TEXT NOT NULL REFERENCES repositories(id),
            target      TEXT NOT NULL,
            last_seen   TEXT NOT NULL,
            last_scan   TEXT NOT NULL,
            PRIMARY KEY (repo_id, target)
        );

        CREATE TABLE IF NOT EXISTS issue_queue (
            id              TEXT PRIMARY KEY,
            repo_id         TEXT NOT NULL REFERENCES repositories(id),
            github_number   INTEGER NOT NULL,
            title           TEXT NOT NULL,
            body            TEXT,
            labels          TEXT,
            author          TEXT NOT NULL,
            analysis_report TEXT,
            status          TEXT NOT NULL DEFAULT 'pending',
            worker_id       TEXT,
            branch_name     TEXT,
            pr_number       INTEGER,
            error_message   TEXT,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS pr_queue (
            id              TEXT PRIMARY KEY,
            repo_id         TEXT NOT NULL REFERENCES repositories(id),
            github_number   INTEGER NOT NULL,
            title           TEXT NOT NULL,
            body            TEXT,
            author          TEXT NOT NULL,
            head_branch     TEXT NOT NULL,
            base_branch     TEXT NOT NULL,
            review_comment  TEXT,
            status          TEXT NOT NULL DEFAULT 'pending',
            worker_id       TEXT,
            error_message   TEXT,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS merge_queue (
            id              TEXT PRIMARY KEY,
            repo_id         TEXT NOT NULL REFERENCES repositories(id),
            pr_number       INTEGER NOT NULL,
            title           TEXT NOT NULL,
            head_branch     TEXT NOT NULL,
            base_branch     TEXT NOT NULL,
            status          TEXT NOT NULL DEFAULT 'pending',
            conflict_files  TEXT,
            worker_id       TEXT,
            error_message   TEXT,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL
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

        CREATE INDEX IF NOT EXISTS idx_issue_queue_status ON issue_queue(repo_id, status);
        CREATE INDEX IF NOT EXISTS idx_pr_queue_status ON pr_queue(repo_id, status);
        CREATE INDEX IF NOT EXISTS idx_merge_queue_status ON merge_queue(repo_id, status);
        CREATE INDEX IF NOT EXISTS idx_consumer_logs_repo ON consumer_logs(repo_id, started_at);
        ",
    )?;

    // Migration: add gh_host column to existing repo_configs tables.
    // ALTER TABLE ADD COLUMN fails if the column already exists,
    // so we silently ignore the error.
    let _ = conn.execute("ALTER TABLE repo_configs ADD COLUMN gh_host TEXT DEFAULT NULL", []);

    Ok(())
}
