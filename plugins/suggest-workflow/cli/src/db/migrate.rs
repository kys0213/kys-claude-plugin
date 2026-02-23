use anyhow::{Context, Result};
use rusqlite::Connection;

use super::schema;

/// Check schema version and run migrations if needed.
///
/// Possible outcomes:
/// - Fresh DB (no meta table) → return Ok(Fresh) for caller to initialize
/// - Version matches → return Ok(UpToDate)
/// - Version < current → run migration chain, return Ok(Migrated)
/// - Version > current → return error (downgrade not supported)
pub fn check_and_migrate(conn: &Connection) -> Result<MigrateResult> {
    // Check if meta table exists
    let has_meta: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='meta'",
        [],
        |row| row.get(0),
    )?;

    if !has_meta {
        return Ok(MigrateResult::Fresh);
    }

    // Read stored schema version
    let stored: Option<u32> = match conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(v) => v.parse().ok(),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.into()),
    };

    let stored = match stored {
        Some(v) => v,
        None => return Ok(MigrateResult::Fresh),
    };

    let current = schema::SCHEMA_VERSION;

    if stored == current {
        return Ok(MigrateResult::UpToDate);
    }

    if stored > current {
        anyhow::bail!(
            "DB schema version ({}) is newer than this CLI ({}). \
             Please upgrade suggest-workflow or run with --full to rebuild.",
            stored,
            current
        );
    }

    // Run migration chain: stored → stored+1 → ... → current
    run_migrations(conn, stored, current)?;

    // Update schema_version in meta
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
        [current.to_string()],
    )?;

    Ok(MigrateResult::Migrated {
        from: stored,
        to: current,
    })
}

#[derive(Debug, PartialEq)]
pub enum MigrateResult {
    Fresh,
    UpToDate,
    Migrated { from: u32, to: u32 },
}

/// Run sequential migrations from `from_version` to `to_version`.
fn run_migrations(conn: &Connection, from: u32, to: u32) -> Result<()> {
    for version in from..to {
        migrate_step(conn, version, version + 1)
            .with_context(|| format!("migration v{} → v{} failed", version, version + 1))?;
    }
    Ok(())
}

/// Execute a single migration step.
///
/// Add new migration steps here as the schema evolves.
fn migrate_step(conn: &Connection, from: u32, to: u32) -> Result<()> {
    match (from, to) {
        (3, 4) => {
            conn.execute_batch("ALTER TABLE sessions ADD COLUMN first_prompt_snippet TEXT;")?;
            // Backfill from existing prompts: take first prompt text (up to 500 chars)
            conn.execute_batch(
                "UPDATE sessions SET first_prompt_snippet = (
                    SELECT SUBSTR(p.text, 1, 500)
                    FROM prompts p
                    WHERE p.session_id = sessions.id
                    ORDER BY p.timestamp ASC
                    LIMIT 1
                ) WHERE first_prompt_snippet IS NULL;",
            )?;
            Ok(())
        }
        _ => {
            anyhow::bail!(
                "no migration path from v{} to v{}. Run with --full to rebuild.",
                from,
                to
            );
        }
    }
}
