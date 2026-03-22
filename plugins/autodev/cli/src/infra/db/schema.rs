use anyhow::Result;
use rusqlite::Connection;

/// v5 migration: transition_events + history tables + source_id lineage + item_history.
pub fn migrate_v5(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS transition_events (
            id          TEXT PRIMARY KEY,
            work_id     TEXT NOT NULL,
            source_id   TEXT NOT NULL,
            event_type  TEXT NOT NULL,
            phase       TEXT,
            detail      TEXT,
            created_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_transition_events_work_id
            ON transition_events(work_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_transition_events_created
            ON transition_events(created_at);

        CREATE TABLE IF NOT EXISTS history (
            id              TEXT PRIMARY KEY,
            source_id       TEXT NOT NULL,
            workspace_id    TEXT NOT NULL,
            task_kind       TEXT NOT NULL,
            status          TEXT NOT NULL,
            error_message   TEXT,
            duration_ms     INTEGER,
            created_at      TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_history_source ON history(source_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_history_workspace ON history(workspace_id, created_at);
        ",
    )?;

    // source_id column on queue_items
    let alter_sql = "ALTER TABLE queue_items ADD COLUMN source_id TEXT DEFAULT ''";
    match conn.execute(alter_sql, []) {
        Ok(_) => {}
        Err(e) if e.to_string().contains("duplicate column") => {}
        Err(e) => return Err(e.into()),
    }

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_queue_items_source_id ON queue_items(source_id);",
    )?;

    // append-only item_history table
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS item_history (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id   TEXT NOT NULL,
            work_id     TEXT NOT NULL,
            state       TEXT NOT NULL,
            status      TEXT NOT NULL,
            attempt     INTEGER NOT NULL DEFAULT 1,
            summary     TEXT,
            error       TEXT,
            created_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_item_history_source ON item_history(source_id, created_at);
        ",
    )?;

    Ok(())
}

/// v4 마이그레이션: queue_items에 failure_count, escalation_level 컬럼 추가.
pub fn migrate_v4(conn: &Connection) -> Result<()> {
    let migrations = [
        "ALTER TABLE queue_items ADD COLUMN failure_count INTEGER DEFAULT 0",
        "ALTER TABLE queue_items ADD COLUMN escalation_level INTEGER DEFAULT 0",
    ];
    for sql in &migrations {
        match conn.execute(sql, []) {
            Ok(_) => {}
            Err(e) if e.to_string().contains("duplicate column") => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// v3 마이그레이션: specs에 priority 컬럼 추가.
pub fn migrate_v3(conn: &Connection) -> Result<()> {
    let sql = "ALTER TABLE specs ADD COLUMN priority INTEGER";
    match conn.execute(sql, []) {
        Ok(_) => {}
        Err(e) if e.to_string().contains("duplicate column") => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

/// v2 마이그레이션: queue_items에 task_kind, github_number, metadata_json 컬럼 추가.
pub fn migrate_v2(conn: &Connection) -> Result<()> {
    let migrations = [
        "ALTER TABLE queue_items ADD COLUMN task_kind TEXT NOT NULL DEFAULT 'analyze'",
        "ALTER TABLE queue_items ADD COLUMN github_number INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE queue_items ADD COLUMN metadata_json TEXT",
    ];
    for sql in &migrations {
        match conn.execute(sql, []) {
            Ok(_) => {}
            Err(e) if e.to_string().contains("duplicate column") => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

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

        CREATE TABLE IF NOT EXISTS specs (
            id                  TEXT PRIMARY KEY,
            repo_id             TEXT NOT NULL REFERENCES repositories(id),
            title               TEXT NOT NULL,
            body                TEXT NOT NULL,
            status              TEXT NOT NULL DEFAULT 'active',
            source_path         TEXT,
            test_commands       TEXT,
            acceptance_criteria TEXT,
            priority            INTEGER,
            created_at          TEXT NOT NULL,
            updated_at          TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS spec_issues (
            spec_id        TEXT NOT NULL REFERENCES specs(id),
            issue_number   INTEGER NOT NULL,
            created_at     TEXT NOT NULL,
            PRIMARY KEY (spec_id, issue_number)
        );

        CREATE TABLE IF NOT EXISTS hitl_events (
            id          TEXT PRIMARY KEY,
            repo_id     TEXT NOT NULL REFERENCES repositories(id),
            spec_id     TEXT,
            work_id     TEXT,
            severity    TEXT NOT NULL,
            situation   TEXT NOT NULL,
            context     TEXT NOT NULL DEFAULT '',
            options     TEXT NOT NULL DEFAULT '[]',
            status      TEXT NOT NULL DEFAULT 'pending',
            created_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_hitl_events_repo ON hitl_events(repo_id, status);

        CREATE TABLE IF NOT EXISTS hitl_responses (
            id          TEXT PRIMARY KEY,
            event_id    TEXT NOT NULL REFERENCES hitl_events(id),
            choice      INTEGER,
            message     TEXT,
            source      TEXT NOT NULL,
            created_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS cron_jobs (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            repo_id         TEXT REFERENCES repositories(id),
            schedule_type   TEXT NOT NULL,
            schedule_value  TEXT NOT NULL,
            script_path     TEXT NOT NULL,
            status          TEXT NOT NULL DEFAULT 'active',
            builtin         INTEGER NOT NULL DEFAULT 0,
            last_run_at     TEXT,
            created_at      TEXT NOT NULL,
            UNIQUE(name, repo_id)
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_cron_jobs_name_global
            ON cron_jobs(name) WHERE repo_id IS NULL;

        CREATE TABLE IF NOT EXISTS queue_items (
            work_id           TEXT PRIMARY KEY,
            repo_id           TEXT NOT NULL REFERENCES repositories(id),
            queue_type        TEXT NOT NULL,
            phase             TEXT NOT NULL DEFAULT 'pending',
            title             TEXT,
            skip_reason       TEXT,
            created_at        TEXT NOT NULL,
            updated_at        TEXT NOT NULL,
            failure_count     INTEGER DEFAULT 0,
            escalation_level  INTEGER DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_queue_items_repo_phase ON queue_items(repo_id, phase);

        CREATE TABLE IF NOT EXISTS claw_decisions (
            id              TEXT PRIMARY KEY,
            repo_id         TEXT NOT NULL REFERENCES repositories(id),
            spec_id         TEXT,
            decision_type   TEXT NOT NULL,
            target_work_id  TEXT,
            reasoning       TEXT NOT NULL,
            context_json    TEXT,
            created_at      TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_claw_decisions_repo ON claw_decisions(repo_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_claw_decisions_spec ON claw_decisions(spec_id, created_at);

        CREATE TABLE IF NOT EXISTS feedback_patterns (
            id              TEXT PRIMARY KEY,
            repo_id         TEXT NOT NULL REFERENCES repositories(id),
            pattern_type    TEXT NOT NULL,
            suggestion      TEXT NOT NULL,
            source          TEXT NOT NULL,
            occurrence_count INTEGER NOT NULL DEFAULT 1,
            confidence      REAL NOT NULL DEFAULT 0.5,
            status          TEXT NOT NULL DEFAULT 'active',
            sources_json    TEXT NOT NULL DEFAULT '{}',
            created_at      TEXT NOT NULL,
            last_seen_at    TEXT NOT NULL,
            UNIQUE(repo_id, pattern_type, suggestion)
        );
        CREATE INDEX IF NOT EXISTS idx_feedback_patterns_repo ON feedback_patterns(repo_id, status);

        CREATE TABLE IF NOT EXISTS history (
            id              TEXT PRIMARY KEY,
            source_id       TEXT NOT NULL,
            workspace_id    TEXT NOT NULL,
            task_kind       TEXT NOT NULL,
            status          TEXT NOT NULL,
            error_message   TEXT,
            duration_ms     INTEGER,
            created_at      TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_history_source ON history(source_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_history_workspace ON history(workspace_id, created_at);

        COMMIT;
        ",
    )?;
    Ok(())
}
