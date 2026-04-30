PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS epics (
  name         TEXT PRIMARY KEY,
  spec_path    TEXT NOT NULL,
  branch       TEXT NOT NULL,
  status       TEXT NOT NULL CHECK (status IN ('active','completed','abandoned')),
  created_at   TEXT NOT NULL,
  completed_at TEXT
);

CREATE TABLE IF NOT EXISTS tasks (
  id              TEXT PRIMARY KEY,
  epic_name       TEXT NOT NULL REFERENCES epics(name),
  source          TEXT NOT NULL CHECK (source IN ('decompose','gap-watch','qa-boost','ci-watch','human')),
  fingerprint     TEXT,
  title           TEXT NOT NULL,
  body            TEXT,
  status          TEXT NOT NULL CHECK (status IN ('pending','ready','wip','blocked','done','escalated')),
  attempts        INTEGER NOT NULL DEFAULT 0,
  branch          TEXT,
  pr_number       INTEGER,
  escalated_issue INTEGER,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_deps (
  task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  depends_on TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  PRIMARY KEY (task_id, depends_on)
);

CREATE TABLE IF NOT EXISTS events (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  epic_name TEXT,
  task_id   TEXT,
  kind      TEXT NOT NULL,
  payload   TEXT NOT NULL DEFAULT '{}',
  at        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS escalation_suppression (
  fingerprint    TEXT NOT NULL,
  reason         TEXT NOT NULL,
  suppress_until TEXT NOT NULL,
  PRIMARY KEY (fingerprint, reason)
);

CREATE INDEX IF NOT EXISTS idx_tasks_epic_status ON tasks(epic_name, status);
CREATE INDEX IF NOT EXISTS idx_tasks_fingerprint ON tasks(fingerprint);
CREATE INDEX IF NOT EXISTS idx_events_epic_at   ON events(epic_name, at);
CREATE INDEX IF NOT EXISTS idx_events_task_at   ON events(task_id, at);

INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '1');
