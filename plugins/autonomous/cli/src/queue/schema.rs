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
            retry_count     INTEGER NOT NULL DEFAULT 0,
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
            retry_count     INTEGER NOT NULL DEFAULT 0,
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
            retry_count     INTEGER NOT NULL DEFAULT 0,
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

    // 기존 DB에 중복 데이터가 있을 수 있으므로 먼저 정리 후 UNIQUE 인덱스 생성
    migrate_unique_constraints(conn)?;

    Ok(())
}

/// 기존 DB 마이그레이션: 중복 제거 후 UNIQUE 인덱스 추가
fn migrate_unique_constraints(conn: &Connection) -> Result<()> {
    // 이미 인덱스가 존재하면 스킵 (CREATE UNIQUE INDEX IF NOT EXISTS)
    // 중복 데이터가 있으면 인덱스 생성이 실패하므로 먼저 정리
    let tables = [
        ("issue_queue", "github_number"),
        ("pr_queue", "github_number"),
        ("merge_queue", "pr_number"),
    ];

    for (table, number_col) in &tables {
        let idx_name = format!("idx_{table}_unique");

        // 인덱스가 이미 존재하는지 확인
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='index' AND name=?1",
            rusqlite::params![idx_name],
            |row| row.get(0),
        )?;

        if exists {
            continue;
        }

        // 중복 제거: 각 (repo_id, number) 그룹에서 가장 오래된 항목만 유지
        conn.execute(
            &format!(
                "DELETE FROM {table} WHERE rowid NOT IN (\
                 SELECT MIN(rowid) FROM {table} GROUP BY repo_id, {number_col})"
            ),
            [],
        )?;

        // UNIQUE 인덱스 생성
        conn.execute(
            &format!("CREATE UNIQUE INDEX {idx_name} ON {table}(repo_id, {number_col})"),
            [],
        )?;
    }

    Ok(())
}
