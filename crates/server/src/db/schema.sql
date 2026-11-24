PRAGMA journal_mode = WAL;

-- 权威业务库（SPEC §4 原样，外加 server_rev）
CREATE TABLE IF NOT EXISTS lists (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, color TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  deleted_at INTEGER,
  updated_at INTEGER NOT NULL, origin TEXT NOT NULL DEFAULT '',
  server_rev INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  list_id TEXT NOT NULL,
  title TEXT NOT NULL, notes TEXT NOT NULL DEFAULT '',
  status INTEGER NOT NULL DEFAULT 0, priority INTEGER NOT NULL DEFAULT 0,
  due_at INTEGER, completed_at INTEGER,
  sort_order REAL NOT NULL DEFAULT 0,
  deleted_at INTEGER,
  updated_at INTEGER NOT NULL, origin TEXT NOT NULL DEFAULT '',
  server_rev INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_rev ON tasks(server_rev);
CREATE INDEX IF NOT EXISTS idx_lists_rev ON lists(server_rev);
CREATE INDEX IF NOT EXISTS idx_tasks_due ON tasks(due_at) WHERE due_at IS NOT NULL AND deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS devices (
  device_id TEXT PRIMARY KEY,
  name TEXT NOT NULL, platform TEXT NOT NULL,
  last_cursor INTEGER NOT NULL DEFAULT 0,
  last_sync_at INTEGER,
  clock_offset_ms INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS conflict_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  entity TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  winner_origin TEXT NOT NULL,
  loser_origin TEXT NOT NULL,
  winner_updated_at INTEGER NOT NULL,
  loser_updated_at INTEGER NOT NULL,
  resolved_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS reminder_ledger (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  anchor INTEGER NOT NULL,
  offset_min INTEGER NOT NULL DEFAULT 0,
  channel TEXT NOT NULL,
  status INTEGER NOT NULL DEFAULT 0,
  attempts INTEGER NOT NULL DEFAULT 0,
  sent_at INTEGER, last_error TEXT, created_at INTEGER NOT NULL,
  UNIQUE(task_id, kind, anchor, offset_min, channel)
);

CREATE TABLE IF NOT EXISTS runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  started_at INTEGER NOT NULL, finished_at INTEGER,
  scanned INTEGER NOT NULL DEFAULT 0, enqueued INTEGER NOT NULL DEFAULT 0,
  sent INTEGER NOT NULL DEFAULT 0, failed INTEGER NOT NULL DEFAULT 0, note TEXT
);

CREATE TABLE IF NOT EXISTS admin (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  username TEXT NOT NULL, password_hash TEXT NOT NULL, created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS secrets (key TEXT PRIMARY KEY, value_enc BLOB NOT NULL);

-- 通知配置（JSON，见 api/config.rs 的 NotifyConfig；密码/密钥另存 secrets 表加密）。
CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- Admin 会话（M10 落库持久化，替代内存表；重启后会话仍有效直至过期）。
CREATE TABLE IF NOT EXISTS sessions (
  token TEXT PRIMARY KEY,
  expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);
