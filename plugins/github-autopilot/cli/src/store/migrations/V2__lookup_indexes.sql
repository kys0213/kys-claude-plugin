-- Partial indexes:
--   * idx_tasks_pr_number — most tasks have pr_number IS NULL, so a full
--     index would mostly store NULL keys.
--   * idx_epics_active_spec — abandoned/completed epics never participate
--     in WatchDispatcher routing, so the index only needs active rows.

CREATE INDEX IF NOT EXISTS idx_tasks_pr_number
  ON tasks(pr_number)
  WHERE pr_number IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_epics_active_spec
  ON epics(spec_path)
  WHERE status = 'active';

UPDATE meta SET value = '2' WHERE key = 'schema_version';
