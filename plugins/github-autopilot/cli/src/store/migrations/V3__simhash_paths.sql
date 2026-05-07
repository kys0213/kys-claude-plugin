-- C10 ledger-based stagnation detection — store the per-task simhash
-- signature (64-bit, mapped to i64 by rusqlite — bit pattern preserved
-- via `as i64` cast at insert and `as u64` at select) and the JSON-
-- serialized list of affected source paths. Both are nullable so that
-- legacy rows from V1/V2 keep working without backfill (per spec
-- §3.2 "Storage").
--
-- The columns are added with `ALTER TABLE`; SQLite's `ADD COLUMN` is
-- always idempotent guard-wise via the SCHEMA_VERSION bump, so we don't
-- need an `IF NOT EXISTS` (which `ADD COLUMN` doesn't support anyway —
-- but the migrate() runner only applies V3 when found < 3).

ALTER TABLE tasks ADD COLUMN simhash INTEGER;
ALTER TABLE tasks ADD COLUMN affected_paths TEXT;

UPDATE meta SET value = '3' WHERE key = 'schema_version';
