-- Partial indexes for the lookup helpers added in the spec refinement.
-- find_task_by_pr / find_active_by_spec_path are called frequently from
-- MergeLoop and WatchDispatcher; full scans are unnecessary.
--
-- The indexes are partial because:
--   * Most tasks are not bound to a PR (pr_number IS NULL), so a full
--     index would mostly store NULL keys.
--   * Only active epics are matched by spec_path; abandoned/completed
--     epics never participate in WatchDispatcher routing.

CREATE INDEX IF NOT EXISTS idx_tasks_pr_number
  ON tasks(pr_number)
  WHERE pr_number IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_epics_active_spec
  ON epics(spec_path)
  WHERE status = 'active';

UPDATE meta SET value = '2' WHERE key = 'schema_version';
