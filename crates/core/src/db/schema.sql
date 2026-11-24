PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS lists (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, color TEXT NOT NULL DEFAULT '#378ADD',
  sort_order INTEGER NOT NULL DEFAULT 0,
  deleted_at INTEGER,
  dirty INTEGER NOT NULL DEFAULT 1,
  updated_at INTEGER NOT NULL,
  origin TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  list_id TEXT NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
  title TEXT NOT NULL, notes TEXT NOT NULL DEFAULT '',
  status INTEGER NOT NULL DEFAULT 0, priority INTEGER NOT NULL DEFAULT 0,
  due_at INTEGER, completed_at INTEGER,
  sort_order REAL NOT NULL DEFAULT 0,
  deleted_at INTEGER,
  dirty INTEGER NOT NULL DEFAULT 1,
  updated_at INTEGER NOT NULL,
  origin TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_list_status ON tasks(list_id, status, sort_order);
CREATE INDEX IF NOT EXISTS idx_tasks_due ON tasks(due_at) WHERE due_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_dirty ON tasks(dirty) WHERE dirty = 1;

CREATE TABLE IF NOT EXISTS sync_client_state (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  cursor INTEGER NOT NULL DEFAULT 0,
  generation TEXT,
  last_sync_at INTEGER,
  last_result TEXT,
  last_error TEXT,
  clock_offset_ms INTEGER
);

CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);
